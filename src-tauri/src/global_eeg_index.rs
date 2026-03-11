// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Persistent cross-day HNSW index for EEG embeddings.
//!
//! ## Storage
//!
//! ```text
//! ~/.skill/
//!   eeg_global.hnsw     ← single flat index across all days
//!   20260223/
//!     eeg_embeddings.hnsw  ← per-day index (still maintained)
//!     eeg.sqlite
//!   20260224/
//!     …
//! ```
//!
//! Every node stores the `YYYYMMDDHHmmss` timestamp (i64) as its HNSW
//! payload.  During search hydration the date is derived from the timestamp
//! (`ts / 1_000_000 → YYYYMMDD`) so the right daily `eeg.sqlite` can be
//! opened without a separate metadata store.
//!
//! ## Lifecycle
//!
//! 1. **Startup** — [`load_or_build`] is called from a background thread.
//!    If `eeg_global.hnsw` exists it is loaded directly; otherwise every
//!    daily `eeg.sqlite` is scanned and a fresh index is built and saved.
//!
//! 2. **Live session** — the embed worker calls [`GlobalEegIndex::insert`]
//!    after each successful daily insertion.  The file is re-saved every
//!    [`GLOBAL_HNSW_SAVE_EVERY`] insertions (see constants.rs).
//!
//! 3. **On demand** — [`rebuild_global_eeg_index`] (Tauri command) triggers
//!    a full rebuild, e.g. after importing data from another machine.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use fast_hnsw::{Builder, distance::Cosine, labeled::LabeledIndex};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use crate::MutexExt;
use crate::constants::{GLOBAL_HNSW_FILE, HNSW_EF_CONSTRUCTION, HNSW_M, SQLITE_FILE};

// ── Managed state ─────────────────────────────────────────────────────────────

/// Tauri managed state for the persistent cross-day EEG HNSW index.
///
/// The inner `Option` is `None` while the startup load / build is still
/// running (background thread), and `Some` once the index is ready.  All
/// callers must tolerate `None` and fall back to per-day index loading.
///
/// Wrapped in `Arc` so the embed-worker thread can hold its own reference
/// without going through Tauri's managed-state system.
pub struct GlobalEegIndex(pub Arc<Mutex<Option<LabeledIndex<Cosine, i64>>>>);

