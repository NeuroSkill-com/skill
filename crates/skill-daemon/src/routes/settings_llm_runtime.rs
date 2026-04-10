// SPDX-License-Identifier: GPL-3.0-only
//! Runtime/server/catalog/download LLM handlers extracted from settings.

use axum::{extract::State, Json};

use crate::{
    routes::{
        settings::{BoolValueRequest, FilenameRequest, LlmAddModelRequest, LlmFilenameRequest},
        settings_io::{load_user_settings, save_user_settings},
    },
    state::AppState,
};

#[cfg(feature = "llm")]
#[derive(Clone)]
struct DaemonLlmEmitter {
    events_tx: tokio::sync::broadcast::Sender<skill_daemon_common::EventEnvelope>,
}

#[cfg(feature = "llm")]
impl skill_llm::LlmEventEmitter for DaemonLlmEmitter {
    fn emit_event(&self, event: &str, payload: serde_json::Value) {
        let ts_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = self.events_tx.send(skill_daemon_common::EventEnvelope {
            r#type: format!("Llm{}", event.replace(':', "_")),
            ts_unix_ms,
            correlation_id: None,
            payload,
        });
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn emit_daemon_event(state: &AppState, event_type: &str, payload: serde_json::Value) {
    let ts_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let _ = state.events_tx.send(skill_daemon_common::EventEnvelope {
        r#type: event_type.to_string(),
        ts_unix_ms,
        correlation_id: None,
        payload,
    });
}

fn persist_llm_catalog(state: &AppState) {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    if let Ok(cat) = state.llm_catalog.lock() {
        cat.save(&skill_dir);
    }
}

fn infer_quant(filename: &str) -> String {
    let upper = filename.to_uppercase();
    for q in [
        "IQ4_NL", "IQ4_XS", "IQ3_XXS", "IQ3_XS", "IQ3_M", "IQ3_S", "IQ2_XXS", "IQ2_XS", "IQ2_M", "IQ2_S", "Q6_K_L",
        "Q6_K", "Q5_K_L", "Q5_K_M", "Q5_K_S", "Q4_K_L", "Q4_K_M", "Q4_K_S", "Q4_0", "Q4_1", "Q3_K_XL", "Q3_K_L",
        "Q3_K_M", "Q3_K_S", "Q2_K_L", "Q2_K", "Q8_0", "Q8_1", "BF16", "F16", "F32",
    ] {
        if upper.contains(q) {
            return q.to_string();
        }
    }
    "unknown".to_string()
}

fn set_download_state(
    state: &AppState,
    filename: &str,
    new_state: skill_llm::catalog::DownloadState,
    msg: Option<String>,
) {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
            e.state = new_state.clone();
            e.status_msg = msg.clone();
            e.initiated_at_unix = Some(now_unix());
            if matches!(e.state, skill_llm::catalog::DownloadState::Downloaded) {
                e.progress = 1.0;
            }
        }
    }
    persist_llm_catalog(state);
    emit_daemon_event(
        state,
        "LlmDownloadUpdated",
        serde_json::json!({
            "filename": filename,
            "state": new_state,
            "status_msg": msg
        }),
    );
}

fn set_live_download_cancel_flags(state: &AppState, filename: &str, cancelled: bool, pause_requested: bool) {
    let progress_opt = state.llm_downloads.lock().ok().and_then(|m| m.get(filename).cloned());
    if let Some(progress) = progress_opt {
        if let Ok(mut p) = progress.lock() {
            p.cancelled = cancelled;
            p.pause_requested = pause_requested;
        }
    }
}

