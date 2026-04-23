// SPDX-License-Identifier: GPL-3.0-only
//! Hook and activity route handlers extracted from settings.

use axum::{extract::State, Json};
use skill_data::activity_store::{
    ActiveWindowRow, ActivityStore, BuildEventRow, CoEditRow, DailySummaryRow, EditChunkRow, FileInteractionRow,
    FileUsageRow, FocusSessionRow, HourlyEditRow, InputActivityRow, InputBucketRow, LanguageBreakdownRow,
    ProjectUsageRow,
};

use crate::{
    routes::settings::{
        ActivityBucketsRequest, ActivityFilesRequest, ActivityRecentRequest, CoEditRequest, DaySummaryRequest,
        EditChunksRequest, HookDistanceRequest, HookKeywordsRequest, HookLogRequest,
    },
    state::AppState,
};

pub(crate) async fn get_hooks_impl(State(state): State<AppState>) -> Json<Vec<skill_settings::HookRule>> {
    Json(state.hooks.lock().map(|g| g.clone()).unwrap_or_default())
}

pub(crate) async fn set_hooks_impl(
    State(state): State<AppState>,
    Json(hooks): Json<Vec<skill_settings::HookRule>>,
) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.hooks.lock() {
        *g = hooks.clone();
    }
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let mut settings = skill_settings::load_settings(&skill_dir);
    settings.hooks = hooks;
    let path = skill_settings::settings_path(&skill_dir);
    let ok = serde_json::to_string_pretty(&settings)
        .ok()
        .and_then(|json| std::fs::write(path, json).ok())
        .is_some();
    Json(serde_json::json!({"ok": ok}))
}

pub(crate) async fn get_hook_statuses_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let hooks = state.hooks.lock().map(|g| g.clone()).unwrap_or_default();
    Json(serde_json::Value::Array(
        hooks
            .into_iter()
            .map(|hook| serde_json::json!({"hook": hook, "last_trigger": serde_json::Value::Null}))
            .collect(),
    ))
}

pub(crate) async fn get_hook_log_impl(
    State(state): State<AppState>,
    Json(req): Json<HookLogRequest>,
) -> Json<Vec<skill_data::hooks_log::HookLogRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let rows = tokio::task::spawn_blocking(move || {
        let Some(log) = skill_data::hooks_log::HooksLog::open(&skill_dir) else {
            return vec![];
        };
        log.query(req.limit.unwrap_or(50).clamp(1, 500), req.offset.unwrap_or(0).max(0))
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn get_hook_log_count_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let count = tokio::task::spawn_blocking(move || {
        skill_data::hooks_log::HooksLog::open(&skill_dir)
            .map(|l| l.count())
            .unwrap_or(0)
    })
    .await
    .unwrap_or(0);
    Json(serde_json::json!({"count": count}))
}

pub(crate) async fn suggest_hook_keywords_impl(
    State(state): State<AppState>,
    Json(req): Json<HookKeywordsRequest>,
) -> Json<Vec<serde_json::Value>> {
    let q = req.draft.trim().to_lowercase();
    if q.len() < 2 {
        return Json(Vec::new());
    }
    let max_n = req.limit.unwrap_or(8).clamp(1, 20);
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let labels_db = skill_dir.join(skill_constants::LABELS_FILE);

    let out = tokio::task::spawn_blocking(move || {
        let mut out = Vec::<serde_json::Value>::new();
        if !labels_db.exists() {
            return out;
        }
        let Ok(conn) = skill_data::util::open_readonly(&labels_db) else {
            return out;
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT text FROM labels
             WHERE length(trim(text)) > 0
             GROUP BY text
             ORDER BY MAX(created_at) DESC
             LIMIT 600",
        ) else {
            return out;
        };
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for text in rows.flatten() {
                let cand = text.to_lowercase();
                if cand.contains(&q) {
                    out.push(serde_json::json!({"keyword": text, "source": "fuzzy", "score": 0.92}));
                }
                if out.len() >= max_n {
                    break;
                }
            }
        }
        out
    })
    .await
    .unwrap_or_default();

    Json(out)
}

pub(crate) async fn suggest_hook_distances_impl(
    State(state): State<AppState>,
    Json(req): Json<HookDistanceRequest>,
) -> Json<serde_json::Value> {
    let label_n = req.keywords.len();
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    let mut distances: Vec<f32> = tokio::task::spawn_blocking(move || {
        let Some(log) = skill_data::hooks_log::HooksLog::open(&skill_dir) else {
            return Vec::new();
        };
        let rows = log.query(5000, 0);
        let mut vals = Vec::new();
        for row in rows {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&row.trigger_json) else {
                continue;
            };
            let maybe = v
                .get("distance")
                .and_then(serde_json::Value::as_f64)
                .or_else(|| v.get("eeg_distance").and_then(serde_json::Value::as_f64))
                .or_else(|| v.get("eegDistance").and_then(serde_json::Value::as_f64));
            if let Some(d) = maybe {
                let d = d as f32;
                if d.is_finite() {
                    vals.push(d.clamp(0.0, 1.0));
                }
            }
        }
        vals
    })
    .await
    .unwrap_or_default();

    if distances.is_empty() {
        return Json(serde_json::json!({
            "label_n": label_n,
            "ref_n": 0,
            "sample_n": 0,
            "eeg_min": 0.0,
            "eeg_p25": 0.0,
            "eeg_p50": 0.0,
            "eeg_p75": 0.0,
            "eeg_max": 0.0,
            "suggested": 0.1,
            "note": "No hook trigger distances recorded yet."
        }));
    }

    distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sample_n = distances.len();
    let min = distances[0];
    let max = *distances.last().unwrap_or(&min);
    let q = |p: f32| -> f32 {
        let idx = ((sample_n - 1) as f32 * p).round() as usize;
        distances[idx.min(sample_n - 1)]
    };
    let p25 = q(0.25);
    let p50 = q(0.50);
    let p75 = q(0.75);
    let suggested = p75.clamp(0.05, 0.95);

    Json(serde_json::json!({
        "label_n": label_n,
        "ref_n": sample_n,
        "sample_n": sample_n,
        "eeg_min": min,
        "eeg_p25": p25,
        "eeg_p50": p50,
        "eeg_p75": p75,
        "eeg_max": max,
        "suggested": suggested,
        "note": "Estimated from recent hook trigger EEG distances."
    }))
}

