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
pub struct CreateLabelRequest {
    pub text: String,
    pub context: Option<String>,
    #[serde(rename = "labelStartUtc", alias = "label_start_utc")]
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
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_unix() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Open (or create) the canonical `labels.sqlite` with the full schema.
fn open_labels_db(skill_dir: &std::path::Path) -> Result<rusqlite::Connection, String> {
    let db_path = skill_dir.join(LABELS_FILE);
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
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
    .map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Embed text using fastembed (bge-small-en-v1.5) and return f32 vec.
fn embed_text(text: &str) -> Option<Vec<f32>> {
    if text.trim().is_empty() {
        return None;
    }
    let cache_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".cache")
        .join("fastembed");
    let mut model = fastembed::TextEmbedding::try_new(
        fastembed::TextInitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(false),
    )
    .ok()?;
    let results = model.embed(vec![text], None).ok()?;
    results.into_iter().next()
}

fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

const EMBED_MODEL_NAME: &str = "bge-small-en-v1.5";

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

    let result = tokio::task::spawn_blocking(move || {
        let conn = open_labels_db(&skill_dir)?;
        let now = req.label_start_utc.unwrap_or_else(now_unix);
        let now_secs = now as u64;
        let eeg_start = req.eeg_start.unwrap_or(now_secs);
        let eeg_end = req.eeg_end.unwrap_or(now_secs);
        let context = req.context.clone().unwrap_or_default();

        // Embed text and context
        let text_emb = embed_text(&req.text);
        let ctx_emb = embed_text(&context);
        let text_blob = text_emb.as_ref().map(|v| f32_to_blob(v));
        let ctx_blob = ctx_emb.as_ref().map(|v| f32_to_blob(v));

        conn.execute(
            "INSERT INTO labels (text, context, eeg_start, eeg_end, wall_start, wall_end,
                                 created_at, text_embedding, context_embedding, embedding_model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
        .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();

        // Insert into daemon-owned HNSW indices
        let text_emb_ref = text_emb.as_deref().unwrap_or(&[]);
        let ctx_emb_ref = ctx_emb.as_deref().unwrap_or(&[]);
        skill_label_index::insert_label(
            &skill_dir,
            id,
            text_emb_ref,
            ctx_emb_ref,
            eeg_start,
            eeg_end,
            &label_index,
        );

        Ok::<_, String>(LabelEntry {
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
                message: e,
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

    tokio::task::spawn_blocking(move || {
        let conn = open_labels_db(&skill_dir).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "db_error",
                    message: e,
                }),
            )
        })?;
        let context = req.context.clone().unwrap_or_default();

        // Re-embed updated text/context
        let text_emb = embed_text(&req.text);
        let ctx_emb = embed_text(&context);
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

        // Rebuild HNSW indices to pick up the change
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
                    message: e,
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
    let k = req.k.unwrap_or(10);
    let ef = req.ef.unwrap_or(64);
    let mode = req.mode.clone().unwrap_or_else(|| "text".into());
    let query_text = req.query.clone();

    let results = tokio::task::spawn_blocking(move || {
        // Embed the query text
        let Some(query_vec) = embed_text(&query_text) else {
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