fn spawn_model_download(state: AppState, filename: String) {
    tokio::spawn(async move {
        let entry_opt = state
            .llm_catalog
            .lock()
            .ok()
            .and_then(|cat| cat.entries.iter().find(|e| e.filename == filename).cloned());
        let Some(entry) = entry_opt else {
            return;
        };

        let progress = std::sync::Arc::new(std::sync::Mutex::new(skill_llm::catalog::DownloadProgress {
            filename: entry.filename.clone(),
            state: skill_llm::catalog::DownloadState::Downloading,
            ..Default::default()
        }));

        if let Ok(mut m) = state.llm_downloads.lock() {
            m.insert(filename.clone(), progress.clone());
        }

        let progress_for_job = progress.clone();
        let entry_for_job = entry.clone();
        let mut job =
            tokio::task::spawn_blocking(move || skill_llm::catalog::download_model(&entry_for_job, &progress_for_job));

        loop {
            if job.is_finished() {
                break;
            }
            if let Ok(p) = progress.lock() {
                if let Ok(mut cat) = state.llm_catalog.lock() {
                    if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
                        e.state = p.state.clone();
                        e.progress = p.progress;
                        e.status_msg = p.status_msg.clone();
                        e.initiated_at_unix = Some(now_unix());
                    }
                }
                emit_daemon_event(
                    &state,
                    "LlmDownloadProgress",
                    serde_json::json!({
                        "filename": filename.clone(),
                        "state": p.state.clone(),
                        "progress": p.progress,
                        "status_msg": p.status_msg.clone(),
                        "current_shard": p.current_shard,
                        "shard_count": p.total_shards
                    }),
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        let res = (&mut job).await;

        if let Ok(mut m) = state.llm_downloads.lock() {
            m.remove(&filename);
        }

        match res {
            Ok(Ok(path)) => {
                if let Ok(mut cat) = state.llm_catalog.lock() {
                    if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
                        e.state = skill_llm::catalog::DownloadState::Downloaded;
                        e.progress = 1.0;
                        e.local_path = Some(path);
                        e.status_msg = Some("Downloaded".to_string());
                    }
                }
                persist_llm_catalog(&state);
                emit_daemon_event(
                    &state,
                    "LlmDownloadCompleted",
                    serde_json::json!({"filename": filename.clone()}),
                );
            }
            Ok(Err(err)) => {
                let msg = err.to_string();
                let st = if msg.contains("paused") {
                    skill_llm::catalog::DownloadState::Paused
                } else if msg.contains("cancelled") {
                    skill_llm::catalog::DownloadState::Cancelled
                } else {
                    skill_llm::catalog::DownloadState::Failed
                };
                set_download_state(&state, &filename, st, Some(msg));
            }
            Err(err) => {
                set_download_state(
                    &state,
                    &filename,
                    skill_llm::catalog::DownloadState::Failed,
                    Some(err.to_string()),
                );
            }
        }
    });
}

pub(crate) async fn llm_server_start_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        let mut cfg = state.llm_config.lock().map(|g| g.clone()).unwrap_or_default();
        let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        let cell = state.llm_state_cell.clone();
        let log_buf = state.llm_log_buffer.clone();

        // UX: pressing “Start” should start the server even if the user left
        // the global enable toggle off. Persist this so subsequent starts work.
        if !cfg.enabled {
            cfg.enabled = true;
            if let Ok(mut g) = state.llm_config.lock() {
                *g = cfg.clone();
            }
            let mut settings = load_user_settings(&state);
            settings.llm = cfg.clone();
            save_user_settings(&state, &settings);
        }

        if cell.lock().ok().and_then(|g| g.clone()).is_some() {
            return Json(serde_json::json!({"ok": true, "result": "already_running"}));
        }

        if cat.active_model_path().or_else(|| cfg.model_path.clone()).is_none() {
            return Json(serde_json::json!({
                "ok": false,
                "result": "failed",
                "error": "no model selected (choose a downloaded model in Settings → LLM)",
            }));
        }

        let emitter: std::sync::Arc<dyn skill_llm::LlmEventEmitter> = std::sync::Arc::new(DaemonLlmEmitter {
            events_tx: state.events_tx.clone(),
        });
        match tokio::task::spawn_blocking(move || skill_llm::init(&cfg, &cat, emitter, log_buf, &skill_dir)).await {
            Ok(Some(srv)) => {
                let model_name = srv.model_name.clone();
                if let Ok(mut g) = cell.lock() {
                    *g = Some(srv);
                }
                if let Ok(mut st) = state.llm_status.lock() {
                    *st = "running".to_string();
                }
                if let Ok(mut m) = state.llm_model_name.lock() {
                    *m = model_name;
                }
                return Json(serde_json::json!({"ok": true, "result": "starting"}));
            }
            Ok(None) => {
                return Json(serde_json::json!({"ok": false, "result": "failed", "error": "init returned none"}));
            }
            Err(e) => {
                return Json(serde_json::json!({"ok": false, "result": "failed", "error": e.to_string()}));
            }
        }
    }

    #[cfg(not(feature = "llm"))]
    {
        if let Ok(mut st) = state.llm_status.lock() {
            *st = "running".to_string();
        }
        Json(serde_json::json!({"ok": true, "result": "starting"}))
    }
}

