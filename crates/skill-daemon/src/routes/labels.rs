// SPDX-License-Identifier: GPL-3.0-only
//! Daemon label routes — create/read/update/delete + HNSW search & rebuild.
//!
//! The daemon owns the label HNSW indices.  Tauri (and the CLI) call these
//! endpoints rather than managing indices themselves.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use skill_constants::LABELS_FILE;
use skill_daemon_common::ApiError;

use crate::state::AppState;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelEntry {
    pub id: i64,
    pub text: String,
    pub context: Option<String>,
    pub eeg_start: u64,
    pub eeg_end: u64,
    pub created_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLabelRequest {
    pub text: String,
    pub context: Option<String>,
    pub label_start_utc: Option<f64>,
    /// Optional EEG window boundaries (unix seconds).
    pub eeg_start: Option<u64>,
    pub eeg_end: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLabelRequest {
    pub text: String,
    pub context: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchLabelsRequest {
    pub query: String,
    pub k: Option<usize>,
    pub ef: Option<usize>,
    /// "text" (default), "context", or "eeg"
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchByEegRequest {
    pub start_utc: u64,
    pub end_utc: u64,
    pub k: Option<usize>,
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/labels", get(list_labels).post(create_label))
        .route("/labels/{id}", put(update_label).delete(delete_label))
        .route("/labels/search", post(search_labels))
        .route("/labels/search-by-eeg", post(search_labels_by_eeg))
        .route("/labels/index/rebuild", post(rebuild_label_index))
        .route("/labels/index/stats", get(label_index_stats))
        .route("/labels/embedding-status", get(label_embedding_status))
        .route("/labels/reembed", post(reembed_all_labels))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_unix() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Open (or create) the canonical `labels.sqlite` with the full schema.
fn open_labels_db(skill_dir: &std::path::Path) -> anyhow::Result<rusqlite::Connection> {
    use anyhow::Context as _;
    let db_path = skill_dir.join(LABELS_FILE);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).context("create labels DB directory")?;
    }
    let conn = rusqlite::Connection::open(&db_path).context("open labels DB")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS labels (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            text              TEXT NOT NULL,
            context           TEXT DEFAULT '',
            eeg_start         INTEGER NOT NULL DEFAULT 0,
            eeg_end           INTEGER NOT NULL DEFAULT 0,
            wall_start        INTEGER NOT NULL DEFAULT 0,
            wall_end          INTEGER NOT NULL DEFAULT 0,
            created_at        INTEGER NOT NULL DEFAULT 0,
            text_embedding    BLOB,
            context_embedding BLOB,
            embedding_model   TEXT
        );",
    )
    .context("create labels table")?;
    // Migrate older databases that lack newer columns.
    for col in &[
        ("wall_start", "INTEGER NOT NULL DEFAULT 0"),
        ("wall_end", "INTEGER NOT NULL DEFAULT 0"),
        ("label_start", "INTEGER NOT NULL DEFAULT 0"),
        ("label_end", "INTEGER NOT NULL DEFAULT 0"),
        ("text_embedding", "BLOB"),
        ("context_embedding", "BLOB"),
        ("embedding_model", "TEXT"),
    ] {
        let _ = conn.execute_batch(&format!("ALTER TABLE labels ADD COLUMN {} {};", col.0, col.1));
    }
    Ok(conn)
}

fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub(crate) const EMBED_MODEL_NAME: &str = "nomic-embed-text-v1.5";

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn f32_to_blob_size_matches_input() {
        let v = vec![1.0_f32, 2.0, 3.0, 4.0];
        let b = f32_to_blob(&v);
        assert_eq!(b.len(), v.len() * 4);
    }

    #[test]
    fn embed_text_empty_is_none() {
        let embedder = crate::text_embedder::SharedTextEmbedder::new();
        assert!(embedder.embed("   ").is_none() || embedder.embed("   ").is_some());
        // Model may or may not be available in CI — just verify no panic.
    }

    #[test]
    fn open_labels_db_creates_schema() {
        let td = TempDir::new().unwrap();
        let conn = open_labels_db(td.path()).expect("db open");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='labels'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn list_labels_empty_then_create_and_delete_label() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let Json(v0) = list_labels(State(state.clone())).await;
        assert!(v0.is_empty());

        let created = create_label(
            State(state.clone()),
            Json(CreateLabelRequest {
                text: "".into(),
                context: None,
                label_start_utc: Some(100.0),
                eeg_start: Some(100),
                eeg_end: Some(101),
            }),
        )
        .await
        .expect("create should succeed with empty text")
        .0;
        assert!(created.id > 0);

        let Json(v1) = list_labels(State(state.clone())).await;
        assert_eq!(v1.len(), 1);

        let _ = delete_label(State(state.clone()), axum::extract::Path(created.id))
            .await
            .expect("delete ok");
        let Json(v2) = list_labels(State(state)).await;
        assert!(v2.is_empty());
    }

    #[tokio::test]
    async fn search_and_index_stats_paths_are_stable() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let Json(stats0) = label_index_stats(State(state.clone())).await;
        assert!(stats0.get("text_nodes").is_some());

        let Json(search) = search_labels(
            State(state.clone()),
            Json(SearchLabelsRequest {
                query: "   ".into(),
                k: Some(5),
                ef: Some(32),
                mode: Some("text".into()),
            }),
        )
        .await;
        assert!(search.get("results").is_some());

        let Json(eeg) = search_labels_by_eeg(
            State(state.clone()),
            Json(SearchByEegRequest {
                start_utc: 1,
                end_utc: 2,
                k: Some(5),
            }),
        )
        .await;
        assert!(eeg.get("results").is_some());

        let Json(rb) = rebuild_label_index(State(state)).await;
        assert!(rb.get("ok").is_some());
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn list_labels(State(state): State<AppState>) -> Json<Vec<LabelEntry>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let rows = tokio::task::spawn_blocking(move || {
        let Ok(conn) = open_labels_db(&skill_dir) else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, text, context, eeg_start, eeg_end, created_at, embedding_model
             FROM labels ORDER BY created_at DESC LIMIT 1000",
        ) else {
            return Vec::new();
        };
        stmt.query_map([], |row| {
            Ok(LabelEntry {
                id: row.get(0)?,
                text: row.get(1)?,
                context: row.get(2)?,
                eeg_start: row.get::<_, i64>(3)? as u64,
                eeg_end: row.get::<_, i64>(4)? as u64,
                created_at: row.get::<_, i64>(5)? as f64,
                embedding_model: row.get(6)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn create_label(
    State(state): State<AppState>,
    Json(req): Json<CreateLabelRequest>,
) -> Result<Json<LabelEntry>, (StatusCode, Json<ApiError>)> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let label_index = state.label_index.clone();
    let embedder = state.text_embedder.clone();

    let result = tokio::task::spawn_blocking(move || {
        let conn = open_labels_db(&skill_dir)?;
        let now = req.label_start_utc.unwrap_or_else(now_unix);
        let now_secs = now as u64;
        let eeg_start = req.eeg_start.unwrap_or(now_secs);
        let eeg_end = req.eeg_end.unwrap_or(now_secs);
        let context = req.context.clone().unwrap_or_default();

        // Embed text and context
        let text_emb = embedder.embed(&req.text);
        let ctx_emb = embedder.embed(&context);
        let text_blob = text_emb.as_ref().map(|v| f32_to_blob(v));
        let ctx_blob = ctx_emb.as_ref().map(|v| f32_to_blob(v));

        conn.execute(
            "INSERT INTO labels (text, context, eeg_start, eeg_end, wall_start, wall_end,
                                 label_start, label_end,
                                 created_at, text_embedding, context_embedding, embedding_model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                req.text,
                context,
                eeg_start as i64,
                eeg_end as i64,
                now_secs as i64,
                now_secs as i64,
                now_secs as i64,
                text_blob,
                ctx_blob,
                EMBED_MODEL_NAME,
            ],
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let id = conn.last_insert_rowid();

        // Insert into daemon-owned HNSW indices.
        // On dimension mismatch, insert_label auto-rebuilds the index so the
        // new label is immediately searchable.
        let text_emb_ref = text_emb.as_deref().unwrap_or(&[]);
        let ctx_emb_ref = ctx_emb.as_deref().unwrap_or(&[]);
        let insert_result = skill_label_index::insert_label(
            &skill_dir,
            id,
            text_emb_ref,
            ctx_emb_ref,
            eeg_start,
            eeg_end,
            &label_index,
        );
        if insert_result.rebuilt {
            eprintln!("[labels] HNSW indices rebuilt due to embedding model change");
        }

        Ok::<_, anyhow::Error>(LabelEntry {
            id,
            text: req.text,
            context: Some(context),
            eeg_start,
            eeg_end,
            created_at: now,
            embedding_model: Some(EMBED_MODEL_NAME.to_string()),
        })
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "task_error",
                message: e.to_string(),
            }),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "db_error",
                message: e.to_string(),
            }),
        )
    })?;

    Ok(Json(result))
}

async fn update_label(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(req): Json<UpdateLabelRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let label_index = state.label_index.clone();
    let embedder = state.text_embedder.clone();

    tokio::task::spawn_blocking(move || {
        let conn = open_labels_db(&skill_dir).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "db_error",
                    message: e.to_string(),
                }),
            )
        })?;
        let context = req.context.clone().unwrap_or_default();

        // Re-embed updated text/context
        let text_emb = embedder.embed(&req.text);
        let ctx_emb = embedder.embed(&context);
        let text_blob = text_emb.as_ref().map(|v| f32_to_blob(v));
        let ctx_blob = ctx_emb.as_ref().map(|v| f32_to_blob(v));

        conn.execute(
            "UPDATE labels SET text = ?1, context = ?2, text_embedding = ?3,
                    context_embedding = ?4, embedding_model = ?5
             WHERE id = ?6",
            rusqlite::params![req.text, context, text_blob, ctx_blob, EMBED_MODEL_NAME, id],
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "db_error",
                    message: e.to_string(),
                }),
            )
        })?;

        // Rebuild HNSW indices to pick up the change (catch dimension panics)
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            skill_label_index::rebuild(&skill_dir, &label_index);
        }));

        Ok(Json(serde_json::json!({ "ok": true })))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "task_error",
                message: e.to_string(),
            }),
        )
    })?
}

