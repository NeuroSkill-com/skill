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

/// `search_screenshots_by_image_b64` — search screenshots by image (base64-encoded).
///
/// The client sends a base64-encoded image; the server decodes it, embeds it
/// via the CLIP vision model, then searches the `screenshots.hnsw` index.
///
/// **Required**: `image_b64` (base64-encoded image bytes).
/// **Optional**: `k` (default 20).
pub fn search_screenshots_by_image_b64(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let b64 = msg.get("image_b64")
        .and_then(|v| v.as_str())
        .ok_or("missing \"image_b64\" field (base64-encoded image)")?;
    let k = msg.get("k").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    use base64::Engine;
    let image_bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("invalid base64: {e}"))?;

    let (config, skill_dir, store) = {
        let st = app.app_state();
        let s  = st.lock_or_recover();
        (s.screenshot_config.clone(), s.skill_dir.clone(), s.screenshot_store.clone())
    };

    let store = store
        .or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))
        .ok_or("screenshot store not available")?;

    // Embed the image via CLIP
    let mut encoder = crate::screenshot::load_fastembed_image_pub(&config, &skill_dir);
    let query_emb = encoder.as_mut()
        .and_then(|fe| crate::screenshot::fastembed_embed_pub(fe, &image_bytes))
        .ok_or("CLIP vision model not available — check Settings → Screenshots")?;

    // Search HNSW
    let hnsw_path = skill_dir.join(skill_constants::SCREENSHOTS_HNSW);
    let hnsw = fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(
        &hnsw_path, fast_hnsw::distance::Cosine,
    ).map_err(|e| format!("CLIP HNSW not available: {e}"))?;

    let results = crate::screenshot::search_by_vector(&hnsw, &store, &query_emb, k);

    Ok(serde_json::json!({
        "mode":    "vision",
        "k":       k,
        "count":   results.len(),
        "results": results,
    }))
}

/// `search_screenshots_vision` — search screenshots by CLIP vision embedding vector.
///
/// Searches the `screenshots.hnsw` index (CLIP vision embeddings) for the
/// `k` nearest screenshots to the given query vector.  This is the WS
/// counterpart to the Tauri `search_screenshots_by_vector` command.
///
/// **Required**: `vector` (array of f32).
/// **Optional**: `k` (default 20).
pub fn search_screenshots_vision(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let vector: Vec<f32> = msg.get("vector")
        .and_then(|v| v.as_array())
        .ok_or("missing \"vector\" field (array of floats)")?
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();
    if vector.is_empty() {
        return Err("\"vector\" must be a non-empty array of floats".into());
    }
    let k = msg.get("k").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let (skill_dir, store) = {
        let st = app.app_state();
        let s  = st.lock_or_recover();
        (s.skill_dir.clone(), s.screenshot_store.clone())
    };

    let store = store
        .or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))
        .ok_or("screenshot store not available")?;

    let hnsw_path = skill_dir.join(skill_constants::SCREENSHOTS_HNSW);
    let hnsw = fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(
        &hnsw_path, fast_hnsw::distance::Cosine,
    ).map_err(|e| format!("CLIP HNSW not available: {e}"))?;

    let results = crate::screenshot::search_by_vector(&hnsw, &store, &vector, k);

    Ok(serde_json::json!({
        "mode":    "vision",
        "k":       k,
        "count":   results.len(),
        "results": results,
    }))
}