pub(crate) async fn llm_server_stop_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        skill_llm::shutdown_cell(&state.llm_state_cell);

        // Persist enabled = false so the server stays stopped across daemon
        // restarts.  Mirrors the `enabled = true` write in llm_server_start_impl.
        if let Ok(mut g) = state.llm_config.lock() {
            g.enabled = false;
        }
        let mut settings = load_user_settings(&state);
        settings.llm.enabled = false;
        save_user_settings(&state, &settings);
    }
    if let Ok(mut st) = state.llm_status.lock() {
        *st = "stopped".to_string();
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_server_status_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        use std::sync::atomic::Ordering;
        let (status, model_name) = skill_llm::cell_status(&state.llm_state_cell);
        let (n_ctx, supports_vision, supports_tools) = state
            .llm_state_cell
            .lock()
            .ok()
            .and_then(|g| {
                g.as_ref().map(|srv| {
                    (
                        srv.n_ctx.load(Ordering::Relaxed),
                        srv.vision_ready.load(Ordering::Relaxed),
                        srv.is_ready(),
                    )
                })
            })
            .unwrap_or((0, false, false));

        return Json(serde_json::json!({
            "status": serde_json::to_value(status).unwrap_or(serde_json::json!("stopped")),
            "model_name": model_name,
            "n_ctx": n_ctx,
            "supports_vision": supports_vision,
            "supports_tools": supports_tools,
            "start_error": serde_json::Value::Null
        }));
    }

    #[cfg(not(feature = "llm"))]
    {
        let status = state
            .llm_status
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "stopped".into());
        let model_name = state.llm_model_name.lock().map(|g| g.clone()).unwrap_or_default();
        let supports_vision = state.llm_mmproj_name.lock().map(|g| g.is_some()).unwrap_or(false);
        let supports_tools = status == "running";
        Json(serde_json::json!({
            "status": status,
            "model_name": model_name,
            "n_ctx": if supports_tools { 8192 } else { 0 },
            "supports_vision": supports_vision,
            "supports_tools": supports_tools,
            "start_error": serde_json::Value::Null
        }))
    }
}

pub(crate) async fn llm_server_logs_impl(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    #[cfg(feature = "llm")]
    {
        let logs = state
            .llm_log_buffer
            .lock()
            .map(|q| q.iter().filter_map(|e| serde_json::to_value(e).ok()).collect())
            .unwrap_or_default();
        return Json(logs);
    }

    #[cfg(not(feature = "llm"))]
    {
        Json(state.llm_logs.lock().map(|g| g.clone()).unwrap_or_default())
    }
}

pub(crate) async fn llm_server_switch_model_impl(
    State(state): State<AppState>,
    Json(req): Json<FilenameRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_model = req.filename.clone();
        if !cat.active_mmproj_matches_active_model() {
            cat.active_mmproj.clear();
        }
    }
    persist_llm_catalog(&state);

    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
            cfg.model_path = cat.active_model_path();
            cfg.mmproj = if cfg.autoload_mmproj {
                cat.active_mmproj_path()
            } else {
                None
            };
        }
        skill_llm::shutdown_cell(&state.llm_state_cell);
        let _ = llm_server_start_impl(State(state.clone())).await;
    }

    Json(serde_json::json!({"ok": true, "result": "switching"}))
}