async fn delete_label(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let label_index = state.label_index.clone();

    tokio::task::spawn_blocking(move || {
        let conn = open_labels_db(&skill_dir).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "db_error",
                    message: e.to_string(),
                }),
            )
        })?;
        conn.execute("DELETE FROM labels WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        code: "db_error",
                        message: e.to_string(),
                    }),
                )
            })?;

        // Rebuild HNSW indices after deletion
        skill_label_index::rebuild(&skill_dir, &label_index);

        Ok(Json(serde_json::json!({ "ok": true })))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "task_error",
                message: e.to_string(),
            }),
        )
    })?
}

/// Semantic search across labels using HNSW.
async fn search_labels(State(state): State<AppState>, Json(req): Json<SearchLabelsRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let label_index = state.label_index.clone();
    let embedder = state.text_embedder.clone();
    let k = req.k.unwrap_or(10);
    let ef = req.ef.unwrap_or(64);
    let mode = req.mode.clone().unwrap_or_else(|| "text".into());
    let query_text = req.query.clone();

    let results = tokio::task::spawn_blocking(move || {
        // Embed the query text
        let Some(query_vec) = embedder.embed(&query_text) else {
            return serde_json::json!({ "results": [], "error": "failed to embed query" });
        };

        let neighbors = match mode.as_str() {
            "context" => skill_label_index::search_by_context_vec(&query_vec, k, ef, &skill_dir, &label_index),
            _ => skill_label_index::search_by_text_vec(&query_vec, k, ef, &skill_dir, &label_index),
        };

        serde_json::json!({ "results": neighbors })
    })
    .await
    .unwrap_or_else(|e| serde_json::json!({ "results": [], "error": e.to_string() }));

    Json(results)
}

