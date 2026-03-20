// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! In-memory log ring-buffer, per-session file sink, and push helpers.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use serde::Serialize;

use crate::event::LlmEventEmitter;

/// Current time as milliseconds since the Unix epoch.
pub fn unix_ts_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Log buffer ────────────────────────────────────────────────────────────────

/// One line in the LLM server log.
#[derive(Debug, Clone, Serialize)]
pub struct LlmLogEntry {
    /// Unix timestamp in milliseconds.
    pub ts:      u64,
    /// `"info"` | `"warn"` | `"error"`
    pub level:   String,
    /// Human-readable message.
    pub message: String,
}

/// Shared log ring-buffer (max [`LOG_CAP`] entries, oldest dropped first).
pub type LlmLogBuffer = Arc<Mutex<VecDeque<LlmLogEntry>>>;

/// Maximum number of log lines kept in memory.
const LOG_CAP: usize = skill_constants::LLM_LOG_CAP;

/// Create a new, empty log buffer.
pub fn new_log_buffer() -> LlmLogBuffer {
    Arc::new(Mutex::new(VecDeque::with_capacity(LOG_CAP)))
}

/// Optional file sink for LLM log lines.
///
/// `Arc<Mutex<…>>` so both `push_log` (called from any thread via macros)
/// and `run_actor` (which creates it) can hold a reference.
pub type LlmLogFile = Arc<Mutex<std::io::BufWriter<std::fs::File>>>;

/// Append a log entry to the in-memory buffer, emit a `llm:log` Tauri event,
/// and optionally write to the per-session log file.
pub fn push_log_inner(
    app:  &dyn LlmEventEmitter,
    buf:  &LlmLogBuffer,
    file: Option<&LlmLogFile>,
    level: &str,
    msg:   &str,
) {
    llm_log!("llm", "[{level}] {msg}");
    let ts    = unix_ts_ms();
    let entry = LlmLogEntry { ts, level: level.to_string(), message: msg.to_string() };

    { let mut q = buf.lock().expect("lock poisoned"); if q.len() >= LOG_CAP { q.pop_front(); } q.push_back(entry.clone()); }
    app.emit_event("llm:log", serde_json::to_value(&entry).unwrap_or_default());

    if let Some(f) = file {
        use std::io::Write;
        let dt = chrono_iso(ts);
        let _ = writeln!(f.lock().expect("lock poisoned"), "[{dt}] [{level:5}] {msg}");
    }
}

/// Convenience wrapper — no file sink (used from axum handlers / cmds).
pub fn push_log(app: &dyn LlmEventEmitter, buf: &LlmLogBuffer, level: &str, msg: &str) {
    push_log_inner(app, buf, None, level, msg);
}

/// Format a Unix-ms timestamp as `HH:MM:SS.mmm` (no libc/chrono dependency).
fn chrono_iso(ts_ms: u64) -> String {
    let total_s  = ts_ms / 1000;
    let ms       = ts_ms % 1000;
    let secs     = total_s % 60;
    let mins     = (total_s / 60) % 60;
    let hours    = (total_s / 3600) % 24;
    format!("{hours:02}:{mins:02}:{secs:02}.{ms:03}")
}
