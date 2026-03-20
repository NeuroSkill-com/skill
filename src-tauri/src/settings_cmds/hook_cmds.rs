// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Hook distance suggestion and audit log Tauri commands.

use std::sync::Mutex;

use crate::{AppState, skill_dir};

// ── Hook distance suggestion ──────────────────────────────────────────────────

/// Percentile distribution of EEG distances, used to suggest a threshold.
#[derive(serde::Serialize)]
pub struct HookDistanceSuggestion {
    /// Number of labels that text-matched at least one keyword.
    pub label_n:    usize,
    /// Number of label EEG reference embeddings with real EEG data.
    pub ref_n:      usize,
    /// Number of recent EEG samples used for the distribution.
    pub sample_n:   usize,
    pub eeg_min:    f32,
    pub eeg_p25:    f32,
    pub eeg_p50:    f32,
    pub eeg_p75:    f32,
    pub eeg_max:    f32,
    /// Suggested `distance_threshold` value (p25 of the distribution).
    pub suggested:  f32,
    /// Human-readable explanation of the suggestion.
    pub note:       String,
}

/// Suggest a `distance_threshold` value by analysing real HNSW and SQLite data.
///
/// Steps:
/// 1. Query `labels.sqlite` for labels that fuzzy-match any of the supplied keywords.
/// 2. Compute the mean EEG embedding for each matched label's time window.
/// 3. Sample up to 300 recent EEG embeddings from `eeg.sqlite` daily files.
/// 4. Compute cosine distance from every sample to every label reference.
/// 5. Return a percentile breakdown + suggested threshold.
/// Suggest a `distance_threshold` value by analysing real HNSW and SQLite data.
///
/// Runs on a blocking thread — involves SQLite queries, EEG embedding sampling,
/// and pairwise distance computation which can take several seconds.
#[tauri::command]
pub async fn suggest_hook_distances(
    keywords: Vec<String>,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<HookDistanceSuggestion, String> {
    let skill_dir = skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        suggest_hook_distances_sync(keywords, &skill_dir)
    })
    .await
    .map_err(|e| e.to_string())
}

fn suggest_hook_distances_sync(
    keywords: Vec<String>,
    skill_dir: &std::path::Path,
) -> HookDistanceSuggestion {
    let empty = HookDistanceSuggestion {
        label_n: 0, ref_n: 0, sample_n: 0,
        eeg_min: 0.0, eeg_p25: 0.0, eeg_p50: 0.0, eeg_p75: 0.0, eeg_max: 0.0,
        suggested: 0.1,
        note: "No label data found. Keep the default 0.1 and adjust after recording sessions with labels."
            .to_owned(),
    };

    let kws: Vec<String> = keywords.iter()
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty())
        .collect();
    if kws.is_empty() {
        return empty;
    }

    // ── Step 1: find matching labels ─────────────────────────────────────────
    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() {
        return empty;
    }
    let Ok(conn) = skill_data::util::open_readonly(&labels_db) else {
        return empty;
    };

    let all_labels: Vec<(i64, String, u64, u64)> = {
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, text, eeg_start, eeg_end FROM labels WHERE length(trim(text)) > 0",
        ) else { return empty; };
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    };

    let matched: Vec<(i64, String, u64, u64)> = all_labels
        .into_iter()
        .filter(|(_, text, _, _)| kws.iter().any(|k| skill_exg::fuzzy_match(k, text)))
        .collect();

    let label_n = matched.len();
    if label_n == 0 {
        return HookDistanceSuggestion {
            note: format!(
                "No labels matched your keywords ({kws_fmt}). Add labels to your sessions first.",
                kws_fmt = kws.join(", ")
            ),
            suggested: 0.1,
            ..empty
        };
    }

    // ── Step 2: get mean EEG embeddings for matched labels ───────────────────
    let refs: Vec<Vec<f32>> = matched.iter()
        .filter_map(|(_, _, eeg_start, eeg_end)| {
            crate::label_index::mean_eeg_for_window(skill_dir, *eeg_start, *eeg_end)
        })
        .collect();

    let ref_n = refs.len();
    if ref_n == 0 {
        return HookDistanceSuggestion {
            label_n,
            note: format!(
                "{label_n} label(s) matched but no EEG recordings cover their time windows yet.",
            ),
            suggested: 0.1,
            ..empty
        };
    }

    // ── Step 3: sample recent EEG embeddings ─────────────────────────────────
    let samples = sample_recent_eeg_embeddings(skill_dir, 300);
    let sample_n = samples.len();
    if sample_n == 0 {
        return HookDistanceSuggestion {
            label_n,
            ref_n,
            note: "No recent EEG embeddings found. Record a session first.".to_owned(),
            suggested: 0.1,
            ..empty
        };
    }

    // ── Step 4: compute all pairwise distances ────────────────────────────────
    let mut distances: Vec<f32> = Vec::with_capacity(samples.len() * refs.len());
    for sample in &samples {
        for r in &refs {
            let d = skill_exg::cosine_distance(sample, r);
            if d < 2.0 {
                distances.push(d);
            }
        }
    }
    if distances.is_empty() {
        return HookDistanceSuggestion {
            label_n, ref_n, sample_n,
            note: "Could not compute distances (dimension mismatch).".to_owned(),
            suggested: 0.1,
            ..empty
        };
    }

    // ── Step 5: percentiles ───────────────────────────────────────────────────
    distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = distances.len();
    let percentile = |p: f32| -> f32 {
        let idx = ((p / 100.0) * (n as f32 - 1.0)).round() as usize;
        distances[idx.min(n - 1)]
    };
    let eeg_min = distances[0];
    let eeg_p25 = percentile(25.0);
    let eeg_p50 = percentile(50.0);
    let eeg_p75 = percentile(75.0);
    let eeg_max = distances.last().copied().unwrap_or(1.0);
    // Suggest p25 rounded to 2 decimal places — catches the closest quarter of hits.
    let suggested = (eeg_p25 * 100.0).round() / 100.0;
    let suggested = suggested.clamp(0.01, 0.99);

    let note = format!(
        "{label_n} label(s) matched ({ref_n} with EEG data). Distribution of {n} \
         distances — min {eeg_min:.3}, p25 {eeg_p25:.3}, median {eeg_p50:.3}, \
         p75 {eeg_p75:.3}, max {eeg_max:.3}. \
         Suggested threshold {suggested:.2} (p25 = fairly strict match).",
    );

    HookDistanceSuggestion { label_n, ref_n, sample_n, eeg_min, eeg_p25, eeg_p50, eeg_p75, eeg_max, suggested, note }
}