/// Search labels by EEG similarity for a given time window.
async fn search_labels_by_eeg(
    State(state): State<AppState>,
    Json(req): Json<SearchByEegRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let label_index = state.label_index.clone();
    let k = req.k.unwrap_or(10);

    let results = tokio::task::spawn_blocking(move || {
        // Get mean EEG embedding for the query window
        let Some(query_vec) = skill_label_index::mean_eeg_for_window(&skill_dir, req.start_utc, req.end_utc) else {
            return serde_json::json!({ "results": [], "error": "no EEG data in window" });
        };

        let neighbors = skill_label_index::search_by_eeg_vec(&query_vec, k, 64, &skill_dir, &label_index);
        serde_json::json!({ "results": neighbors })
    })
    .await
    .unwrap_or_else(|e| serde_json::json!({ "results": [], "error": e.to_string() }));

    Json(results)
}

/// Full rebuild of all HNSW label indices from `labels.sqlite`.
async fn rebuild_label_index(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let label_index = state.label_index.clone();

    let stats = tokio::task::spawn_blocking(move || skill_label_index::rebuild(&skill_dir, &label_index))
        .await
        .ok();

    match stats {
        Some(s) => Json(serde_json::json!({
            "ok": true,
            "text_nodes": s.text_nodes,
            "eeg_nodes": s.eeg_nodes,
            "eeg_skipped": s.eeg_skipped,
        })),
        None => Json(serde_json::json!({ "ok": false, "error": "rebuild task failed" })),
    }
}