/// `screenshots_for_eeg` — find screenshots that were captured near EEG embedding timestamps.
///
/// Given an EEG time range `[start_utc, end_utc]`, finds all embedding timestamps
/// in that range, then for each one returns any screenshots within `window_secs`
/// (default 30s).  This is the "EEG → screenshots" cross-modal bridge.
///
/// **Required**: `start_utc`, `end_utc`.
/// **Optional**: `window_secs` (default 30), `limit` (max results, default 50).
pub fn screenshots_for_eeg(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start_utc = msg.get("start_utc").and_then(|v| v.as_u64())
        .ok_or("missing \"start_utc\" field")?;
    let end_utc = msg.get("end_utc").and_then(|v| v.as_u64())
        .ok_or("missing \"end_utc\" field")?;
    let window_secs = msg.get("window_secs").and_then(|v| v.as_i64()).unwrap_or(30) as i32;
    let limit = msg.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

    let (skill_dir, store) = {
        let st = app.app_state();
        let s  = st.lock_or_recover();
        (s.skill_dir.clone(), s.screenshot_store.clone())
    };

    let store = store
        .or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))
        .ok_or("screenshot store not available")?;

    // Get EEG embedding timestamps in the range
    let start_ts = skill_commands::unix_to_ts(start_utc);
    let end_ts   = skill_commands::unix_to_ts(end_utc);
    let date_dirs = skill_commands::list_date_dirs(&skill_dir);

    let mut eeg_timestamps: Vec<u64> = Vec::new();
    for (_date, dir) in &date_dirs {
        let db_path = dir.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() { continue; }
        if let Ok(conn) = skill_data::util::open_readonly(&db_path) {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT timestamp FROM embeddings WHERE timestamp BETWEEN ?1 AND ?2 ORDER BY timestamp"
            ) {
                let _ = stmt.query_map(rusqlite::params![start_ts, end_ts], |row| {
                    let ts: i64 = row.get(0)?;
                    Ok(skill_commands::ts_to_unix(ts))
                }).map(|rows| {
                    for row in rows.flatten() {
                        eeg_timestamps.push(row);
                    }
                });
            }
        }
    }

    // Deduplicate and find screenshots near each EEG timestamp
    eeg_timestamps.dedup();
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut seen_files: std::collections::HashSet<String> = std::collections::HashSet::new();

    for eeg_ts in &eeg_timestamps {
        let screenshots = crate::screenshot::get_around(&store, *eeg_ts as i64, window_secs);
        for ss in screenshots {
            if seen_files.insert(ss.filename.clone()) {
                results.push(serde_json::json!({
                    "eeg_timestamp_utc": eeg_ts,
                    "screenshot":        ss,
                }));
                if results.len() >= limit { break; }
            }
        }
        if results.len() >= limit { break; }
    }

    Ok(serde_json::json!({
        "start_utc":   start_utc,
        "end_utc":     end_utc,
        "window_secs": window_secs,
        "eeg_count":   eeg_timestamps.len(),
        "count":       results.len(),
        "results":     results,
    }))
}

/// `eeg_for_screenshots` — find EEG embeddings near screenshot timestamps.
///
/// Given a screenshot search query (OCR text), finds matching screenshots,
/// then for each screenshot timestamp finds the nearest EEG embeddings and
/// their associated labels.  This is the "screenshots → EEG" cross-modal bridge.
///
/// **Required**: `query` (OCR text to search for).
/// **Optional**: `k` (screenshot results, default 10), `window_secs` (EEG
/// temporal window around each screenshot, default 60), `mode` (semantic/substring,
/// default semantic).
pub fn eeg_for_screenshots(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let query = msg.get("query")
        .and_then(|v| v.as_str())
        .ok_or("missing \"query\" field")?
        .to_owned();
    let k = msg.get("k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let window_secs = msg.get("window_secs").and_then(|v| v.as_u64()).unwrap_or(60);
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

    // Step 1: Find screenshots matching the query
    let screenshots = match mode.as_str() {
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

    // Step 2: For each screenshot, find EEG embeddings and labels near its timestamp
    let labels_db = skill_dir.join(skill_constants::LABELS_FILE);
    let mut results: Vec<serde_json::Value> = Vec::new();

    for ss in &screenshots {
        let ss_unix = ss.unix_ts;

        // Find labels near this screenshot
        let labels = if labels_db.exists() {
            skill_commands::get_labels_near(&labels_db, ss_unix, window_secs)
        } else {
            vec![]
        };

        // Find the session that contains this timestamp
        let date_str = {
            // Derive YYYYMMDD from unix timestamp
            let z = (ss_unix / 86400) as i64 + 719_468;
            let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
            let doe = z - era * 146_097;
            let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
            let y   = yoe + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp  = (5 * doy + 2) / 153;
            let d   = doy - (153 * mp + 2) / 5 + 1;
            let mo  = if mp < 10 { mp + 3 } else { mp - 9 };
            let yr  = if mo <= 2 { y + 1 } else { y };
            format!("{yr:04}{mo:02}{d:02}")
        };

        let session_ref = skill_commands::find_session_for_timestamp_in(
            &skill_dir, ss_unix, &date_str,
        );

        results.push(serde_json::json!({
            "screenshot": ss,
            "labels":     labels,
            "session":    session_ref,
        }));
    }

    Ok(serde_json::json!({
        "query":   query,
        "mode":    mode,
        "k":       k,
        "window_secs": window_secs,
        "screenshot_count": screenshots.len(),
        "count":   results.len(),
        "results": results,
    }))
}
