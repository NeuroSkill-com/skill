// SPDX-License-Identifier: GPL-3.0-only
//! Label commands — all delegated to the daemon HTTP API.

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbedQueueStats {
    pub pending: usize,
    pub processing: bool,
}

#[tauri::command]
pub fn get_queue_stats() -> EmbedQueueStats {
    EmbedQueueStats {
        pending: 0,
        processing: false,
    }
}

#[tauri::command]
pub async fn rebuild_label_index() -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(|| {
        crate::daemon_cmds::daemon_post("/v1/labels/index/rebuild", &serde_json::json!({}))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn search_labels_by_eeg(
    start_utc: u64,
    end_utc: u64,
    k: Option<u64>,
) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({
        "start_utc": start_utc,
        "end_utc": end_utc,
        "k": k.unwrap_or(10),
    });
    tokio::task::spawn_blocking(move || {
        crate::daemon_cmds::daemon_post("/v1/labels/search-by-eeg", &body)
    })
    .await
    .map_err(|e| e.to_string())?
}
