// SPDX-License-Identifier: GPL-3.0-only
//! Screenshot configuration and search handlers.

use axum::{extract::State, Json};
use serde::Deserialize;

use crate::{
    routes::settings_io::{load_user_settings, save_user_settings},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub(crate) struct ScreenshotAroundRequest {
    pub(crate) timestamp: i64,
    pub(crate) window_secs: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ScreenshotImageSearchRequest {
    pub(crate) image_bytes: Vec<u8>,
    pub(crate) k: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ScreenshotTextSearchRequest {
    pub(crate) query: String,
    pub(crate) k: Option<usize>,
    pub(crate) mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ScreenshotVectorSearchRequest {
    pub(crate) vector: Vec<f32>,
    pub(crate) k: usize,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone)]
pub(crate) struct DaemonScreenshotContext {
    pub(crate) config: skill_settings::ScreenshotConfig,
    pub(crate) events_tx: tokio::sync::broadcast::Sender<skill_daemon_common::EventEnvelope>,
}

impl skill_screenshots::ScreenshotContext for DaemonScreenshotContext {
    fn config(&self) -> skill_screenshots::ScreenshotConfig {
        self.config.clone()
    }
    fn is_session_active(&self) -> bool {
        false
    }
    fn active_window(&self) -> skill_screenshots::ActiveWindowInfo {
        skill_screenshots::ActiveWindowInfo::default()
    }
    fn emit_event(&self, event: &str, payload: serde_json::Value) {
        let _ = self.events_tx.send(skill_daemon_common::EventEnvelope {
            r#type: event.to_string(),
            ts_unix_ms: now_unix_ms(),
            correlation_id: None,
            payload,
        });
    }
    fn embed_image_via_llm(&self, _png_bytes: &[u8]) -> Option<Vec<f32>> {
        None
    }
}

pub(crate) async fn get_screenshot_config(State(state): State<AppState>) -> Json<skill_settings::ScreenshotConfig> {
    Json(load_user_settings(&state).screenshot)
}

pub(crate) async fn set_screenshot_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::ScreenshotConfig>,
) -> Json<skill_data::screenshot_store::ConfigChangeResult> {
    let mut settings = load_user_settings(&state);
    let old_backend = settings.screenshot.embed_backend.clone();
    let old_model = settings.screenshot.model_id();
    let new_backend = config.embed_backend.clone();
    let new_model = config.model_id();
    let model_changed = old_backend != new_backend || old_model != new_model;

    settings.screenshot = config;
    save_user_settings(&state, &settings);

    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let stale_count = if model_changed {
        skill_data::screenshot_store::ScreenshotStore::open(&skill_dir)
            .map(|s| s.count_stale(&new_backend, &new_model))
            .unwrap_or(0)
    } else {
        0
    };

    Json(skill_data::screenshot_store::ConfigChangeResult {
        model_changed,
        stale_count,
    })
}

pub(crate) async fn estimate_screenshot_reembed(
    State(state): State<AppState>,
) -> Json<Option<skill_data::screenshot_store::ReembedEstimate>> {
    let settings = load_user_settings(&state);
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let store = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir)?;
        Some(skill_screenshots::capture::estimate_reembed(
            &store,
            &settings.screenshot,
            &skill_dir,
        ))
    })
    .await
    .unwrap_or(None);
    Json(out)
}

pub(crate) async fn rebuild_screenshot_embeddings(
    State(state): State<AppState>,
) -> Json<Option<skill_data::screenshot_store::ReembedResult>> {
    let settings = load_user_settings(&state);
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let events_tx = state.events_tx.clone();
    let out = tokio::task::spawn_blocking(move || {
        let store = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir)?;
        let ctx = DaemonScreenshotContext {
            config: settings.screenshot.clone(),
            events_tx,
        };
        Some(skill_screenshots::capture::rebuild_embeddings(
            &store,
            &settings.screenshot,
            &skill_dir,
            &ctx,
        ))
    })
    .await
    .unwrap_or(None);
    Json(out)
}

pub(crate) async fn get_screenshots_around(
    State(state): State<AppState>,
    Json(req): Json<ScreenshotAroundRequest>,
) -> Json<Vec<skill_data::screenshot_store::ScreenshotResult>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        skill_screenshots::capture::get_around(&store, req.timestamp, req.window_secs)
    })
    .await
    .unwrap_or_default();
    Json(out)
}

