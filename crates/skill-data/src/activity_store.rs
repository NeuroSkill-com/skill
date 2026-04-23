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

// ── DDL ───────────────────────────────────────────────────────────────────────

const DDL: &str = "
CREATE TABLE IF NOT EXISTS active_windows (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    app_name     TEXT    NOT NULL,
    app_path     TEXT    NOT NULL DEFAULT '',
    window_title TEXT    NOT NULL DEFAULT '',
    activated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_aw_activated ON active_windows (activated_at DESC);

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
    eeg_mood      REAL
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
    size_delta     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_fec_interaction ON file_edit_chunks (interaction_id);
CREATE INDEX IF NOT EXISTS idx_fec_chunk ON file_edit_chunks (chunk_at DESC);
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
        if let Err(e) = conn.execute_batch(DDL) {
            eprintln!("[activity] DDL: {e}");
            return None;
        }
        Some(Self { conn: Mutex::new(conn) })
    }

    // ── Writers ───────────────────────────────────────────────────────────────

    /// Record that the frontmost window changed to `info`.
    pub fn insert_active_window(&self, info: &ActiveWindowInfo) {
        let c = self.conn.lock_or_recover();
        if let Err(e) = c.execute(
            "INSERT INTO active_windows (app_name, app_path, window_title, activated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                &info.app_name,
                &info.app_path,
                &info.window_title,
                info.activated_at as i64,
            ],
        ) {
            eprintln!("[activity] insert_active_window: {e}");
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
            "SELECT id, app_name, app_path, window_title, activated_at
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
    pub fn finalize_file_interaction(
        &self,
        row_id: i64,
        duration_secs: u64,
        was_modified: bool,
        size_delta: i64,
        lines_added: u64,
        lines_removed: u64,
        words_delta: i64,
    ) {
        let c = self.conn.lock_or_recover();
        if let Err(e) = c.execute(
            "UPDATE file_interactions
             SET duration_secs = ?1, was_modified = ?2, size_delta = ?3,
                 lines_added = ?4, lines_removed = ?5, words_delta = ?6
             WHERE id = ?7",
            params![
                duration_secs as i64,
                was_modified as i64,
                size_delta,
                lines_added as i64,
                lines_removed as i64,
                words_delta,
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

    // ── Focus sessions ───────────────────────────────────────────────────────

    /// Insert a detected focus session.
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

    // ── Edit chunks ────────────────────────────────────────────────────────────

    /// Record a 5-second edit chunk within a file interaction.
    pub fn insert_edit_chunk(
        &self,
        interaction_id: i64,
        chunk_at: u64,
        lines_added: u64,
        lines_removed: u64,
        size_delta: i64,
    ) {
        let c = self.conn.lock_or_recover();
        if let Err(e) = c.execute(
            "INSERT INTO file_edit_chunks (interaction_id, chunk_at, lines_added, lines_removed, size_delta)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                interaction_id,
                chunk_at as i64,
                lines_added as i64,
                lines_removed as i64,
                size_delta,
            ],
        ) {
            eprintln!("[activity] insert_edit_chunk: {e}");
        }
    }

    /// Return all edit chunks for a given file interaction, oldest first.
    pub fn get_edit_chunks(&self, interaction_id: i64) -> Vec<EditChunkRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, interaction_id, chunk_at, lines_added, lines_removed, size_delta
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
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return all edit chunks in a time range, oldest first.
    pub fn get_edit_chunks_range(&self, from_ts: u64, to_ts: u64) -> Vec<EditChunkRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, interaction_id, chunk_at, lines_added, lines_removed, size_delta
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

    /// Return the `limit` most recent file interaction records, newest first.
    pub fn get_recent_files(&self, limit: u32, since: Option<u64>) -> Vec<FileInteractionRow> {
        let c = self.conn.lock_or_recover();
        let (sql, p): (&str, Vec<i64>) = match since {
            Some(ts) => (
                "SELECT id, file_path, app_name, project, language, category,
                        git_branch, seen_at, duration_secs, was_modified,
                        size_delta, lines_added, lines_removed, words_delta,
                        eeg_focus, eeg_mood
                 FROM file_interactions WHERE seen_at >= ?1
                 ORDER BY seen_at DESC LIMIT ?2",
                vec![ts as i64, limit as i64],
            ),
            None => (
                "SELECT id, file_path, app_name, project, language, category,
                        git_branch, seen_at, duration_secs, was_modified,
                        size_delta, lines_added, lines_removed, words_delta,
                        eeg_focus, eeg_mood
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

// ── Tests ──────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
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
            });
        }
        for _ in 0..3 {
            store.insert_active_window(&ActiveWindowInfo {
                app_name: "Terminal".into(),
                app_path: "".into(),
                window_title: "".into(),
                document_path: None,
                activated_at: 200,
            });
        }
        store.insert_active_window(&ActiveWindowInfo {
            app_name: "Code".into(),
            app_path: "".into(),
            window_title: "".into(),
            document_path: None,
            activated_at: 300,
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
            });
        }
        for _ in 0..2 {
            store.insert_active_window(&ActiveWindowInfo {
                app_name: "New".into(),
                app_path: "".into(),
                window_title: "".into(),
                document_path: None,
                activated_at: 500,
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
        store.finalize_file_interaction(id, 120, true, 42, 8, 3, 15);
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
        store.finalize_file_interaction(id1, 60, true, 10, 5, 2, 8);
        let id2 = ins(&store, "/a.rs", "code", "", 200).unwrap();
        store.finalize_file_interaction(id2, 30, false, 0, 0, 0, 0);
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
        store.insert_edit_chunk(id, 1_005, 3, 1, 20);
        store.insert_edit_chunk(id, 1_010, 2, 0, 15);
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
        store.insert_edit_chunk(id, 1_000, 1, 0, 5);
        store.insert_edit_chunk(id, 1_005, 2, 1, 10);
        store.insert_edit_chunk(id, 1_010, 3, 0, 15);
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
}