pub(crate) async fn llm_server_switch_mmproj_impl(
    State(state): State<AppState>,
    Json(req): Json<FilenameRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_mmproj = req.filename.clone();
    }
    persist_llm_catalog(&state);

    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
            cfg.mmproj = if cfg.autoload_mmproj {
                cat.active_mmproj_path()
            } else {
                None
            };
        }
        skill_llm::shutdown_cell(&state.llm_state_cell);
        let _ = llm_server_start_impl(State(state.clone())).await;
    }

    Json(serde_json::json!({"ok": true, "result": "switching"}))
}

pub(crate) async fn llm_get_catalog_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    Json(serde_json::to_value(cat).unwrap_or_default())
}

pub(crate) async fn llm_refresh_catalog_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.refresh_cache();
        cat.auto_select();
    }
    persist_llm_catalog(&state);
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_add_model_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmAddModelRequest>,
) -> Json<serde_json::Value> {
    let should_download = req.download.unwrap_or(false);
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if !cat.entries.iter().any(|e| e.filename == req.filename) {
            let entry = skill_llm::catalog::LlmModelEntry {
                repo: req.repo.clone(),
                filename: req.filename.clone(),
                quant: infer_quant(&req.filename),
                size_gb: req.size_gb.unwrap_or(0.0),
                description: "External model".to_string(),
                family_id: req
                    .repo
                    .split('/')
                    .next_back()
                    .unwrap_or("external")
                    .to_lowercase()
                    .replace(' ', "-"),
                family_name: req
                    .repo
                    .split('/')
                    .next_back()
                    .unwrap_or("External")
                    .replace(['_', '-'], " "),
                family_desc: String::new(),
                tags: vec!["external".to_string()],
                is_mmproj: req.mmproj.as_ref().map(|m| m == &req.filename).unwrap_or(false)
                    || req.filename.to_ascii_lowercase().contains("mmproj"),
                recommended: false,
                advanced: false,
                params_b: 0.0,
                max_context_length: 0,
                shard_files: Vec::new(),
                local_path: None,
                state: if should_download {
                    skill_llm::catalog::DownloadState::Downloading
                } else {
                    skill_llm::catalog::DownloadState::NotDownloaded
                },
                status_msg: if should_download {
                    Some("Queued in daemon".to_string())
                } else {
                    None
                },
                progress: 0.0,
                initiated_at_unix: Some(now_unix()),
            };
            cat.entries.push(entry);
        }
        cat.auto_select();
    }
    persist_llm_catalog(&state);
    Json(serde_json::json!({"ok": true, "filename": req.filename}))
}

pub(crate) async fn llm_get_downloads_impl(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    let downloads = state.llm_downloads.lock().map(|g| g.clone()).unwrap_or_default();
    let items = cat
        .entries
        .into_iter()
        .filter(|e| {
            use skill_llm::catalog::DownloadState;
            matches!(
                e.state,
                DownloadState::Downloading
                    | DownloadState::Paused
                    | DownloadState::Failed
                    | DownloadState::Cancelled
                    | DownloadState::Downloaded
            )
        })
        .map(|e| {
            let live = downloads
                .get(&e.filename)
                .and_then(|p| p.lock().ok().map(|g| g.clone()));
            serde_json::json!({
                "repo": e.repo,
                "filename": e.filename,
                "quant": e.quant,
                "size_gb": e.size_gb,
                "description": e.description,
                "is_mmproj": e.is_mmproj,
                "state": live.as_ref().map(|p| p.state.clone()).unwrap_or(e.state.clone()),
                "status_msg": live.as_ref().and_then(|p| p.status_msg.clone()).or(e.status_msg.clone()),
                "progress": live.as_ref().map(|p| p.progress).unwrap_or(e.progress),
                "initiated_at_unix": e.initiated_at_unix,
                "local_path": e.local_path,
                "shard_count": e.shard_count(),
                "current_shard": live.as_ref().map(|p| p.current_shard).unwrap_or(0)
            })
        })
        .collect();
    Json(items)
}

