// SPDX-License-Identifier: GPL-3.0-only
//! LLM commands: status, start/stop, catalog, download management, chat, and helpers.

use serde_json::{json, Value};

use super::{bool_field, f64_field, skill_dir, str_field};
use crate::state::AppState;

// ── LLM ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "llm")]
#[derive(Clone)]
struct CmdLlmEmitter {
    events_tx: tokio::sync::broadcast::Sender<skill_daemon_common::EventEnvelope>,
}

#[cfg(feature = "llm")]
impl skill_llm::LlmEventEmitter for CmdLlmEmitter {
    fn emit_event(&self, event: &str, payload: serde_json::Value) {
        let _ = self.events_tx.send(skill_daemon_common::EventEnvelope {
            r#type: format!("Llm{}", event.replace(':', "_")),
            ts_unix_ms: now_unix() * 1000,
            correlation_id: None,
            payload,
        });
    }
}

pub(super) async fn cmd_llm_status(state: &AppState) -> Result<Value, String> {
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

        return Ok(json!({
            "status": serde_json::to_value(status).unwrap_or(json!("stopped")),
            "model_name": model_name,
            "n_ctx": n_ctx,
            "supports_vision": supports_vision,
            "supports_tools": supports_tools,
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
        Ok(json!({
            "status": status,
            "model_name": model_name,
        }))
    }
}

pub(super) async fn cmd_llm_start(state: &AppState) -> Result<Value, String> {
    // Delegate to the REST handler logic
    #[cfg(feature = "llm")]
    {
        let cfg = state.llm_config.lock().map(|g| g.clone()).unwrap_or_default();
        let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
        let skill_dir = skill_dir(state);
        let cell = state.llm_state_cell.clone();
        let log_buf = state.llm_log_buffer.clone();

        if cell.lock().ok().and_then(|g| g.clone()).is_some() {
            return Ok(json!({"result": "already_running"}));
        }

        let emitter: std::sync::Arc<dyn skill_llm::LlmEventEmitter> = std::sync::Arc::new(CmdLlmEmitter {
            events_tx: state.events_tx.clone(),
        });
        match tokio::task::spawn_blocking(move || skill_llm::init(&cfg, &cat, emitter, log_buf, &skill_dir)).await {
            Ok(Some(srv)) => {
                let model_name = srv.model_name.clone();
                if let Ok(mut g) = cell.lock() {
                    *g = Some(srv);
                }
                if let Ok(mut st) = state.llm_status.lock() {
                    *st = "running".into();
                }
                if let Ok(mut m) = state.llm_model_name.lock() {
                    *m = model_name;
                }
                return Ok(json!({"result": "starting"}));
            }
            Ok(None) => return Err("LLM init returned none".into()),
            Err(e) => return Err(e.to_string()),
        }
    }

    #[cfg(not(feature = "llm"))]
    {
        if let Ok(mut st) = state.llm_status.lock() {
            *st = "running".into();
        }
        Ok(json!({"result": "starting"}))
    }
}

pub(super) async fn cmd_llm_stop(state: &AppState) -> Result<Value, String> {
    #[cfg(feature = "llm")]
    {
        skill_llm::shutdown_cell(&state.llm_state_cell);
    }
    if let Ok(mut st) = state.llm_status.lock() {
        *st = "stopped".into();
    }
    Ok(json!({}))
}

pub(super) async fn cmd_llm_catalog(state: &AppState) -> Result<Value, String> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    Ok(serde_json::to_value(cat).unwrap_or_default())
}

pub(super) async fn cmd_llm_add_model(state: &AppState, msg: &Value) -> Result<Value, String> {
    let repo = str_field(msg, "repo").ok_or("missing repo")?;
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    let size_gb = f64_field(msg, "size_gb").map(|v| v as f32);
    let mmproj = str_field(msg, "mmproj");
    let download = bool_field(msg, "download").unwrap_or(false);

    if let Ok(mut cat) = state.llm_catalog.lock() {
        if !cat.entries.iter().any(|e| e.filename == filename) {
            let is_mmproj = mmproj.as_ref().map(|m| m == &filename).unwrap_or(false)
                || filename.to_ascii_lowercase().contains("mmproj");
            cat.entries.push(skill_llm::catalog::LlmModelEntry {
                repo: repo.clone(),
                filename: filename.clone(),
                quant: infer_quant(&filename),
                size_gb: size_gb.unwrap_or(0.0),
                description: "External model".to_string(),
                family_id: repo
                    .split('/')
                    .next_back()
                    .unwrap_or("external")
                    .to_lowercase()
                    .replace(' ', "-"),
                family_name: repo
                    .split('/')
                    .next_back()
                    .unwrap_or("External")
                    .replace(['_', '-'], " "),
                family_desc: String::new(),
                tags: vec!["external".to_string()],
                is_mmproj,
                recommended: false,
                advanced: false,
                params_b: 0.0,
                max_context_length: 0,
                shard_files: Vec::new(),
                local_path: None,
                state: if download {
                    skill_llm::catalog::DownloadState::Downloading
                } else {
                    skill_llm::catalog::DownloadState::NotDownloaded
                },
                status_msg: if download { Some("Queued".into()) } else { None },
                progress: 0.0,
                initiated_at_unix: Some(now_unix()),
            });
        }
        cat.auto_select();
    }
    persist_llm_catalog(state);
    Ok(json!({ "filename": filename }))
}

pub(super) async fn cmd_llm_select_model(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_model = filename.clone();
        if !cat.active_mmproj_matches_active_model() {
            cat.active_mmproj.clear();
        }
    }
    persist_llm_catalog(state);
    Ok(json!({ "active_model": filename }))
}