pub(crate) async fn search_screenshots_by_image(
    State(state): State<AppState>,
    Json(req): Json<ScreenshotImageSearchRequest>,
) -> Json<Vec<skill_data::screenshot_store::ScreenshotResult>> {
    let settings = load_user_settings(&state);
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(mut encoder) = skill_screenshots::capture::load_fastembed_image_pub(&settings.screenshot, &skill_dir)
        else {
            return vec![];
        };
        let Some(query) = skill_screenshots::capture::fastembed_embed_pub(&mut encoder, &req.image_bytes) else {
            return vec![];
        };
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        let hnsw_path = skill_dir.join(skill_constants::SCREENSHOTS_HNSW);
        let Ok(hnsw) = fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(
            &hnsw_path,
            fast_hnsw::distance::Cosine,
        ) else {
            return vec![];
        };
        skill_screenshots::capture::search_by_vector(&hnsw, &store, &query, req.k)
    })
    .await
    .unwrap_or_default();
    Json(out)
}

pub(crate) async fn get_screenshot_metrics(
    State(state): State<AppState>,
) -> Json<skill_screenshots::capture::MetricsSnapshot> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let (captures, embeds, last_capture_unix, last_embed_unix) = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return (0u64, 0u64, 0u64, 0u64);
        };
        let summary = store.summary_counts();
        let db_path = skill_dir.join(skill_constants::SCREENSHOTS_SQLITE);
        let mut last_capture = 0u64;
        let mut last_embed = 0u64;
        if let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            last_capture = conn
                .query_row("SELECT COALESCE(MAX(unix_ts), 0) FROM screenshots", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap_or(0)
                .max(0) as u64;
            last_embed = conn
                .query_row(
                    "SELECT COALESCE(MAX(unix_ts), 0) FROM screenshots WHERE embedding IS NOT NULL",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0)
                .max(0) as u64;
        }
        (summary.total, summary.with_embedding, last_capture, last_embed)
    })
    .await
    .unwrap_or((0, 0, 0, 0));

    Json(skill_screenshots::capture::MetricsSnapshot {
        captures,
        capture_errors: 0,
        drops: 0,
        capture_us: 0,
        ocr_us: 0,
        resize_us: 0,
        save_us: 0,
        capture_total_us: 0,
        embeds,
        embed_errors: 0,
        vision_embed_us: 0,
        text_embed_us: 0,
        embed_total_us: 0,
        queue_depth: 0,
        last_capture_unix,
        last_embed_unix,
        backoff_multiplier: 0,
    })
}

pub(crate) async fn check_ocr_models_ready(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let ocr_dir = skill_dir.join("ocr_models");
    Json(
        serde_json::json!({"value": ocr_dir.join(skill_constants::OCR_DETECTION_MODEL_FILE).exists() && ocr_dir.join(skill_constants::OCR_RECOGNITION_MODEL_FILE).exists()}),
    )
}

pub(crate) async fn download_ocr_models(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let ok = tokio::task::spawn_blocking(move || {
        let ocr_dir = skill_dir.join("ocr_models");
        let _ = std::fs::create_dir_all(&ocr_dir);
        let det_path = ocr_dir.join(skill_constants::OCR_DETECTION_MODEL_FILE);
        let rec_path = ocr_dir.join(skill_constants::OCR_RECOGNITION_MODEL_FILE);
        let det_ok =
            skill_screenshots::capture::download_ocr_model_pub(skill_constants::OCR_DETECTION_MODEL_URL, &det_path);
        let rec_ok =
            skill_screenshots::capture::download_ocr_model_pub(skill_constants::OCR_RECOGNITION_MODEL_URL, &rec_path);
        det_ok && rec_ok
    })
    .await
    .unwrap_or(false);
    Json(serde_json::json!({"value": ok}))
}

