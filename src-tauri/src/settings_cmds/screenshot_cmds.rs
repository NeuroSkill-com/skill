// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Screenshot configuration and search Tauri commands.

use std::sync::Mutex;
use crate::MutexExt;
use tauri::AppHandle;

use crate::{AppState, skill_dir};

// ── Screenshot config ─────────────────────────────────────────────────────────

/// Get current screenshot configuration.
#[tauri::command]
pub fn get_screenshot_config(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> crate::settings::ScreenshotConfig {
    state.lock_or_recover().screenshot_config.clone()
}

/// Update screenshot configuration.  Returns whether the embedding model
/// changed (so the frontend can prompt re-embedding).
#[tauri::command]
pub fn set_screenshot_config(
    config: crate::settings::ScreenshotConfig,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> skill_data::screenshot_store::ConfigChangeResult {
    let (old_backend, old_model, skill_dir) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.embed_backend.clone(),
         g.screenshot_config.model_id(),
         g.skill_dir.clone())
    };

    let new_backend = config.embed_backend.clone();
    let new_model = config.model_id();
    let model_changed = old_backend != new_backend || old_model != new_model;

    {
        let mut g = state.lock_or_recover();
        g.screenshot_config = config;
    }
    crate::save_settings(&app);

    let stale_count = if model_changed {
        skill_data::screenshot_store::ScreenshotStore::open(&skill_dir)
            .map(|s| s.count_stale(&new_backend, &new_model))
            .unwrap_or(0)
    } else {
        0
    };

    skill_data::screenshot_store::ConfigChangeResult { model_changed, stale_count }
}

/// Count screenshots needing (re-)embedding and estimate wall-clock time.
/// Runs on a background thread to avoid blocking the UI.
#[tauri::command]
pub async fn estimate_screenshot_reembed(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Option<skill_data::screenshot_store::ReembedEstimate>, String> {
    let (config, skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.clone(), g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))?;
        Some(crate::screenshot::estimate_reembed(&store, &config, &skill_dir))
    }).await.unwrap_or(None))
}

/// Re-embed all screenshots with the current model.
/// Emits `screenshot-reembed-progress` events.
/// Runs on a background thread to avoid blocking the UI.
#[tauri::command]
pub async fn rebuild_screenshot_embeddings(
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Option<skill_data::screenshot_store::ReembedResult>, String> {
    let (config, skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.clone(), g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))?;
        let ctx = crate::screenshot::TauriScreenshotContext { app };
        Some(crate::screenshot::rebuild_embeddings(&store, &config, &skill_dir, &ctx))
    }).await.unwrap_or(None))
}

/// Find screenshots by timestamp range (for EEG correlation).
#[tauri::command]
pub async fn get_screenshots_around(
    timestamp: i64,
    window_secs: i32,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<skill_data::screenshot_store::ScreenshotResult>, String> {
    let (skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = match store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
            Some(s) => s,
            None => return vec![],
        };
        crate::screenshot::get_around(&store, timestamp, window_secs)
    }).await.unwrap_or_default())
}

/// Find screenshots visually similar to a query image.
/// Embeds the query image with the current model, then searches HNSW.
/// Runs on a background thread (model loading + inference is heavy).
#[tauri::command]
pub async fn search_screenshots_by_image(
    image_bytes: Vec<u8>,
    k: usize,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<skill_data::screenshot_store::ScreenshotResult>, String> {
    let (config, skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.clone(), g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let mut encoder = crate::screenshot::load_fastembed_image_pub(&config, &skill_dir);
        let query_emb = if let Some(ref mut fe) = encoder {
            crate::screenshot::fastembed_embed_pub(fe, &image_bytes)
        } else {
            None
        };
        let query = match query_emb {
            Some(v) => v,
            None => return vec![],
        };
        let store = match store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
            Some(s) => s,
            None => return vec![],
        };
        let hnsw_path = skill_dir.join(crate::constants::SCREENSHOTS_HNSW);
        let hnsw = match fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(&hnsw_path, fast_hnsw::distance::Cosine) {
            Ok(idx) => idx,
            Err(_) => return vec![],
        };
        crate::screenshot::search_by_vector(&hnsw, &store, &query, k)
    }).await.unwrap_or_default())
}

