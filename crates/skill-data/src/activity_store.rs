// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Persistent activity store — `~/.skill/activity.sqlite`.
//!
//! Three tables live in this database:
//!
//! * **`active_windows`** — one row inserted each time the frontmost window
//!   changes: app name, binary path, window title, and unix-second timestamp.
//!
//! * **`input_activity`** — periodic samples (every 60 s) of the last
//!   keyboard and mouse unix-second timestamps.  A row is only written when at
//!   least one value has changed since the previous flush, so idle periods
//!   produce no rows.
//!
//! * **`input_buckets`** — one row per calendar minute, storing a running count
//!   of keyboard events and mouse/scroll/click events that occurred during that
//!   minute.  Rows are upserted (incremented) by the flush thread every 60 s.
//!   This table is the primary source for activity-over-time charts.
//!
//! All writes come from background threads, so the connection is wrapped in a
//! `Mutex`.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

use crate::active_window::ActiveWindowInfo;
use crate::util::MutexExt;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── DDL ───────────────────────────────────────────────────────────────────────

const DDL: &str = "
CREATE TABLE IF NOT EXISTS active_windows (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    app_name      TEXT    NOT NULL,
    app_path      TEXT    NOT NULL DEFAULT '',
    window_title  TEXT    NOT NULL DEFAULT '',
    activated_at  INTEGER NOT NULL,
    browser_title TEXT,
    monitor_id    INTEGER
);
CREATE INDEX IF NOT EXISTS idx_aw_activated ON active_windows (activated_at DESC);

-- Windows visible on secondary monitors at the time the primary window changed.
CREATE TABLE IF NOT EXISTS secondary_windows (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    primary_id    INTEGER NOT NULL,
    app_name      TEXT    NOT NULL,
    window_title  TEXT    NOT NULL DEFAULT '',
    monitor_id    INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (primary_id) REFERENCES active_windows(id)
);

CREATE TABLE IF NOT EXISTS input_activity (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    last_keyboard INTEGER,           -- unix seconds; NULL = no keyboard this period
    last_mouse    INTEGER,           -- unix seconds; NULL = no mouse this period
    sampled_at    INTEGER NOT NULL   -- when this row was written
);
CREATE INDEX IF NOT EXISTS idx_ia_sampled ON input_activity (sampled_at DESC);

-- Per-minute event-count buckets used for activity charts.
-- minute_ts is the Unix timestamp of the start of the minute (ts - ts % 60).
-- Rows are upserted: counts accumulate across multiple flush cycles that fall
-- within the same calendar minute.
CREATE TABLE IF NOT EXISTS input_buckets (
    minute_ts   INTEGER NOT NULL PRIMARY KEY,
    key_count   INTEGER NOT NULL DEFAULT 0,
    mouse_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_ib_minute ON input_buckets (minute_ts DESC);

-- File interactions: which files the user worked with, attributed to the
-- frontmost application.  One row per (file_path, app_name) transition —
-- de-duplicated by the poller so repeated focus on the same file produces
-- a single row until the user switches away and back.
CREATE TABLE IF NOT EXISTS file_interactions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path     TEXT    NOT NULL,
    app_name      TEXT    NOT NULL DEFAULT '',
    project       TEXT    NOT NULL DEFAULT '',
    language      TEXT    NOT NULL DEFAULT '',
    category      TEXT    NOT NULL DEFAULT '',
    git_branch    TEXT    NOT NULL DEFAULT '',
    seen_at       INTEGER NOT NULL,
    duration_secs INTEGER,
    was_modified  INTEGER NOT NULL DEFAULT 0,
    size_delta    INTEGER NOT NULL DEFAULT 0,
    lines_added   INTEGER NOT NULL DEFAULT 0,
    lines_removed INTEGER NOT NULL DEFAULT 0,
    words_delta   INTEGER NOT NULL DEFAULT 0,
    eeg_focus     REAL,
    eeg_mood      REAL,
    undo_count    INTEGER NOT NULL DEFAULT 0
);

-- Focus sessions — contiguous file-interaction clusters split by idle gaps.
CREATE TABLE IF NOT EXISTS focus_sessions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    start_at    INTEGER NOT NULL,
    end_at      INTEGER NOT NULL,
    project     TEXT    NOT NULL DEFAULT '',
    file_count  INTEGER NOT NULL DEFAULT 0,
    edit_count  INTEGER NOT NULL DEFAULT 0,
    total_lines_added   INTEGER NOT NULL DEFAULT 0,
    total_lines_removed INTEGER NOT NULL DEFAULT 0,
    avg_eeg_focus REAL,
    avg_eeg_mood  REAL
);
CREATE INDEX IF NOT EXISTS idx_fs_start ON focus_sessions (start_at DESC);

