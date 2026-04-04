#![allow(unused_imports)]
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tauri commands for the LLM subsystem (UI proxy surface).

mod catalog;
mod downloads;
mod hardware_fit;
mod selection;
mod server;
mod streaming;

pub use catalog::{
    add_llm_model, get_llm_catalog, get_llm_downloads, refresh_llm_catalog, LlmDownloadItem,
};
pub use downloads::{
    cancel_llm_download, delete_llm_model, download_llm_model, open_downloads_window,
    pause_llm_download, resume_llm_download,
};
pub use hardware_fit::{get_model_hardware_fit, ModelHardwareFit};
pub use selection::{set_llm_active_mmproj, set_llm_active_model, set_llm_autoload_mmproj};
pub use server::{
    get_llm_logs, get_llm_server_status, start_llm_server, stop_llm_server, switch_llm_mmproj,
    switch_llm_model, LlmServerStatusResponse,
};
pub use streaming::{
    abort_llm_stream, cancel_tool_call, chat_completions_ipc, open_chat_window, ChatChunk,
};