impl GlobalEegIndex {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }

    /// Clone the inner `Arc` so the embed worker (or other background threads)
    /// can hold a reference independently of Tauri's state lifetime.
    pub fn arc(&self) -> Arc<Mutex<Option<LabeledIndex<Cosine, i64>>>> {
        Arc::clone(&self.0)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn fresh_index() -> LabeledIndex<Cosine, i64> {
    Builder::new()
        .m(HNSW_M)
        .ef_construction(HNSW_EF_CONSTRUCTION)
        .build_labeled(Cosine)
}

fn index_path(skill_dir: &Path) -> PathBuf {
    skill_dir.join(GLOBAL_HNSW_FILE)
}

/// List all valid `YYYYMMDD` sub-directories under `skill_dir`, oldest first.
fn date_dirs(skill_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(skill_dir) else { return out };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.len() == 8
            && name.bytes().all(|b| b.is_ascii_digit())
            && entry.path().is_dir()
        {
            out.push((name, entry.path()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Load the global index from disk if it exists, otherwise build it from
/// scratch by scanning all daily `eeg.sqlite` files.
///
/// **Blocking** — always call from a dedicated background thread.
pub fn load_or_build(skill_dir: &Path) -> LabeledIndex<Cosine, i64> {
    let path = index_path(skill_dir);
    if path.exists() {
        match LabeledIndex::load(&path, Cosine) {
            Ok(idx) => {
                eprintln!(
                    "[global_idx] loaded {} embeddings ← {}",
                    idx.len(),
                    path.display()
                );
                return idx;
            }
            Err(e) => {
                eprintln!("[global_idx] load failed ({e}) — rebuilding from scratch");
            }
        }
    }
    rebuild_from_scratch(skill_dir)
}

/// Scan every daily `eeg.sqlite`, re-insert all embeddings into a fresh
/// global HNSW, persist it, and return the new index.
///
/// **Blocking** — may take several seconds for large datasets.
pub fn rebuild_from_scratch(skill_dir: &Path) -> LabeledIndex<Cosine, i64> {
    eprintln!("[global_idx] rebuilding cross-day HNSW from all daily SQLite files…");
    let mut idx = fresh_index();
    let mut total_embeddings = 0usize;
    let mut days_scanned     = 0usize;

    for (_date, dir) in date_dirs(skill_dir) {
        let db_path = dir.join(SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        days_scanned += 1;

        let Ok(conn) = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) else {
            eprintln!("[global_idx] could not open {}", db_path.display());
            continue;
        };

        let Ok(mut stmt) = conn.prepare(
            "SELECT timestamp, eeg_embedding \
             FROM embeddings \
             WHERE eeg_embedding IS NOT NULL \
             ORDER BY timestamp ASC",
        ) else {
            eprintln!("[global_idx] prepare failed for {}", db_path.display());
            continue;
        };

        let rows: Vec<(i64, Vec<u8>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map(|r| r.flatten().collect())
            .unwrap_or_default();

        for (ts, blob) in rows {
            if blob.is_empty() {
                continue;
            }
            let vec: Vec<f32> = blob
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            if vec.is_empty() {
                continue;
            }
            idx.insert(vec, ts);
            total_embeddings += 1;
        }
    }

    eprintln!(
        "[global_idx] built index: {} embeddings from {} day(s)",
        total_embeddings, days_scanned
    );

    let path = index_path(skill_dir);
    if let Err(e) = idx.save(&path) {
        eprintln!("[global_idx] save failed: {e}");
    } else {
        eprintln!("[global_idx] saved → {}", path.display());
    }
    idx
}

/// Persist `idx` to `~/.skill/eeg_global.hnsw`.
/// Called by the embed worker every `GLOBAL_HNSW_SAVE_EVERY` insertions.
pub fn save_index(idx: &LabeledIndex<Cosine, i64>, skill_dir: &Path) {
    let path = index_path(skill_dir);
    if let Err(e) = idx.save(&path) {
        eprintln!("[global_idx] periodic save failed: {e}");
    }
}

// ── Stats ─────────────────────────────────────────────────────────────────────

/// Statistics about the global HNSW index returned to the frontend.
#[derive(Debug, Serialize, Clone)]
pub struct GlobalIndexStats {
    /// Number of embeddings currently in the index.
    pub total_embeddings: usize,
    /// Size of the on-disk file in bytes (0 if not yet saved).
    pub file_size_bytes:  u64,
    /// Absolute path of the global HNSW file.
    pub path:             String,
    /// `true` once the startup load/build has completed.
    pub ready:            bool,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Return live statistics about the global cross-day HNSW index.
///
/// `ready: false` while the background build is still running.
#[tauri::command]
pub fn get_global_index_stats(
    state:  tauri::State<'_, std::sync::Mutex<crate::AppState>>,
    global: tauri::State<'_, std::sync::Arc<GlobalEegIndex>>,
) -> GlobalIndexStats {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    let path      = index_path(&skill_dir);
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let guard     = global.0.lock_or_recover();
    GlobalIndexStats {
        total_embeddings: guard.as_ref().map(|i| i.len()).unwrap_or(0),
        file_size_bytes:  file_size,
        path:             path.display().to_string(),
        ready:            guard.is_some(),
    }
}

/// Rebuild the global HNSW index from scratch, scanning every daily
/// `eeg.sqlite`.  Replaces the in-memory index and overwrites the on-disk
/// file.  Runs on a blocking thread; may take a few seconds.
#[tauri::command]
pub async fn rebuild_global_eeg_index(
    state:  tauri::State<'_, std::sync::Mutex<crate::AppState>>,
    global: tauri::State<'_, std::sync::Arc<GlobalEegIndex>>,
) -> Result<GlobalIndexStats, String> {
    let skill_dir  = state.lock_or_recover().skill_dir.clone();
    let global_arc = global.arc();

    tokio::task::spawn_blocking(move || {
        let new_idx   = rebuild_from_scratch(&skill_dir);
        let path      = index_path(&skill_dir);
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let stats = GlobalIndexStats {
            total_embeddings: new_idx.len(),
            file_size_bytes:  file_size,
            path:             path.display().to_string(),
            ready:            true,
        };
        *global_arc.lock_or_recover() = Some(new_idx);
        Ok(stats)
    })
    .await
    .map_err(|e| e.to_string())?
}