pub(super) async fn cmd_llm_select_mmproj(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_mmproj = filename.clone();
    }
    persist_llm_catalog(state);
    Ok(json!({ "active_mmproj": filename }))
}

pub(super) async fn cmd_llm_set_autoload_mmproj(state: &AppState, msg: &Value) -> Result<Value, String> {
    let enabled = bool_field(msg, "enabled").ok_or("missing enabled")?;
    let skill_dir = skill_dir(state);
    let mut settings = skill_settings::load_settings(&skill_dir);
    settings.llm.autoload_mmproj = enabled;
    let path = skill_settings::settings_path(&skill_dir);
    let _ = serde_json::to_string_pretty(&settings)
        .ok()
        .and_then(|json| std::fs::write(path, json).ok());
    Ok(json!({ "value": enabled }))
}

pub(super) async fn cmd_llm_download(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    set_download_state_cmd(
        state,
        &filename,
        skill_llm::catalog::DownloadState::Downloading,
        Some("Queued".into()),
    );
    spawn_model_download_cmd(state.clone(), filename.clone());
    Ok(json!({}))
}

pub(super) async fn cmd_llm_pause_download(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    set_live_cancel_flags(state, &filename, true, true);
    set_download_state_cmd(
        state,
        &filename,
        skill_llm::catalog::DownloadState::Paused,
        Some("Pausing".into()),
    );
    Ok(json!({}))
}

pub(super) async fn cmd_llm_resume_download(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    let is_active = state
        .llm_downloads
        .lock()
        .ok()
        .map(|m| m.contains_key(&filename))
        .unwrap_or(false);
    if !is_active {
        set_download_state_cmd(
            state,
            &filename,
            skill_llm::catalog::DownloadState::Downloading,
            Some("Resumed".into()),
        );
        spawn_model_download_cmd(state.clone(), filename);
    }
    Ok(json!({}))
}

pub(super) async fn cmd_llm_cancel_download(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    set_live_cancel_flags(state, &filename, true, false);
    set_download_state_cmd(
        state,
        &filename,
        skill_llm::catalog::DownloadState::Cancelled,
        Some("Cancelling".into()),
    );
    Ok(json!({}))
}

pub(super) async fn cmd_llm_delete(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    set_live_cancel_flags(state, &filename, true, false);
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
            e.state = skill_llm::catalog::DownloadState::NotDownloaded;
            e.status_msg = None;
            e.progress = 0.0;
            e.local_path = None;
        }
    }
    if let Ok(mut m) = state.llm_downloads.lock() {
        m.remove(&filename);
    }
    persist_llm_catalog(state);
    Ok(json!({}))
}

