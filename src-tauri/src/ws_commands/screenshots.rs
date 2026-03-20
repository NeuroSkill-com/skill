// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! WebSocket screenshot search commands.

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::AppStateExt;
use crate::MutexExt;

/// `search_screenshots` — search screenshots by OCR text (semantic or substring).
pub fn search_screenshots(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let query = msg.get("query")
        .and_then(|v| v.as_str())
        .ok_or("missing \"query\" field")?
        .to_owned();
    let k    = msg.get("k").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let mode = msg.get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("semantic")
        .to_owned();

    let (skill_dir, store) = {
        let st = app.app_state();
        let s  = st.lock_or_recover();
        (s.skill_dir.clone(), s.screenshot_store.clone())
    };

    let embedder = std::sync::Arc::clone(&*app.state::<std::sync::Arc<crate::EmbedderState>>());

    let store = store
        .or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))
        .ok_or("screenshot store not available")?;

    let results = match mode.as_str() {
        "substring" => crate::screenshot::search_by_ocr_text_like(&store, &query, k),
        _ => {
            let embed_fn = |text: &str| -> Option<Vec<f32>> {
                let mut guard = embedder.0.lock().ok()?;
                let te = guard.as_mut()?;
                let mut vecs = te.embed(vec![text], None).ok()?;
                if vecs.is_empty() { None } else { Some(vecs.remove(0)) }
            };
            crate::screenshot::search_by_ocr_text_embedding(&skill_dir, &store, &query, k, &embed_fn)
        }
    };

    Ok(serde_json::json!({
        "query":   query,
        "mode":    mode,
        "k":       k,
        "count":   results.len(),
        "results": results,
    }))
}

/// `screenshots_around` — find screenshots near a given unix timestamp.
pub fn screenshots_around(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let timestamp   = msg.get("timestamp")
        .and_then(|v| v.as_i64())
        .ok_or("missing \"timestamp\" field")?;
    let window_secs = msg.get("window_secs")
        .and_then(|v| v.as_i64())
        .unwrap_or(60) as i32;

    let (skill_dir, store) = {
        let st = app.app_state();
        let s  = st.lock_or_recover();
        (s.skill_dir.clone(), s.screenshot_store.clone())
    };

    let store = store
        .or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))
        .ok_or("screenshot store not available")?;

    let results = crate::screenshot::get_around(&store, timestamp, window_secs);

    Ok(serde_json::json!({
        "timestamp":   timestamp,
        "window_secs": window_secs,
        "count":       results.len(),
        "results":     results,
    }))
}
