// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tauri commands for the LLM subsystem.
//!
//! Split into sub-modules by concern:
//! - `catalog`     — catalog queries, adding external models
//! - `downloads`   — download / cancel / pause / resume / delete
//! - `selection`   — active model & mmproj selection
//! - `server`      — server lifecycle (start / stop / switch / status)
//! - `chat`        — chat history persistence
//! - `streaming`   — IPC chat streaming, abort, tool-call cancellation
//! - `hardware_fit`— per-model hardware fit prediction

mod catalog;
mod downloads;
mod selection;
mod server;
mod chat;
mod streaming;
mod hardware_fit;

// ── Re-exports ────────────────────────────────────────────────────────────────
// Every public item is re-exported so that `crate::llm::cmds::foo` paths
// continue to work unchanged.

#[allow(unused_imports)]
pub use catalog::{
    get_llm_catalog, get_llm_downloads, add_llm_model, refresh_llm_catalog,
    LlmDownloadItem,
};
pub use downloads::{
    download_llm_model, cancel_llm_download, cancel_llm_download_with_app,
    pause_llm_download, resume_llm_download, delete_llm_model,
    open_downloads_window,
};
pub use selection::{
    set_llm_active_model, set_llm_active_mmproj, set_llm_autoload_mmproj,
};
#[allow(unused_imports)]
pub use server::{
    start_llm_server, stop_llm_server, switch_llm_model,
    get_llm_server_status, get_llm_logs, LlmServerStatusResponse,
};
#[allow(unused_imports)]
pub use chat::{
    get_last_chat_session, load_chat_session, list_chat_sessions,
    rename_chat_session, delete_chat_session,
    archive_chat_session, unarchive_chat_session, list_archived_chat_sessions,
    save_chat_message, save_chat_tool_calls,
    get_session_params, set_session_params, new_chat_session,
    ChatSessionResponse,
};
#[allow(unused_imports)]
pub use streaming::{
    chat_completions_ipc, abort_llm_stream, cancel_tool_call,
    ChatChunk, open_chat_window,
};
#[allow(unused_imports)]
pub use hardware_fit::{
    get_model_hardware_fit, ModelHardwareFit,
};

// ── Shared helpers ────────────────────────────────────────────────────────────

use tauri::AppHandle;
use crate::AppState;

/// Persist the catalog to disk (called after state changes).
pub(super) fn save_catalog(app: &AppHandle, state: &AppState) {
    state.llm.catalog.save(&state.skill_dir);
    let _ = app; // suppress unused warning
}

/// Infer quant string from a GGUF filename.
///
/// Tries known quant substrings (e.g. `Q4_K_M`, `IQ3_XS`, `BF16`, `F16`, `F32`).
/// Falls back to `"unknown"` if nothing matches.
pub(super) fn infer_quant(filename: &str) -> String {
    let upper = filename.to_uppercase();
    let quants = [
        "IQ4_NL", "IQ4_XS",
        "IQ3_XXS", "IQ3_XS", "IQ3_M", "IQ3_S",
        "IQ2_XXS", "IQ2_XS", "IQ2_M", "IQ2_S",
        "Q6_K_L", "Q6_K",
        "Q5_K_L", "Q5_K_M", "Q5_K_S",
        "Q4_K_L", "Q4_K_M", "Q4_K_S",
        "Q4_0", "Q4_1",
        "Q3_K_XL", "Q3_K_L", "Q3_K_M", "Q3_K_S",
        "Q2_K_L", "Q2_K",
        "Q8_0", "Q8_1",
        "BF16", "F16", "F32",
    ];
    for q in &quants {
        if upper.contains(q) { return q.to_string(); }
    }
    "unknown".to_string()
}

/// Infer whether a filename is a multimodal projector.
pub(super) fn infer_is_mmproj(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.contains("mmproj") || lower.contains("mm-proj") || lower.contains("vision-proj")
}

/// Derive a family name from a repo string like `"bartowski/Qwen_Qwen3.5-4B-GGUF"`.
pub(super) fn infer_family_name(repo: &str) -> String {
    let name = repo.split('/').next_back().unwrap_or(repo);
    let name = name.strip_suffix("-GGUF").or_else(|| name.strip_suffix("-gguf")).unwrap_or(name);
    name.replace(['_', '-'], " ")
}

/// Derive a family_id from a repo string (lowercase, hyphenated).
pub(super) fn infer_family_id(repo: &str) -> String {
    let name = repo.split('/').next_back().unwrap_or(repo);
    let name = name.strip_suffix("-GGUF").or_else(|| name.strip_suffix("-gguf")).unwrap_or(name);
    name.to_lowercase().replace(' ', "-")
}

/// Ensure a single file is in the catalog, creating the entry if needed.
/// Returns `true` if the entry was newly created.
pub(super) fn ensure_catalog_entry(
    s:        &mut AppState,
    repo:     &str,
    filename: &str,
    size_gb:  Option<f32>,
    is_mmproj_override: Option<bool>,
) -> bool {
    if s.llm.catalog.entries.iter().any(|e| e.filename == filename) {
        return false;
    }

    let is_mmproj = is_mmproj_override.unwrap_or_else(|| infer_is_mmproj(filename));
    let quant     = infer_quant(filename);
    let family_id   = infer_family_id(repo);
    let family_name = infer_family_name(repo);

    let entry = super::catalog::LlmModelEntry {
        repo:        repo.to_string(),
        filename:    filename.to_string(),
        quant,
        size_gb:     size_gb.unwrap_or(0.0),
        description: if is_mmproj {
            format!("Vision projector from {repo}")
        } else {
            format!("Custom model from {repo}")
        },
        family_id,
        family_name,
        family_desc: String::new(),
        tags:        if is_mmproj { vec!["vision".into(), "multimodal".into()] }
                     else { vec!["chat".into()] },
        is_mmproj,
        recommended: false,
        advanced:    false,
        params_b:    0.0,
        max_context_length: 0,
        shard_files: Vec::new(),
        local_path:  None,
        state:       super::catalog::DownloadState::NotDownloaded,
        status_msg:  None,
        progress:    0.0,
        initiated_at_unix: None,
    };

    s.llm.catalog.entries.push(entry);
    true
}
