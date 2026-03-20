// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! WebSocket LLM command handlers (feature = "llm").

#[allow(unused_imports)]
use serde_json::Value;
#[allow(unused_imports)]
use tauri::AppHandle;

#[allow(unused_imports)]
use crate::AppStateExt;
#[allow(unused_imports)]
use crate::MutexExt;

// ── LLM commands (feature = "llm") ───────────────────────────────────────────

/// `llm_status` — return the current LLM server state.
///
/// ```json
/// { "command": "llm_status" }
/// → { "command": "llm_status", "ok": true,
///     "status": "stopped"|"loading"|"running",
///     "model_name": "Qwen3-1.7B-Q4_K_M.gguf",
///     "n_ctx": 4096, "supports_vision": false }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_status(app: &AppHandle) -> Result<Value, String> {
    use std::sync::atomic::Ordering;
    let state = app.app_state();
    let s = state.lock_or_recover();
    let (status, model_name) = crate::llm::cell_status(&s.llm.state_cell);
    let (n_ctx, supports_vision) = s.llm.state_cell.lock().expect("lock poisoned")
        .as_ref()
        .map(|srv| (
            srv.n_ctx.load(Ordering::Relaxed),
            srv.vision_ready.load(Ordering::Relaxed),
        ))
        .unwrap_or((0, false));
    Ok(serde_json::json!({
        "status":          status,
        "model_name":      model_name,
        "n_ctx":           n_ctx,
        "supports_vision": supports_vision,
    }))
}

/// `llm_start` — load the active model and start the LLM inference server.
///
/// Blocks until the model is fully loaded (which can take several seconds
/// depending on model size and hardware).  Returns `ok=false` on failure.
///
/// ```json
/// { "command": "llm_start" }
/// → { "command": "llm_start", "ok": true, "result": "started"|"already_running" }
/// ```
#[cfg(feature = "llm")]
pub(super) async fn llm_start(app: &AppHandle) -> Result<Value, String> {
    let (mut config, catalog, log_buf, cell, skill_dir) = {
        let st = app.app_state();
        let s = st.lock_or_recover();
        (
            s.llm.config.clone(),
            s.llm.catalog.clone(),
            s.llm.logs.clone(),
            s.llm.state_cell.clone(),
            s.skill_dir.clone(),
        )
    };

    if cell.lock().expect("lock poisoned").is_some() {
        return Ok(serde_json::json!({ "result": "already_running" }));
    }

    // Resolve mmproj if autoload is on and none is set.
    if config.mmproj.is_none() {
        config.mmproj = catalog.resolve_mmproj_path(config.autoload_mmproj);
    }

    let emitter = crate::llm::TauriEmitter(app.clone());
    crate::llm::push_log(&emitter, &log_buf, "info", "llm_start command received via WebSocket");

    let emitter_arc: std::sync::Arc<dyn crate::llm::LlmEventEmitter> = std::sync::Arc::new(emitter);
    let new_state = tokio::task::spawn_blocking(move || {
        crate::llm::init(&config, &catalog, emitter_arc, log_buf, &skill_dir)
    }).await.map_err(|e| e.to_string())?;

    match new_state {
        Some(s) => {
            *cell.lock().expect("lock poisoned") = Some(s);
            Ok(serde_json::json!({ "result": "started" }))
        }
        None => Err(
            "Failed to start LLM server. \
             Check that a model is downloaded and selected in Settings → LLM.".to_string()
        ),
    }
}

/// `llm_stop` — stop the LLM inference server and free all GPU/CPU resources.
///
/// ```json
/// { "command": "llm_stop" }
/// → { "command": "llm_stop", "ok": true, "result": "stopped"|"not_running" }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_stop(app: &AppHandle) -> Result<Value, String> {
    let (cell, log_buf) = {
        let st = app.app_state();
        let s = st.lock_or_recover();
        (s.llm.state_cell.clone(), s.llm.logs.clone())
    };
    let server_state = { cell.lock().expect("lock poisoned").take() };
    if let Some(server_state) = server_state {
        let emitter = crate::llm::TauriEmitter(app.clone());
        crate::llm::push_log(&emitter, &log_buf, "info", "llm_stop command received via WebSocket");
        match std::sync::Arc::try_unwrap(server_state) {
            Ok(owned) => owned.shutdown(),
            Err(arc)  => drop(arc),
        }
        crate::llm::push_log(&emitter, &log_buf, "info", "LLM server stopped");
        Ok(serde_json::json!({ "result": "stopped" }))
    } else {
        Ok(serde_json::json!({ "result": "not_running" }))
    }
}

