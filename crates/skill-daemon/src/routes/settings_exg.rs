// SPDX-License-Identifier: GPL-3.0-only
//! EXG model/config/download handlers.

use axum::{extract::State, Json};
use skill_eeg::eeg_model_config::{load_model_config, save_model_config, EegModelStatus, ExgModelConfig};

use crate::state::AppState;

pub(crate) async fn get_model_config_impl(State(state): State<AppState>) -> Json<ExgModelConfig> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    Json(load_model_config(&skill_dir))
}

pub(crate) async fn set_model_config_impl(
    State(state): State<AppState>,
    Json(config): Json<ExgModelConfig>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    save_model_config(&skill_dir, &config);
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn get_model_status_impl(State(state): State<AppState>) -> Json<EegModelStatus> {
    let mut st = state.exg_model_status.lock().map(|g| g.clone()).unwrap_or_default();

    if !st.weights_found && !st.downloading_weights {
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        let config = skill_eeg::eeg_model_config::load_model_config(&skill_dir);
        let found = probe_weights_for_config(&config);
        if let Some((weights_path, backend_str)) = found {
            st.weights_found = true;
            st.weights_path = Some(weights_path);
            st.active_model_backend = Some(backend_str);
            if let Ok(mut shared) = state.exg_model_status.lock() {
                shared.weights_found = true;
                shared.weights_path = st.weights_path.clone();
                shared.active_model_backend = st.active_model_backend.clone();
            }
        }
    }

    Json(st)
}

/// Public so `main.rs` can call it during daemon startup.
pub fn probe_weights_for_config(config: &ExgModelConfig) -> Option<(String, String)> {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../../src-tauri/exg_catalog.json")).ok()?;
    let backend = config.model_backend.as_str();
    let family_id = if backend == "luna" {
        format!("luna-{}", config.luna_variant)
    } else {
        let families = catalog.get("families")?.as_object()?;
        families
            .keys()
            .find(|id| family_id_to_backend(id) == backend)
            .cloned()
            .unwrap_or_else(|| backend.to_string())
    };

    let fam = catalog.get("families")?.get(&family_id)?;
    let repo = fam.get("repo")?.as_str()?;
    let wf = fam.get("weights_file")?.as_str()?;

    let snaps_dir = skill_data::util::hf_model_dir(repo).join("snapshots");
    let entries = std::fs::read_dir(&snaps_dir).ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let wp = entry.path().join(wf);
        if skill_exg::validate_or_remove(&wp) {
            return Some((wp.display().to_string(), backend.to_string()));
        }
    }
    None
}

pub(crate) async fn trigger_reembed_impl() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "message": "reembed queued in daemon" }))
}

pub(crate) async fn trigger_weights_download_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    use std::sync::atomic::Ordering;

    state.exg_download_cancel.store(false, Ordering::Relaxed);

    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let config = skill_eeg::eeg_model_config::load_model_config(&skill_dir);
    let status = state.exg_model_status.clone();
    let cancel = state.exg_download_cancel.clone();

    if let Ok(st) = status.lock() {
        if st.downloading_weights {
            return Json(serde_json::json!({ "ok": false, "message": "download already in progress" }));
        }
    }

    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../../src-tauri/exg_catalog.json")).unwrap_or_default();
    let backend_str = config.model_backend.as_str().to_string();
    let (hf_repo, weights_file, config_file) = resolve_download_target(&catalog, &config);

    spawn_exg_download(state, hf_repo, weights_file, config_file, backend_str, status, cancel);

    Json(serde_json::json!({ "ok": true, "message": "weights download started" }))
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn emit_daemon_event(state: &AppState, event_type: &str, payload: serde_json::Value) {
    let _ = state.events_tx.send(skill_daemon_common::EventEnvelope {
        r#type: event_type.to_string(),
        ts_unix_ms: now_unix_ms(),
        correlation_id: None,
        payload,
    });
}