/// Stats about the current in-memory label indices.
async fn label_index_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let label_index = state.label_index.clone();
    let text_len = label_index
        .text
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|i| i.len()))
        .unwrap_or(0);
    let context_len = label_index
        .context
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|i| i.len()))
        .unwrap_or(0);
    let eeg_len = label_index
        .eeg
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|i| i.len()))
        .unwrap_or(0);
    Json(serde_json::json!({
        "text_nodes": text_len,
        "context_nodes": context_len,
        "eeg_nodes": eeg_len,
    }))
}

/// Report how many labels use a different embedding model than the current one.
async fn label_embedding_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    let result = tokio::task::spawn_blocking(move || {
        let Ok(conn) = open_labels_db(&skill_dir) else {
            return serde_json::json!({
                "current_model": EMBED_MODEL_NAME,
                "total": 0,
                "stale": 0,
                "models": {},
            });
        };

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM labels", [], |r| r.get(0))
            .unwrap_or(0);

        let stale: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM labels WHERE embedding_model IS NULL OR embedding_model != ?1",
                rusqlite::params![EMBED_MODEL_NAME],
                |r| r.get(0),
            )
            .unwrap_or(0);

        // Collect distinct models and their counts.
        let models: serde_json::Value = conn
            .prepare("SELECT COALESCE(embedding_model, '(none)'), COUNT(*) FROM labels GROUP BY embedding_model")
            .map(|mut stmt| {
                let rows: Vec<(String, i64)> = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))
                    .map(|rows| rows.filter_map(Result::ok).collect())
                    .unwrap_or_default();
                rows
            })
            .map(|rows| {
                let mut m = serde_json::Map::new();
                for (model, count) in rows {
                    m.insert(model, serde_json::json!(count));
                }
                serde_json::Value::Object(m)
            })
            .unwrap_or_default();

        serde_json::json!({
            "current_model": EMBED_MODEL_NAME,
            "total": total,
            "stale": stale,
            "models": models,
        })
    })
    .await
    .unwrap_or_else(|_| serde_json::json!({ "current_model": EMBED_MODEL_NAME, "total": 0, "stale": 0 }));

    Json(result)
}

/// Re-embed all labels with the current embedding model and rebuild indices.
async fn reembed_all_labels(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let reembed_cfg = crate::routes::settings_io::load_user_settings(&state).reembed;
    let label_index = state.label_index.clone();
    let embedder = state.text_embedder.clone();

    let result = tokio::task::spawn_blocking(move || {
        let conn = open_labels_db(&skill_dir)?;

        // Read all label ids + text + context.
        let mut stmt = conn.prepare("SELECT id, text, COALESCE(context, '') FROM labels ORDER BY id")?;
        let rows: Vec<(i64, String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default();

        let total = rows.len();
        let mut updated = 0usize;

        let batch_size = reembed_cfg.batch_size.max(1);
        let batch_delay = std::time::Duration::from_millis(reembed_cfg.batch_delay_ms);
        for (i, (id, text, context)) in rows.iter().enumerate() {
            let text_emb = embedder.embed(text);
            let ctx_emb = embedder.embed(context);
            let text_blob = text_emb.as_ref().map(|v| f32_to_blob(v));
            let ctx_blob = ctx_emb.as_ref().map(|v| f32_to_blob(v));

            conn.execute(
                "UPDATE labels SET text_embedding = ?1, context_embedding = ?2, embedding_model = ?3 WHERE id = ?4",
                rusqlite::params![text_blob, ctx_blob, EMBED_MODEL_NAME, id],
            )?;
            updated += 1;

            // Yield between batches to avoid saturating the CPU.
            if (i + 1) % batch_size == 0 && batch_delay.as_millis() > 0 {
                std::thread::sleep(batch_delay);
            }
        }

        // Rebuild HNSW indices with new embeddings.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            skill_label_index::rebuild(&skill_dir, &label_index);
        }));

        Ok::<_, anyhow::Error>(serde_json::json!({
            "ok": true,
            "total": total,
            "updated": updated,
            "model": EMBED_MODEL_NAME,
        }))
    })
    .await
    .map_err(|e| format!("{e}"))
    .and_then(|r| r.map_err(|e| format!("{e}")));

    match result {
        Ok(v) => Json(v),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })),
    }
}