pub(crate) async fn activity_recent_windows_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityRecentRequest>,
) -> Json<Vec<ActiveWindowRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let limit = req.limit.unwrap_or(50).min(500);
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|store| store.get_recent_windows(limit))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_recent_input_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityRecentRequest>,
) -> Json<Vec<InputActivityRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let limit = req.limit.unwrap_or(50).min(500);
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|store| store.get_recent_input(limit))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_input_buckets_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityBucketsRequest>,
) -> Json<Vec<InputBucketRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let end = req.to_ts.unwrap_or(now);
    let start = req.from_ts.unwrap_or_else(|| end.saturating_sub(24 * 3600));
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|store| store.get_input_buckets(start, end))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_recent_files_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityFilesRequest>,
) -> Json<Vec<FileInteractionRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let limit = req.limit.unwrap_or(50).min(500);
    let since = req.since;
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|store| store.get_recent_files(limit, since))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_top_files_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityFilesRequest>,
) -> Json<Vec<FileUsageRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let limit = req.limit.unwrap_or(20).min(200);
    let since = req.since;
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|store| store.top_files(limit, since))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_top_projects_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityFilesRequest>,
) -> Json<Vec<ProjectUsageRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let limit = req.limit.unwrap_or(20).min(200);
    let since = req.since;
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|store| store.top_projects(limit, since))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_edit_chunks_impl(
    State(state): State<AppState>,
    Json(req): Json<EditChunksRequest>,
) -> Json<Vec<EditChunkRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let rows = tokio::task::spawn_blocking(move || {
        let Some(store) = ActivityStore::open(&skill_dir) else {
            return vec![];
        };
        if let Some(id) = req.interaction_id {
            store.get_edit_chunks(id)
        } else {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let end = req.to_ts.unwrap_or(now);
            let start = req.from_ts.unwrap_or_else(|| end.saturating_sub(3600));
            store.get_edit_chunks_range(start, end)
        }
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_language_breakdown_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityFilesRequest>,
) -> Json<Vec<LanguageBreakdownRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since;
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|s| s.language_breakdown(since))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_context_switch_rate_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityBucketsRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let end = req.to_ts.unwrap_or(now);
    let start = req.from_ts.unwrap_or_else(|| end.saturating_sub(3600));
    let rate = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|s| s.context_switch_rate(start, end))
            .unwrap_or(0.0)
    })
    .await
    .unwrap_or(0.0);
    Json(serde_json::json!({"switches_per_minute": rate}))
}

pub(crate) async fn activity_coedited_files_impl(
    State(state): State<AppState>,
    Json(req): Json<CoEditRequest>,
) -> Json<Vec<CoEditRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let window = req.window_secs.unwrap_or(600);
    let limit = req.limit.unwrap_or(20).min(100);
    let since = req.since;
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|s| s.coedited_files(window, limit, since))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_daily_summary_impl(
    State(state): State<AppState>,
    Json(req): Json<DaySummaryRequest>,
) -> Json<DailySummaryRow> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let day = req.day_start;
    let row = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|s| s.daily_summary(day))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(row)
}

pub(crate) async fn activity_hourly_heatmap_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityFilesRequest>,
) -> Json<Vec<HourlyEditRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since;
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|s| s.hourly_edit_heatmap(since))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_focus_sessions_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityFilesRequest>,
) -> Json<Vec<FocusSessionRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let limit = req.limit.unwrap_or(20).min(100);
    let since = req.since;
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|s| s.get_focus_sessions(limit, since))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_forgotten_files_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityFilesRequest>,
) -> Json<Vec<String>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let since = req.since.unwrap_or_else(|| now.saturating_sub(86400));
    let files = tokio::task::spawn_blocking(move || {
        let Some(store) = ActivityStore::open(&skill_dir) else {
            return vec![];
        };
        let modified = store.modified_files_since(since);
        modified
            .into_iter()
            .filter(|f| {
                let dir = std::path::Path::new(f).parent().unwrap_or(std::path::Path::new("."));
                std::process::Command::new("git")
                    .args(["diff", "--name-only", "HEAD", "--", f])
                    .current_dir(dir)
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| !o.stdout.is_empty())
                    .unwrap_or(false)
            })
            .collect()
    })
    .await
    .unwrap_or_default();
    Json(files)
}

pub(crate) async fn activity_recent_builds_impl(State(state): State<AppState>) -> Json<Vec<BuildEventRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|s| s.get_recent_builds(50))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}
