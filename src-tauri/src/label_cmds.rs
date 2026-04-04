// SPDX-License-Identifier: GPL-3.0-only
//! Label commands — all delegated to daemon.

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbedQueueStats {
    pub pending: usize,
    pub processing: bool,
}

// ── Label CRUD (daemon-backed) ────────────────────────────────────────────────

#[tauri::command]
pub fn get_queue_stats() -> EmbedQueueStats {
    EmbedQueueStats {
        pending: 0,
        processing: false,
    }
}

#[tauri::command]
pub async fn rebuild_label_index() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn search_labels_by_eeg(
    _start_utc: u64,
    _end_utc: u64,
    _k: Option<u64>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "results": [] }))
}