pub(super) async fn cmd_llm_downloads(state: &AppState) -> Result<Value, String> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    let downloads = state.llm_downloads.lock().map(|g| g.clone()).unwrap_or_default();
    let items: Vec<Value> = cat
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
            json!({
                "repo": e.repo,
                "filename": e.filename,
                "quant": e.quant,
                "size_gb": e.size_gb,
                "state": live.as_ref().map(|p| p.state.clone()).unwrap_or(e.state),
                "status_msg": live.as_ref().and_then(|p| p.status_msg.clone()).or(e.status_msg),
                "progress": live.as_ref().map(|p| p.progress).unwrap_or(e.progress),
            })
        })
        .collect();
    Ok(json!({ "downloads": items }))
}

pub(super) async fn cmd_llm_refresh(state: &AppState) -> Result<Value, String> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.refresh_cache();
        cat.auto_select();
    }
    persist_llm_catalog(state);
    Ok(json!({}))
}

pub(super) async fn cmd_llm_hardware_fit(state: &AppState) -> Result<Value, String> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    let gpu = skill_data::gpu_stats::read();
    let sys_ram_gb = sysinfo::System::new_all().total_memory() as f64 / 1_073_741_824.0;
    let vram_gb = gpu
        .as_ref()
        .and_then(|g| g.total_memory_bytes.map(|b| b as f64 / 1_073_741_824.0))
        .unwrap_or(0.0);

    let fits: Vec<Value> = cat
        .entries
        .iter()
        .filter(|e| e.size_gb > 0.0)
        .map(|e| {
            let size = e.size_gb as f64;
            let (fit_level, run_mode) = if vram_gb > 0.0 && size * 1.2 <= vram_gb {
                ("perfect", "gpu")
            } else if size * 1.2 <= sys_ram_gb {
                ("good", "cpu")
            } else if size <= sys_ram_gb {
                ("marginal", "cpu")
            } else {
                ("too_large", "none")
            };
            json!({
                "filename": e.filename,
                "size_gb": e.size_gb,
                "fit_level": fit_level,
                "run_mode": run_mode,
                "memory_required_gb": size * 1.2,
            })
        })
        .collect();

    Ok(json!({ "fits": fits }))
}

pub(super) async fn cmd_llm_logs(state: &AppState) -> Result<Value, String> {
    #[cfg(feature = "llm")]
    {
        let logs: Vec<Value> = state
            .llm_log_buffer
            .lock()
            .map(|q| q.iter().filter_map(|e| serde_json::to_value(e).ok()).collect())
            .unwrap_or_default();
        return Ok(json!({ "logs": logs }));
    }

    #[cfg(not(feature = "llm"))]
    {
        let logs = state.llm_logs.lock().map(|g| g.clone()).unwrap_or_default();
        Ok(json!({ "logs": logs }))
    }
}

/// Non-streaming LLM chat (for HTTP mode).  For WS streaming see `dispatch_llm_chat_streaming`.
#[allow(unused_variables)]
pub(super) async fn cmd_llm_chat(state: &AppState, msg: &Value) -> Result<Value, String> {
    let messages = msg.get("messages").cloned().unwrap_or(Value::Array(Vec::new()));
    let params = msg.get("params").cloned().unwrap_or(Value::Object(Default::default()));

    #[cfg(feature = "llm")]
    {
        let srv_opt = state.llm_state_cell.lock().ok().and_then(|g| g.clone());
        let Some(srv) = srv_opt else {
            return Err("LLM server not running".into());
        };
        let messages_vec: Vec<Value> = serde_json::from_value(messages).unwrap_or_default();
        let gen_params: skill_llm::GenParams = serde_json::from_value(params).unwrap_or_default();

        let result =
            skill_llm::run_chat_with_builtin_tools(&srv, messages_vec, gen_params, Vec::new(), |_delta| {}, |_evt| {})
                .await;

        return match result {
            Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx)) => Ok(json!({
                "content": text,
                "finish_reason": finish_reason,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "n_ctx": n_ctx,
            })),
            Err(e) => Err(e.to_string()),
        };
    }

    #[cfg(not(feature = "llm"))]
    {
        let _ = (messages, params);
        Err("LLM unavailable (compiled without llm feature)".into())
    }
}