/// `llm_catalog` — return the model catalog with download states and selections.
///
/// ```json
/// { "command": "llm_catalog" }
/// → { "command": "llm_catalog", "ok": true,
///     "entries": [...], "active_model": "...", "active_mmproj": "..." }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_catalog(app: &AppHandle) -> Result<Value, String> {
    let state = app.app_state();
    let mut s = state.lock_or_recover();
    // Sync in-flight downloads into the catalog so callers see live progress.
    let downloads = s.llm.downloads.clone();
    for (filename, prog_arc) in &downloads {
        if let Ok(prog) = prog_arc.lock() {
            if let Some(entry) = s.llm.catalog.entries
                .iter_mut()
                .find(|e| &e.filename == filename)
            {
                entry.state      = prog.state.clone();
                entry.status_msg = prog.status_msg.clone();
                entry.progress   = prog.progress;
            }
        }
    }
    serde_json::to_value(&s.llm.catalog).map_err(|e| e.to_string())
}

/// `llm_download` — start downloading a GGUF model by filename (fire-and-forget).
///
/// Poll `llm_catalog` for progress updates.
///
/// ```json
/// { "command": "llm_download", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_download", "ok": true, "result": "queued", "filename": "..." }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_download(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_download: 'filename' field required (string)".to_string())?
        .to_string();
    crate::llm::cmds::download_llm_model(
        filename.clone(),
        app.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({ "result": "queued", "filename": filename }))
}

/// `llm_cancel_download` — cancel an in-progress model download.
///
/// ```json
/// { "command": "llm_cancel_download", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_cancel_download", "ok": true, "filename": "..." }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_cancel_download(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_cancel_download: 'filename' field required".to_string())?
        .to_string();
    crate::llm::cmds::cancel_llm_download_with_app(filename.clone(), app, app.app_state());
    Ok(serde_json::json!({ "filename": filename }))
}

/// `llm_delete` — delete a locally-cached model file.
///
/// ```json
/// { "command": "llm_delete", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_delete", "ok": true, "filename": "..." }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_delete(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_delete: 'filename' field required".to_string())?
        .to_string();
    crate::llm::cmds::delete_llm_model(
        filename.clone(),
        app.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({ "filename": filename }))
}

/// `llm_logs` — return the last ≤500 LLM server log lines.
///
/// ```json
/// { "command": "llm_logs" }
/// → { "command": "llm_logs", "ok": true,
///     "logs": [{ "ts": 1740412800000, "level": "info", "message": "..." }, …] }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_logs(app: &AppHandle) -> Result<Value, String> {
    let state = app.app_state();
    let s = state.lock_or_recover();
    let log = s.llm.logs.lock().expect("lock poisoned");
    let logs: Vec<&crate::llm::LlmLogEntry> = log.iter().collect();
    Ok(serde_json::json!({ "logs": logs, "count": logs.len() }))
}

/// `llm_select_model` — set the active text model by filename.
///
/// ```json
/// { "command": "llm_select_model", "filename": "Qwen_Qwen3.5-4B-Q4_K_M.gguf" }
/// → { "command": "llm_select_model", "ok": true, "filename": "...", "active_model": "..." }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_select_model(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_select_model: 'filename' field required (string)".to_string())?
        .to_string();
    crate::llm::cmds::set_llm_active_model(
        filename.clone(),
        app.clone(),
        app.app_state(),
    );
    let state = app.app_state();
    let s = state.lock_or_recover();
    Ok(serde_json::json!({
        "filename": filename,
        "active_model": s.llm.catalog.active_model,
        "active_mmproj": s.llm.catalog.active_mmproj,
    }))
}

/// `llm_select_mmproj` — set the active vision projector by filename (empty to disable).
///
/// ```json
/// { "command": "llm_select_mmproj", "filename": "mmproj-Qwen_Qwen3.5-4B-BF16.gguf" }
/// → { "command": "llm_select_mmproj", "ok": true, "filename": "...", "active_mmproj": "..." }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_select_mmproj(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .unwrap_or("")
        .to_string();
    crate::llm::cmds::set_llm_active_mmproj(
        filename.clone(),
        app.clone(),
        app.app_state(),
    );
    let state = app.app_state();
    let s = state.lock_or_recover();
    Ok(serde_json::json!({
        "filename": filename,
        "active_model": s.llm.catalog.active_model,
        "active_mmproj": s.llm.catalog.active_mmproj,
    }))
}