pub(crate) async fn llm_download_start_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    let is_active = state
        .llm_downloads
        .lock()
        .ok()
        .map(|m| m.contains_key(&req.filename))
        .unwrap_or(false);
    if is_active {
        return Json(serde_json::json!({"ok": true, "result": "already_downloading"}));
    }

    set_download_state(
        &state,
        &req.filename,
        skill_llm::catalog::DownloadState::Downloading,
        Some("Queued in daemon".into()),
    );
    spawn_model_download(state.clone(), req.filename.clone());
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_download_cancel_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    set_live_download_cancel_flags(&state, &req.filename, true, false);
    set_download_state(
        &state,
        &req.filename,
        skill_llm::catalog::DownloadState::Cancelled,
        Some("Cancelling".into()),
    );
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_download_pause_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    set_live_download_cancel_flags(&state, &req.filename, true, true);
    set_download_state(
        &state,
        &req.filename,
        skill_llm::catalog::DownloadState::Paused,
        Some("Pausing".into()),
    );
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_download_resume_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    let is_active = state
        .llm_downloads
        .lock()
        .ok()
        .map(|m| m.contains_key(&req.filename))
        .unwrap_or(false);
    if !is_active {
        set_download_state(
            &state,
            &req.filename,
            skill_llm::catalog::DownloadState::Downloading,
            Some("Resumed".into()),
        );
        spawn_model_download(state.clone(), req.filename.clone());
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_download_delete_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    set_live_download_cancel_flags(&state, &req.filename, true, false);
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == req.filename) {
            e.state = skill_llm::catalog::DownloadState::NotDownloaded;
            e.status_msg = None;
            e.progress = 0.0;
            e.local_path = None;
        }
    }
    if let Ok(mut m) = state.llm_downloads.lock() {
        m.remove(&req.filename);
    }
    persist_llm_catalog(&state);
    emit_daemon_event(
        &state,
        "LlmDownloadDeleted",
        serde_json::json!({"filename": req.filename}),
    );
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_set_active_model_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_model = req.filename;
        if !cat.active_mmproj_matches_active_model() {
            cat.active_mmproj.clear();
        }
    }
    persist_llm_catalog(&state);
    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
            cfg.model_path = cat.active_model_path();
        }
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_set_active_mmproj_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_mmproj = req.filename;
    }
    persist_llm_catalog(&state);
    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
            cfg.mmproj = if cfg.autoload_mmproj {
                cat.active_mmproj_path()
            } else {
                None
            };
        }
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_set_autoload_mmproj_impl(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.llm.autoload_mmproj = req.value;
    save_user_settings(&state, &settings);
    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            cfg.autoload_mmproj = req.value;
            if !req.value {
                cfg.mmproj = None;
            }
        }
    }
    Json(serde_json::json!({"ok": true, "value": req.value}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::settings::{BoolValueRequest, LlmAddModelRequest, LlmFilenameRequest};

    fn mk_state() -> (tempfile::TempDir, AppState) {
        let td = tempfile::tempdir().unwrap();
        let st = AppState::new("t".into(), td.path().to_path_buf());
        (td, st)
    }

    #[test]
    fn infer_quant_detects_known_and_unknown_patterns() {
        assert_eq!(infer_quant("model-Q5_K_M.gguf"), "Q5_K_M");
        assert_eq!(infer_quant("vision-mmproj-f16.gguf"), "F16");
        assert_eq!(infer_quant("mystery-model.bin"), "unknown");
    }

    #[tokio::test]
    async fn llm_server_status_and_logs_paths_are_stable() {
        let (_td, st) = mk_state();
        if let Ok(mut s) = st.llm_status.lock() {
            *s = "running".into();
        }
        if let Ok(mut m) = st.llm_model_name.lock() {
            *m = "model.gguf".into();
        }
        let Json(status) = llm_server_status_impl(State(st.clone())).await;
        assert!(status.get("status").is_some());
        assert!(status.get("model_name").is_some());
        assert!(status.get("n_ctx").is_some());

        let Json(logs) = llm_server_logs_impl(State(st)).await;
        let _ = logs.len();
    }

    #[tokio::test]
    async fn llm_add_model_is_idempotent_for_same_filename() {
        let (_td, st) = mk_state();
        let _ = llm_add_model_impl(
            State(st.clone()),
            Json(LlmAddModelRequest {
                repo: "a/b".into(),
                filename: "model-q4.gguf".into(),
                size_gb: Some(1.2),
                mmproj: None,
                download: Some(false),
            }),
        )
        .await;
        let _ = llm_add_model_impl(
            State(st.clone()),
            Json(LlmAddModelRequest {
                repo: "a/b".into(),
                filename: "model-q4.gguf".into(),
                size_gb: Some(1.2),
                mmproj: None,
                download: Some(false),
            }),
        )
        .await;
        let cat = st.llm_catalog.lock().unwrap().clone();
        let n = cat.entries.iter().filter(|e| e.filename == "model-q4.gguf").count();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn set_download_state_marks_downloaded_progress() {
        let (_td, st) = mk_state();
        if let Ok(mut cat) = st.llm_catalog.lock() {
            cat.entries.push(skill_llm::catalog::LlmModelEntry {
                repo: "a/b".into(),
                filename: "model.gguf".into(),
                quant: "Q4".into(),
                size_gb: 1.0,
                description: String::new(),
                family_id: "f".into(),
                family_name: "F".into(),
                family_desc: String::new(),
                tags: vec![],
                is_mmproj: false,
                recommended: false,
                advanced: false,
                params_b: 1.0,
                max_context_length: 2048,
                shard_files: vec![],
                local_path: None,
                state: skill_llm::catalog::DownloadState::NotDownloaded,
                status_msg: None,
                progress: 0.0,
                initiated_at_unix: None,
            });
        }

        set_download_state(
            &st,
            "model.gguf",
            skill_llm::catalog::DownloadState::Downloaded,
            Some("done".into()),
        );
        let cat = st.llm_catalog.lock().unwrap().clone();
        let e = cat.entries.iter().find(|e| e.filename == "model.gguf").unwrap();
        assert!(matches!(e.state, skill_llm::catalog::DownloadState::Downloaded));
        assert_eq!(e.progress, 1.0);
        assert_eq!(e.status_msg.as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn llm_set_autoload_mmproj_persists_setting() {
        let (_td, st) = mk_state();
        let Json(v) = llm_set_autoload_mmproj_impl(State(st.clone()), Json(BoolValueRequest { value: true })).await;
        assert_eq!(v["ok"], true);

        let loaded = crate::routes::settings_io::load_user_settings(&st);
        assert!(loaded.llm.autoload_mmproj);
    }

    #[tokio::test]
    async fn llm_set_active_model_updates_active_model() {
        let (_td, st) = mk_state();
        if let Ok(mut cat) = st.llm_catalog.lock() {
            cat.entries.push(skill_llm::catalog::LlmModelEntry {
                repo: "a/b".into(),
                filename: "model-a.gguf".into(),
                quant: "Q4".into(),
                size_gb: 1.0,
                description: String::new(),
                family_id: "f1".into(),
                family_name: "F1".into(),
                family_desc: String::new(),
                tags: vec![],
                is_mmproj: false,
                recommended: false,
                advanced: false,
                params_b: 1.0,
                max_context_length: 2048,
                shard_files: vec![],
                local_path: None,
                state: skill_llm::catalog::DownloadState::NotDownloaded,
                status_msg: None,
                progress: 0.0,
                initiated_at_unix: None,
            });
            cat.entries.push(skill_llm::catalog::LlmModelEntry {
                repo: "a/b".into(),
                filename: "model-b-mmproj.gguf".into(),
                quant: "F16".into(),
                size_gb: 0.2,
                description: String::new(),
                family_id: "f2".into(),
                family_name: "F2".into(),
                family_desc: String::new(),
                tags: vec![],
                is_mmproj: true,
                recommended: false,
                advanced: false,
                params_b: 0.0,
                max_context_length: 0,
                shard_files: vec![],
                local_path: None,
                state: skill_llm::catalog::DownloadState::NotDownloaded,
                status_msg: None,
                progress: 0.0,
                initiated_at_unix: None,
            });
            cat.active_mmproj = "model-b-mmproj.gguf".into();
        }

        let _ = llm_set_active_model_impl(
            State(st.clone()),
            Json(LlmFilenameRequest {
                filename: "model-a.gguf".into(),
            }),
        )
        .await;

        let cat = st.llm_catalog.lock().unwrap().clone();
        assert_eq!(cat.active_model, "model-a.gguf");
    }
}