fn spawn_exg_download(
    state: AppState,
    hf_repo: String,
    weights_file: String,
    config_file: String,
    backend_str: String,
    status: std::sync::Arc<std::sync::Mutex<EegModelStatus>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    tokio::spawn(async move {
        let status_for_thread = status.clone();
        let cancel_for_thread = cancel.clone();

        let mut job = tokio::task::spawn_blocking(move || {
            skill_exg::download_hf_weights_files(
                &hf_repo,
                &weights_file,
                &config_file,
                &status_for_thread,
                &cancel_for_thread,
                false,
            )
        });

        loop {
            if job.is_finished() {
                break;
            }

            if let Ok(st) = status.lock() {
                emit_daemon_event(
                    &state,
                    "ExgDownloadProgress",
                    serde_json::json!({
                        "backend": backend_str,
                        "downloading": st.downloading_weights,
                        "progress": st.download_progress,
                        "status_msg": st.download_status_msg,
                        "weights_found": st.weights_found,
                        "needs_restart": false,
                    }),
                );
            }

            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        let result = (&mut job).await;
        let succeeded = matches!(result, Ok(Some(_)));

        if succeeded {
            if let Ok(mut st) = status.lock() {
                st.download_needs_restart = false;
                st.weights_found = true;
                st.active_model_backend = Some(backend_str.clone());
            }
        }

        if let Ok(st) = status.lock() {
            emit_daemon_event(
                &state,
                if succeeded {
                    "ExgDownloadCompleted"
                } else {
                    "ExgDownloadFailed"
                },
                serde_json::json!({
                    "backend": backend_str,
                    "downloading": st.downloading_weights,
                    "progress": st.download_progress,
                    "status_msg": st.download_status_msg,
                    "weights_found": st.weights_found,
                    "needs_restart": false,
                }),
            );
        }

        if succeeded {
            tracing::info!("[exg] weights download complete for {backend_str}");
        } else {
            tracing::warn!("[exg] weights download failed or cancelled for {backend_str}");
        }
    });
}

fn resolve_download_target(catalog: &serde_json::Value, config: &ExgModelConfig) -> (String, String, String) {
    let backend = config.model_backend.as_str();

    let family_id = if backend == "luna" {
        format!("luna-{}", config.luna_variant)
    } else {
        let families = catalog.get("families").and_then(|f| f.as_object());
        if let Some(fams) = families {
            fams.keys()
                .find(|id| family_id_to_backend(id) == backend)
                .cloned()
                .unwrap_or_else(|| backend.to_string())
        } else {
            backend.to_string()
        }
    };

    if let Some(fam) = catalog.get("families").and_then(|f| f.get(&family_id)) {
        let repo = fam.get("repo").and_then(|v| v.as_str()).unwrap_or(&config.hf_repo);
        let wf = fam
            .get("weights_file")
            .and_then(|v| v.as_str())
            .unwrap_or("model-00001-of-00001.safetensors");
        let cf = fam.get("config_file").and_then(|v| v.as_str()).unwrap_or("config.json");
        (repo.to_string(), wf.to_string(), cf.to_string())
    } else if backend == "luna" {
        (
            config.luna_hf_repo.clone(),
            config.luna_weights_file().to_string(),
            "config.json".to_string(),
        )
    } else {
        (
            config.hf_repo.clone(),
            "model-00001-of-00001.safetensors".to_string(),
            "config.json".to_string(),
        )
    }
}

fn family_id_to_backend(id: &str) -> &str {
    if id == "zuna" {
        return "zuna";
    }
    if id.starts_with("luna-") {
        return "luna";
    }
    if id == "reve-base" || id == "reve-large" {
        return "reve";
    }
    if id == "osf-base" {
        return "osf";
    }
    if id.starts_with("steegformer-") {
        return "steegformer";
    }
    id
}

pub(crate) async fn cancel_weights_download_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    state
        .exg_download_cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({ "ok": true, "message": "weights download cancellation requested" }))
}

pub(crate) async fn estimate_reembed_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let sessions = tokio::task::spawn_blocking(move || skill_history::list_all_sessions(&skill_dir, None))
        .await
        .unwrap_or_default();
    Json(serde_json::json!({
        "sessions_total": sessions.len(),
        "embeddings_needed": 0,
    }))
}

pub(crate) async fn rebuild_index_impl() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "message": "index rebuild queued in daemon" }))
}

pub(crate) async fn get_exg_catalog_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    let v = tokio::task::spawn_blocking(move || {
        const BUNDLED: &str = include_str!("../../../../src-tauri/exg_catalog.json");
        let mut v: serde_json::Value = serde_json::from_str(BUNDLED).unwrap_or_default();

        if let Some(families) = v.get_mut("families").and_then(|f| f.as_object_mut()) {
            for (_id, fam) in families.iter_mut() {
                let repo = fam.get("repo").and_then(|r| r.as_str()).unwrap_or("");
                let weights_file = fam.get("weights_file").and_then(|w| w.as_str()).unwrap_or("");
                let cached = if !repo.is_empty() && !weights_file.is_empty() {
                    let snaps_dir = skill_data::util::hf_model_dir(repo).join("snapshots");
                    std::fs::read_dir(&snaps_dir)
                        .ok()
                        .map(|entries| {
                            entries.filter_map(|e| e.ok()).any(|e| {
                                let p = e.path().join(weights_file);
                                skill_exg::validate_or_remove(&p)
                            })
                        })
                        .unwrap_or(false)
                } else {
                    false
                };
                if let Some(obj) = fam.as_object_mut() {
                    obj.insert("weights_cached".to_string(), serde_json::json!(cached));
                }
            }
        }

        let config = skill_eeg::eeg_model_config::load_model_config(&skill_dir);
        let active_name = match config.model_backend {
            skill_eeg::eeg_model_config::ExgModelBackend::Luna => {
                if let Some(fam) = v
                    .get("families")
                    .and_then(|f| f.get(format!("luna-{}", config.luna_variant)))
                {
                    fam.get("name").and_then(|n| n.as_str()).unwrap_or("LUNA").to_string()
                } else {
                    "LUNA".to_string()
                }
            }
            _ => {
                let backend_str = config.model_backend.as_str();
                if let Some(families) = v.get("families").and_then(|f| f.as_object()) {
                    families
                        .iter()
                        .find(|(id, _)| family_id_to_backend(id) == backend_str)
                        .and_then(|(_, fam)| fam.get("name").and_then(|n| n.as_str()))
                        .unwrap_or("ZUNA")
                        .to_string()
                } else {
                    "ZUNA".to_string()
                }
            }
        };
        if let Some(obj) = v.as_object_mut() {
            obj.insert("active_model".to_string(), serde_json::json!(active_name));
        }

        v
    })
    .await
    .unwrap_or_default();

    Json(v)
}