/// Get screenshot pipeline metrics (capture + embed thread performance).
/// Lightweight — just reads atomics, no spawn_blocking needed.
#[tauri::command]
pub fn get_screenshot_metrics(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> crate::screenshot::MetricsSnapshot {
    let metrics = state.lock_or_recover().screenshot_metrics.clone();
    metrics.snapshot()
}

/// Check whether OCR models are downloaded and ready.
#[tauri::command]
pub fn check_ocr_models_ready(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> bool {
    let skill_dir = skill_dir(&state);
    let ocr_dir = skill_dir.join("ocr_models");
    ocr_dir.join(crate::constants::OCR_DETECTION_MODEL_FILE).exists()
        && ocr_dir.join(crate::constants::OCR_RECOGNITION_MODEL_FILE).exists()
}

/// Download OCR models (text-detection.rten + text-recognition.rten).
/// Returns true if both models are now available.
/// Runs on a background thread (network download).
#[tauri::command]
pub async fn download_ocr_models(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<bool, String> {
    let skill_dir = skill_dir(&state);
    Ok(tokio::task::spawn_blocking(move || {
        let ocr_dir = skill_dir.join("ocr_models");
        let _ = std::fs::create_dir_all(&ocr_dir);
        let det_path = ocr_dir.join(crate::constants::OCR_DETECTION_MODEL_FILE);
        let rec_path = ocr_dir.join(crate::constants::OCR_RECOGNITION_MODEL_FILE);
        let det_ok = crate::screenshot::download_ocr_model_pub(
            crate::constants::OCR_DETECTION_MODEL_URL, &det_path,
        );
        let rec_ok = crate::screenshot::download_ocr_model_pub(
            crate::constants::OCR_RECOGNITION_MODEL_URL, &rec_path,
        );
        det_ok && rec_ok
    }).await.unwrap_or(false))
}

/// Search screenshots by OCR text — both semantic (embedding similarity)
/// and substring (SQL LIKE) modes.
/// `mode`: "semantic" (default) uses text embedding HNSW search,
///         "substring" uses SQL LIKE matching.
/// Runs on a background thread (semantic mode loads an embedding model).
#[tauri::command]
pub async fn search_screenshots_by_text(
    query: String,
    k: Option<usize>,
    mode: Option<String>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
    embedder: tauri::State<'_, std::sync::Arc<crate::EmbedderState>>,
) -> Result<Vec<skill_data::screenshot_store::ScreenshotResult>, String> {
    let (skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.screenshot_store.clone())
    };
    let embedder = std::sync::Arc::clone(&embedder);
    Ok(tokio::task::spawn_blocking(move || {
        let store = match store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
            Some(s) => s,
            None => return vec![],
        };
        let k = k.unwrap_or(20);
        let mode = mode.unwrap_or_else(|| "semantic".into());
        match mode.as_str() {
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
        }
    }).await.unwrap_or_default())
}

/// Return the screenshots directory path and the WebSocket server port.
/// The frontend constructs image URLs as `http://127.0.0.1:{port}/screenshots/{filename}`
/// which are served by the axum HTTP server — no asset protocol scope needed.
#[tauri::command]
pub fn get_screenshots_dir(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> (String, u16) {
    let g = state.lock_or_recover();
    let dir = g.skill_dir
        .join(crate::constants::SCREENSHOTS_DIR)
        .to_string_lossy()
        .into_owned();
    let port = g.ws_port;
    (dir, port)
}

/// Find screenshots visually similar to a query embedding vector.
/// Runs on a background thread (HNSW load + search).
#[tauri::command]
pub async fn search_screenshots_by_vector(
    vector: Vec<f32>,
    k: usize,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<skill_data::screenshot_store::ScreenshotResult>, String> {
    let (skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = match store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
            Some(s) => s,
            None => return vec![],
        };
        let hnsw_path = skill_dir.join(crate::constants::SCREENSHOTS_HNSW);
        let hnsw = match fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(&hnsw_path, fast_hnsw::distance::Cosine) {
            Ok(idx) => idx,
            Err(_) => return vec![],
        };
        crate::screenshot::search_by_vector(&hnsw, &store, &vector, k)
    }).await.unwrap_or_default())
}