/// `llm_pause_download` — pause an in-progress model download.
///
/// ```json
/// { "command": "llm_pause_download", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_pause_download", "ok": true, "filename": "..." }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_pause_download(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_pause_download: 'filename' field required".to_string())?
        .to_string();
    crate::llm::cmds::pause_llm_download(
        filename.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({ "filename": filename }))
}

/// `llm_resume_download` — resume a paused model download.
///
/// ```json
/// { "command": "llm_resume_download", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_resume_download", "ok": true, "filename": "..." }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_resume_download(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_resume_download: 'filename' field required".to_string())?
        .to_string();
    crate::llm::cmds::resume_llm_download(
        filename.clone(),
        app.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({ "filename": filename }))
}

/// `llm_refresh_catalog` — re-probe the HF Hub cache and update download states.
///
/// ```json
/// { "command": "llm_refresh_catalog" }
/// → { "command": "llm_refresh_catalog", "ok": true }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_refresh_catalog(app: &AppHandle) -> Result<Value, String> {
    crate::llm::cmds::refresh_llm_catalog(
        app.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({}))
}

/// `llm_downloads` — list all downloads (active, paused, completed, failed).
///
/// ```json
/// { "command": "llm_downloads" }
/// → { "command": "llm_downloads", "ok": true, "downloads": [...] }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_downloads(app: &AppHandle) -> Result<Value, String> {
    let items = crate::llm::cmds::get_llm_downloads(
        app.app_state(),
    );
    Ok(serde_json::json!({ "downloads": items, "count": items.len() }))
}

/// `llm_set_autoload_mmproj` — toggle whether the vision projector auto-loads on start.
///
/// ```json
/// { "command": "llm_set_autoload_mmproj", "enabled": true }
/// → { "command": "llm_set_autoload_mmproj", "ok": true, "enabled": true }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_set_autoload_mmproj(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let enabled = msg["enabled"]
        .as_bool()
        .ok_or_else(|| "llm_set_autoload_mmproj: 'enabled' field required (bool)".to_string())?;
    crate::llm::cmds::set_llm_autoload_mmproj(
        enabled,
        app.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({ "enabled": enabled }))
}

/// `llm_add_model` — add an external HuggingFace model to the catalog and optionally download it.
///
/// Creates a new catalog entry from the repo and filename if it doesn't already exist.
/// Metadata (quant, mmproj, family) is inferred from the filename/repo.
///
/// ```json
/// { "command": "llm_add_model", "repo": "bartowski/Phi-4-mini-reasoning-GGUF",
///   "filename": "Phi-4-mini-reasoning-Q4_K_M.gguf", "download": true }
/// → { "command": "llm_add_model", "ok": true, "filename": "...", "repo": "..." }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_add_model(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let repo = msg["repo"]
        .as_str()
        .ok_or_else(|| "llm_add_model: 'repo' field required (string, e.g. \"bartowski/Phi-4-GGUF\")".to_string())?
        .to_string();
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_add_model: 'filename' field required (string, e.g. \"Phi-4-Q4_K_M.gguf\")".to_string())?
        .to_string();
    let size_gb = msg["size_gb"].as_f64().map(|v| v as f32);
    let mmproj = msg["mmproj"].as_str().map(|s| s.to_string());
    let download = msg.get("download").and_then(|v| v.as_bool());

    let result = crate::llm::cmds::add_llm_model(
        repo.clone(),
        filename.clone(),
        size_gb,
        mmproj.clone(),
        download,
        app.clone(),
        app.app_state(),
    )?;
    Ok(serde_json::json!({ "filename": result, "repo": repo, "mmproj": mmproj }))
}

/// `llm_hardware_fit` — check which models fit in available memory.
///
/// ```json
/// { "command": "llm_hardware_fit" }
/// → { "command": "llm_hardware_fit", "ok": true,
///     "fits": [{ "filename": "...", "fit_level": "good", "run_mode": "gpu", ... }, …] }
/// ```
#[cfg(feature = "llm")]
pub(super) fn llm_hardware_fit(app: &AppHandle, _msg: &Value) -> Result<Value, String> {
    let result = crate::llm::cmds::get_model_hardware_fit(
        app.app_state(),
    );
    Ok(serde_json::json!({ "fits": result }))
}

