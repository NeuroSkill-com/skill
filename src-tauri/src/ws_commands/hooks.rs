// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! WebSocket hook command handlers (hooks_get, hooks_set, hooks_status, hooks_suggest, hooks_log).

use serde_json::Value;
use tauri::AppHandle;

use crate::constants::SQLITE_FILE;
use crate::skill_dir;
use crate::AppStateExt;
use crate::MutexExt;

/// `hooks_get` — return raw hook rules (no runtime trigger state).
pub(super) fn hooks_get(app: &AppHandle) -> Result<Value, String> {
    let st = app.app_state();
    let s = st.lock_or_recover();
    Ok(serde_json::json!({ "hooks": s.hooks }))
}

/// `hooks_set` — replace all hooks with the provided list.
///
/// Accepts `{ "hooks": [ { name, enabled, keywords, scenario, command, text,
///   distance_threshold, recent_limit }, … ] }`.
/// Each rule is sanitised identically to the Tauri `set_hooks` command.
pub(super) fn hooks_set(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let raw: Vec<crate::settings::HookRule> = msg
        .get("hooks")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let clean: Vec<crate::settings::HookRule> = raw
        .into_iter()
        .filter_map(crate::settings_cmds::sanitize_hook)
        .take(100)
        .collect();

    {
        let st = app.app_state();
        let mut s = st.lock_or_recover();
        s.hooks = clean;
        let keep: std::collections::HashSet<String> =
            s.hooks.iter().map(|h| h.name.clone()).collect();
        s.hook_runtime
            .lock_or_recover()
            .retain(|name, _| keep.contains(name));
    }
    crate::save_settings_handle(app);

    // Return the saved hooks so callers can verify sanitisation.
    let st = app.app_state();
    let s = st.lock_or_recover();
    Ok(serde_json::json!({ "hooks": s.hooks }))
}

/// `hooks_status` — return all hooks with last-trigger metadata.
pub(super) fn hooks_status(app: &AppHandle) -> Result<Value, String> {
    let st = app.app_state();
    let s = st.lock_or_recover();
    let runtime = s.hook_runtime.lock_or_recover();
    let statuses: Vec<crate::settings::HookStatus> = s
        .hooks
        .iter()
        .cloned()
        .map(|hook| crate::settings::HookStatus {
            last_trigger: runtime.get(&hook.name).cloned(),
            hook,
        })
        .collect();
    Ok(serde_json::json!({ "hooks": statuses }))
}