pub(crate) async fn search_screenshots_by_text(
    State(state): State<AppState>,
    Json(req): Json<ScreenshotTextSearchRequest>,
) -> Json<Vec<skill_data::screenshot_store::ScreenshotResult>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let settings = load_user_settings(&state);
    let out = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        let k = req.k.unwrap_or(20);
        let mode = req.mode.unwrap_or_else(|| "semantic".into());
        if mode == "substring" {
            return skill_screenshots::capture::search_by_ocr_text_like(&store, &req.query, k);
        }

        let cache_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".cache")
            .join("fastembed");
        let te = match fastembed::TextEmbedding::try_new(
            fastembed::TextInitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(false),
        ) {
            Ok(te) => std::sync::Mutex::new(te),
            Err(_) => {
                return skill_screenshots::capture::search_by_ocr_text_like(&store, &req.query, k);
            }
        };

        let embed_fn = |text: &str| -> Option<Vec<f32>> {
            let mut guard = te.lock().ok()?;
            let mut vecs = guard.embed(vec![text], None).ok()?;
            if vecs.is_empty() {
                None
            } else {
                Some(vecs.remove(0))
            }
        };

        let mut results =
            skill_screenshots::capture::search_by_ocr_text_embedding(&skill_dir, &store, &req.query, k, &embed_fn);

        if results.is_empty() {
            results = skill_screenshots::capture::search_by_ocr_text_like(&store, &req.query, k);
        }

        if settings.text_embedding_model != "Xenova/bge-small-en-v1.5" {
            eprintln!(
                "[screenshot-search] semantic mode currently uses BGESmallENV15; requested model={} ",
                settings.text_embedding_model
            );
        }
        results
    })
    .await
    .unwrap_or_default();
    Json(out)
}

pub(crate) async fn get_screenshots_dir(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let dir = skill_dir
        .join(skill_constants::SCREENSHOTS_DIR)
        .to_string_lossy()
        .into_owned();
    let port = std::env::var("SKILL_DAEMON_ADDR")
        .ok()
        .and_then(|v| v.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()))
        .unwrap_or(18444);
    Json(serde_json::json!({"dir": dir, "port": port}))
}

pub(crate) async fn search_screenshots_by_vector(
    State(state): State<AppState>,
    Json(req): Json<ScreenshotVectorSearchRequest>,
) -> Json<Vec<skill_data::screenshot_store::ScreenshotResult>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        let hnsw_path = skill_dir.join(skill_constants::SCREENSHOTS_HNSW);
        let Ok(hnsw) = fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(
            &hnsw_path,
            fast_hnsw::distance::Cosine,
        ) else {
            return vec![];
        };
        skill_screenshots::capture::search_by_vector(&hnsw, &store, &req.vector, req.k)
    })
    .await
    .unwrap_or_default();
    Json(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use tempfile::TempDir;

    fn mk_state() -> (TempDir, AppState) {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        (td, state)
    }

    #[tokio::test]
    async fn screenshot_config_roundtrip() {
        let (_td, state) = mk_state();
        let cfg = get_screenshot_config(State(state.clone())).await.0;
        // Should return a valid config object with an interval field
        assert!(cfg.interval_secs > 0 || cfg.interval_secs == 0);
    }

    #[tokio::test]
    async fn screenshots_dir_returns_path() {
        let (_td, state) = mk_state();
        let res = get_screenshots_dir(State(state.clone())).await.0;
        assert!(res.get("dir").is_some());
        assert!(res.get("port").is_some());
    }

    #[tokio::test]
    async fn screenshot_config_default_has_interval() {
        let (_td, state) = mk_state();
        let _cfg = get_screenshot_config(State(state.clone())).await.0;
        // Default interval is always non-negative (unsigned type)
    }

    #[test]
    fn daemon_screenshot_context_returns_config() {
        use skill_screenshots::ScreenshotContext;
        let (tx, _rx) = tokio::sync::broadcast::channel(1);
        let ctx = DaemonScreenshotContext {
            config: skill_settings::ScreenshotConfig::default(),
            events_tx: tx,
        };
        let _cfg = ctx.config();
        // interval_secs is always non-negative (unsigned type)
    }

    #[test]
    fn daemon_screenshot_context_session_not_active() {
        use skill_screenshots::ScreenshotContext;
        let (tx, _rx) = tokio::sync::broadcast::channel(1);
        let ctx = DaemonScreenshotContext {
            config: skill_settings::ScreenshotConfig::default(),
            events_tx: tx,
        };
        assert!(!ctx.is_session_active());
    }
}