/// Read up to `max` EEG embedding blobs from the most-recent daily `eeg.sqlite` files.
fn sample_recent_eeg_embeddings(skill_dir: &std::path::Path, max: usize) -> Vec<Vec<f32>> {
    let mut date_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(skill_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.len() == 8 && name.chars().all(|c| c.is_ascii_digit()) {
                Some(e.path())
            } else {
                None
            }
        })
        .collect();
    date_dirs.sort_by(|a, b| b.cmp(a)); // newest first

    let mut out: Vec<Vec<f32>> = Vec::new();
    let per_day = (max / date_dirs.len().max(1)).max(20);

    for dir in &date_dirs {
        let db = dir.join(crate::constants::SQLITE_FILE);
        if !db.exists() { continue; }
        let Ok(conn) = skill_data::util::open_readonly(&db) else { continue };

        let Ok(mut stmt) = conn.prepare(
            "SELECT eeg_embedding FROM embeddings ORDER BY timestamp DESC LIMIT ?1",
        ) else { continue };

        let blobs: Vec<Vec<f32>> = stmt
            .query_map(rusqlite::params![per_day as i64], |r| r.get::<_, Vec<u8>>(0))
            .map(|rows| {
                rows.flatten()
                    .map(|b| b.chunks_exact(4)
                        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                        .collect())
                    .collect()
            })
            .unwrap_or_default();

        out.extend(blobs);
        if out.len() >= max { break; }
    }
    out
}

// ── Hook audit log ────────────────────────────────────────────────────────────

/// Return the most-recent hook-fire events from `hooks.sqlite`.
///
/// Runs on a blocking thread — opens and queries a SQLite database.
#[tauri::command]
pub async fn get_hook_log(
    limit:  Option<i64>,
    offset: Option<i64>,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<skill_data::hooks_log::HookLogRow>, String> {
    let skill_dir = skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        let Some(log) = skill_data::hooks_log::HooksLog::open(&skill_dir) else {
            return vec![];
        };
        log.query(limit.unwrap_or(50).clamp(1, 500), offset.unwrap_or(0).max(0))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Return the total number of hook-fire events in the audit log.
///
/// Runs on a blocking thread — opens and queries a SQLite database.
#[tauri::command]
pub async fn get_hook_log_count(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Result<i64, String> {
    let skill_dir = skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        skill_data::hooks_log::HooksLog::open(&skill_dir)
            .map(|l| l.count())
            .unwrap_or(0)
    })
    .await
    .map_err(|e| e.to_string())
}

