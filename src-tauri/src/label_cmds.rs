// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Label persistence, fastembed text embeddings, and HNSW index commands.

use std::sync::Mutex;
use crate::MutexExt;
use tauri::{AppHandle, Emitter, Manager};

use crate::{AppState, unix_secs, save_settings_handle};
use crate::skill_log::SkillLogger;

// ── Label CRUD ─────────────────────────────────────────────────────────────────

/// List all labels, optionally filtered by time range.
/// If both timestamps are absent, all labels are returned.
#[tauri::command]
pub fn query_annotations(
    start_utc: Option<u64>,
    end_utc:   Option<u64>,
    state:     tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<serde_json::Value> {
    let s = state.lock_or_recover();
    let skill_dir = s.skill_dir.clone();
    drop(s);
    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() { return vec![]; }
    let conn = match rusqlite::Connection::open_with_flags(
        &labels_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) { Ok(c) => c, Err(_) => return vec![] };
    let query = if start_utc.is_some() && end_utc.is_some() {
        "SELECT id, eeg_start, eeg_end, label_start, label_end, text, context, created_at \
         FROM labels WHERE eeg_end >= ?1 AND eeg_start <= ?2 ORDER BY created_at DESC"
    } else {
        "SELECT id, eeg_start, eeg_end, label_start, label_end, text, context, created_at \
         FROM labels ORDER BY created_at DESC"
    };
    let mut stmt = match conn.prepare(query) { Ok(s) => s, Err(_) => return vec![] };
    let params: Vec<Box<dyn rusqlite::types::ToSql>> = if let (Some(s), Some(e)) = (start_utc, end_utc) {
        vec![Box::new(s as i64), Box::new(e as i64)]
    } else { vec![] };
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    stmt.query_map(param_refs.as_slice(), |row| {
        Ok(serde_json::json!({
            "id":          row.get::<_, i64>(0)?,
            "eeg_start":   row.get::<_, i64>(1)?,
            "eeg_end":     row.get::<_, i64>(2)?,
            "label_start": row.get::<_, i64>(3)?,
            "label_end":   row.get::<_, i64>(4)?,
            "text":        row.get::<_, String>(5)?,
            "context":     row.get::<_, Option<String>>(6)?.unwrap_or_default(),
            "created_at":  row.get::<_, i64>(7)?,
        }))
    }).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

#[tauri::command]
pub fn get_recent_labels(
    limit: Option<usize>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<String> {
    let s = state.lock_or_recover();
    let skill_dir = s.skill_dir.clone();
    drop(s);

    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() { return vec![]; }

    let conn = match rusqlite::Connection::open_with_flags(
        &labels_db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let max_rows = limit.unwrap_or(12).clamp(1, 100) as i64;
    let mut stmt = match conn.prepare(
        "SELECT text FROM labels
         WHERE length(trim(text)) > 0
         GROUP BY text
         ORDER BY MAX(created_at) DESC
         LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map(rusqlite::params![max_rows], |row| row.get::<_, String>(0))
        .map(|rows| {
            rows.flatten()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn delete_label(label_id: i64, state: tauri::State<'_, Mutex<Box<AppState>>>) -> Result<(), String> {
    let skill_dir = crate::skill_dir(&state);
    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() { return Err("labels db not found".into()); }
    let conn = rusqlite::Connection::open(&labels_db).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM labels WHERE id = ?1", rusqlite::params![label_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_label(
    label_id: i64, text: String, context: Option<String>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let text    = text.trim().to_owned();
    let context = context.unwrap_or_default().trim().to_owned();
    if text.is_empty() { return Err("label text is empty".into()); }
    let skill_dir = crate::skill_dir(&state);
    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() { return Err("labels db not found".into()); }
    let conn = rusqlite::Connection::open(&labels_db).map_err(|e| e.to_string())?;
    let n = conn.execute(
        "UPDATE labels SET text = ?1, context = ?2 WHERE id = ?3",
        rusqlite::params![text, context, label_id],
    ).map_err(|e| e.to_string())?;
    if n == 0 { return Err(format!("label {label_id} not found")); }
    Ok(())
}

#[tauri::command]
pub fn get_queue_stats(
    queue: tauri::State<'_, std::sync::Arc<crate::job_queue::JobQueue>>,
) -> serde_json::Value {
    queue.stats()
}

#[tauri::command]
pub fn submit_label(
    label_start_utc: u64,
    text:            String,
    context:         Option<String>,
    state:           tauri::State<'_, Mutex<Box<AppState>>>,
    embedder:        tauri::State<'_, std::sync::Arc<EmbedderState>>,
    label_idx:       tauri::State<'_, std::sync::Arc<crate::label_index::LabelIndexState>>,
    app:             AppHandle,
) -> Result<i64, String> {
    let text    = text.trim().to_owned();
    let context = context.unwrap_or_default().trim().to_owned();
    if text.is_empty() { return Err("label text is empty".into()); }
    let s = state.lock_or_recover();
    let now        = unix_secs();
    let skill_dir  = s.skill_dir.clone();
    let model_code = s.text_embedding_model.clone();
    match &s.label_store {
        Some(store) => {
            let id = store
                .insert(label_start_utc, now, label_start_utc, now, &text, &context, now)
                .ok_or_else(|| "db insert failed".to_string())?;
            drop(s);
            let _ = app.emit("label-created", serde_json::json!({
                "text": text.clone(), "context": context.clone(), "label_id": id,
            }));
            let embedder  = std::sync::Arc::clone(&embedder);
            let label_idx = std::sync::Arc::clone(&label_idx);
            let logger    = app.state::<std::sync::Arc<crate::skill_log::SkillLogger>>().inner().clone();
            // submit_label is a *sync* command — no Tokio runtime on this thread.
            std::thread::spawn(move || {
                embed_and_store_label(
                    id, text, context, model_code,
                    label_start_utc, now,
                    embedder, label_idx, skill_dir, logger,
                );
            });
            Ok(id)
        }
        None => Err("label store not available".into()),
    }
}

// ── Embedder state & helpers ───────────────────────────────────────────────────

/// Separate Tauri-managed state for the fastembed text embedder.
pub struct EmbedderState(pub std::sync::Mutex<Option<fastembed::TextEmbedding>>);

fn build_embedder(model_code: &str, skill_dir: &std::path::Path)
    -> Result<fastembed::TextEmbedding, String>
{
    use std::str::FromStr;
    let model = fastembed::EmbeddingModel::from_str(model_code)
        .map_err(|e| format!("unknown model {model_code}: {e}"))?;
    let cache = skill_dir.join("fastembed_cache");
    fastembed::TextEmbedding::try_new(
        fastembed::InitOptions::new(model)
            .with_cache_dir(cache)
            .with_show_download_progress(true),
    ).map_err(|e| e.to_string())
}

pub fn init_embedder(embedder: &EmbedderState, model_code: &str, skill_dir: &std::path::Path,
                     logger: &std::sync::Arc<crate::skill_log::SkillLogger>) {
    match build_embedder(model_code, skill_dir) {
        Ok(te) => { *embedder.0.lock_or_recover() = Some(te); }
        Err(e) => skill_log!(logger, "embedder", "init failed for {model_code}: {e}"),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn embed_and_store_label(
    label_id:    i64,
    text:        String,
    context:     String,
    model_code:  String,
    eeg_start:   u64,
    eeg_end:     u64,
    embedder:    std::sync::Arc<EmbedderState>,
    label_idx:   std::sync::Arc<crate::label_index::LabelIndexState>,
    skill_dir:   std::path::PathBuf,
    logger:      std::sync::Arc<crate::skill_log::SkillLogger>,
) {
    let mut guard = embedder.0.lock_or_recover();
    let Some(te) = guard.as_mut() else { return };

    let has_ctx = !context.trim().is_empty();
    let inputs: Vec<&str> = if has_ctx {
        vec![text.as_str(), context.as_str()]
    } else {
        vec![text.as_str()]
    };

    match te.embed(inputs, None) {
        Ok(mut vecs) => {
            let text_emb    = vecs.remove(0);
            let context_emb = if has_ctx { vecs.remove(0) } else { vec![] };
            drop(guard);

            if let Some(store) = crate::label_store::LabelStore::open(&skill_dir) {
                store.update_embeddings(label_id, &text_emb, &context_emb, &model_code);
            } else {
                skill_log!(logger, "embedder", "could not open label store at {}", skill_dir.display());
            }
            crate::label_index::insert_label(
                &skill_dir, label_id, &text_emb, &context_emb, eeg_start, eeg_end, &label_idx,
            );
        }
        Err(e) => skill_log!(logger, "embedder", "embed failed for label {label_id}: {e}"),
    }
}

// ── Embedding model commands ───────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct EmbedModelInfo {
    pub code:        String,
    pub dim:         usize,
    pub description: String,
}

#[tauri::command]
pub fn list_embedding_models() -> Vec<EmbedModelInfo> {
    fastembed::TextEmbedding::list_supported_models()
        .into_iter()
        .map(|m| EmbedModelInfo {
            code:        m.model_code,
            dim:         m.dim,
            description: m.description,
        })
        .collect()
}

#[tauri::command]
pub fn get_embedding_model(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().text_embedding_model.clone()
}

#[tauri::command]
pub async fn set_embedding_model(
    model_code: String,
    state:      tauri::State<'_, Mutex<Box<AppState>>>,
    embedder:   tauri::State<'_, std::sync::Arc<EmbedderState>>,
    app:        AppHandle,
) -> Result<(), String> {
    use std::str::FromStr;
    fastembed::EmbeddingModel::from_str(&model_code)
        .map_err(|e| format!("unknown model: {e}"))?;
    {
        let mut s = state.lock_or_recover();
        s.text_embedding_model = model_code.clone();
    }
    save_settings_handle(&app);

    let skill_dir = crate::skill_dir(&state);
    let embedder  = std::sync::Arc::clone(&embedder);
    let mc2 = model_code.clone();
    let sd2 = skill_dir.clone();
    let logger = app.state::<std::sync::Arc<SkillLogger>>().inner().clone();
    tokio::task::spawn_blocking(move || {
        init_embedder(&embedder, &model_code, &skill_dir, &logger);
        let mut guard = embedder.0.lock_or_recover();
        let Some(te) = guard.as_mut() else { return };
        let Some(store) = crate::label_store::LabelStore::open(&sd2) else { return };
        let rows = store.rows_needing_embed(&mc2);
        if rows.is_empty() { return; }
        const BATCH: usize = 32;
        for chunk in rows.chunks(BATCH) {
            let mut inputs:  Vec<&str>          = Vec::with_capacity(chunk.len() * 2);
            let mut idx_map: Vec<(usize, bool)> = Vec::with_capacity(chunk.len());
            for (_, text, ctx) in chunk {
                let start = inputs.len();
                inputs.push(text.as_str());
                let has_ctx = !ctx.trim().is_empty();
                if has_ctx { inputs.push(ctx.as_str()); }
                idx_map.push((start, has_ctx));
            }
            if let Ok(embeddings) = te.embed(inputs, None) {
                for ((id, _, _), (start, has_ctx)) in chunk.iter().zip(idx_map.iter()) {
                    let text_emb    = &embeddings[*start];
                    let context_emb = if *has_ctx { &embeddings[*start + 1] } else { &[][..] };
                    store.update_embeddings(*id, text_emb, context_emb, &mc2);
                }
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_stale_label_count(state: tauri::State<'_, Mutex<Box<AppState>>>) -> usize {
    let s = state.lock_or_recover();
    let model_code = s.text_embedding_model.clone();
    let skill_dir  = s.skill_dir.clone();
    drop(s);
    crate::label_store::LabelStore::open(&skill_dir)
        .map(|store| store.rows_needing_embed(&model_code).len())
        .unwrap_or(0)
}

#[tauri::command]
pub async fn rebuild_label_index(
    state:     tauri::State<'_, Mutex<Box<AppState>>>,
    label_idx: tauri::State<'_, std::sync::Arc<crate::label_index::LabelIndexState>>,
) -> Result<crate::label_index::RebuildStats, String> {
    let skill_dir = crate::skill_dir(&state);
    let label_idx = std::sync::Arc::clone(&label_idx);
    tokio::task::spawn_blocking(move || Ok(crate::label_index::rebuild(&skill_dir, &label_idx)))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn search_labels_by_text(
    query:     String,
    k:         usize,
    state:     tauri::State<'_, Mutex<Box<AppState>>>,
    embedder:  tauri::State<'_, std::sync::Arc<EmbedderState>>,
    label_idx: tauri::State<'_, std::sync::Arc<crate::label_index::LabelIndexState>>,
) -> Result<Vec<crate::label_index::LabelNeighbor>, String> {
    let skill_dir = crate::skill_dir(&state);
    let embedder  = std::sync::Arc::clone(&embedder);
    let label_idx = std::sync::Arc::clone(&label_idx);
    let ef = (k * 4).max(64);
    tokio::task::spawn_blocking(move || {
        let mut guard = embedder.0.lock_or_recover();
        let te = guard.as_mut().ok_or("embedder not initialized")?;
        let mut vecs = te.embed(vec![query.as_str()], None).map_err(|e| e.to_string())?;
        let query_vec = vecs.remove(0);
        Ok(crate::label_index::search_by_text_vec(&query_vec, k, ef, &skill_dir, &label_idx))
    })
    .await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn search_labels_by_eeg(
    eeg_embedding: Vec<f32>,
    k:             usize,
    state:         tauri::State<'_, Mutex<Box<AppState>>>,
    label_idx:     tauri::State<'_, std::sync::Arc<crate::label_index::LabelIndexState>>,
) -> Result<Vec<crate::label_index::LabelNeighbor>, String> {
    let skill_dir = crate::skill_dir(&state);
    let label_idx = std::sync::Arc::clone(&label_idx);
    let ef = (k * 4).max(64);
    tokio::task::spawn_blocking(move || {
        Ok(crate::label_index::search_by_eeg_vec(&eeg_embedding, k, ef, &skill_dir, &label_idx))
    })
    .await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn reembed_all_labels(
    state:     tauri::State<'_, Mutex<Box<AppState>>>,
    embedder:  tauri::State<'_, std::sync::Arc<EmbedderState>>,
    label_idx: tauri::State<'_, std::sync::Arc<crate::label_index::LabelIndexState>>,
    app:       AppHandle,
) -> Result<(), String> {
    let (skill_dir, model_code) = crate::read_state(&state,
        |s| (s.skill_dir.clone(), s.text_embedding_model.clone()));
    let embedder   = std::sync::Arc::clone(&embedder);
    let label_idx  = std::sync::Arc::clone(&label_idx);

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let store = crate::label_store::LabelStore::open(&skill_dir)
            .ok_or("could not open label store")?;
        let rows  = store.all_rows_for_embed();
        let total = rows.len();
        let _ = app.emit("embed-progress", serde_json::json!({ "done": 0, "total": total }));

        let mut guard = embedder.0.lock_or_recover();
        let te = guard.as_mut().ok_or("embedder not initialized")?;

        const BATCH: usize = 32;
        let mut done = 0usize;
        for chunk in rows.chunks(BATCH) {
            let ids:      Vec<i64>  = chunk.iter().map(|(id,_,_)| *id).collect();
            let texts:    Vec<&str> = chunk.iter().map(|(_, t, _)| t.as_str()).collect();
            let contexts: Vec<&str> = chunk.iter().map(|(_, _, c)| c.as_str()).collect();
            let mut inputs:  Vec<&str>          = Vec::with_capacity(chunk.len() * 2);
            let mut idx_map: Vec<(usize, bool)> = Vec::with_capacity(chunk.len());
            for i in 0..chunk.len() {
                let embed_start = inputs.len();
                inputs.push(texts[i]);
                let has_ctx = !contexts[i].trim().is_empty();
                if has_ctx { inputs.push(contexts[i]); }
                idx_map.push((embed_start, has_ctx));
            }
            let embeddings = te.embed(inputs, None).map_err(|e| e.to_string())?;
            for (label_id, (embed_start, has_ctx)) in ids.iter().zip(idx_map.iter()) {
                let text_emb    = &embeddings[*embed_start];
                let context_emb = if *has_ctx { &embeddings[*embed_start + 1] } else { &[][..] };
                store.update_embeddings(*label_id, text_emb, context_emb, &model_code);
            }
            done += chunk.len();
            let _ = app.emit("embed-progress", serde_json::json!({ "done": done, "total": total }));
        }
        crate::label_index::rebuild(&skill_dir, &label_idx);
        Ok(())
    })
    .await.map_err(|e| e.to_string())?
}
