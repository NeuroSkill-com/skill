// SPDX-License-Identifier: GPL-3.0-only
//! Hook and activity route handlers extracted from settings.

use axum::{extract::State, Json};
use skill_data::activity_store::{
    ActiveWindowRow, ActivityStore, BuildEventRow, ClipboardEventRow, CoEditRow, DailySummaryRow, EditChunkRow,
    FileInteractionRow, FileUsageRow, FocusSessionRow, HourlyEditRow, InputActivityRow, InputBucketRow,
    LanguageBreakdownRow, MeetingEventRow, ProductivityScore, ProjectUsageRow, StaleFileRow, WeeklyDigest,
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
        ActivityStore::open_readonly(&skill_dir)
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
        ActivityStore::open_readonly(&skill_dir)
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
        ActivityStore::open_readonly(&skill_dir)
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
        ActivityStore::open_readonly(&skill_dir)
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
        ActivityStore::open_readonly(&skill_dir)
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
        ActivityStore::open_readonly(&skill_dir)
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
        let Some(store) = ActivityStore::open_readonly(&skill_dir) else {
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
        ActivityStore::open_readonly(&skill_dir)
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
        ActivityStore::open_readonly(&skill_dir)
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
        ActivityStore::open_readonly(&skill_dir)
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
        ActivityStore::open_readonly(&skill_dir)
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
    let tz = chrono::Local::now().offset().local_minus_utc();
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open_readonly(&skill_dir)
            .map(|s| s.hourly_edit_heatmap(since, tz))
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
        ActivityStore::open_readonly(&skill_dir)
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
        let Some(store) = ActivityStore::open_readonly(&skill_dir) else {
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
        ActivityStore::open_readonly(&skill_dir)
            .map(|s| s.get_recent_builds(50))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_files_in_range_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityBucketsRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let from = req.from_ts.unwrap_or(0);
    let to = req.to_ts.unwrap_or(u64::MAX);
    let result = tokio::task::spawn_blocking(move || {
        ActivityStore::open_readonly(&skill_dir).map(|s| {
            let files = s.get_files_in_range(from, to, 200);
            let sessions = s.get_focus_sessions_in_range(from, to);
            let meetings = s.get_meetings_in_range(from, to);
            serde_json::json!({
                "files": files,
                "focus_sessions": sessions,
                "meetings": meetings,
            })
        })
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| serde_json::json!({"files": [], "focus_sessions": [], "meetings": []}));
    Json(result)
}

pub(crate) async fn activity_meetings_in_range_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityBucketsRequest>,
) -> Json<Vec<MeetingEventRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let from = req.from_ts.unwrap_or(0);
    let to = req.to_ts.unwrap_or(u64::MAX);
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open_readonly(&skill_dir)
            .map(|s| s.get_meetings_in_range(from, to))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_recent_clipboard_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityFilesRequest>,
) -> Json<Vec<ClipboardEventRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let limit = req.limit.unwrap_or(50) as u32;
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open_readonly(&skill_dir)
            .map(|s| s.get_recent_clipboard(limit))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

pub(crate) async fn activity_productivity_score_impl(
    State(state): State<AppState>,
    Json(req): Json<DaySummaryRequest>,
) -> Json<ProductivityScore> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let day_start = req.day_start;
    let result = tokio::task::spawn_blocking(move || {
        ActivityStore::open_readonly(&skill_dir).map(|s| s.productivity_score(day_start))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(ProductivityScore {
        day_start,
        score: 0.0,
        edit_velocity: 0.0,
        deep_work: 0.0,
        context_stability: 0.0,
        eeg_focus: 0.0,
        deep_work_minutes: 0,
        switch_rate: 0.0,
    });
    Json(result)
}

pub(crate) async fn activity_weekly_digest_impl(
    State(state): State<AppState>,
    Json(req): Json<DaySummaryRequest>,
) -> Json<WeeklyDigest> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let week_start = req.day_start;
    let result = tokio::task::spawn_blocking(move || {
        ActivityStore::open_readonly(&skill_dir).map(|s| s.weekly_digest(week_start))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(WeeklyDigest {
        week_start,
        days: vec![],
        total_interactions: 0,
        total_edits: 0,
        total_secs: 0,
        total_lines_added: 0,
        total_lines_removed: 0,
        avg_eeg_focus: None,
        top_projects: vec![],
        top_languages: vec![],
        focus_session_count: 0,
        meeting_count: 0,
        peak_day_idx: 0,
        peak_hour: 0,
    });
    Json(result)
}

pub(crate) async fn activity_stale_files_impl(
    State(state): State<AppState>,
    Json(req): Json<ActivityFilesRequest>,
) -> Json<Vec<StaleFileRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since.unwrap_or(0);
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open_readonly(&skill_dir)
            .map(|s| s.stale_files(7, since))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

/// Process a batch of VS Code extension events.
pub(crate) async fn activity_vscode_events_impl(
    State(state): State<AppState>,
    events: Vec<serde_json::Value>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let embedder = state.text_embedder.clone();
    let label_index = state.label_index.clone();
    let processed = tokio::task::spawn_blocking(move || {
        let Some(store) = ActivityStore::open(&skill_dir) else {
            return 0u64;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Open labels DB for auto-labeling EEG recordings.
        let labels_db = skill_dir.join(skill_constants::LABELS_FILE);
        let labels_conn = rusqlite::Connection::open(&labels_db).ok();

        let mut count = 0u64;
        for event in &events {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let path = event.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let language = event.get("language").and_then(|v| v.as_str()).unwrap_or("");
            let command = event.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let basename = path.rsplit('/').next().unwrap_or(path);

            // ── Data storage (activity tables) ───────────────────────────
            match event_type {
                "file_focus" if !path.is_empty() => {
                    store.insert_file_interaction(path, "Visual Studio Code", "", language, "", "", now, None, None);
                }
                "edit" => {
                    let added = event.get("lines_added").and_then(|v| v.as_u64()).unwrap_or(0);
                    let removed = event.get("lines_removed").and_then(|v| v.as_u64()).unwrap_or(0);
                    let is_undo = event.get("undo").and_then(|v| v.as_bool()).unwrap_or(false);
                    if !path.is_empty() && (added > 0 || removed > 0) {
                        let recent = store.get_recent_files(1, None);
                        if let Some(fi) = recent.first().filter(|f| f.file_path == path) {
                            let undo_est = if is_undo { added.max(removed) } else { 0 };
                            store.insert_edit_chunk(fi.id, now, added, removed, 0, undo_est);
                        }
                    }
                }
                "debug_start" | "debug_stop" | "task_start" | "task_end" => {
                    let exit_code = event.get("exit_code").and_then(|v| v.as_i64());
                    let outcome = match exit_code {
                        Some(0) => "pass",
                        Some(_) => "fail",
                        _ => "running",
                    };
                    let cmd = if command.is_empty() { event_type } else { command };
                    store.insert_build_event(cmd, outcome, "", now);
                }
                "ai_suggestion_shown"
                | "ai_suggestion_accepted"
                | "ai_suggestion_rejected"
                | "ai_chat_start"
                | "ai_chat_end" => {
                    let source = event.get("source").and_then(|v| v.as_str()).unwrap_or("");
                    store.insert_ai_event(event_type, source, path, language, now);
                }
                "terminal_command_start" if !command.is_empty() => {
                    let source = event.get("source").and_then(|v| v.as_str()).unwrap_or("");
                    store.insert_terminal_command_start(source, command, path, now, None, None);
                }
                "terminal_command_end" => {
                    let exit_code = event.get("exit_code").and_then(|v| v.as_i64());
                    let source = event.get("source").and_then(|v| v.as_str()).unwrap_or("");
                    store.update_terminal_command_end(command, source, exit_code, now, None);
                }
                "zone_switch" => {
                    let from = event.get("source").and_then(|v| v.as_str()).unwrap_or("");
                    store.insert_zone_switch(command, from, now, None);
                }
                "layout_snapshot" => {
                    // command field carries JSON payload
                    if let Ok(snap) = serde_json::from_str::<serde_json::Value>(command) {
                        let groups = snap.get("editorGroups").and_then(|v| v.as_i64()).unwrap_or(1);
                        let visible = snap.get("visibleEditors").and_then(|v| v.as_i64()).unwrap_or(1);
                        let tabs = snap.get("openTabs").and_then(|v| v.as_i64()).unwrap_or(0);
                        let terms = snap.get("terminals").and_then(|v| v.as_i64()).unwrap_or(0);
                        store.insert_layout_snapshot(now, groups, visible, tabs, terms);
                    }
                }
                _ => {}
            }

            // ── EEG auto-labeling (smart categorization) ─────────────────
            // Label text: short, searchable phrase (what happened)
            // Context: details (file, language, counts, etc.)
            let (label, ctx) = match event_type {
                // === Editing — only label significant events, not every keystroke ===
                "file_focus" if !path.is_empty() => (format!("editing {language}"), basename.to_string()),
                "save" if !path.is_empty() => ("file saved".to_string(), format!("{basename} ({language})")),

                // === Debugging ===
                "debug_start" => ("debugging started".to_string(), language.to_string()),
                "debug_stop" => ("debugging ended".to_string(), String::new()),
                "breakpoint_change" => {
                    let n = event.get("breakpoint_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    ("breakpoint changed".to_string(), format!("{n} breakpoints"))
                }

                // === Build / test ===
                "task_start" => (format!("task: {command}"), String::new()),
                "task_end" => {
                    let code = event.get("exit_code").and_then(|v| v.as_i64());
                    let status = match code {
                        Some(0) => "passed",
                        Some(_) => "failed",
                        None => "ended",
                    };
                    (format!("task {status}"), command.to_string())
                }

                // === Git operations ===
                "git_commit" => ("git commit".to_string(), command.to_string()),
                "git_push" => ("git push".to_string(), String::new()),
                "git_pull" => ("git pull".to_string(), String::new()),
                "git_checkout" => ("git branch switch".to_string(), String::new()),

                // === Navigation — code comprehension ===
                "code_jump" => {
                    let line = event.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
                    ("code navigation".to_string(), format!("{basename}:{line}"))
                }

                // === AI assistance ===
                "ai_suggestion_accepted" => ("AI suggestion accepted".to_string(), format!("{basename} ({language})")),
                "ai_suggestion_rejected" => ("AI suggestion rejected".to_string(), basename.to_string()),
                "ai_chat_start" => {
                    let source = event.get("source").and_then(|v| v.as_str()).unwrap_or("copilot");
                    (format!("AI chat: {source}"), basename.to_string())
                }
                "ai_inline_chat" => ("AI inline assist".to_string(), format!("{basename} ({language})")),

                // === Collaboration ===
                "liveshare_start" => ("pair programming started".to_string(), String::new()),
                "liveshare_end" => ("pair programming ended".to_string(), String::new()),

                // === Focus management ===
                "zen_mode" => ("zen mode toggled".to_string(), String::new()),
                "visible_editors" => {
                    let n = event.get("selections").and_then(|v| v.as_u64()).unwrap_or(0);
                    if n > 2 {
                        (format!("{n} editors open"), String::new())
                    } else {
                        continue;
                    }
                }

                // === Diagnostics (only label when errors appear/disappear) ===
                "diagnostics" => {
                    let errors = event.get("errors").and_then(|v| v.as_u64()).unwrap_or(0);
                    if errors > 0 {
                        (format!("{errors} errors"), format!("{basename} ({language})"))
                    } else {
                        continue;
                    }
                }

                // === Workspace context ===
                "workspace_add" => ("project opened".to_string(), basename.to_string()),
                "workspace_remove" => ("project closed".to_string(), basename.to_string()),

                // === Multi-cursor (power user pattern) ===
                "multi_cursor" => {
                    let n = event.get("selections").and_then(|v| v.as_u64()).unwrap_or(0);
                    if n >= 3 {
                        (format!("multi-cursor ({n})"), basename.to_string())
                    } else {
                        continue;
                    }
                }

                // === Command execution (navigation, refactoring, search) ===
                "command" => {
                    let cmd_name = command.rsplit('.').next().unwrap_or(command);
                    let category =
                        if command.contains("find") || command.contains("search") || command.contains("quickOpen") {
                            "searching"
                        } else if command.contains("rename")
                            || command.contains("refactor")
                            || command.contains("organizeImports")
                        {
                            "refactoring"
                        } else if command.contains("Definition")
                            || command.contains("References")
                            || command.contains("Implementation")
                        {
                            "navigating"
                        } else if command.contains("format") {
                            "formatting"
                        } else if command.contains("fold") {
                            "folding"
                        } else if command.contains("Zen") || command.contains("split") || command.contains("toggle") {
                            "focus management"
                        } else if command.contains("copilot") || command.contains("inlineChat") {
                            "AI assist"
                        } else if command.contains("debug") {
                            "debugging"
                        } else {
                            cmd_name
                        };
                    (category.to_string(), format!("{cmd_name} in {basename}"))
                }

                // === IntelliSense completion accepted ===
                "completion_accepted" => ("autocomplete accepted".to_string(), format!("{basename} ({language})")),

                // === Clipboard activity ===
                "clipboard_change" => {
                    let lines = event.get("lines_added").and_then(|v| v.as_u64()).unwrap_or(0);
                    if lines > 3 {
                        (
                            "clipboard: large paste".to_string(),
                            format!("{lines} lines in {basename}"),
                        )
                    } else {
                        continue; // small clipboard changes are noise
                    }
                }

                // === Terminal activity ===
                "terminal_focus" => ("terminal focused".to_string(), command.to_string()),
                "terminal_created" => ("terminal opened".to_string(), String::new()),
                "terminal_command_start" if !command.is_empty() => {
                    let short = if command.len() > 40 { &command[..40] } else { command };
                    (format!("running: {short}"), path.to_string())
                }
                "terminal_command_end" => {
                    let code = event.get("exit_code").and_then(|v| v.as_i64());
                    let status = match code {
                        Some(0) => "passed".to_string(),
                        Some(c) => format!("failed (exit {c})"),
                        None => "ended".to_string(),
                    };
                    let short = if command.len() > 30 { &command[..30] } else { command };
                    (format!("{short} {status}"), String::new())
                }
                "zone_switch" => {
                    let from = event.get("source").and_then(|v| v.as_str()).unwrap_or("");
                    (format!("switched to {command}"), format!("from {from}"))
                }

                // === Project file changes (external) ===
                "project_file_changed" => ("project config changed".to_string(), basename.to_string()),

                // === Environment context (one-time, no label needed) ===
                "env_context" => {
                    count += 1;
                    continue;
                }

                // === Low-signal events — count but don't label ===
                _ => {
                    count += 1;
                    continue;
                }
            };

            // Insert the auto-label with inline embeddings for immediate searchability.
            if let Some(ref conn) = labels_conn {
                let text_emb = embedder.embed(&label);
                let ctx_emb = if !ctx.is_empty() { embedder.embed(&ctx) } else { None };
                let text_blob = text_emb.as_ref().map(|v| skill_data::util::f32_to_blob(v));
                let ctx_blob = ctx_emb.as_ref().map(|v| skill_data::util::f32_to_blob(v));
                let model_name = if text_blob.is_some() { Some("nomic-embed-text-v1.5") } else { None };
                let label_id = conn.execute(
                    "INSERT INTO labels (text, context, eeg_start, eeg_end, wall_start, wall_end, created_at, text_embedding, context_embedding, embedding_model)
                     VALUES (?1, ?2, ?3, ?3, ?3, ?3, ?3, ?4, ?5, ?6)",
                    rusqlite::params![label, ctx, now as i64, text_blob, ctx_blob, model_name],
                ).ok().map(|_| conn.last_insert_rowid());
                // Incrementally insert into HNSW index for immediate searchability.
                if let (Some(id), Some(ref te)) = (label_id, &text_emb) {
                    let ce = ctx_emb.as_deref().unwrap_or(&[]);
                    skill_label_index::insert_label(&skill_dir, id, te, ce, now, now, &label_index);
                }
            }
            count += 1;
        }
        count
    })
    .await
    .unwrap_or(0);
    Json(serde_json::json!({"ok": true, "processed": processed}))
}