/// Streaming LLM chat over WebSocket.  Sends incremental `delta` messages
/// followed by a `done` message.  Returns `None` to indicate the caller
/// should NOT send a normal response (we already sent the streaming messages).
#[cfg(feature = "llm")]
pub async fn dispatch_llm_chat_streaming(
    state: AppState,
    msg: Value,
    ws_tx: &mut tokio::sync::mpsc::Sender<String>,
) -> bool {
    let messages = msg.get("messages").cloned().unwrap_or(Value::Array(Vec::new()));
    let params = msg.get("params").cloned().unwrap_or(Value::Object(Default::default()));

    let srv_opt = state.llm_state_cell.lock().ok().and_then(|g| g.clone());
    let Some(srv) = srv_opt else {
        let err = json!({ "command": "llm_chat", "ok": false, "type": "error", "error": "LLM server not running" });
        let _ = ws_tx.send(serde_json::to_string(&err).unwrap_or_default()).await;
        return true;
    };

    let messages_vec: Vec<Value> = serde_json::from_value(messages).unwrap_or_default();
    let gen_params: skill_llm::GenParams = serde_json::from_value(params).unwrap_or_default();

    // Send session message.
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let session_id = skill_llm::chat_store::ChatStore::open(&skill_dir)
        .map(|mut store| store.get_or_create_last_session())
        .unwrap_or(0);
    let session_msg = json!({ "command": "llm_chat", "type": "session", "session_id": session_id });
    let _ = ws_tx
        .send(serde_json::to_string(&session_msg).unwrap_or_default())
        .await;

    // Set up streaming callback.
    let tx_for_delta = ws_tx.clone();
    let delta_callback = move |delta: &str| {
        let msg = json!({ "command": "llm_chat", "type": "delta", "text": delta });
        // Use try_send (non-blocking) — safe from async context; drops token if buffer full.
        let _ = tx_for_delta.try_send(serde_json::to_string(&msg).unwrap_or_default());
    };

    let result =
        skill_llm::run_chat_with_builtin_tools(&srv, messages_vec, gen_params, Vec::new(), delta_callback, |_evt| {})
            .await;

    match result {
        Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx)) => {
            let done = json!({
                "command": "llm_chat",
                "ok": true,
                "type": "done",
                "content": text,
                "finish_reason": finish_reason,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "n_ctx": n_ctx,
                "session_id": session_id,
            });
            let _ = ws_tx.send(serde_json::to_string(&done).unwrap_or_default()).await;
        }
        Err(e) => {
            let err = json!({
                "command": "llm_chat",
                "ok": false,
                "type": "error",
                "error": e.to_string(),
            });
            let _ = ws_tx.send(serde_json::to_string(&err).unwrap_or_default()).await;
        }
    }
    true
}

// ── LLM helpers ──────────────────────────────────────────────────────────────

fn infer_quant(filename: &str) -> String {
    let upper = filename.to_uppercase();
    for q in [
        "IQ4_NL", "IQ4_XS", "IQ3_XXS", "IQ3_XS", "IQ3_M", "IQ3_S", "Q6_K", "Q5_K_M", "Q5_K_S", "Q4_K_M", "Q4_K_S",
        "Q4_0", "Q3_K_M", "Q3_K_S", "Q2_K", "Q8_0", "BF16", "F16", "F32",
    ] {
        if upper.contains(q) {
            return q.to_string();
        }
    }
    "unknown".to_string()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn persist_llm_catalog(state: &AppState) {
    let skill_dir = skill_dir(state);
    if let Ok(cat) = state.llm_catalog.lock() {
        cat.save(&skill_dir);
    }
}

fn set_download_state_cmd(
    state: &AppState,
    filename: &str,
    new_state: skill_llm::catalog::DownloadState,
    msg: Option<String>,
) {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
            e.state = new_state;
            e.status_msg = msg;
            e.initiated_at_unix = Some(now_unix());
        }
    }
    persist_llm_catalog(state);
}

fn set_live_cancel_flags(state: &AppState, filename: &str, cancelled: bool, pause_requested: bool) {
    let progress_opt = state.llm_downloads.lock().ok().and_then(|m| m.get(filename).cloned());
    if let Some(progress) = progress_opt {
        if let Ok(mut p) = progress.lock() {
            p.cancelled = cancelled;
            p.pause_requested = pause_requested;
        }
    }
}