/// `hooks_suggest` — suggest a hook distance threshold from existing labels + EEG embeddings.
pub(super) fn hooks_suggest(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let keywords: Vec<String> = if let Some(arr) = msg.get("keywords").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    } else if let Some(s) = msg.get("keywords").and_then(|v| v.as_str()) {
        s.split(',')
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    let skill_dir = {
        let st = app.app_state();
        skill_dir(&st)
    };

    // Keep this implementation parallel to settings_cmds::suggest_hook_distances.
    let empty = crate::settings_cmds::HookDistanceSuggestion {
        label_n: 0,
        ref_n: 0,
        sample_n: 0,
        eeg_min: 0.0,
        eeg_p25: 0.0,
        eeg_p50: 0.0,
        eeg_p75: 0.0,
        eeg_max: 0.0,
        suggested: 0.1,
        note: "No label data found. Keep the default 0.1 and adjust after recording sessions with labels.".to_owned(),
    };

    if keywords.is_empty() {
        return Ok(serde_json::json!({ "suggestion": empty }));
    }

    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() {
        return Ok(serde_json::json!({ "suggestion": empty }));
    }
    let Ok(conn) = skill_data::util::open_readonly(&labels_db) else {
        return Ok(serde_json::json!({ "suggestion": empty }));
    };

    let all_labels: Vec<(i64, String, u64, u64)> = {
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, text, eeg_start, eeg_end FROM labels WHERE length(trim(text)) > 0",
        ) else {
            return Ok(serde_json::json!({ "suggestion": empty }));
        };
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    };

    let matched: Vec<(i64, String, u64, u64)> = all_labels
        .into_iter()
        .filter(|(_, text, _, _)| keywords.iter().any(|k| skill_exg::fuzzy_match(k, text)))
        .collect();
    let label_n = matched.len();
    if label_n == 0 {
        let out = crate::settings_cmds::HookDistanceSuggestion {
            note: format!(
                "No labels matched your keywords ({}). Add labels to your sessions first.",
                keywords.join(", ")
            ),
            ..empty
        };
        return Ok(serde_json::json!({ "suggestion": out }));
    }

    let refs: Vec<Vec<f32>> = matched
        .iter()
        .filter_map(|(_, _, eeg_start, eeg_end)| {
            crate::label_index::mean_eeg_for_window(&skill_dir, *eeg_start, *eeg_end)
        })
        .collect();
    let ref_n = refs.len();
    if ref_n == 0 {
        let out = crate::settings_cmds::HookDistanceSuggestion {
            label_n,
            note: format!(
                "{label_n} label(s) matched but no EEG recordings cover their time windows yet.",
            ),
            ..empty
        };
        return Ok(serde_json::json!({ "suggestion": out }));
    }

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
        date_dirs.sort_by(|a, b| b.cmp(a));

        let mut out: Vec<Vec<f32>> = Vec::new();
        let per_day = (max / date_dirs.len().max(1)).max(20);

        for dir in &date_dirs {
            let db = dir.join(SQLITE_FILE);
            if !db.exists() {
                continue;
            }
            let Ok(conn) = skill_data::util::open_readonly(&db) else {
                continue;
            };

            let Ok(mut stmt) = conn
                .prepare("SELECT eeg_embedding FROM embeddings ORDER BY timestamp DESC LIMIT ?1")
            else {
                continue;
            };

            let blobs: Vec<Vec<f32>> = stmt
                .query_map(rusqlite::params![per_day as i64], |r| {
                    r.get::<_, Vec<u8>>(0)
                })
                .map(|rows| {
                    rows.flatten()
                        .map(|b| {
                            b.chunks_exact(4)
                                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                                .collect()
                        })
                        .collect()
                })
                .unwrap_or_default();

            out.extend(blobs);
            if out.len() >= max {
                break;
            }
        }
        out
    }

    let samples = sample_recent_eeg_embeddings(&skill_dir, 300);
    let sample_n = samples.len();
    if sample_n == 0 {
        let out = crate::settings_cmds::HookDistanceSuggestion {
            label_n,
            ref_n,
            note: "No recent EEG embeddings found. Record a session first.".to_owned(),
            ..empty
        };
        return Ok(serde_json::json!({ "suggestion": out }));
    }

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
        let out = crate::settings_cmds::HookDistanceSuggestion {
            label_n,
            ref_n,
            sample_n,
            note: "Could not compute distances (dimension mismatch).".to_owned(),
            ..empty
        };
        return Ok(serde_json::json!({ "suggestion": out }));
    }

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
    let eeg_max = *distances.last().unwrap_or(&0.0);
    let suggested = ((eeg_p25 * 100.0).round() / 100.0).clamp(0.01, 0.99);

    let out = crate::settings_cmds::HookDistanceSuggestion {
        label_n,
        ref_n,
        sample_n,
        eeg_min,
        eeg_p25,
        eeg_p50,
        eeg_p75,
        eeg_max,
        suggested,
        note: format!(
            "{label_n} label(s) matched ({ref_n} with EEG data). Distribution of {n} distances — min {eeg_min:.3}, p25 {eeg_p25:.3}, median {eeg_p50:.3}, p75 {eeg_p75:.3}, max {eeg_max:.3}. Suggested threshold {suggested:.2} (p25 = fairly strict match)."
        ),
    };

    Ok(serde_json::json!({ "suggestion": out }))
}

/// `hooks_log` — fetch paginated hook trigger audit rows from hooks.sqlite.
pub(super) fn hooks_log(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let limit = msg
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .map(|v| v as i64)
        .unwrap_or(50)
        .clamp(1, 500);
    let offset = msg
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .map(|v| v as i64)
        .unwrap_or(0)
        .max(0);

    let skill_dir = {
        let st = app.app_state();
        skill_dir(&st)
    };
    let Some(log) = skill_data::hooks_log::HooksLog::open(&skill_dir) else {
        return Ok(serde_json::json!({ "rows": [], "total": 0, "limit": limit, "offset": offset }));
    };

    let rows = log.query(limit, offset);
    let total = log.count();
    Ok(serde_json::json!({
        "rows": rows,
        "total": total,
        "limit": limit,
        "offset": offset
    }))
}