-- Build/test outcomes observed in terminal window titles.
CREATE TABLE IF NOT EXISTS build_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    command     TEXT    NOT NULL,
    outcome     TEXT    NOT NULL DEFAULT '',
    project     TEXT    NOT NULL DEFAULT '',
    detected_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_be_detected ON build_events (detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_fi_seen ON file_interactions (seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_fi_path ON file_interactions (file_path);
CREATE INDEX IF NOT EXISTS idx_fi_project ON file_interactions (project);
CREATE INDEX IF NOT EXISTS idx_fi_language ON file_interactions (language);
CREATE INDEX IF NOT EXISTS idx_fi_category ON file_interactions (category);

-- Per-5-second edit chunks within a file interaction.
-- Gives a granular timeline of when edits happened during a focus session.
CREATE TABLE IF NOT EXISTS file_edit_chunks (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    interaction_id INTEGER NOT NULL,
    chunk_at       INTEGER NOT NULL,
    lines_added    INTEGER NOT NULL DEFAULT 0,
    lines_removed  INTEGER NOT NULL DEFAULT 0,
    size_delta     INTEGER NOT NULL DEFAULT 0,
    undo_estimate  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_fec_interaction ON file_edit_chunks (interaction_id);
CREATE INDEX IF NOT EXISTS idx_fec_chunk ON file_edit_chunks (chunk_at DESC);

-- Meeting/call events detected from window titles (Zoom, Teams, Slack, etc.).
CREATE TABLE IF NOT EXISTS meeting_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    platform    TEXT    NOT NULL,
    title       TEXT    NOT NULL DEFAULT '',
    app_name    TEXT    NOT NULL DEFAULT '',
    start_at    INTEGER NOT NULL,
    end_at      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_me_start ON meeting_events (start_at DESC);

-- Clipboard change events (metadata only — content is never stored).
CREATE TABLE IF NOT EXISTS clipboard_events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    source_app   TEXT    NOT NULL DEFAULT '',
    content_type TEXT    NOT NULL DEFAULT 'text',
    content_size INTEGER NOT NULL DEFAULT 0,
    copied_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ce_copied ON clipboard_events (copied_at DESC);

-- AI code suggestion and chat events.
CREATE TABLE IF NOT EXISTS ai_events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type   TEXT    NOT NULL,
    source       TEXT    NOT NULL DEFAULT '',
    file_path    TEXT    NOT NULL DEFAULT '',
    language     TEXT    NOT NULL DEFAULT '',
    at           INTEGER NOT NULL,
    eeg_focus    REAL,
    eeg_mood     REAL
);
CREATE INDEX IF NOT EXISTS idx_ai_at ON ai_events (at DESC);
";

// ── Store ─────────────────────────────────────────────────────────────────────

pub struct ActivityStore {
    conn: Mutex<Connection>,
}

impl ActivityStore {
    /// Open (or create) the activity database inside `skill_dir`.
    /// Returns `None` only when SQLite cannot open the file at all.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let path = skill_dir.join(skill_constants::ACTIVITY_FILE);
        let conn = match Connection::open(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[activity] open {}: {e}", path.display());
                return None;
            }
        };
        crate::util::init_wal_pragmas(&conn);
        // Schema migrations BEFORE DDL — add columns that may be missing from
        // older databases so the DDL (which uses CREATE TABLE IF NOT EXISTS)
        // succeeds. Each ALTER is idempotent: "duplicate column" errors ignored.
        for alter in [
            // active_windows
            "ALTER TABLE active_windows ADD COLUMN browser_title TEXT",
            "ALTER TABLE active_windows ADD COLUMN monitor_id INTEGER",
            // file_interactions — columns added across multiple releases
            "ALTER TABLE file_interactions ADD COLUMN project TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE file_interactions ADD COLUMN language TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE file_interactions ADD COLUMN category TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE file_interactions ADD COLUMN git_branch TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE file_interactions ADD COLUMN duration_secs INTEGER",
            "ALTER TABLE file_interactions ADD COLUMN was_modified INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE file_interactions ADD COLUMN size_delta INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE file_interactions ADD COLUMN lines_added INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE file_interactions ADD COLUMN lines_removed INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE file_interactions ADD COLUMN words_delta INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE file_interactions ADD COLUMN eeg_focus REAL",
            "ALTER TABLE file_interactions ADD COLUMN eeg_mood REAL",
            "ALTER TABLE file_interactions ADD COLUMN undo_count INTEGER NOT NULL DEFAULT 0",
            // file_edit_chunks
            "ALTER TABLE file_edit_chunks ADD COLUMN undo_estimate INTEGER NOT NULL DEFAULT 0",
            // focus_sessions
            "ALTER TABLE focus_sessions ADD COLUMN project TEXT NOT NULL DEFAULT ''",
        ] {
            let _ = conn.execute_batch(alter);
        }
        if let Err(e) = conn.execute_batch(DDL) {
            eprintln!("[activity] DDL: {e}");
            return None;
        }
        Some(Self { conn: Mutex::new(conn) })
    }

    // ── Writers ───────────────────────────────────────────────────────────────

    /// Record that the frontmost window changed to `info`.
    /// Returns the row id of the newly inserted row.
    pub fn insert_active_window(&self, info: &ActiveWindowInfo) -> Option<i64> {
        let c = self.conn.lock_or_recover();
        match c.execute(
            "INSERT INTO active_windows (app_name, app_path, window_title, activated_at, browser_title, monitor_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &info.app_name,
                &info.app_path,
                &info.window_title,
                info.activated_at as i64,
                &info.browser_title,
                info.monitor_id.map(|m| m as i64),
            ],
        ) {
            Ok(_) => Some(c.last_insert_rowid()),
            Err(e) => {
                eprintln!("[activity] insert_active_window: {e}");
                None
            }
        }
    }

    /// Record windows visible on secondary monitors at the time of a window change.
    pub fn insert_secondary_windows(&self, primary_id: i64, windows: &[crate::active_window::SecondaryWindowInfo]) {
        if windows.is_empty() {
            return;
        }
        let c = self.conn.lock_or_recover();
        for w in windows {
            let _ = c.execute(
                "INSERT INTO secondary_windows (primary_id, app_name, window_title, monitor_id)
                 VALUES (?1, ?2, ?3, ?4)",
                params![primary_id, &w.app_name, &w.window_title, w.monitor_id as i64],
            );
        }
    }

    /// Flush current last-keyboard / last-mouse timestamps to the database.
    /// `None` means the device type was never used since tracking started.
    pub fn insert_input_activity(&self, last_keyboard: Option<u64>, last_mouse: Option<u64>, sampled_at: u64) {
        let c = self.conn.lock_or_recover();
        if let Err(e) = c.execute(
            "INSERT INTO input_activity (last_keyboard, last_mouse, sampled_at)
             VALUES (?1, ?2, ?3)",
            params![
                last_keyboard.map(|t| t as i64),
                last_mouse.map(|t| t as i64),
                sampled_at as i64,
            ],
        ) {
            eprintln!("[activity] insert_input_activity: {e}");
        }
    }

    /// Increment (or create) the per-minute bucket for `minute_ts`.
    /// `minute_ts` must already be rounded to a 60-second boundary.
    /// `key_delta` / `mouse_delta` are the number of events since the last flush.
    pub fn upsert_input_bucket(&self, minute_ts: u64, key_delta: u64, mouse_delta: u64) {
        if key_delta == 0 && mouse_delta == 0 {
            return;
        }
        let c = self.conn.lock_or_recover();
        if let Err(e) = c.execute(
            "INSERT INTO input_buckets (minute_ts, key_count, mouse_count)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(minute_ts) DO UPDATE SET
                 key_count   = key_count   + excluded.key_count,
                 mouse_count = mouse_count + excluded.mouse_count",
            params![minute_ts as i64, key_delta as i64, mouse_delta as i64,],
        ) {
            eprintln!("[activity] upsert_input_bucket: {e}");
        }
    }

    // ── Readers ───────────────────────────────────────────────────────────────

    /// Return the `limit` most recent active-window records, newest first.
    pub fn get_recent_windows(&self, limit: u32) -> Vec<ActiveWindowRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, app_name, app_path, window_title, activated_at, browser_title, monitor_id
             FROM active_windows ORDER BY activated_at DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] prepare recent_windows: {e}");
                return vec![];
            }
        };
        stmt.query_map([limit as i64], |row| {
            Ok(ActiveWindowRow {
                id: row.get(0)?,
                app_name: row.get(1)?,
                app_path: row.get(2)?,
                window_title: row.get(3)?,
                activated_at: row.get::<_, i64>(4)? as u64,
                browser_title: row.get(5)?,
                monitor_id: row.get::<_, Option<i64>>(6)?.map(|m| m as u32),
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return the `limit` most recent input-activity samples, newest first.
    pub fn get_recent_input(&self, limit: u32) -> Vec<InputActivityRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, last_keyboard, last_mouse, sampled_at
             FROM input_activity ORDER BY sampled_at DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] prepare recent_input: {e}");
                return vec![];
            }
        };
        stmt.query_map([limit as i64], |row| {
            Ok(InputActivityRow {
                id: row.get(0)?,
                last_keyboard: row.get::<_, Option<i64>>(1)?.map(|t| t as u64),
                last_mouse: row.get::<_, Option<i64>>(2)?.map(|t| t as u64),
                sampled_at: row.get::<_, i64>(3)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return the top `limit` most-used apps by number of active-window
    /// switches, optionally filtered to windows activated at or after `since`.
    pub fn top_apps(&self, limit: u32, since: Option<u64>) -> Vec<AppUsageRow> {
        let c = self.conn.lock_or_recover();
        match since {
            Some(ts) => {
                let mut stmt = match c.prepare_cached(
                    "SELECT app_name, COUNT(*) AS cnt, MAX(activated_at) AS last_seen
                     FROM active_windows WHERE activated_at >= ?1
                     GROUP BY app_name ORDER BY cnt DESC LIMIT ?2",
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("[activity] top_apps: {e}");
                        return vec![];
                    }
                };
                stmt.query_map(params![ts as i64, limit as i64], |row| {
                    Ok(AppUsageRow {
                        app_name: row.get(0)?,
                        switches: row.get::<_, i64>(1)? as u64,
                        last_seen: row.get::<_, i64>(2)? as u64,
                    })
                })
                .map(|rows| rows.filter_map(std::result::Result::ok).collect())
                .unwrap_or_default()
            }
            None => {
                let mut stmt = match c.prepare_cached(
                    "SELECT app_name, COUNT(*) AS cnt, MAX(activated_at) AS last_seen
                     FROM active_windows
                     GROUP BY app_name ORDER BY cnt DESC LIMIT ?1",
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("[activity] top_apps: {e}");
                        return vec![];
                    }
                };
                stmt.query_map(params![limit as i64], |row| {
                    Ok(AppUsageRow {
                        app_name: row.get(0)?,
                        switches: row.get::<_, i64>(1)? as u64,
                        last_seen: row.get::<_, i64>(2)? as u64,
                    })
                })
                .map(|rows| rows.filter_map(std::result::Result::ok).collect())
                .unwrap_or_default()
            }
        }
    }

    // ── File interactions ──────────────────────────────────────────────────────

    /// Record that the user interacted with `file_path` via `app_name`.
    /// Returns the row id of the newly inserted row.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_file_interaction(
        &self,
        file_path: &str,
        app_name: &str,
        project: &str,
        language: &str,
        category: &str,
        git_branch: &str,
        seen_at: u64,
        eeg_focus: Option<f32>,
        eeg_mood: Option<f32>,
    ) -> Option<i64> {
        let c = self.conn.lock_or_recover();
        match c.execute(
            "INSERT INTO file_interactions
             (file_path, app_name, project, language, category, git_branch, seen_at, eeg_focus, eeg_mood)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                file_path,
                app_name,
                project,
                language,
                category,
                git_branch,
                seen_at as i64,
                eeg_focus,
                eeg_mood,
            ],
        ) {
            Ok(_) => Some(c.last_insert_rowid()),
            Err(e) => {
                eprintln!("[activity] insert_file_interaction: {e}");
                None
            }
        }
    }

    /// Back-fill duration and modification stats for a previously inserted row.
    #[allow(clippy::too_many_arguments)]
    pub fn finalize_file_interaction(
        &self,
        row_id: i64,
        duration_secs: u64,
        was_modified: bool,
        size_delta: i64,
        lines_added: u64,
        lines_removed: u64,
        words_delta: i64,
        undo_count: u64,
    ) {
        let c = self.conn.lock_or_recover();
        if let Err(e) = c.execute(
            "UPDATE file_interactions
             SET duration_secs = ?1, was_modified = ?2, size_delta = ?3,
                 lines_added = ?4, lines_removed = ?5, words_delta = ?6,
                 undo_count = ?7
             WHERE id = ?8",
            params![
                duration_secs as i64,
                was_modified as i64,
                size_delta,
                lines_added as i64,
                lines_removed as i64,
                words_delta,
                undo_count as i64,
                row_id,
            ],
        ) {
            eprintln!("[activity] finalize_file_interaction: {e}");
        }
    }

    // ── Analytics queries ──────────────────────────────────────────────────────

    /// Language breakdown: total time and edit count per language.
    pub fn language_breakdown(&self, since: Option<u64>) -> Vec<LanguageBreakdownRow> {
        let c = self.conn.lock_or_recover();
        let (sql, p): (&str, Vec<i64>) = match since {
            Some(ts) => (
                "SELECT language, COUNT(*) AS cnt, SUM(was_modified) AS edits,
                        COALESCE(SUM(duration_secs), 0) AS total
                 FROM file_interactions WHERE language != '' AND seen_at >= ?1
                 GROUP BY language ORDER BY total DESC",
                vec![ts as i64],
            ),
            None => (
                "SELECT language, COUNT(*) AS cnt, SUM(was_modified) AS edits,
                        COALESCE(SUM(duration_secs), 0) AS total
                 FROM file_interactions WHERE language != ''
                 GROUP BY language ORDER BY total DESC",
                vec![],
            ),
        };
        let mut stmt = match c.prepare_cached(sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] language_breakdown: {e}");
                return vec![];
            }
        };
        let params: Vec<&dyn rusqlite::types::ToSql> = p.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();
        stmt.query_map(params.as_slice(), |row| {
            Ok(LanguageBreakdownRow {
                language: row.get(0)?,
                interactions: row.get::<_, i64>(1)? as u64,
                edits: row.get::<_, i64>(2)? as u64,
                total_secs: row.get::<_, i64>(3)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Context-switch frequency: file switches per minute in a time range.
    pub fn context_switch_rate(&self, from_ts: u64, to_ts: u64) -> f64 {
        let c = self.conn.lock_or_recover();
        let count: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM file_interactions WHERE seen_at >= ?1 AND seen_at <= ?2",
                params![from_ts as i64, to_ts as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let minutes = (to_ts.saturating_sub(from_ts) as f64) / 60.0;
        if minutes > 0.0 {
            count as f64 / minutes
        } else {
            0.0
        }
    }

    /// Co-editing pairs: files frequently edited together within `window_secs`.
    pub fn coedited_files(&self, window_secs: u64, limit: u32, since: Option<u64>) -> Vec<CoEditRow> {
        let c = self.conn.lock_or_recover();
        let since_ts = since.unwrap_or(0) as i64;
        let mut stmt = match c.prepare_cached(
            "SELECT a.file_path, b.file_path, COUNT(*) AS cnt
             FROM file_interactions a
             JOIN file_interactions b
               ON a.id < b.id
              AND a.file_path != b.file_path
              AND ABS(a.seen_at - b.seen_at) <= ?1
              AND a.seen_at >= ?2 AND b.seen_at >= ?2
             GROUP BY a.file_path, b.file_path
             ORDER BY cnt DESC LIMIT ?3",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] coedited_files: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![window_secs as i64, since_ts, limit as i64], |row| {
            Ok(CoEditRow {
                file_a: row.get(0)?,
                file_b: row.get(1)?,
                co_occurrences: row.get::<_, i64>(2)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Daily summary for a given day (unix timestamp of midnight).
    pub fn daily_summary(&self, day_start: u64) -> DailySummaryRow {
        let c = self.conn.lock_or_recover();
        let day_end = day_start + 86400;
        let row = c.query_row(
            "SELECT COUNT(*) AS interactions,
                    SUM(was_modified) AS edits,
                    COALESCE(SUM(duration_secs), 0) AS total_secs,
                    COALESCE(SUM(lines_added), 0) AS added,
                    COALESCE(SUM(lines_removed), 0) AS removed,
                    COUNT(DISTINCT project) AS projects,
                    COUNT(DISTINCT file_path) AS files,
                    AVG(CASE WHEN eeg_focus IS NOT NULL THEN eeg_focus END) AS avg_focus
             FROM file_interactions WHERE seen_at >= ?1 AND seen_at < ?2",
            params![day_start as i64, day_end as i64],
            |row| {
                Ok(DailySummaryRow {
                    day_start,
                    interactions: row.get::<_, i64>(0)? as u64,
                    edits: row.get::<_, i64>(1)? as u64,
                    total_secs: row.get::<_, i64>(2)? as u64,
                    lines_added: row.get::<_, i64>(3)? as u64,
                    lines_removed: row.get::<_, i64>(4)? as u64,
                    distinct_projects: row.get::<_, i64>(5)? as u64,
                    distinct_files: row.get::<_, i64>(6)? as u64,
                    avg_eeg_focus: row.get::<_, Option<f64>>(7)?.map(|v| v as f32),
                })
            },
        );
        row.unwrap_or(DailySummaryRow {
            day_start,
            ..Default::default()
        })
    }

    /// Hourly edit heatmap: lines changed per hour of day (0-23).
    pub fn hourly_edit_heatmap(&self, since: Option<u64>) -> Vec<HourlyEditRow> {
        let c = self.conn.lock_or_recover();
        let since_ts = since.unwrap_or(0) as i64;
        let mut stmt = match c.prepare_cached(
            "SELECT (seen_at % 86400) / 3600 AS hour,
                    COUNT(*) AS interactions,
                    COALESCE(SUM(lines_added + lines_removed), 0) AS total_churn,
                    AVG(CASE WHEN eeg_focus IS NOT NULL THEN eeg_focus END) AS avg_focus
             FROM file_interactions WHERE seen_at >= ?1
             GROUP BY hour ORDER BY hour",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] hourly_edit_heatmap: {e}");
                return vec![];
            }
        };
        stmt.query_map([since_ts], |row| {
            Ok(HourlyEditRow {
                hour: row.get::<_, i64>(0)? as u8,
                interactions: row.get::<_, i64>(1)? as u64,
                total_churn: row.get::<_, i64>(2)? as u64,
                avg_eeg_focus: row.get::<_, Option<f64>>(3)?.map(|v| v as f32),
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Files modified but not in a clean git state ("forgotten files").
    /// Returns file_interactions where was_modified=1, grouped by file,
    /// that the caller can cross-check against `git status`.
    pub fn modified_files_since(&self, since: u64) -> Vec<String> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT DISTINCT file_path FROM file_interactions
             WHERE was_modified = 1 AND seen_at >= ?1
             ORDER BY MAX(seen_at) DESC",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] modified_files_since: {e}");
                return vec![];
            }
        };
        stmt.query_map([since as i64], |row| row.get::<_, String>(0))
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default()
    }

    // ── Analysis ─────────────────────────────────────────────────────────────

    /// Compute a productivity score for a day (0–100).
    /// Composite of: focus session time, edit velocity, low context-switch rate, EEG focus.
    pub fn productivity_score(&self, day_start: u64) -> ProductivityScore {
        let summary = self.daily_summary(day_start);
        let day_end = day_start + 86400;
        let switch_rate = self.context_switch_rate(day_start, day_end);
        let sessions = self.get_focus_sessions_in_range(day_start, day_end);

        // Deep work minutes = sum of focus sessions > 15 min.
        let deep_work_secs: u64 = sessions
            .iter()
            .filter(|s| s.end_at - s.start_at >= 900)
            .map(|s| s.end_at - s.start_at)
            .sum();

        // Scoring (each 0–25, total 0–100):
        // 1. Edit velocity: lines changed per hour (cap at 200 lines/hr = 25 pts).
        let hours = (summary.total_secs as f64 / 3600.0).max(0.01);
        let churn = (summary.lines_added + summary.lines_removed) as f64;
        let velocity_score = ((churn / hours) / 200.0 * 25.0).min(25.0);

        // 2. Deep work: minutes of sustained focus (cap at 120 min = 25 pts).
        let deep_score = ((deep_work_secs as f64 / 60.0) / 120.0 * 25.0).min(25.0);

        // 3. Context stability: low switch rate is better (0 switches = 25, 10+/min = 0).
        let switch_score = ((10.0 - switch_rate) / 10.0 * 25.0).clamp(0.0, 25.0);

        // 4. EEG focus (if available): avg focus 0-100 mapped to 0-25.
        let eeg_score = summary
            .avg_eeg_focus
            .map(|f| (f as f64 / 100.0 * 25.0).min(25.0))
            .unwrap_or(0.0);

        let total = velocity_score + deep_score + switch_score + eeg_score;

        ProductivityScore {
            day_start,
            score: total as f32,
            edit_velocity: velocity_score as f32,
            deep_work: deep_score as f32,
            context_stability: switch_score as f32,
            eeg_focus: eeg_score as f32,
            deep_work_minutes: (deep_work_secs / 60) as u32,
            switch_rate: switch_rate as f32,
        }
    }

    /// Weekly digest: aggregate stats for 7 days starting at `week_start`.
    pub fn weekly_digest(&self, week_start: u64) -> WeeklyDigest {
        let mut days = Vec::with_capacity(7);
        let mut total_interactions = 0u64;
        let mut total_edits = 0u64;
        let mut total_secs = 0u64;
        let mut total_added = 0u64;
        let mut total_removed = 0u64;
        let mut focus_sum = 0.0f64;
        let mut focus_count = 0u32;

        for d in 0..7u64 {
            let day = self.daily_summary(week_start + d * 86400);
            total_interactions += day.interactions;
            total_edits += day.edits;
            total_secs += day.total_secs;
            total_added += day.lines_added;
            total_removed += day.lines_removed;
            if let Some(f) = day.avg_eeg_focus {
                focus_sum += f as f64;
                focus_count += 1;
            }
            days.push(day);
        }

        let week_end = week_start + 7 * 86400;
        let top_projects = self.top_projects(5, Some(week_start));
        let languages = self.language_breakdown(Some(week_start));
        let sessions = self.get_focus_sessions_in_range(week_start, week_end);
        let meetings = self.get_meetings_in_range(week_start, week_end);

        // Find peak day (most edits).
        let peak_day_idx = days
            .iter()
            .enumerate()
            .max_by_key(|(_, d)| d.edits)
            .map(|(i, _)| i as u8)
            .unwrap_or(0);

        // Find peak hour from heatmap.
        let heatmap = self.hourly_edit_heatmap(Some(week_start));
        let peak_hour = heatmap
            .iter()
            .max_by_key(|h| h.total_churn)
            .map(|h| h.hour)
            .unwrap_or(0);

        WeeklyDigest {
            week_start,
            days,
            total_interactions,
            total_edits,
            total_secs,
            total_lines_added: total_added,
            total_lines_removed: total_removed,
            avg_eeg_focus: if focus_count > 0 {
                Some((focus_sum / focus_count as f64) as f32)
            } else {
                None
            },
            top_projects,
            top_languages: languages,
            focus_session_count: sessions.len() as u32,
            meeting_count: meetings.len() as u32,
            peak_day_idx,
            peak_hour,
        }
    }

    /// Detect files that haven't been touched in `threshold_days` but were
    /// modified within `since` — potential stale/abandoned work.
    pub fn stale_files(&self, threshold_days: u32, since: u64) -> Vec<StaleFileRow> {
        let c = self.conn.lock_or_recover();
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(threshold_days as u64 * 86400);
        let mut stmt = match c.prepare_cached(
            "SELECT file_path, MAX(seen_at) AS last_seen, SUM(was_modified) AS total_edits,
                    project, language
             FROM file_interactions
             WHERE seen_at >= ?1
             GROUP BY file_path
             HAVING last_seen < ?2 AND total_edits > 0
             ORDER BY last_seen ASC
             LIMIT 50",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] stale_files: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![since as i64, cutoff as i64], |row| {
            Ok(StaleFileRow {
                file_path: row.get(0)?,
                last_seen: row.get::<_, i64>(1)? as u64,
                total_edits: row.get::<_, i64>(2)? as u64,
                project: row.get(3)?,
                language: row.get(4)?,
                days_stale: ((std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    - row.get::<_, i64>(1)? as u64)
                    / 86400) as u32,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Brain awareness ──────────────────────────────────────────────────────

    /// Real-time flow state check: is the user currently in deep focus?
    #[allow(clippy::manual_let_else)]
    pub fn flow_state_now(&self, window_secs: u64) -> FlowStateResult {
        let c = self.conn.lock_or_recover();
        let now = now_secs();
        let since = now.saturating_sub(window_secs);
        let row = c.query_row(
            "SELECT COUNT(DISTINCT file_path) AS switches, COUNT(*) AS interactions,
                    COALESCE(SUM(lines_added + lines_removed), 0) AS churn,
                    AVG(CASE WHEN eeg_focus IS NOT NULL THEN eeg_focus END) AS avg_focus,
                    MIN(seen_at) AS first_ts, MAX(seen_at) AS last_ts
             FROM file_interactions WHERE seen_at >= ?1",
            params![since as i64],
            |row| {
                let switches: u64 = row.get::<_, i64>(0)? as u64;
                let _interactions: u64 = row.get::<_, i64>(1)? as u64;
                let churn: u64 = row.get::<_, i64>(2)? as u64;
                let avg_focus: Option<f32> = row.get::<_, Option<f64>>(3)?.map(|v| v as f32);
                let first_ts: u64 = row.get::<_, Option<i64>>(4)?.unwrap_or(0) as u64;
                let last_ts: u64 = row.get::<_, Option<i64>>(5)?.unwrap_or(0) as u64;
                let duration = last_ts.saturating_sub(first_ts);
                let focus = avg_focus.unwrap_or(0.0);
                let in_flow = focus > 60.0 && switches <= 3 && churn > 0 && duration > 120;
                let velocity = if duration > 0 {
                    churn as f32 / (duration as f32 / 60.0)
                } else {
                    0.0
                };
                let score = (focus * 0.5 + (100.0 - switches.min(10) as f32 * 10.0) * 0.3 + velocity.min(50.0) * 0.4)
                    .clamp(0.0, 100.0);
                Ok(FlowStateResult {
                    in_flow,
                    score,
                    duration_secs: duration,
                    avg_focus,
                    file_switches: switches as u32,
                    edit_velocity: velocity,
                })
            },
        );
        row.unwrap_or(FlowStateResult {
            in_flow: false,
            score: 0.0,
            duration_secs: 0,
            avg_focus: None,
            file_switches: 0,
            edit_velocity: 0.0,
        })
    }

    /// Cognitive load aggregated by file or language.
    pub fn cognitive_load_by(&self, since: u64, by_language: bool) -> Vec<CognitiveLoadRow> {
        let c = self.conn.lock_or_recover();
        let group_col = if by_language { "language" } else { "file_path" };
        let sql = format!(
            "SELECT {g}, AVG(eeg_focus) AS avg_focus, AVG(undo_count) AS avg_undos,
                    COUNT(*) AS interactions, COALESCE(SUM(duration_secs), 0) AS total_secs
             FROM file_interactions WHERE seen_at >= ?1 AND {g} != '' AND eeg_focus IS NOT NULL
             GROUP BY {g} ORDER BY avg_focus ASC LIMIT 30",
            g = group_col
        );
        let mut stmt = match c.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([since as i64], |row| {
            let avg_focus: f32 = row.get::<_, f64>(1).unwrap_or(50.0) as f32;
            let avg_undos: f32 = row.get::<_, f64>(2).unwrap_or(0.0) as f32;
            Ok(CognitiveLoadRow {
                key: row.get(0)?,
                avg_focus: Some(avg_focus),
                avg_undos,
                interactions: row.get::<_, i64>(3)? as u64,
                total_secs: row.get::<_, i64>(4)? as u64,
                load_score: ((100.0 - avg_focus) + avg_undos * 5.0).clamp(0.0, 100.0),
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Measure focus recovery time after each meeting.
    pub fn meeting_recovery_times(&self, since: u64, limit: u32) -> MeetingRecoveryResult {
        let meetings = self.get_meetings_in_range(since, now_secs());
        let c = self.conn.lock_or_recover();
        let mut rows = Vec::new();
        let mut total_recovery = 0u64;
        let mut count = 0u32;
        for mtg in meetings.iter().take(limit as usize) {
            let end = match mtg.end_at {
                Some(e) => e,
                None => continue,
            };
            let recovery: Option<u64> = c
                .query_row(
                    "SELECT MIN(seen_at) FROM file_interactions WHERE seen_at > ?1 AND eeg_focus > 50",
                    params![end as i64],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .ok()
                .flatten()
                .map(|ts| (ts as u64).saturating_sub(end));
            if let Some(r) = recovery {
                total_recovery += r;
                count += 1;
            }
            rows.push(MeetingRecoveryRow {
                meeting_id: mtg.id,
                title: mtg.title.clone(),
                platform: mtg.platform.clone(),
                meeting_duration_secs: end.saturating_sub(mtg.start_at),
                recovery_secs: recovery,
            });
        }
        let avg = if count > 0 { total_recovery / count as u64 } else { 0 };
        MeetingRecoveryResult {
            meetings: rows,
            avg_recovery_secs: avg,
        }
    }

    /// Score each hour of the day by productivity.
    pub fn optimal_hours(&self, since: u64, top_n: usize) -> OptimalHoursResult {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT (seen_at % 86400) / 3600 AS hour,
                    COUNT(*) AS interactions,
                    COALESCE(SUM(lines_added + lines_removed), 0) AS churn,
                    AVG(CASE WHEN eeg_focus IS NOT NULL THEN eeg_focus END) AS avg_focus
             FROM file_interactions WHERE seen_at >= ?1 AND was_modified = 1
             GROUP BY hour ORDER BY hour",
        ) {
            Ok(s) => s,
            Err(_) => {
                return OptimalHoursResult {
                    hours: vec![],
                    best_hours: vec![],
                    worst_hours: vec![],
                }
            }
        };
        let mut hours: Vec<HourScore> = stmt
            .query_map([since as i64], |row| {
                Ok(HourScore {
                    hour: row.get::<_, i64>(0)? as u8,
                    interactions: row.get::<_, i64>(1)? as u64,
                    total_churn: row.get::<_, i64>(2)? as u64,
                    avg_focus: row.get::<_, Option<f64>>(3)?.map(|v| v as f32),
                    score: 0.0,
                })
            })
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();
        let max_churn = hours.iter().map(|h| h.total_churn).max().unwrap_or(1).max(1) as f32;
        for h in &mut hours {
            let focus = h.avg_focus.unwrap_or(50.0);
            h.score = focus * 0.6 + (h.total_churn as f32 / max_churn) * 40.0;
        }
        let mut sorted: Vec<(u8, f32)> = hours.iter().map(|h| (h.hour, h.score)).collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let best: Vec<u8> = sorted.iter().take(top_n).map(|s| s.0).collect();
        let worst: Vec<u8> = sorted.iter().rev().take(top_n).map(|s| s.0).collect();
        OptimalHoursResult {
            hours,
            best_hours: best,
            worst_hours: worst,
        }
    }

    /// Check for declining focus trend (fatigue).
    pub fn fatigue_check(&self) -> FatigueAlert {
        let c = self.conn.lock_or_recover();
        let now = now_secs();
        let mut stmt = match c.prepare_cached(
            "SELECT (?1 - seen_at) / 900 AS quarter, AVG(eeg_focus) AS avg_focus, COUNT(*) AS n
             FROM file_interactions WHERE seen_at >= ?1 - 3600 AND eeg_focus IS NOT NULL
             GROUP BY quarter ORDER BY quarter DESC",
        ) {
            Ok(s) => s,
            Err(_) => {
                return FatigueAlert {
                    fatigued: false,
                    trend: vec![],
                    focus_decline_pct: 0.0,
                    suggestion: String::new(),
                    continuous_work_mins: 0,
                }
            }
        };
        let buckets: Vec<FatigueBucket> = stmt
            .query_map([now as i64], |row| {
                Ok(FatigueBucket {
                    quarter: row.get::<_, i64>(0)? as u8,
                    avg_focus: row.get::<_, f64>(1).unwrap_or(50.0) as f32,
                    interactions: row.get::<_, i64>(2)? as u64,
                })
            })
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();
        let decline = if buckets.len() >= 2 {
            let newest = buckets.first().map(|b| b.avg_focus).unwrap_or(50.0);
            let oldest = buckets.last().map(|b| b.avg_focus).unwrap_or(50.0);
            if oldest > 0.0 {
                (newest - oldest) / oldest * 100.0
            } else {
                0.0
            }
        } else {
            0.0
        };
        let fatigued = decline < -10.0 && buckets.len() >= 3;
        let suggestion = if fatigued {
            "Consider a 10-minute break — your focus has been declining.".to_string()
        } else {
            String::new()
        };
        // Continuous work: time since last gap > 10min in file_interactions.
        let continuous: u64 = c
            .query_row(
                "SELECT MIN(seen_at) FROM file_interactions WHERE seen_at >= ?1 - 7200",
                params![now as i64],
                |row| row.get::<_, Option<i64>>(0),
            )
            .ok()
            .flatten()
            .map(|ts| (now - ts as u64) / 60)
            .unwrap_or(0);
        FatigueAlert {
            fatigued,
            trend: buckets,
            focus_decline_pct: decline,
            suggestion,
            continuous_work_mins: continuous,
        }
    }

    /// Files with high undo counts and low focus — struggling indicators.
    pub fn undo_struggle(&self, since: u64, undo_threshold: u64) -> Vec<StruggleRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT file_path, language, project, SUM(undo_count) AS total_undos,
                    AVG(eeg_focus) AS avg_focus, COALESCE(SUM(lines_added + lines_removed), 0) AS churn,
                    COALESCE(SUM(duration_secs), 0) AS total_secs
             FROM file_interactions WHERE seen_at >= ?1 AND undo_count > 0
             GROUP BY file_path HAVING total_undos >= ?2
             ORDER BY total_undos DESC LIMIT 20",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64, undo_threshold as i64], |row| {
            let undos: f32 = row.get::<_, i64>(3)? as f32;
            let focus: f32 = row.get::<_, Option<f64>>(4)?.unwrap_or(50.0) as f32;
            Ok(StruggleRow {
                file_path: row.get(0)?,
                language: row.get::<_, String>(1).unwrap_or_default(),
                project: row.get::<_, String>(2).unwrap_or_default(),
                total_undos: undos as u64,
                avg_focus: Some(focus),
                total_churn: row.get::<_, i64>(5)? as u64,
                total_secs: row.get::<_, i64>(6)? as u64,
                struggle_score: (undos * (100.0 - focus) / 100.0).clamp(0.0, 100.0),
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Daily brain report split by morning/afternoon/evening.
    pub fn daily_brain_report(&self, day_start: u64) -> DailyBrainReport {
        let c = self.conn.lock_or_recover();
        let day_end = day_start + 86400;
        let mut stmt = match c.prepare_cached(
            "SELECT CASE
                WHEN (seen_at % 86400) / 3600 BETWEEN 6 AND 11 THEN 'morning'
                WHEN (seen_at % 86400) / 3600 BETWEEN 12 AND 17 THEN 'afternoon'
                ELSE 'evening'
             END AS period,
             COUNT(*) AS n, COALESCE(SUM(lines_added + lines_removed), 0) AS churn,
             AVG(CASE WHEN eeg_focus IS NOT NULL THEN eeg_focus END) AS avg_focus,
             SUM(undo_count) AS undos, COUNT(DISTINCT file_path) AS files
             FROM file_interactions WHERE seen_at >= ?1 AND seen_at < ?2
             GROUP BY period",
        ) {
            Ok(s) => s,
            Err(_) => return DailyBrainReport::default_for(day_start),
        };
        let periods: Vec<PeriodSummary> = stmt
            .query_map(params![day_start as i64, day_end as i64], |row| {
                Ok(PeriodSummary {
                    period: row.get(0)?,
                    avg_focus: row.get::<_, Option<f64>>(3)?.map(|v| v as f32),
                    churn: row.get::<_, i64>(2)? as u64,
                    interactions: row.get::<_, i64>(1)? as u64,
                    files_touched: row.get::<_, i64>(5)? as u32,
                    undos: row.get::<_, i64>(4)? as u64,
                })
            })
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();
        let best = periods
            .iter()
            .max_by(|a, b| {
                a.avg_focus
                    .unwrap_or(0.0)
                    .partial_cmp(&b.avg_focus.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.period.clone())
            .unwrap_or_default();
        let overall = periods.iter().filter_map(|p| p.avg_focus).sum::<f32>()
            / periods.iter().filter(|p| p.avg_focus.is_some()).count().max(1) as f32;
        let score = self.productivity_score(day_start);
        DailyBrainReport {
            day_start,
            periods,
            overall_focus: Some(overall),
            productivity_score: score.score,
            best_period: best,
        }
    }

    /// Detect natural focus cycle length from 5-minute buckets.
    pub fn break_timing(&self, since: u64) -> BreakTimingResult {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT (seen_at / 300) * 300 AS bucket, AVG(eeg_focus) AS avg_focus, COUNT(*) AS n
             FROM file_interactions WHERE seen_at >= ?1 AND eeg_focus IS NOT NULL
             GROUP BY bucket ORDER BY bucket ASC",
        ) {
            Ok(s) => s,
            Err(_) => {
                return BreakTimingResult {
                    natural_cycle_mins: None,
                    focus_curve: vec![],
                    suggested_break_interval_mins: 52,
                    confidence: 0.0,
                }
            }
        };
        let curve: Vec<FocusBucket> = stmt
            .query_map([since as i64], |row| {
                Ok(FocusBucket {
                    ts: row.get::<_, i64>(0)? as u64,
                    avg_focus: row.get::<_, f64>(1).unwrap_or(50.0) as f32,
                    churn: row.get::<_, i64>(2)? as u64,
                })
            })
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();
        // Simple valley detection: find local minima.
        let mut valleys = Vec::new();
        for i in 1..curve.len().saturating_sub(1) {
            if curve[i].avg_focus < curve[i - 1].avg_focus && curve[i].avg_focus < curve[i + 1].avg_focus {
                valleys.push(curve[i].ts);
            }
        }
        // Average interval between valleys.
        let cycle = if valleys.len() >= 2 {
            let intervals: Vec<u64> = valleys.windows(2).map(|w| w[1] - w[0]).collect();
            let avg = intervals.iter().sum::<u64>() / intervals.len() as u64;
            Some((avg / 60) as u32)
        } else {
            None
        };
        let suggested = cycle.unwrap_or(52);
        let confidence = if valleys.len() >= 3 {
            0.7
        } else if valleys.len() >= 2 {
            0.4
        } else {
            0.1
        };
        BreakTimingResult {
            natural_cycle_mins: cycle,
            focus_curve: curve,
            suggested_break_interval_mins: suggested,
            confidence,
        }
    }

    /// Deep work streak: consecutive days with sufficient deep work.
    pub fn deep_work_streak(&self, min_deep_work_mins: u32) -> DeepWorkStreak {
        let c = self.conn.lock_or_recover();
        let now = now_secs();
        let since = now.saturating_sub(30 * 86400);
        let mut stmt = match c.prepare_cached(
            "SELECT seen_at / 86400 AS day_idx, COALESCE(SUM(duration_secs), 0) / 60 AS total_mins,
                    AVG(CASE WHEN eeg_focus IS NOT NULL THEN eeg_focus END) AS avg_focus
             FROM file_interactions WHERE seen_at >= ?1 AND was_modified = 1
             GROUP BY day_idx ORDER BY day_idx DESC",
        ) {
            Ok(s) => s,
            Err(_) => return DeepWorkStreak::default(),
        };
        let days: Vec<DayDeepWork> = stmt
            .query_map([since as i64], |row| {
                let mins = row.get::<_, i64>(1)? as u32;
                Ok(DayDeepWork {
                    day_start: row.get::<_, i64>(0)? as u64 * 86400,
                    deep_work_mins: mins,
                    avg_focus: row.get::<_, Option<f64>>(2)?.map(|v| v as f32),
                    qualified: mins >= min_deep_work_mins,
                })
            })
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();
        let mut streak = 0u32;
        for d in &days {
            if d.qualified {
                streak += 1;
            } else {
                break;
            }
        }
        let longest = {
            let mut max = 0u32;
            let mut cur = 0u32;
            for d in &days {
                if d.qualified {
                    cur += 1;
                    max = max.max(cur);
                } else {
                    cur = 0;
                }
            }
            max
        };
        let today_mins = days.first().map(|d| d.deep_work_mins).unwrap_or(0);
        let weekly: f32 = days.iter().take(7).map(|d| d.deep_work_mins as f32).sum::<f32>() / 7.0;
        DeepWorkStreak {
            current_streak_days: streak,
            longest_streak_days: longest,
            today_deep_mins: today_mins,
            today_qualifies: today_mins >= min_deep_work_mins,
            threshold_mins: min_deep_work_mins,
            daily_history: days,
            weekly_avg_deep_mins: weekly,
        }
    }

    // ── AI events ─────────────────────────────────────────────────────────────

    pub fn insert_ai_event(&self, event_type: &str, source: &str, file_path: &str, language: &str, at: u64) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO ai_events (event_type, source, file_path, language, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![event_type, source, file_path, language, at as i64],
        );
    }

    pub fn get_recent_ai_events(&self, limit: u32) -> Vec<AiEventRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, event_type, source, file_path, language, at FROM ai_events ORDER BY at DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([limit as i64], |row| {
            Ok(AiEventRow {
                id: row.get(0)?,
                event_type: row.get(1)?,
                source: row.get(2)?,
                file_path: row.get(3)?,
                language: row.get(4)?,
                at: row.get::<_, i64>(5)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Fusion insights (EEG + activity + editor) ─────────────────────────────

    /// Classify what the user is currently doing.
    pub fn detect_task_type(&self, window_secs: u64) -> TaskTypeResult {
        let c = self.conn.lock_or_recover();
        let since = now_secs().saturating_sub(window_secs);
        let row = c.query_row(
            "SELECT SUM(lines_added) AS la, SUM(lines_removed) AS lr, SUM(undo_count) AS undos,
                    COUNT(DISTINCT file_path) AS files, COUNT(*) AS n
             FROM file_interactions WHERE seen_at >= ?1",
            params![since as i64],
            |row| {
                Ok((
                    row.get::<_, i64>(0).unwrap_or(0) as u64,
                    row.get::<_, i64>(1).unwrap_or(0) as u64,
                    row.get::<_, i64>(2).unwrap_or(0) as u64,
                    row.get::<_, i64>(3).unwrap_or(0) as u64,
                    row.get::<_, i64>(4).unwrap_or(0) as u64,
                ))
            },
        );
        let (la, lr, undos, files, _n) = row.unwrap_or_default();
        // Check for debug/test events in build_events.
        let build_fails: u64 = c
            .query_row(
                "SELECT COUNT(*) FROM build_events WHERE detected_at >= ?1 AND outcome = 'fail'",
                params![since as i64],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as u64;
        let has_debug: bool = c
            .query_row(
                "SELECT COUNT(*) FROM build_events WHERE detected_at >= ?1 AND command LIKE '%debug%'",
                params![since as i64],
                |row| Ok(row.get::<_, i64>(0).unwrap_or(0) > 0),
            )
            .unwrap_or(false);
        let has_test: bool = c
            .query_row(
                "SELECT COUNT(*) FROM build_events WHERE detected_at >= ?1 AND (command LIKE '%test%' OR command LIKE '%jest%' OR command LIKE '%pytest%')",
                params![since as i64],
                |row| Ok(row.get::<_, i64>(0).unwrap_or(0) > 0),
            )
            .unwrap_or(false);

        if has_debug || (build_fails > 0 && undos > 5) {
            return TaskTypeResult {
                task_type: "debugging".into(),
                confidence: 0.8,
                signals: vec!["debug events or build failures + high undos".into()],
            };
        }
        if has_test {
            return TaskTypeResult {
                task_type: "testing".into(),
                confidence: 0.7,
                signals: vec!["test commands detected".into()],
            };
        }
        if files > 5 && la < 5 && lr < 5 {
            return TaskTypeResult {
                task_type: "reviewing".into(),
                confidence: 0.6,
                signals: vec!["many file switches, low edits".into()],
            };
        }
        if la > 10 && lr > 10 && undos < 3 {
            return TaskTypeResult {
                task_type: "refactoring".into(),
                confidence: 0.6,
                signals: vec!["high add + remove, low undos".into()],
            };
        }
        TaskTypeResult {
            task_type: "coding".into(),
            confidence: 0.5,
            signals: vec!["default: editing code".into()],
        }
    }

    /// Predict whether the user is stuck based on undo rate, velocity, and focus.
    pub fn predict_struggle(&self, window_secs: u64) -> StrugglePrediction {
        let c = self.conn.lock_or_recover();
        let now = now_secs();
        let since = now.saturating_sub(window_secs);
        // Recent undo rate from edit chunks.
        let (undo_total, _chunk_count): (u64, u64) = c
            .query_row(
                "SELECT COALESCE(SUM(undo_estimate), 0), COUNT(*) FROM file_edit_chunks WHERE chunk_at >= ?1",
                params![since as i64],
                |row| {
                    Ok((
                        row.get::<_, i64>(0).unwrap_or(0) as u64,
                        row.get::<_, i64>(1).unwrap_or(0) as u64,
                    ))
                },
            )
            .unwrap_or_default();
        // Recent file: focus + time on it.
        let recent = self.get_recent_files(1, None);
        let current_file = recent.first().map(|f| f.file_path.clone()).unwrap_or_default();
        let focus = recent.first().and_then(|f| f.eeg_focus);
        let time_on_file = recent.first().and_then(|f| f.duration_secs).unwrap_or(0);
        // Compute velocity drop: compare last 5min vs prior 15min.
        let recent_churn: u64 = c
            .query_row(
                "SELECT COALESCE(SUM(lines_added + lines_removed), 0) FROM file_interactions WHERE seen_at >= ?1",
                params![(now - 300) as i64],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as u64;
        let prior_churn: u64 = c
            .query_row(
                "SELECT COALESCE(SUM(lines_added + lines_removed), 0) FROM file_interactions WHERE seen_at >= ?1 AND seen_at < ?2",
                params![(now - 1200) as i64, (now - 300) as i64],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as u64;

        let mins = (window_secs as f32 / 60.0).max(1.0);
        let undo_rate = undo_total as f32 / mins;
        let velocity_drop = if prior_churn > 0 {
            (1.0 - recent_churn as f32 / (prior_churn as f32 / 3.0)).clamp(0.0, 1.0) * 100.0
        } else {
            0.0
        };
        let focus_penalty = 100.0 - focus.unwrap_or(50.0);
        let time_stuck = (time_on_file as f32 / 60.0).min(30.0) / 30.0 * 100.0;
        let score = (undo_rate * 20.0 + velocity_drop * 0.3 + focus_penalty * 0.3 + time_stuck * 0.2).clamp(0.0, 100.0);
        let struggling = score > 60.0;
        let suggestion = if struggling {
            if undo_rate > 3.0 {
                "High undo rate — try a different approach or break the problem down."
            } else if focus_penalty > 60.0 {
                "Focus is low — consider a short break to reset."
            } else {
                "You've been stuck on this file for a while — step back and rethink."
            }
        } else {
            ""
        };
        StrugglePrediction {
            struggling,
            score,
            factors: StruggleFactors {
                undo_rate,
                edit_velocity_drop: velocity_drop,
                focus_score: focus,
                time_on_file_mins: (time_on_file / 60) as u32,
            },
            suggestion: suggestion.to_string(),
            current_file,
        }
    }

    /// Measure focus recovery time after interruptions (app switches, meetings).
    pub fn interruption_recovery(&self, since: u64, limit: u32) -> InterruptionRecoveryResult {
        let c = self.conn.lock_or_recover();
        // Find app switches away from code editors.
        let mut stmt = match c.prepare_cached(
            "SELECT app_name, activated_at FROM active_windows
             WHERE activated_at >= ?1
             AND app_name NOT IN ('Code','code','Cursor','cursor','Visual Studio Code','Xcode','IntelliJ IDEA','WebStorm','PyCharm','Sublime Text','Neovim','vim')
             ORDER BY activated_at DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return InterruptionRecoveryResult { interruptions: vec![], avg_recovery_secs: 0, by_type: vec![] },
        };
        let switches: Vec<(String, u64)> = stmt
            .query_map(params![since as i64, limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();

        let mut rows = Vec::new();
        let mut type_totals: std::collections::HashMap<String, (u64, u32)> = std::collections::HashMap::new();
        for (app, at) in &switches {
            let recovery: Option<u64> = c
                .query_row(
                    "SELECT MIN(seen_at) FROM file_interactions WHERE seen_at > ?1 AND eeg_focus > 50",
                    params![*at as i64],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .ok()
                .flatten()
                .map(|ts| (ts as u64).saturating_sub(*at));
            let lower = app.to_lowercase();
            let itype = if lower.contains("slack") {
                "slack"
            } else if lower.contains("zoom") || lower.contains("teams") {
                "meeting"
            } else if lower.contains("chrome")
                || app.to_lowercase().contains("safari")
                || app.to_lowercase().contains("firefox")
            {
                "browser"
            } else {
                "app_switch"
            };
            if let Some(r) = recovery {
                let entry = type_totals.entry(itype.to_string()).or_insert((0, 0));
                entry.0 += r;
                entry.1 += 1;
            }
            rows.push(InterruptionRow {
                interruption_type: itype.to_string(),
                source_app: app.clone(),
                at: *at,
                recovery_secs: recovery,
            });
        }
        let total_recovery: u64 = rows.iter().filter_map(|r| r.recovery_secs).sum();
        let count = rows.iter().filter(|r| r.recovery_secs.is_some()).count().max(1) as u64;
        let by_type: Vec<RecoveryByType> = type_totals
            .into_iter()
            .map(|(t, (total, cnt))| RecoveryByType {
                interruption_type: t,
                avg_recovery_secs: total / cnt as u64,
                count: cnt,
            })
            .collect();
        InterruptionRecoveryResult {
            interruptions: rows,
            avg_recovery_secs: total_recovery / count,
            by_type,
        }
    }

    /// Correlate EEG brain states with code files, languages, and projects.
    pub fn code_eeg_correlation(&self, since: u64) -> CodeEegCorrelation {
        let c = self.conn.lock_or_recover();
        let by_lang = |sql: &str| -> Vec<CodeBrainRow> {
            c.prepare(sql)
                .ok()
                .map(|mut stmt| {
                    stmt.query_map([since as i64], |row| {
                        Ok(CodeBrainRow {
                            key: row.get(0)?,
                            avg_focus: row.get::<_, f64>(1).unwrap_or(50.0) as f32,
                            total_mins: row.get::<_, i64>(2).unwrap_or(0) as u32,
                            interactions: row.get::<_, i64>(3).unwrap_or(0) as u64,
                            avg_undos: row.get::<_, f64>(4).unwrap_or(0.0) as f32,
                        })
                    })
                    .map(|rows| rows.filter_map(std::result::Result::ok).collect())
                    .unwrap_or_default()
                })
                .unwrap_or_default()
        };
        let langs = by_lang(
            "SELECT language, AVG(eeg_focus), SUM(duration_secs)/60, COUNT(*), AVG(undo_count)
             FROM file_interactions WHERE seen_at >= ?1 AND language != '' AND eeg_focus IS NOT NULL
             GROUP BY language ORDER BY AVG(eeg_focus) DESC",
        );
        let projects = by_lang(
            "SELECT project, AVG(eeg_focus), SUM(duration_secs)/60, COUNT(*), AVG(undo_count)
             FROM file_interactions WHERE seen_at >= ?1 AND project != '' AND eeg_focus IS NOT NULL
             GROUP BY project ORDER BY AVG(eeg_focus) DESC",
        );
        let best = by_lang(
            "SELECT file_path, AVG(eeg_focus), SUM(duration_secs)/60, COUNT(*), AVG(undo_count)
             FROM file_interactions WHERE seen_at >= ?1 AND eeg_focus IS NOT NULL
             GROUP BY file_path ORDER BY AVG(eeg_focus) DESC LIMIT 5",
        );
        let worst = by_lang(
            "SELECT file_path, AVG(eeg_focus), SUM(duration_secs)/60, COUNT(*), AVG(undo_count)
             FROM file_interactions WHERE seen_at >= ?1 AND eeg_focus IS NOT NULL
             GROUP BY file_path ORDER BY AVG(eeg_focus) ASC LIMIT 5",
        );
        CodeEegCorrelation {
            by_language: langs,
            by_project: projects,
            best_files: best,
            worst_files: worst,
        }
    }

    // ── Unified timeline ──────────────────────────────────────────────────────

    /// Return a unified chronological stream of all activity events in a time range.
    pub fn activity_timeline(&self, from_ts: u64, to_ts: u64, limit: u32) -> Vec<TimelineEvent> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare(
            "SELECT kind, title, detail, ts, focus FROM (
                SELECT 'file' AS kind, file_path AS title, language AS detail, seen_at AS ts, eeg_focus AS focus
                FROM file_interactions WHERE seen_at >= ?1 AND seen_at <= ?2
                UNION ALL
                SELECT 'build', command, outcome, detected_at, NULL
                FROM build_events WHERE detected_at >= ?1 AND detected_at <= ?2
                UNION ALL
                SELECT 'meeting', platform, title, start_at, NULL
                FROM meeting_events WHERE start_at >= ?1 AND start_at <= ?2
                UNION ALL
                SELECT 'ai', event_type, source, at, eeg_focus
                FROM ai_events WHERE at >= ?1 AND at <= ?2
                UNION ALL
                SELECT 'clipboard', source_app, content_type, copied_at, NULL
                FROM clipboard_events WHERE copied_at >= ?1 AND copied_at <= ?2
            ) ORDER BY ts DESC LIMIT ?3",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] activity_timeline: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![from_ts as i64, to_ts as i64, limit as i64], |row| {
            Ok(TimelineEvent {
                kind: row.get(0)?,
                title: row.get(1)?,
                detail: row.get::<_, String>(2).unwrap_or_default(),
                ts: row.get::<_, i64>(3)? as u64,
                eeg_focus: row.get(4)?,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Focus sessions ───────────────────────────────────────────────────────

    /// Insert a detected focus session.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_focus_session(
        &self,
        start_at: u64,
        end_at: u64,
        project: &str,
        file_count: u64,
        edit_count: u64,
        lines_added: u64,
        lines_removed: u64,
        avg_focus: Option<f32>,
        avg_mood: Option<f32>,
    ) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO focus_sessions
             (start_at, end_at, project, file_count, edit_count,
              total_lines_added, total_lines_removed, avg_eeg_focus, avg_eeg_mood)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                start_at as i64,
                end_at as i64,
                project,
                file_count as i64,
                edit_count as i64,
                lines_added as i64,
                lines_removed as i64,
                avg_focus,
                avg_mood,
            ],
        );
    }

    /// Return recent focus sessions, newest first.
    pub fn get_focus_sessions(&self, limit: u32, since: Option<u64>) -> Vec<FocusSessionRow> {
        let c = self.conn.lock_or_recover();
        let (sql, p): (&str, Vec<i64>) = match since {
            Some(ts) => (
                "SELECT id, start_at, end_at, project, file_count, edit_count,
                        total_lines_added, total_lines_removed, avg_eeg_focus, avg_eeg_mood
                 FROM focus_sessions WHERE start_at >= ?1
                 ORDER BY start_at DESC LIMIT ?2",
                vec![ts as i64, limit as i64],
            ),
            None => (
                "SELECT id, start_at, end_at, project, file_count, edit_count,
                        total_lines_added, total_lines_removed, avg_eeg_focus, avg_eeg_mood
                 FROM focus_sessions ORDER BY start_at DESC LIMIT ?1",
                vec![limit as i64],
            ),
        };
        let mut stmt = match c.prepare_cached(sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] get_focus_sessions: {e}");
                return vec![];
            }
        };
        let params: Vec<&dyn rusqlite::types::ToSql> = p.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();
        stmt.query_map(params.as_slice(), |row| {
            Ok(FocusSessionRow {
                id: row.get(0)?,
                start_at: row.get::<_, i64>(1)? as u64,
                end_at: row.get::<_, i64>(2)? as u64,
                project: row.get(3)?,
                file_count: row.get::<_, i64>(4)? as u64,
                edit_count: row.get::<_, i64>(5)? as u64,
                total_lines_added: row.get::<_, i64>(6)? as u64,
                total_lines_removed: row.get::<_, i64>(7)? as u64,
                avg_eeg_focus: row.get::<_, Option<f64>>(8)?.map(|v| v as f32),
                avg_eeg_mood: row.get::<_, Option<f64>>(9)?.map(|v| v as f32),
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Build events ─────────────────────────────────────────────────────────

    pub fn insert_build_event(&self, command: &str, outcome: &str, project: &str, detected_at: u64) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO build_events (command, outcome, project, detected_at) VALUES (?1, ?2, ?3, ?4)",
            params![command, outcome, project, detected_at as i64],
        );
    }

    pub fn get_recent_builds(&self, limit: u32) -> Vec<BuildEventRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, command, outcome, project, detected_at
             FROM build_events ORDER BY detected_at DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] get_recent_builds: {e}");
                return vec![];
            }
        };
        stmt.query_map([limit as i64], |row| {
            Ok(BuildEventRow {
                id: row.get(0)?,
                command: row.get(1)?,
                outcome: row.get(2)?,
                project: row.get(3)?,
                detected_at: row.get::<_, i64>(4)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Meeting events ─────────────────────────────────────────────────────────

    /// Start a meeting event. Returns the row id.
    pub fn insert_meeting_start(&self, platform: &str, title: &str, app_name: &str, start_at: u64) -> Option<i64> {
        let c = self.conn.lock_or_recover();
        match c.execute(
            "INSERT INTO meeting_events (platform, title, app_name, start_at) VALUES (?1, ?2, ?3, ?4)",
            params![platform, title, app_name, start_at as i64],
        ) {
            Ok(_) => Some(c.last_insert_rowid()),
            Err(e) => {
                eprintln!("[activity] insert_meeting_start: {e}");
                None
            }
        }
    }

    /// Mark the end of a meeting event.
    pub fn update_meeting_end(&self, id: i64, end_at: u64) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "UPDATE meeting_events SET end_at = ?1 WHERE id = ?2",
            params![end_at as i64, id],
        );
    }

    /// Return meetings overlapping the given time range.
    pub fn get_meetings_in_range(&self, from_ts: u64, to_ts: u64) -> Vec<MeetingEventRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, platform, title, app_name, start_at, end_at
             FROM meeting_events
             WHERE start_at <= ?2 AND (end_at IS NULL OR end_at >= ?1)
             ORDER BY start_at ASC",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] get_meetings_in_range: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![from_ts as i64, to_ts as i64], |row| {
            Ok(MeetingEventRow {
                id: row.get(0)?,
                platform: row.get(1)?,
                title: row.get(2)?,
                app_name: row.get(3)?,
                start_at: row.get::<_, i64>(4)? as u64,
                end_at: row.get::<_, Option<i64>>(5)?.map(|t| t as u64),
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Clipboard events ──────────────────────────────────────────────────────

    /// Record a clipboard change event (metadata only).
    pub fn insert_clipboard_event(&self, source_app: &str, content_type: &str, content_size: u64, copied_at: u64) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO clipboard_events (source_app, content_type, content_size, copied_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![source_app, content_type, content_size as i64, copied_at as i64],
        );
    }

    /// Return recent clipboard events, newest first.
    pub fn get_recent_clipboard(&self, limit: u32) -> Vec<ClipboardEventRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, source_app, content_type, content_size, copied_at
             FROM clipboard_events ORDER BY copied_at DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] get_recent_clipboard: {e}");
                return vec![];
            }
        };
        stmt.query_map([limit as i64], |row| {
            Ok(ClipboardEventRow {
                id: row.get(0)?,
                source_app: row.get(1)?,
                content_type: row.get(2)?,
                content_size: row.get::<_, i64>(3)? as u64,
                copied_at: row.get::<_, i64>(4)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Range queries ─────────────────────────────────────────────────────────

    /// Return file interactions within a time range, chronologically.
    pub fn get_files_in_range(&self, from_ts: u64, to_ts: u64, limit: u32) -> Vec<FileInteractionRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, file_path, app_name, project, language, category, git_branch,
                    seen_at, duration_secs, was_modified, size_delta,
                    lines_added, lines_removed, words_delta, eeg_focus, eeg_mood, undo_count
             FROM file_interactions
             WHERE seen_at >= ?1 AND seen_at <= ?2
             ORDER BY seen_at ASC LIMIT ?3",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] get_files_in_range: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![from_ts as i64, to_ts as i64, limit as i64], |row| {
            Ok(FileInteractionRow {
                id: row.get(0)?,
                file_path: row.get(1)?,
                app_name: row.get(2)?,
                project: row.get(3)?,
                language: row.get(4)?,
                category: row.get(5)?,
                git_branch: row.get(6)?,
                seen_at: row.get::<_, i64>(7)? as u64,
                duration_secs: row.get::<_, Option<i64>>(8)?.map(|t| t as u64),
                was_modified: row.get::<_, i64>(9)? != 0,
                size_delta: row.get(10)?,
                lines_added: row.get::<_, i64>(11)? as u64,
                lines_removed: row.get::<_, i64>(12)? as u64,
                words_delta: row.get(13)?,
                eeg_focus: row.get(14)?,
                eeg_mood: row.get(15)?,
                undo_count: row.get::<_, i64>(16)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return focus sessions overlapping a time range.
    pub fn get_focus_sessions_in_range(&self, from_ts: u64, to_ts: u64) -> Vec<FocusSessionRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, start_at, end_at, project, file_count, edit_count,
                    total_lines_added, total_lines_removed, avg_eeg_focus, avg_eeg_mood
             FROM focus_sessions
             WHERE start_at <= ?2 AND end_at >= ?1
             ORDER BY start_at ASC",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] get_focus_sessions_in_range: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![from_ts as i64, to_ts as i64], |row| {
            Ok(FocusSessionRow {
                id: row.get(0)?,
                start_at: row.get::<_, i64>(1)? as u64,
                end_at: row.get::<_, i64>(2)? as u64,
                project: row.get(3)?,
                file_count: row.get::<_, i64>(4)? as u64,
                edit_count: row.get::<_, i64>(5)? as u64,
                total_lines_added: row.get::<_, i64>(6)? as u64,
                total_lines_removed: row.get::<_, i64>(7)? as u64,
                avg_eeg_focus: row.get(8)?,
                avg_eeg_mood: row.get(9)?,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Edit chunks ────────────────────────────────────────────────────────────

    /// Record a 5-second edit chunk within a file interaction.
    pub fn insert_edit_chunk(
        &self,
        interaction_id: i64,
        chunk_at: u64,
        lines_added: u64,
        lines_removed: u64,
        size_delta: i64,
        undo_estimate: u64,
    ) {
        let c = self.conn.lock_or_recover();
        if let Err(e) = c.execute(
            "INSERT INTO file_edit_chunks (interaction_id, chunk_at, lines_added, lines_removed, size_delta, undo_estimate)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                interaction_id,
                chunk_at as i64,
                lines_added as i64,
                lines_removed as i64,
                size_delta,
                undo_estimate as i64,
            ],
        ) {
            eprintln!("[activity] insert_edit_chunk: {e}");
        }
    }

    /// Return all edit chunks for a given file interaction, oldest first.
    pub fn get_edit_chunks(&self, interaction_id: i64) -> Vec<EditChunkRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, interaction_id, chunk_at, lines_added, lines_removed, size_delta, undo_estimate
             FROM file_edit_chunks WHERE interaction_id = ?1
             ORDER BY chunk_at ASC",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] prepare get_edit_chunks: {e}");
                return vec![];
            }
        };
        stmt.query_map([interaction_id], |row| {
            Ok(EditChunkRow {
                id: row.get(0)?,
                interaction_id: row.get(1)?,
                chunk_at: row.get::<_, i64>(2)? as u64,
                lines_added: row.get::<_, i64>(3)? as u64,
                lines_removed: row.get::<_, i64>(4)? as u64,
                size_delta: row.get::<_, i64>(5)?,
                undo_estimate: row.get::<_, i64>(6)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return all edit chunks in a time range, oldest first.
    pub fn get_edit_chunks_range(&self, from_ts: u64, to_ts: u64) -> Vec<EditChunkRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, interaction_id, chunk_at, lines_added, lines_removed, size_delta, undo_estimate
             FROM file_edit_chunks WHERE chunk_at >= ?1 AND chunk_at <= ?2
             ORDER BY chunk_at ASC",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] prepare get_edit_chunks_range: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![from_ts as i64, to_ts as i64], |row| {
            Ok(EditChunkRow {
                id: row.get(0)?,
                interaction_id: row.get(1)?,
                chunk_at: row.get::<_, i64>(2)? as u64,
                lines_added: row.get::<_, i64>(3)? as u64,
                lines_removed: row.get::<_, i64>(4)? as u64,
                size_delta: row.get::<_, i64>(5)?,
                undo_estimate: row.get::<_, i64>(6)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Delete file interaction rows (and their edit chunks) older than `cutoff`.
    pub fn prune_file_interactions(&self, cutoff: u64) -> u64 {
        let c = self.conn.lock_or_recover();
        // Delete orphaned chunks first.
        let _ = c.execute(
            "DELETE FROM file_edit_chunks WHERE interaction_id IN
             (SELECT id FROM file_interactions WHERE seen_at < ?1)",
            params![cutoff as i64],
        );
        match c.execute(
            "DELETE FROM file_interactions WHERE seen_at < ?1",
            params![cutoff as i64],
        ) {
            Ok(n) => n as u64,
            Err(e) => {
                eprintln!("[activity] prune_file_interactions: {e}");
                0
            }
        }
    }

    /// Delete meeting events older than `cutoff`.
    pub fn prune_meetings(&self, cutoff: u64) -> u64 {
        let c = self.conn.lock_or_recover();
        c.execute("DELETE FROM meeting_events WHERE start_at < ?1", params![cutoff as i64])
            .unwrap_or(0) as u64
    }

    /// Delete clipboard events older than `cutoff`.
    pub fn prune_clipboard(&self, cutoff: u64) -> u64 {
        let c = self.conn.lock_or_recover();
        c.execute(
            "DELETE FROM clipboard_events WHERE copied_at < ?1",
            params![cutoff as i64],
        )
        .unwrap_or(0) as u64
    }

    /// Delete secondary window records whose primary window is older than `cutoff`.
    pub fn prune_secondary_windows(&self, cutoff: u64) -> u64 {
        let c = self.conn.lock_or_recover();
        c.execute(
            "DELETE FROM secondary_windows WHERE primary_id IN
             (SELECT id FROM active_windows WHERE activated_at < ?1)",
            params![cutoff as i64],
        )
        .unwrap_or(0) as u64
    }

    /// Return secondary windows associated with a primary window ID.
    pub fn get_secondary_windows(&self, primary_id: i64) -> Vec<SecondaryWindowRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, primary_id, app_name, window_title, monitor_id
             FROM secondary_windows WHERE primary_id = ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] get_secondary_windows: {e}");
                return vec![];
            }
        };
        stmt.query_map([primary_id], |row| {
            Ok(SecondaryWindowRow {
                id: row.get(0)?,
                primary_id: row.get(1)?,
                app_name: row.get(2)?,
                window_title: row.get(3)?,
                monitor_id: row.get::<_, i64>(4)? as u32,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return the `limit` most recent file interaction records, newest first.
    pub fn get_recent_files(&self, limit: u32, since: Option<u64>) -> Vec<FileInteractionRow> {
        let c = self.conn.lock_or_recover();
        let (sql, p): (&str, Vec<i64>) = match since {
            Some(ts) => (
                "SELECT id, file_path, app_name, project, language, category,
                        git_branch, seen_at, duration_secs, was_modified,
                        size_delta, lines_added, lines_removed, words_delta,
                        eeg_focus, eeg_mood, undo_count
                 FROM file_interactions WHERE seen_at >= ?1
                 ORDER BY seen_at DESC LIMIT ?2",
                vec![ts as i64, limit as i64],
            ),
            None => (
                "SELECT id, file_path, app_name, project, language, category,
                        git_branch, seen_at, duration_secs, was_modified,
                        size_delta, lines_added, lines_removed, words_delta,
                        eeg_focus, eeg_mood, undo_count
                 FROM file_interactions ORDER BY seen_at DESC LIMIT ?1",
                vec![limit as i64],
            ),
        };
        let mut stmt = match c.prepare_cached(sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] prepare recent_files: {e}");
                return vec![];
            }
        };
        let params: Vec<&dyn rusqlite::types::ToSql> = p.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();
        stmt.query_map(params.as_slice(), |row| {
            Ok(FileInteractionRow {
                id: row.get(0)?,
                file_path: row.get(1)?,
                app_name: row.get(2)?,
                project: row.get::<_, String>(3).unwrap_or_default(),
                language: row.get::<_, String>(4).unwrap_or_default(),
                category: row.get::<_, String>(5).unwrap_or_default(),
                git_branch: row.get::<_, String>(6).unwrap_or_default(),
                seen_at: row.get::<_, i64>(7)? as u64,
                duration_secs: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                was_modified: row.get::<_, i64>(9).unwrap_or(0) != 0,
                size_delta: row.get::<_, i64>(10).unwrap_or(0),
                lines_added: row.get::<_, i64>(11).unwrap_or(0) as u64,
                lines_removed: row.get::<_, i64>(12).unwrap_or(0) as u64,
                words_delta: row.get::<_, i64>(13).unwrap_or(0),
                eeg_focus: row.get::<_, Option<f64>>(14).ok().flatten().map(|v| v as f32),
                eeg_mood: row.get::<_, Option<f64>>(15).ok().flatten().map(|v| v as f32),
                undo_count: row.get::<_, i64>(16).unwrap_or(0) as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return the top `limit` most-interacted files by number of focus events,
    /// optionally filtered to events at or after `since`.
    pub fn top_files(&self, limit: u32, since: Option<u64>) -> Vec<FileUsageRow> {
        let c = self.conn.lock_or_recover();
        let (sql, p): (&str, Vec<i64>) = match since {
            Some(ts) => (
                "SELECT file_path, COUNT(*) AS cnt,
                        SUM(was_modified) AS edits,
                        COALESCE(SUM(duration_secs), 0) AS total,
                        MAX(seen_at) AS last_seen
                 FROM file_interactions WHERE seen_at >= ?1
                 GROUP BY file_path ORDER BY cnt DESC LIMIT ?2",
                vec![ts as i64, limit as i64],
            ),
            None => (
                "SELECT file_path, COUNT(*) AS cnt,
                        SUM(was_modified) AS edits,
                        COALESCE(SUM(duration_secs), 0) AS total,
                        MAX(seen_at) AS last_seen
                 FROM file_interactions
                 GROUP BY file_path ORDER BY cnt DESC LIMIT ?1",
                vec![limit as i64],
            ),
        };
        let mut stmt = match c.prepare_cached(sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] top_files: {e}");
                return vec![];
            }
        };
        let params: Vec<&dyn rusqlite::types::ToSql> = p.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();
        stmt.query_map(params.as_slice(), |row| {
            Ok(FileUsageRow {
                file_path: row.get(0)?,
                interactions: row.get::<_, i64>(1)? as u64,
                edits: row.get::<_, i64>(2)? as u64,
                total_secs: row.get::<_, i64>(3)? as u64,
                last_seen: row.get::<_, i64>(4)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return the top `limit` projects by interaction count.
    pub fn top_projects(&self, limit: u32, since: Option<u64>) -> Vec<ProjectUsageRow> {
        let c = self.conn.lock_or_recover();
        let (sql, p): (&str, Vec<i64>) = match since {
            Some(ts) => (
                "SELECT project, COUNT(*) AS cnt,
                        COALESCE(SUM(duration_secs), 0) AS total,
                        MAX(seen_at) AS last_seen
                 FROM file_interactions WHERE project != '' AND seen_at >= ?1
                 GROUP BY project ORDER BY cnt DESC LIMIT ?2",
                vec![ts as i64, limit as i64],
            ),
            None => (
                "SELECT project, COUNT(*) AS cnt,
                        COALESCE(SUM(duration_secs), 0) AS total,
                        MAX(seen_at) AS last_seen
                 FROM file_interactions WHERE project != ''
                 GROUP BY project ORDER BY cnt DESC LIMIT ?1",
                vec![limit as i64],
            ),
        };
        let mut stmt = match c.prepare_cached(sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] top_projects: {e}");
                return vec![];
            }
        };
        let params: Vec<&dyn rusqlite::types::ToSql> = p.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();
        stmt.query_map(params.as_slice(), |row| {
            Ok(ProjectUsageRow {
                project: row.get(0)?,
                interactions: row.get::<_, i64>(1)? as u64,
                total_secs: row.get::<_, i64>(2)? as u64,
                last_seen: row.get::<_, i64>(3)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return all per-minute buckets whose `minute_ts` falls in `[from_ts, to_ts]`,
    /// ordered oldest-first (natural order for charting).
    pub fn get_input_buckets(&self, from_ts: u64, to_ts: u64) -> Vec<InputBucketRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT minute_ts, key_count, mouse_count
             FROM input_buckets
             WHERE minute_ts >= ?1 AND minute_ts <= ?2
             ORDER BY minute_ts ASC",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] prepare get_input_buckets: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![from_ts as i64, to_ts as i64], |row| {
            Ok(InputBucketRow {
                minute_ts: row.get::<_, i64>(0)? as u64,
                key_count: row.get::<_, i64>(1)? as u64,
                mouse_count: row.get::<_, i64>(2)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }
}

// ── Row types (returned to the frontend) ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveWindowRow {
    pub id: i64,
    pub app_name: String,
    pub app_path: String,
    pub window_title: String,
    pub activated_at: u64,
    pub browser_title: Option<String>,
    pub monitor_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputActivityRow {
    pub id: i64,
    /// Unix seconds of the last keyboard event in this sampling window; `None` if absent.
    pub last_keyboard: Option<u64>,
    /// Unix seconds of the last mouse event in this sampling window; `None` if absent.
    pub last_mouse: Option<u64>,
    /// Unix seconds when this row was written (flush time).
    pub sampled_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUsageRow {
    pub app_name: String,
    /// Number of window-activation switches to this app.
    pub switches: u64,
    /// Most recent activation timestamp (unix seconds).
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInteractionRow {
    pub id: i64,
    pub file_path: String,
    pub app_name: String,
    pub project: String,
    pub language: String,
    /// Broad file category: code, document, spreadsheet, presentation, image,
    /// design, data, media, config, other.
    pub category: String,
    pub git_branch: String,
    pub seen_at: u64,
    pub duration_secs: Option<u64>,
    pub was_modified: bool,
    pub size_delta: i64,
    pub lines_added: u64,
    pub lines_removed: u64,
    /// Change in word count (for text-based documents).
    pub words_delta: i64,
    pub eeg_focus: Option<f32>,
    pub eeg_mood: Option<f32>,
    pub undo_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUsageRow {
    pub file_path: String,
    /// Number of distinct focus events on this file.
    pub interactions: u64,
    /// How many of those interactions involved a modification.
    pub edits: u64,
    /// Total seconds spent on this file (sum of durations).
    pub total_secs: u64,
    /// Most recent interaction timestamp (unix seconds).
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectUsageRow {
    pub project: String,
    /// Number of distinct file-focus events in this project.
    pub interactions: u64,
    /// Total seconds spent in this project.
    pub total_secs: u64,
    /// Most recent interaction timestamp (unix seconds).
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageBreakdownRow {
    pub language: String,
    pub interactions: u64,
    pub edits: u64,
    pub total_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoEditRow {
    pub file_a: String,
    pub file_b: String,
    pub co_occurrences: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailySummaryRow {
    pub day_start: u64,
    pub interactions: u64,
    pub edits: u64,
    pub total_secs: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub distinct_projects: u64,
    pub distinct_files: u64,
    pub avg_eeg_focus: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyEditRow {
    pub hour: u8,
    pub interactions: u64,
    pub total_churn: u64,
    pub avg_eeg_focus: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusSessionRow {
    pub id: i64,
    pub start_at: u64,
    pub end_at: u64,
    pub project: String,
    pub file_count: u64,
    pub edit_count: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub avg_eeg_focus: Option<f32>,
    pub avg_eeg_mood: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildEventRow {
    pub id: i64,
    pub command: String,
    pub outcome: String,
    pub project: String,
    pub detected_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditChunkRow {
    pub id: i64,
    pub interaction_id: i64,
    /// Unix seconds — start of this 5-second window.
    pub chunk_at: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub size_delta: i64,
    /// Estimated undo events detected via diff reversal heuristic.
    pub undo_estimate: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputBucketRow {
    /// Unix timestamp of the start of this minute (always divisible by 60).
    pub minute_ts: u64,
    /// Total keyboard events recorded in this minute.
    pub key_count: u64,
    /// Total mouse / scroll / click events recorded in this minute.
    pub mouse_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductivityScore {
    pub day_start: u64,
    /// Composite score 0–100.
    pub score: f32,
    /// Edit velocity component (0–25).
    pub edit_velocity: f32,
    /// Deep work component (0–25).
    pub deep_work: f32,
    /// Context stability component (0–25).
    pub context_stability: f32,
    /// EEG focus component (0–25).
    pub eeg_focus: f32,
    /// Minutes in deep-work focus sessions (>15 min).
    pub deep_work_minutes: u32,
    /// Context switches per minute.
    pub switch_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeeklyDigest {
    pub week_start: u64,
    pub days: Vec<DailySummaryRow>,
    pub total_interactions: u64,
    pub total_edits: u64,
    pub total_secs: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub avg_eeg_focus: Option<f32>,
    pub top_projects: Vec<ProjectUsageRow>,
    pub top_languages: Vec<LanguageBreakdownRow>,
    pub focus_session_count: u32,
    pub meeting_count: u32,
    /// Day of week with most edits (0 = first day of the week).
    pub peak_day_idx: u8,
    /// Hour of day with most activity (0–23).
    pub peak_hour: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleFileRow {
    pub file_path: String,
    pub last_seen: u64,
    pub total_edits: u64,
    pub project: String,
    pub language: String,
    pub days_stale: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecondaryWindowRow {
    pub id: i64,
    pub primary_id: i64,
    pub app_name: String,
    pub window_title: String,
    pub monitor_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingEventRow {
    pub id: i64,
    pub platform: String,
    pub title: String,
    pub app_name: String,
    pub start_at: u64,
    pub end_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEventRow {
    pub id: i64,
    pub source_app: String,
    pub content_type: String,
    pub content_size: u64,
    pub copied_at: u64,
}

// ── Brain awareness types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowStateResult {
    pub in_flow: bool,
    pub score: f32,
    pub duration_secs: u64,
    pub avg_focus: Option<f32>,
    pub file_switches: u32,
    pub edit_velocity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveLoadRow {
    pub key: String,
    pub avg_focus: Option<f32>,
    pub avg_undos: f32,
    pub interactions: u64,
    pub total_secs: u64,
    pub load_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingRecoveryResult {
    pub meetings: Vec<MeetingRecoveryRow>,
    pub avg_recovery_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingRecoveryRow {
    pub meeting_id: i64,
    pub title: String,
    pub platform: String,
    pub meeting_duration_secs: u64,
    pub recovery_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimalHoursResult {
    pub hours: Vec<HourScore>,
    pub best_hours: Vec<u8>,
    pub worst_hours: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourScore {
    pub hour: u8,
    pub score: f32,
    pub avg_focus: Option<f32>,
    pub total_churn: u64,
    pub interactions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FatigueAlert {
    pub fatigued: bool,
    pub trend: Vec<FatigueBucket>,
    pub focus_decline_pct: f32,
    pub suggestion: String,
    pub continuous_work_mins: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FatigueBucket {
    pub quarter: u8,
    pub avg_focus: f32,
    pub interactions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StruggleRow {
    pub file_path: String,
    pub language: String,
    pub project: String,
    pub total_undos: u64,
    pub avg_focus: Option<f32>,
    pub total_churn: u64,
    pub total_secs: u64,
    pub struggle_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBrainReport {
    pub day_start: u64,
    pub periods: Vec<PeriodSummary>,
    pub overall_focus: Option<f32>,
    pub productivity_score: f32,
    pub best_period: String,
}

impl DailyBrainReport {
    pub fn default_for(day_start: u64) -> Self {
        Self {
            day_start,
            periods: vec![],
            overall_focus: None,
            productivity_score: 0.0,
            best_period: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodSummary {
    pub period: String,
    pub avg_focus: Option<f32>,
    pub churn: u64,
    pub interactions: u64,
    pub files_touched: u32,
    pub undos: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakTimingResult {
    pub natural_cycle_mins: Option<u32>,
    pub focus_curve: Vec<FocusBucket>,
    pub suggested_break_interval_mins: u32,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusBucket {
    pub ts: u64,
    pub avg_focus: f32,
    pub churn: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeepWorkStreak {
    pub current_streak_days: u32,
    pub longest_streak_days: u32,
    pub today_deep_mins: u32,
    pub today_qualifies: bool,
    pub threshold_mins: u32,
    pub daily_history: Vec<DayDeepWork>,
    pub weekly_avg_deep_mins: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayDeepWork {
    pub day_start: u64,
    pub deep_work_mins: u32,
    pub avg_focus: Option<f32>,
    pub qualified: bool,
}

// ── Fusion insight types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTypeResult {
    pub task_type: String,
    pub confidence: f32,
    pub signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrugglePrediction {
    pub struggling: bool,
    pub score: f32,
    pub factors: StruggleFactors,
    pub suggestion: String,
    pub current_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StruggleFactors {
    pub undo_rate: f32,
    pub edit_velocity_drop: f32,
    pub focus_score: Option<f32>,
    pub time_on_file_mins: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptionRecoveryResult {
    pub interruptions: Vec<InterruptionRow>,
    pub avg_recovery_secs: u64,
    pub by_type: Vec<RecoveryByType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptionRow {
    pub interruption_type: String,
    pub source_app: String,
    pub at: u64,
    pub recovery_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryByType {
    pub interruption_type: String,
    pub avg_recovery_secs: u64,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEegCorrelation {
    pub by_language: Vec<CodeBrainRow>,
    pub by_project: Vec<CodeBrainRow>,
    pub best_files: Vec<CodeBrainRow>,
    pub worst_files: Vec<CodeBrainRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBrainRow {
    pub key: String,
    pub avg_focus: f32,
    pub total_mins: u32,
    pub interactions: u64,
    pub avg_undos: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub ts: u64,
    pub eeg_focus: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiEventRow {
    pub id: i64,
    pub event_type: String,
    pub source: String,
    pub file_path: String,
    pub language: String,
    pub at: u64,
}

// ── Tests ──────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::active_window::SecondaryWindowInfo;
    use tempfile::tempdir;

    fn open_temp() -> ActivityStore {
        let dir = tempdir().unwrap();
        ActivityStore::open(dir.path()).unwrap()
    }

    /// Shorthand for tests — insert with minimal fields.
    fn ins(store: &ActivityStore, path: &str, app: &str, project: &str, ts: u64) -> Option<i64> {
        store.insert_file_interaction(path, app, project, "", "", "", ts, None, None)
    }

    fn dummy_window(ts: u64) -> ActiveWindowInfo {
        ActiveWindowInfo {
            app_name: "TestApp".into(),
            app_path: "/usr/bin/test".into(),
            window_title: "Test Window".into(),
            document_path: None,
            activated_at: ts,
            browser_title: None,
            monitor_id: None,
        }
    }

    #[test]
    fn insert_and_retrieve_window() {
        let store = open_temp();
        store.insert_active_window(&dummy_window(1_000));
        store.insert_active_window(&dummy_window(2_000));
        let rows = store.get_recent_windows(10);
        assert_eq!(rows.len(), 2);
        // newest first
        assert_eq!(rows[0].activated_at, 2_000);
        assert_eq!(rows[1].activated_at, 1_000);
    }

    #[test]
    fn window_limit_respected() {
        let store = open_temp();
        for i in 0..10u64 {
            store.insert_active_window(&dummy_window(i));
        }
        assert_eq!(store.get_recent_windows(3).len(), 3);
    }

    #[test]
    fn insert_and_retrieve_input() {
        let store = open_temp();
        store.insert_input_activity(Some(500), Some(600), 1_000);
        store.insert_input_activity(None, Some(700), 2_000);
        let rows = store.get_recent_input(10);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].sampled_at, 2_000);
        assert_eq!(rows[0].last_keyboard, None);
        assert_eq!(rows[0].last_mouse, Some(700));
        assert_eq!(rows[1].last_keyboard, Some(500));
    }

    #[test]
    fn input_limit_respected() {
        let store = open_temp();
        for i in 0..10u64 {
            store.insert_input_activity(Some(i), Some(i), i);
        }
        assert_eq!(store.get_recent_input(4).len(), 4);
    }

    #[test]
    fn upsert_bucket_creates_and_increments() {
        let store = open_temp();
        let min = 1_000 * 60; // a round minute timestamp
        store.upsert_input_bucket(min, 10, 5);
        store.upsert_input_bucket(min, 3, 2); // second flush in same minute
        let rows = store.get_input_buckets(min, min);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key_count, 13);
        assert_eq!(rows[0].mouse_count, 7);
        assert_eq!(rows[0].minute_ts, min);
    }

    #[test]
    fn bucket_zero_delta_skipped() {
        let store = open_temp();
        store.upsert_input_bucket(60, 0, 0);
        assert_eq!(store.get_input_buckets(0, 120).len(), 0);
    }

    #[test]
    fn bucket_range_query() {
        let store = open_temp();
        // minutes at 0, 60, 120, 180 seconds
        for min in [0u64, 60, 120, 180] {
            store.upsert_input_bucket(min, 1, 1);
        }
        let rows = store.get_input_buckets(60, 120);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].minute_ts, 60);
        assert_eq!(rows[1].minute_ts, 120);
    }

    #[test]
    fn buckets_ordered_oldest_first() {
        let store = open_temp();
        for min in [300u64, 60, 180, 120, 0] {
            store.upsert_input_bucket(min, 1, 0);
        }
        let rows = store.get_input_buckets(0, 300);
        let ts: Vec<u64> = rows.iter().map(|r| r.minute_ts).collect();
        assert_eq!(ts, vec![0, 60, 120, 180, 300]);
    }

    #[test]
    fn top_apps_all_time() {
        let store = open_temp();
        for _ in 0..5 {
            store.insert_active_window(&ActiveWindowInfo {
                app_name: "Firefox".into(),
                app_path: "".into(),
                window_title: "".into(),
                document_path: None,
                activated_at: 100,
                browser_title: None,
                monitor_id: None,
            });
        }
        for _ in 0..3 {
            store.insert_active_window(&ActiveWindowInfo {
                app_name: "Terminal".into(),
                app_path: "".into(),
                window_title: "".into(),
                document_path: None,
                activated_at: 200,
                browser_title: None,
                monitor_id: None,
            });
        }
        store.insert_active_window(&ActiveWindowInfo {
            app_name: "Code".into(),
            app_path: "".into(),
            window_title: "".into(),
            document_path: None,
            activated_at: 300,
            browser_title: None,
            monitor_id: None,
        });
        let top = store.top_apps(10, None);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].app_name, "Firefox");
        assert_eq!(top[0].switches, 5);
        assert_eq!(top[1].app_name, "Terminal");
        assert_eq!(top[1].switches, 3);
        assert_eq!(top[2].app_name, "Code");
    }

    #[test]
    fn top_apps_with_since_filter() {
        let store = open_temp();
        for _ in 0..5 {
            store.insert_active_window(&ActiveWindowInfo {
                app_name: "Old".into(),
                app_path: "".into(),
                window_title: "".into(),
                document_path: None,
                activated_at: 100,
                browser_title: None,
                monitor_id: None,
            });
        }
        for _ in 0..2 {
            store.insert_active_window(&ActiveWindowInfo {
                app_name: "New".into(),
                app_path: "".into(),
                window_title: "".into(),
                document_path: None,
                activated_at: 500,
                browser_title: None,
                monitor_id: None,
            });
        }
        let top = store.top_apps(10, Some(400));
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].app_name, "New");
        assert_eq!(top[0].switches, 2);
    }

    #[test]
    fn top_apps_respects_limit() {
        let store = open_temp();
        for i in 0..10u64 {
            store.insert_active_window(&ActiveWindowInfo {
                app_name: format!("App{}", i),
                app_path: "".into(),
                window_title: "".into(),
                document_path: None,
                activated_at: i,
                browser_title: None,
                monitor_id: None,
            });
        }
        assert_eq!(store.top_apps(3, None).len(), 3);
    }

    // ── file_interactions ────────────────────────────────────────────────────

    #[test]
    fn insert_and_retrieve_file_interaction() {
        let store = open_temp();
        ins(&store, "/home/user/main.rs", "code", "", 1_000);
        ins(&store, "/home/user/lib.rs", "code", "", 2_000);
        let rows = store.get_recent_files(10, None);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].file_path, "/home/user/lib.rs");
        assert_eq!(rows[0].seen_at, 2_000);
        assert_eq!(rows[1].file_path, "/home/user/main.rs");
    }

    #[test]
    fn recent_files_since_filter() {
        let store = open_temp();
        ins(&store, "/old", "vim", "", 100);
        ins(&store, "/new", "vim", "", 500);
        let rows = store.get_recent_files(10, Some(400));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].file_path, "/new");
    }

    #[test]
    fn top_files_counts_and_order() {
        let store = open_temp();
        for _ in 0..5 {
            ins(&store, "/a.rs", "code", "projA", 100);
        }
        for _ in 0..3 {
            ins(&store, "/b.rs", "code", "projA", 200);
        }
        ins(&store, "/c.rs", "code", "projB", 300);
        let top = store.top_files(10, None);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].file_path, "/a.rs");
        assert_eq!(top[0].interactions, 5);
        assert_eq!(top[1].file_path, "/b.rs");
        assert_eq!(top[1].interactions, 3);
        assert_eq!(top[2].file_path, "/c.rs");
    }

    #[test]
    fn top_files_respects_limit() {
        let store = open_temp();
        for i in 0..10u64 {
            ins(&store, &format!("/f{i}"), "app", "", i);
        }
        assert_eq!(store.top_files(3, None).len(), 3);
    }

    #[test]
    fn finalize_backfill() {
        let store = open_temp();
        let id = ins(&store, "/a.rs", "code", "", 1_000).unwrap();
        store.finalize_file_interaction(id, 120, true, 42, 8, 3, 15, 0);
        let rows = store.get_recent_files(1, None);
        assert_eq!(rows[0].duration_secs, Some(120));
        assert!(rows[0].was_modified);
        assert_eq!(rows[0].size_delta, 42);
        assert_eq!(rows[0].lines_added, 8);
        assert_eq!(rows[0].lines_removed, 3);
        assert_eq!(rows[0].words_delta, 15);
    }

    #[test]
    fn top_files_sums_duration_and_edits() {
        let store = open_temp();
        let id1 = ins(&store, "/a.rs", "code", "", 100).unwrap();
        store.finalize_file_interaction(id1, 60, true, 10, 5, 2, 8, 0);
        let id2 = ins(&store, "/a.rs", "code", "", 200).unwrap();
        store.finalize_file_interaction(id2, 30, false, 0, 0, 0, 0, 0);
        let top = store.top_files(10, None);
        assert_eq!(top[0].total_secs, 90);
        assert_eq!(top[0].edits, 1);
    }

    #[test]
    fn top_projects() {
        let store = open_temp();
        ins(&store, "/a.rs", "code", "skill", 100);
        ins(&store, "/b.rs", "code", "skill", 200);
        ins(&store, "/c.py", "code", "other", 300);
        let top = store.top_projects(10, None);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].project, "skill");
        assert_eq!(top[0].interactions, 2);
    }

    #[test]
    fn edit_chunks_per_interaction() {
        let store = open_temp();
        let id = ins(&store, "/a.rs", "code", "", 1_000).unwrap();
        store.insert_edit_chunk(id, 1_005, 3, 1, 20, 0);
        store.insert_edit_chunk(id, 1_010, 2, 0, 15, 0);
        let chunks = store.get_edit_chunks(id);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].lines_added, 3);
        assert_eq!(chunks[0].lines_removed, 1);
        assert_eq!(chunks[1].lines_added, 2);
        assert_eq!(chunks[1].chunk_at, 1_010);
    }

    #[test]
    fn edit_chunks_range_query() {
        let store = open_temp();
        let id = ins(&store, "/a.rs", "code", "", 1_000).unwrap();
        store.insert_edit_chunk(id, 1_000, 1, 0, 5, 0);
        store.insert_edit_chunk(id, 1_005, 2, 1, 10, 1);
        store.insert_edit_chunk(id, 1_010, 3, 0, 15, 0);
        let chunks = store.get_edit_chunks_range(1_004, 1_006);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_at, 1_005);
    }

    #[test]
    fn prune_old_rows() {
        let store = open_temp();
        ins(&store, "/old.rs", "code", "", 100);
        ins(&store, "/new.rs", "code", "", 500);
        let deleted = store.prune_file_interactions(400);
        assert_eq!(deleted, 1);
        assert_eq!(store.get_recent_files(10, None).len(), 1);
    }

    // ── Range query tests ───────────────────────────────────────────────────

    #[test]
    fn files_in_range_returns_chronological() {
        let store = open_temp();
        ins(&store, "/a.rs", "code", "proj", 100);
        ins(&store, "/b.rs", "code", "proj", 200);
        ins(&store, "/c.rs", "code", "proj", 300);
        let rows = store.get_files_in_range(150, 350, 10);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].file_path, "/b.rs");
        assert_eq!(rows[1].file_path, "/c.rs");
    }

    #[test]
    fn files_in_range_empty_for_no_match() {
        let store = open_temp();
        ins(&store, "/a.rs", "code", "", 100);
        let rows = store.get_files_in_range(200, 300, 10);
        assert!(rows.is_empty());
    }

    #[test]
    fn files_in_range_respects_limit() {
        let store = open_temp();
        for i in 0..20u64 {
            ins(&store, &format!("/f{i}.rs"), "code", "", 100 + i);
        }
        let rows = store.get_files_in_range(100, 200, 5);
        assert_eq!(rows.len(), 5);
    }

    // ── Meeting event tests ─────────────────────────────────────────────────

    #[test]
    fn meeting_start_and_end() {
        let store = open_temp();
        let id = store
            .insert_meeting_start("zoom", "Team Sync", "zoom.us", 1000)
            .unwrap();
        store.update_meeting_end(id, 2000);
        let meetings = store.get_meetings_in_range(500, 2500);
        assert_eq!(meetings.len(), 1);
        assert_eq!(meetings[0].platform, "zoom");
        assert_eq!(meetings[0].end_at, Some(2000));
    }

    #[test]
    fn meetings_in_range_filters_correctly() {
        let store = open_temp();
        let id1 = store.insert_meeting_start("zoom", "Early", "zoom", 100).unwrap();
        store.update_meeting_end(id1, 200);
        let id2 = store.insert_meeting_start("teams", "Late", "teams", 500).unwrap();
        store.update_meeting_end(id2, 600);
        // Range 150..250 only overlaps the first meeting.
        let meetings = store.get_meetings_in_range(150, 250);
        assert_eq!(meetings.len(), 1);
        assert_eq!(meetings[0].platform, "zoom");
    }

    #[test]
    fn meetings_open_ended_included() {
        let store = open_temp();
        // Meeting started but not ended — end_at is NULL.
        store.insert_meeting_start("slack", "Huddle", "slack", 1000);
        let meetings = store.get_meetings_in_range(900, 1100);
        assert_eq!(meetings.len(), 1);
        assert_eq!(meetings[0].end_at, None);
    }

    // ── Clipboard event tests ───────────────────────────────────────────────

    #[test]
    fn clipboard_insert_and_retrieve() {
        let store = open_temp();
        store.insert_clipboard_event("Code", "text", 42, 1000);
        store.insert_clipboard_event("Safari", "image", 8192, 2000);
        let rows = store.get_recent_clipboard(10);
        assert_eq!(rows.len(), 2);
        // Newest first.
        assert_eq!(rows[0].source_app, "Safari");
        assert_eq!(rows[0].content_type, "image");
        assert_eq!(rows[0].content_size, 8192);
        assert_eq!(rows[1].source_app, "Code");
    }

    #[test]
    fn clipboard_limit_respected() {
        let store = open_temp();
        for i in 0..10u64 {
            store.insert_clipboard_event("App", "text", i, 100 + i);
        }
        assert_eq!(store.get_recent_clipboard(3).len(), 3);
    }

    // ── Analysis tests ──────────────────────────────────────────────────────

    #[test]
    fn productivity_score_empty_day() {
        let store = open_temp();
        let score = store.productivity_score(1000);
        assert_eq!(score.day_start, 1000);
        // Empty day: context stability gets max (no switches), everything else 0.
        assert!(score.score >= 0.0);
        assert!(score.score <= 100.0);
    }

    #[test]
    fn productivity_score_with_edits() {
        let store = open_temp();
        let day = 1_700_000_000u64;
        // Insert several file interactions during the day.
        for i in 0..20u64 {
            store.insert_file_interaction(
                &format!("/f{}.rs", i % 5),
                "code",
                "proj",
                "rust",
                "code",
                "main",
                day + i * 60,
                Some(70.0),
                None,
            );
        }
        // Finalize some with edits.
        store.finalize_file_interaction(1, 300, true, 100, 50, 10, 20, 3);
        let score = store.productivity_score(day);
        assert!(score.score > 0.0);
    }

    #[test]
    fn weekly_digest_returns_7_days() {
        let store = open_temp();
        let week = 1_700_000_000u64;
        ins(&store, "/a.rs", "code", "proj", week + 100);
        let digest = store.weekly_digest(week);
        assert_eq!(digest.days.len(), 7);
        assert_eq!(digest.week_start, week);
        assert!(digest.total_interactions >= 1);
    }

    #[test]
    fn stale_files_detects_old_edits() {
        let store = open_temp();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // File edited 30 days ago.
        let old_ts = now - 30 * 86400;
        let id = ins(&store, "/stale.rs", "code", "proj", old_ts).unwrap();
        store.finalize_file_interaction(id, 60, true, 0, 10, 5, 0, 0);
        // File edited today.
        let new_id = ins(&store, "/fresh.rs", "code", "proj", now).unwrap();
        store.finalize_file_interaction(new_id, 60, true, 0, 10, 5, 0, 0);

        let stale = store.stale_files(7, old_ts - 86400);
        // Only the old file should be stale.
        assert!(stale.iter().any(|s| s.file_path == "/stale.rs"));
        assert!(!stale.iter().any(|s| s.file_path == "/fresh.rs"));
    }

    // ── Pruning tests ───────────────────────────────────────────────────────

    #[test]
    fn prune_meetings_removes_old() {
        let store = open_temp();
        store.insert_meeting_start("zoom", "Old", "zoom", 100);
        store.insert_meeting_start("teams", "New", "teams", 500);
        let pruned = store.prune_meetings(400);
        assert_eq!(pruned, 1);
        let remaining = store.get_meetings_in_range(0, 1000);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].platform, "teams");
    }

    #[test]
    fn prune_clipboard_removes_old() {
        let store = open_temp();
        store.insert_clipboard_event("App", "text", 10, 100);
        store.insert_clipboard_event("App", "text", 10, 500);
        let pruned = store.prune_clipboard(400);
        assert_eq!(pruned, 1);
        assert_eq!(store.get_recent_clipboard(10).len(), 1);
    }

    #[test]
    fn insert_and_get_secondary_windows() {
        let store = open_temp();
        let primary_id = store.insert_active_window(&dummy_window(1000)).unwrap();
        store.insert_secondary_windows(
            primary_id,
            &[
                SecondaryWindowInfo {
                    app_name: "Safari".into(),
                    window_title: "Docs".into(),
                    monitor_id: 1,
                },
                SecondaryWindowInfo {
                    app_name: "Slack".into(),
                    window_title: "#general".into(),
                    monitor_id: 2,
                },
            ],
        );
        let sec = store.get_secondary_windows(primary_id);
        assert_eq!(sec.len(), 2);
        assert_eq!(sec[0].app_name, "Safari");
        assert_eq!(sec[1].monitor_id, 2);
    }

    #[test]
    fn schema_migration_idempotent() {
        // Opening the store twice should not fail — ALTER TABLE is idempotent.
        let dir = tempdir().unwrap();
        let _s1 = ActivityStore::open(dir.path()).unwrap();
        let _s2 = ActivityStore::open(dir.path()).unwrap();
    }

    #[test]
    fn finalize_writes_undo_count() {
        let store = open_temp();
        let id = ins(&store, "/undo.rs", "code", "proj", 1000).unwrap();
        store.finalize_file_interaction(id, 60, true, 0, 10, 5, 0, 7);
        let files = store.get_recent_files(1, None);
        assert_eq!(files[0].undo_count, 7);
    }

    // ── Fusion insight tests ────────────────────────────────────────────────

    #[test]
    fn detect_task_type_default_coding() {
        let store = open_temp();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        ins(&store, "/a.rs", "code", "proj", now);
        let result = store.detect_task_type(3600);
        assert_eq!(result.task_type, "coding");
    }

    #[test]
    fn detect_task_type_with_debug_events() {
        let store = open_temp();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        ins(&store, "/a.rs", "code", "proj", now);
        store.insert_build_event("debug_start", "running", "", now);
        let result = store.detect_task_type(3600);
        assert_eq!(result.task_type, "debugging");
    }

    #[test]
    fn predict_struggle_empty() {
        let store = open_temp();
        let result = store.predict_struggle(300);
        assert!(!result.struggling);
    }

    #[test]
    fn code_eeg_correlation_groups_by_language() {
        let store = open_temp();
        store.insert_file_interaction("/a.rs", "code", "proj", "rust", "", "", 100, Some(80.0), None);
        store.insert_file_interaction("/b.ts", "code", "proj", "typescript", "", "", 200, Some(40.0), None);
        let result = store.code_eeg_correlation(0);
        assert!(result.by_language.len() >= 2);
        // Rust has higher focus than TypeScript
        let rust = result.by_language.iter().find(|r| r.key == "rust");
        let ts = result.by_language.iter().find(|r| r.key == "typescript");
        assert!(rust.unwrap().avg_focus > ts.unwrap().avg_focus);
    }

    #[test]
    fn ai_events_insert_and_retrieve() {
        let store = open_temp();
        store.insert_ai_event("suggestion_accepted", "copilot", "/a.rs", "rust", 1000);
        store.insert_ai_event("chat_start", "claude", "/b.ts", "typescript", 2000);
        let events = store.get_recent_ai_events(10);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].source, "claude"); // newest first
        assert_eq!(events[1].event_type, "suggestion_accepted");
    }
}