fn spawn_model_download_cmd(state: AppState, filename: String) {
    tokio::spawn(async move {
        let entry_opt = state
            .llm_catalog
            .lock()
            .ok()
            .and_then(|cat| cat.entries.iter().find(|e| e.filename == filename).cloned());
        let Some(entry) = entry_opt else { return };

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
        let job =
            tokio::task::spawn_blocking(move || skill_llm::catalog::download_model(&entry_for_job, &progress_for_job));

        let res = job.await;

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
                        e.status_msg = Some("Downloaded".into());
                    }
                }
                persist_llm_catalog(&state);
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
                set_download_state_cmd(&state, &filename, st, Some(msg));
            }
            Err(err) => {
                set_download_state_cmd(
                    &state,
                    &filename,
                    skill_llm::catalog::DownloadState::Failed,
                    Some(err.to_string()),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_quant_detects_standard_formats() {
        assert_eq!(infer_quant("model-Q4_K_M.gguf"), "Q4_K_M");
        assert_eq!(infer_quant("llama-Q8_0.gguf"), "Q8_0");
        assert_eq!(infer_quant("phi-3-mini-IQ4_NL.gguf"), "IQ4_NL");
        assert_eq!(infer_quant("model-F16.gguf"), "F16");
        assert_eq!(infer_quant("model-BF16.gguf"), "BF16");
        assert_eq!(infer_quant("model-Q6_K.gguf"), "Q6_K");
    }

    #[test]
    fn infer_quant_returns_unknown_for_unrecognized() {
        assert_eq!(infer_quant("model.gguf"), "unknown");
        assert_eq!(infer_quant(""), "unknown");
        assert_eq!(infer_quant("random-file.bin"), "unknown");
    }

    #[test]
    fn infer_quant_is_case_insensitive() {
        assert_eq!(infer_quant("model-q4_k_m.gguf"), "Q4_K_M");
        assert_eq!(infer_quant("MODEL-f32.GGUF"), "F32");
    }

    #[test]
    fn infer_quant_picks_first_match() {
        // If multiple quant tokens appear, returns the first match
        // from the priority list (IQ4_NL before Q4_K_M)
        assert_eq!(infer_quant("model-IQ4_NL-Q4_K_M.gguf"), "IQ4_NL");
    }

    #[test]
    fn now_unix_returns_nonzero() {
        assert!(now_unix() > 1_700_000_000);
    }

    #[test]
    fn infer_quant_handles_all_known_types() {
        let types = [
            "IQ4_NL", "IQ4_XS", "IQ3_XXS", "IQ3_XS", "IQ3_M", "IQ3_S", "Q6_K", "Q5_K_M", "Q5_K_S", "Q4_K_M", "Q4_K_S",
            "Q4_0", "Q3_K_M", "Q3_K_S", "Q2_K", "Q8_0", "BF16", "F16", "F32",
        ];
        for t in types {
            let filename = format!("model-{t}.gguf");
            assert_eq!(infer_quant(&filename), t, "failed for {t}");
        }
    }

    #[test]
    fn infer_quant_with_extra_path_components() {
        assert_eq!(infer_quant("path/to/model-Q4_K_M.gguf"), "Q4_K_M");
        assert_eq!(infer_quant("Model.Q8_0.gguf"), "Q8_0");
    }

    #[tokio::test]
    async fn cmd_llm_status_returns_expected_shape() {
        let td = tempfile::TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        let result = cmd_llm_status(&state).await.unwrap();
        // Should have server_running or enabled field
        assert!(result.is_object());
    }

    #[tokio::test]
    async fn cmd_llm_catalog_returns_entries() {
        let td = tempfile::TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        let result = cmd_llm_catalog(&state).await.unwrap();
        assert!(result.get("entries").is_some());
    }

    #[tokio::test]
    async fn cmd_llm_downloads_returns_array() {
        let td = tempfile::TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        let result = cmd_llm_downloads(&state).await.unwrap();
        assert!(result.get("downloads").is_some());
    }
}
