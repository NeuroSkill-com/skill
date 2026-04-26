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
CREATE INDEX IF NOT EXISTS idx_fi_seen_path ON file_interactions (seen_at, file_path);
CREATE INDEX IF NOT EXISTS idx_fi_seen_project ON file_interactions (seen_at, project);

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

-- Terminal shell commands with EEG correlation.
CREATE TABLE IF NOT EXISTS terminal_commands (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    terminal_name TEXT    NOT NULL DEFAULT '',
    shell_type    TEXT    NOT NULL DEFAULT '',
    command       TEXT    NOT NULL,
    binary        TEXT    NOT NULL DEFAULT '',
    args          TEXT    NOT NULL DEFAULT '',
    cwd           TEXT    NOT NULL DEFAULT '',
    exit_code     INTEGER,
    started_at    INTEGER NOT NULL,
    ended_at      INTEGER,
    duration_secs INTEGER,
    category      TEXT    NOT NULL DEFAULT 'other',
    project       TEXT    NOT NULL DEFAULT '',
    eeg_focus     REAL,
    eeg_focus_end REAL,
    eeg_mood      REAL
);
CREATE INDEX IF NOT EXISTS idx_tc_started ON terminal_commands (started_at DESC);
CREATE INDEX IF NOT EXISTS idx_tc_category ON terminal_commands (category);
CREATE INDEX IF NOT EXISTS idx_tc_binary ON terminal_commands (binary);

-- Dev loops: edit → build/test → result cycles.
CREATE TABLE IF NOT EXISTS dev_loops (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at       INTEGER NOT NULL,
    ended_at         INTEGER,
    loop_type        TEXT    NOT NULL DEFAULT '',
    project          TEXT    NOT NULL DEFAULT '',
    iteration_count  INTEGER NOT NULL DEFAULT 1,
    pass_count       INTEGER NOT NULL DEFAULT 0,
    fail_count       INTEGER NOT NULL DEFAULT 0,
    avg_cycle_secs   REAL,
    avg_focus        REAL,
    focus_start      REAL,
    focus_end        REAL,
    focus_trend      TEXT    NOT NULL DEFAULT 'stable',
    files_touched    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_dl_started ON dev_loops (started_at DESC);

-- Zone switches: editor / terminal / panel transitions.
CREATE TABLE IF NOT EXISTS zone_switches (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    zone      TEXT    NOT NULL,
    from_zone TEXT    NOT NULL DEFAULT '',
    at        INTEGER NOT NULL,
    eeg_focus REAL
);
CREATE INDEX IF NOT EXISTS idx_zs_at ON zone_switches (at DESC);

-- Layout snapshots (periodic).
CREATE TABLE IF NOT EXISTS layout_snapshots (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    sampled_at       INTEGER NOT NULL,
    editor_groups    INTEGER NOT NULL DEFAULT 1,
    visible_editors  INTEGER NOT NULL DEFAULT 1,
    open_tabs        INTEGER NOT NULL DEFAULT 0,
    terminals        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_ls_sampled ON layout_snapshots (sampled_at DESC);

-- EEG time-series: periodic snapshots of all brain metrics.
-- Events correlate by joining on nearest timestamp — no fixed EEG columns needed.
-- JSON metrics column is extensible: add new metrics without schema changes.
-- Example: {\"focus\":72,\"mood\":45,\"alpha\":0.3,\"beta\":0.5,\"theta\":0.2,\"hrv\":65,\"stress\":0.4}
CREATE TABLE IF NOT EXISTS eeg_timeseries (
    ts      INTEGER PRIMARY KEY,
    metrics TEXT    NOT NULL DEFAULT '{}'
);

-- Generic embedding store: decouple embeddings from specific tables.
-- Can re-embed with different models, store multiple vectors per item,
-- and query across all source types.
CREATE TABLE IF NOT EXISTS embeddings (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_type TEXT    NOT NULL,
    source_id   INTEGER NOT NULL,
    source_text TEXT    NOT NULL DEFAULT '',
    model       TEXT    NOT NULL,
    vector      BLOB    NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_emb_source ON embeddings (source_type, source_id);
CREATE INDEX IF NOT EXISTS idx_emb_model ON embeddings (model);
CREATE INDEX IF NOT EXISTS idx_emb_created ON embeddings (created_at DESC);

-- Conversation messages from AI coding assistants (claude, pi, etc.).
-- Stores full text for all roles (user, assistant, tool) for searchability.
CREATE TABLE IF NOT EXISTS conversations (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    app       TEXT    NOT NULL DEFAULT '',
    role      TEXT    NOT NULL DEFAULT '',
    text      TEXT    NOT NULL,
    cwd       TEXT    NOT NULL DEFAULT '',
    at        INTEGER NOT NULL,
    session   TEXT    NOT NULL DEFAULT '',
    eeg_focus REAL,
    eeg_mood  REAL
);
CREATE INDEX IF NOT EXISTS idx_conv_at ON conversations (at DESC);
CREATE INDEX IF NOT EXISTS idx_conv_app ON conversations (app);
CREATE INDEX IF NOT EXISTS idx_conv_role ON conversations (role);

-- Browser activity events from Chrome/Firefox/Safari extensions.
CREATE TABLE IF NOT EXISTS browser_activities (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type        TEXT    NOT NULL,
    url               TEXT    NOT NULL DEFAULT '',
    domain            TEXT    NOT NULL DEFAULT '',
    title             TEXT    NOT NULL DEFAULT '',
    tab_id            INTEGER,
    browser_name      TEXT    NOT NULL DEFAULT '',
    scroll_depth      REAL,
    reading_time_secs INTEGER,
    active_time_secs  INTEGER,
    idle_time_secs    INTEGER,
    typing_detected   INTEGER NOT NULL DEFAULT 0,
    media_playing     INTEGER NOT NULL DEFAULT 0,
    search_query      TEXT    NOT NULL DEFAULT '',
    tab_count         INTEGER,
    devtools_open     INTEGER NOT NULL DEFAULT 0,
    category          TEXT    NOT NULL DEFAULT '',
    content_type      TEXT    NOT NULL DEFAULT '',
    referrer_domain   TEXT    NOT NULL DEFAULT '',
    nav_type          TEXT    NOT NULL DEFAULT '',
    click_target      TEXT    NOT NULL DEFAULT '',
    click_count       INTEGER,
    mouse_distance    INTEGER,
    mouse_idle_secs   INTEGER,
    has_video         INTEGER NOT NULL DEFAULT 0,
    has_audio         INTEGER NOT NULL DEFAULT 0,
    image_count       INTEGER,
    word_count        INTEGER,
    form_count        INTEGER,
    video_watched_secs INTEGER,
    video_playback_rate REAL,
    copy_length       INTEGER,
    paste_length      INTEGER,
    scroll_speed      INTEGER,
    scroll_direction  TEXT    NOT NULL DEFAULT '',
    scroll_reversals  INTEGER,
    llm_provider      TEXT    NOT NULL DEFAULT '',
    llm_turn_count    INTEGER,
    email_mode        TEXT    NOT NULL DEFAULT '',
    email_count       INTEGER,
    revisit_count     INTEGER,
    domain_visit_count INTEGER,
    visible_text      TEXT    NOT NULL DEFAULT '',
    heading           TEXT    NOT NULL DEFAULT '',
    page_title        TEXT    NOT NULL DEFAULT '',
    download_type     TEXT    NOT NULL DEFAULT '',
    at                INTEGER NOT NULL,
    eeg_focus         REAL,
    eeg_mood          REAL
);
CREATE INDEX IF NOT EXISTS idx_ba_at ON browser_activities (at DESC);
CREATE INDEX IF NOT EXISTS idx_ba_domain ON browser_activities (domain);
CREATE INDEX IF NOT EXISTS idx_ba_category ON browser_activities (category);
CREATE INDEX IF NOT EXISTS idx_ba_event ON browser_activities (event_type);
CREATE INDEX IF NOT EXISTS idx_ba_content ON browser_activities (content_type);

CREATE TABLE IF NOT EXISTS user_screenshot_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    screenshot_id INTEGER NOT NULL,
    captured_at   INTEGER NOT NULL,
    app_name      TEXT    NOT NULL DEFAULT '',
    window_title  TEXT    NOT NULL DEFAULT '',
    original_path TEXT    NOT NULL DEFAULT '',
    ocr_preview   TEXT    NOT NULL DEFAULT '',
    eeg_focus     REAL,
    eeg_mood      REAL
);
CREATE INDEX IF NOT EXISTS idx_use_captured ON user_screenshot_events (captured_at DESC);

-- User feedback on brain state predictions (yay/nay).
-- Accumulates over time for statistical weight adjustment.
CREATE TABLE IF NOT EXISTS brain_feedback (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    insight     TEXT    NOT NULL,
    correct     INTEGER NOT NULL,
    score       REAL,
    eeg_focus   REAL,
    eeg_mood    REAL,
    context     TEXT    NOT NULL DEFAULT '',
    at          INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_bf_insight ON brain_feedback (insight);
CREATE INDEX IF NOT EXISTS idx_bf_at ON brain_feedback (at DESC);
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
            // terminal_commands — binary/args extraction
            "ALTER TABLE terminal_commands ADD COLUMN binary TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE terminal_commands ADD COLUMN args TEXT NOT NULL DEFAULT ''",
            // conversations — EEG columns
            "ALTER TABLE conversations ADD COLUMN eeg_focus REAL",
            "ALTER TABLE conversations ADD COLUMN eeg_mood REAL",
            // browser_activities — expanded tracking columns
            "ALTER TABLE browser_activities ADD COLUMN active_time_secs INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN idle_time_secs INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN content_type TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE browser_activities ADD COLUMN nav_type TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE browser_activities ADD COLUMN click_target TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE browser_activities ADD COLUMN click_count INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN mouse_distance INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN mouse_idle_secs INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN has_video INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE browser_activities ADD COLUMN has_audio INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE browser_activities ADD COLUMN image_count INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN word_count INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN form_count INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN video_watched_secs INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN video_playback_rate REAL",
            "ALTER TABLE browser_activities ADD COLUMN copy_length INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN paste_length INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN scroll_speed INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN scroll_direction TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE browser_activities ADD COLUMN scroll_reversals INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN llm_provider TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE browser_activities ADD COLUMN llm_turn_count INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN email_mode TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE browser_activities ADD COLUMN email_count INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN revisit_count INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN domain_visit_count INTEGER",
            "ALTER TABLE browser_activities ADD COLUMN visible_text TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE browser_activities ADD COLUMN heading TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE browser_activities ADD COLUMN page_title TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE browser_activities ADD COLUMN download_type TEXT NOT NULL DEFAULT ''",
        ] {
            let _ = conn.execute_batch(alter);
        }
        if let Err(e) = conn.execute_batch(DDL) {
            eprintln!("[activity] DDL: {e}");
            return None;
        }
        // FTS5 virtual table for full-text search on conversations.
        let _ = conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS conversations_fts USING fts5(
                text, app, role, content=conversations, content_rowid=id
            );",
        );
        Some(Self { conn: Mutex::new(conn) })
    }

    /// Open an existing activity database for read-only queries.
    /// Skips schema migrations and DDL — use only when the database has already
    /// been initialised by a prior `open()` call (e.g. from the activity worker).
    pub fn open_readonly(skill_dir: &Path) -> Option<Self> {
        let path = skill_dir.join(skill_constants::ACTIVITY_FILE);
        if !path.exists() {
            return None;
        }
        let conn = match Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[activity] open_readonly {}: {e}", path.display());
                return None;
            }
        };
        crate::util::init_wal_pragmas(&conn);
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
        avg_eeg_focus: Option<f32>,
        avg_eeg_mood: Option<f32>,
    ) {
        let c = self.conn.lock_or_recover();
        if let Err(e) = c.execute(
            "UPDATE file_interactions
             SET duration_secs = ?1, was_modified = ?2, size_delta = ?3,
                 lines_added = ?4, lines_removed = ?5, words_delta = ?6,
                 undo_count = ?7,
                 eeg_focus = COALESCE(?9, eeg_focus),
                 eeg_mood = COALESCE(?10, eeg_mood)
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
                avg_eeg_focus,
                avg_eeg_mood,
            ],
        ) {
            eprintln!("[activity] finalize_file_interaction: {e}");
        }
    }

    /// Update only the duration of a file interaction (from VSCode dwell time events).
    pub fn update_file_interaction_duration(&self, row_id: i64, duration_secs: u64) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "UPDATE file_interactions SET duration_secs = ?1 WHERE id = ?2 AND (duration_secs IS NULL OR duration_secs < ?1)",
            params![duration_secs as i64, row_id],
        );
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
        Self::context_switch_rate_q(&c, from_ts, to_ts)
    }

    fn context_switch_rate_q(c: &rusqlite::Connection, from_ts: u64, to_ts: u64) -> f64 {
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
        Self::daily_summary_q(&c, day_start)
    }

    fn daily_summary_q(c: &rusqlite::Connection, day_start: u64) -> DailySummaryRow {
        let day_end = day_start + 86400;
        c.query_row(
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
        )
        .unwrap_or(DailySummaryRow {
            day_start,
            ..Default::default()
        })
    }

    /// Hourly edit heatmap: lines changed per hour of day (0-23).
    /// `tz_offset_secs` is the local UTC offset (e.g. -28800 for UTC-8).
    pub fn hourly_edit_heatmap(&self, since: Option<u64>, tz_offset_secs: i32) -> Vec<HourlyEditRow> {
        let c = self.conn.lock_or_recover();
        let since_ts = since.unwrap_or(0) as i64;
        let mut stmt = match c.prepare_cached(
            "SELECT ((seen_at + ?2) % 86400) / 3600 AS hour,
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
        stmt.query_map(params![since_ts, tz_offset_secs as i64], |row| {
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
        let c = self.conn.lock_or_recover();
        let summary = Self::daily_summary_q(&c, day_start);
        let day_end = day_start + 86400;
        let switch_rate = Self::context_switch_rate_q(&c, day_start, day_end);
        let sessions = Self::get_focus_sessions_in_range_q(&c, day_start, day_end);

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
        let c = self.conn.lock_or_recover();
        let mut days = Vec::with_capacity(7);
        let mut total_interactions = 0u64;
        let mut total_edits = 0u64;
        let mut total_secs = 0u64;
        let mut total_added = 0u64;
        let mut total_removed = 0u64;
        let mut focus_sum = 0.0f64;
        let mut focus_count = 0u32;

        for d in 0..7u64 {
            let day = Self::daily_summary_q(&c, week_start + d * 86400);
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
        let sessions = Self::get_focus_sessions_in_range_q(&c, week_start, week_end);
        // Drop the lock before calling methods that manage their own locking.
        drop(c);
        let top_projects = self.top_projects(5, Some(week_start));
        let languages = self.language_breakdown(Some(week_start));
        let meetings = self.get_meetings_in_range(week_start, week_end);

        // Find peak day (most edits).
        let peak_day_idx = days
            .iter()
            .enumerate()
            .max_by_key(|(_, d)| d.edits)
            .map(|(i, _)| i as u8)
            .unwrap_or(0);

        // Find peak hour from heatmap.
        let heatmap = self.hourly_edit_heatmap(Some(week_start), crate::util::local_tz_offset_secs());
        let peak_hour = heatmap
            .iter()
            .max_by_key(|h| h.total_churn)
            .map(|h| h.hour)
            .unwrap_or(0);

        // Browser stats for the week
        let browser_top_domains = self.browser_domain_breakdown(week_start);
        let browser_content_breakdown = self.browser_content_breakdown(week_start);
        let browser_events: u64 = {
            let c2 = self.conn.lock_or_recover();
            c2.query_row(
                "SELECT COUNT(*) FROM browser_activities WHERE at >= ?1 AND at < ?2",
                params![week_start as i64, week_end as i64],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as u64
        };
        let browser_total_reading_secs: u64 = {
            let c2 = self.conn.lock_or_recover();
            c2.query_row(
                "SELECT COALESCE(SUM(reading_time_secs), 0) FROM browser_activities WHERE at >= ?1 AND at < ?2 AND reading_time_secs IS NOT NULL",
                params![week_start as i64, week_end as i64], |row| row.get::<_,i64>(0),
            ).unwrap_or(0) as u64
        };
        let browser_video_watched_secs: u64 = {
            let c2 = self.conn.lock_or_recover();
            c2.query_row(
                "SELECT COALESCE(SUM(video_watched_secs), 0) FROM browser_activities WHERE at >= ?1 AND at < ?2 AND video_watched_secs IS NOT NULL",
                params![week_start as i64, week_end as i64], |row| row.get::<_,i64>(0),
            ).unwrap_or(0) as u64
        };
        let browser_avg_distraction: Option<f64> = {
            let d = self.browser_distraction_score(7 * 86400);
            if d.score > 0.0 {
                Some(d.score)
            } else {
                None
            }
        };

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
            browser_events,
            browser_top_domains,
            browser_content_breakdown,
            browser_total_reading_secs,
            browser_avg_distraction,
            browser_video_watched_secs,
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
        let mut result = row.unwrap_or(FlowStateResult {
            in_flow: false,
            score: 0.0,
            duration_secs: 0,
            avg_focus: None,
            file_switches: 0,
            edit_velocity: 0.0,
        });

        // ── Browser signal integration ──────────────────────────────
        // Frequent tab switching breaks flow. Penalize score if browser
        // data shows high distraction in the same window.
        let tab_switches: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities
             WHERE event_type = 'tab_switch' AND at >= ?1",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let social_events: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities
             WHERE category IN ('social', 'media') AND at >= ?1",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let minutes = window_secs as f32 / 60.0;
        let tab_rate = if minutes > 0.0 {
            tab_switches as f32 / minutes
        } else {
            0.0
        };

        // Apply feedback-adjusted weight
        let browser_weight = self.brain_feedback_weight("flow_browser") as f32;

        // >4 tab switches/min = significant distraction penalty
        if tab_rate > 4.0 {
            let penalty = ((tab_rate - 4.0) * 5.0).min(20.0) * browser_weight;
            result.score = (result.score - penalty).max(0.0);
            result.in_flow = false; // can't be in flow with rapid switching
        }
        // Social media during work = minor penalty
        if social_events > 2 {
            let penalty = (social_events as f32 * 2.0).min(10.0) * browser_weight;
            result.score = (result.score - penalty).max(0.0);
        }

        result
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
    /// `tz_offset_secs` is the local UTC offset (e.g. -28800 for UTC-8).
    pub fn optimal_hours(&self, since: u64, top_n: usize, tz_offset_secs: i32) -> OptimalHoursResult {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT ((seen_at + ?2) % 86400) / 3600 AS hour,
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
            .query_map(params![since as i64, tz_offset_secs as i64], |row| {
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
        let day_end = day_start + 86400;
        // Query periods in a scoped lock — must drop before calling
        // productivity_score(), which re-acquires the same Mutex.
        let periods: Vec<PeriodSummary> = {
            let c = self.conn.lock_or_recover();
            let mut stmt = match c.prepare_cached(
                "SELECT CASE
                    WHEN (seen_at - ?1) / 3600 BETWEEN 6 AND 11 THEN 'morning'
                    WHEN (seen_at - ?1) / 3600 BETWEEN 12 AND 17 THEN 'afternoon'
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
            stmt.query_map(params![day_start as i64, day_end as i64], |row| {
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
            .unwrap_or_default()
        };
        let best = periods
            .iter()
            .filter(|p| p.avg_focus.is_some())
            .max_by(|a, b| {
                a.avg_focus
                    .unwrap_or(0.0)
                    .partial_cmp(&b.avg_focus.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.period.clone())
            .unwrap_or_default();
        let focus_count = periods.iter().filter(|p| p.avg_focus.is_some()).count();
        let overall = if focus_count > 0 {
            Some(periods.iter().filter_map(|p| p.avg_focus).sum::<f32>() / focus_count as f32)
        } else {
            None
        };
        let score = self.productivity_score(day_start);
        DailyBrainReport {
            day_start,
            periods,
            overall_focus: overall,
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
        let week_days = days.len().min(7);
        let weekly: f32 = if week_days > 0 {
            days.iter().take(7).map(|d| d.deep_work_mins as f32).sum::<f32>() / week_days as f32
        } else {
            0.0
        };
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

    #[allow(clippy::too_many_arguments)]
    pub fn insert_ai_event(
        &self,
        event_type: &str,
        source: &str,
        file_path: &str,
        language: &str,
        at: u64,
        eeg_focus: Option<f64>,
        eeg_mood: Option<f64>,
    ) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO ai_events (event_type, source, file_path, language, at, eeg_focus, eeg_mood)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![event_type, source, file_path, language, at as i64, eeg_focus, eeg_mood],
        );
    }

    pub fn get_recent_ai_events(&self, limit: u32) -> Vec<AiEventRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, event_type, source, file_path, language, at, eeg_focus, eeg_mood
             FROM ai_events ORDER BY at DESC LIMIT ?1",
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
                eeg_focus: row.get(6)?,
                eeg_mood: row.get(7)?,
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

        // Terminal command categories — strong task-type signals.
        let term_cats: Vec<(String, i64)> = c
            .prepare_cached(
                "SELECT category, COUNT(*) FROM terminal_commands
                 WHERE started_at >= ?1 GROUP BY category ORDER BY COUNT(*) DESC",
            )
            .and_then(|mut stmt| {
                stmt.query_map(params![since as i64], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            })
            .unwrap_or_default();
        let term_top = term_cats.first().map(|(cat, _)| cat.as_str()).unwrap_or("");
        let term_total: i64 = term_cats.iter().map(|(_, n)| n).sum();

        // Terminal-based detection takes priority when recent commands are present.
        if term_total > 0 {
            if has_debug || term_top == "debug" {
                return TaskTypeResult {
                    task_type: "debugging".into(),
                    confidence: 0.9,
                    signals: vec!["debugger active or debug commands in terminal".into()],
                };
            }
            if term_top == "test" || has_test {
                return TaskTypeResult {
                    task_type: "testing".into(),
                    confidence: 0.85,
                    signals: vec![format!(
                        "test commands in terminal ({}x)",
                        term_cats
                            .iter()
                            .find(|(c, _)| c == "test")
                            .map(|(_, n)| *n)
                            .unwrap_or(0)
                    )],
                };
            }
            if term_top == "docker" || term_cats.iter().any(|(c, _)| c == "docker") {
                return TaskTypeResult {
                    task_type: "infrastructure".into(),
                    confidence: 0.8,
                    signals: vec!["docker/k8s commands in terminal".into()],
                };
            }
            if term_top == "deploy" {
                return TaskTypeResult {
                    task_type: "deploying".into(),
                    confidence: 0.85,
                    signals: vec!["deployment commands in terminal".into()],
                };
            }
            if term_top == "git" {
                return TaskTypeResult {
                    task_type: "git_management".into(),
                    confidence: 0.7,
                    signals: vec!["git commands dominating terminal".into()],
                };
            }
        }

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
        let now = now_secs();
        let since = now.saturating_sub(window_secs);
        // Recent undo rate from edit chunks (scoped lock — must drop before
        // calling self.get_recent_files below).
        let (undo_total, _chunk_count): (u64, u64) = {
            let c = self.conn.lock_or_recover();
            c.query_row(
                "SELECT COALESCE(SUM(undo_estimate), 0), COUNT(*) FROM file_edit_chunks WHERE chunk_at >= ?1",
                params![since as i64],
                |row| {
                    Ok((
                        row.get::<_, i64>(0).unwrap_or(0) as u64,
                        row.get::<_, i64>(1).unwrap_or(0) as u64,
                    ))
                },
            )
            .unwrap_or_default()
        };
        // Recent file: focus + time on it.
        let recent = self.get_recent_files(1, None);
        let current_file = recent.first().map(|f| f.file_path.clone()).unwrap_or_default();
        let focus = recent.first().and_then(|f| f.eeg_focus);
        let time_on_file = recent.first().and_then(|f| f.duration_secs).unwrap_or(0);
        // Compute velocity drop: compare last 5min vs prior 15min.
        let (recent_churn, prior_churn): (u64, u64) = {
            let c = self.conn.lock_or_recover();
            let rc = c
                .query_row(
                    "SELECT COALESCE(SUM(lines_added + lines_removed), 0) FROM file_interactions WHERE seen_at >= ?1",
                    params![(now - 300) as i64],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as u64;
            let pc = c
                .query_row(
                    "SELECT COALESCE(SUM(lines_added + lines_removed), 0) FROM file_interactions WHERE seen_at >= ?1 AND seen_at < ?2",
                    params![(now - 1200) as i64, (now - 300) as i64],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as u64;
            (rc, pc)
        };

        // Terminal failure signal: recent failed commands boost struggle score.
        let (fail_count, rerun_count): (u64, u64) = {
            let c = self.conn.lock_or_recover();
            let fails = c
                .query_row(
                    "SELECT COUNT(*) FROM terminal_commands WHERE started_at >= ?1 AND exit_code IS NOT NULL AND exit_code != 0",
                    params![since as i64],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as u64;
            // Detect repeated same command (re-runs after failure)
            let reruns = c
                .query_row(
                    "SELECT COUNT(*) FROM (
                       SELECT command, COUNT(*) as c FROM terminal_commands
                       WHERE started_at >= ?1 AND category IN ('build','test','run')
                       GROUP BY command HAVING c >= 3
                     )",
                    params![since as i64],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as u64;
            (fails, reruns)
        };

        // ── Browser signals: search refinements + revisits = stuck ──────
        let (search_refinements, browser_revisits): (u64, u64) = {
            let c = self.conn.lock_or_recover();
            let refs = c
                .query_row(
                    "SELECT COUNT(*) FROM browser_activities WHERE event_type = 'search_pattern' AND at >= ?1",
                    params![since as i64],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as u64;
            let revs = c
                .query_row(
                    "SELECT COUNT(*) FROM browser_activities WHERE event_type = 'revisit' AND at >= ?1",
                    params![since as i64],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0) as u64;
            (refs, revs)
        };

        let mins = (window_secs as f32 / 60.0).max(1.0);
        let undo_rate = undo_total as f32 / mins;
        let velocity_drop = if prior_churn > 0 {
            (1.0 - recent_churn as f32 / (prior_churn as f32 / 3.0)).clamp(0.0, 1.0) * 100.0
        } else {
            0.0
        };
        let focus_penalty = 100.0 - focus.unwrap_or(50.0);
        let time_stuck = (time_on_file as f32 / 60.0).min(30.0) / 30.0 * 100.0;
        let fail_penalty = (fail_count as f32 * 8.0).min(40.0);
        let rerun_penalty = (rerun_count as f32 * 15.0).min(30.0);
        // Browser: repeated search refinements and page revisits signal struggle
        let browser_weight = self.brain_feedback_weight("struggle_browser") as f32;
        let search_penalty = (search_refinements as f32 * 5.0).min(15.0) * browser_weight;
        let revisit_penalty = (browser_revisits as f32 * 3.0).min(10.0) * browser_weight;
        let score = (undo_rate * 15.0
            + velocity_drop * 0.25
            + focus_penalty * 0.2
            + time_stuck * 0.1
            + fail_penalty
            + rerun_penalty
            + search_penalty
            + revisit_penalty)
            .clamp(0.0, 100.0);
        let struggling = score > 60.0;
        let suggestion = if struggling {
            if search_refinements >= 4 {
                "You've refined your search query multiple times — try rephrasing the problem entirely or ask for help."
            } else if rerun_count > 0 {
                "You're re-running the same failing command — try a different approach."
            } else if fail_count >= 3 {
                "Multiple failures — step back and read the error messages carefully."
            } else if browser_revisits >= 5 {
                "You keep revisiting the same pages — the answer might not be there. Try a different source."
            } else if undo_rate > 3.0 {
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
                UNION ALL
                SELECT 'screenshot', original_path, app_name, captured_at, eeg_focus
                FROM user_screenshot_events WHERE captured_at >= ?1 AND captured_at <= ?2
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

    // ── Terminal commands ────────────────────────────────────────────────────

    /// Categorize a terminal command by its first word / pattern.
    fn categorize_command(cmd: &str) -> &'static str {
        let first = cmd.split_whitespace().next().unwrap_or("");
        let lower = first.to_lowercase();
        match lower.as_str() {
            "cargo" => {
                if cmd.contains("test") {
                    "test"
                } else if cmd.contains("build") || cmd.contains("check") {
                    "build"
                } else if cmd.contains("run") {
                    "run"
                } else if cmd.contains("add") || cmd.contains("install") {
                    "install"
                } else {
                    "build"
                }
            }
            "npm" | "npx" | "pnpm" | "yarn" | "bun" => {
                if cmd.contains("test") {
                    "test"
                } else if cmd.contains("build") {
                    "build"
                } else if cmd.contains("start") || cmd.contains("dev") {
                    "run"
                } else if cmd.contains("install") || cmd.contains("add") {
                    "install"
                } else {
                    "run"
                }
            }
            "make" | "cmake" | "ninja" | "tsc" | "webpack" | "vite" | "esbuild" | "swc" | "go"
                if cmd.contains("build") =>
            {
                "build"
            }
            "go" if cmd.contains("test") => "test",
            "go" if cmd.contains("run") => "run",
            "pytest" | "jest" | "vitest" | "mocha" | "rspec" | "phpunit" => "test",
            "python" | "node" | "deno" | "ruby" | "php" | "java" | "dotnet" => "run",
            // Version control
            "git" => "git",
            "gh" => "git",   // GitHub CLI
            "glab" => "git", // GitLab CLI
            "hg" => "git",   // Mercurial
            "svn" => "git",
            // Containers & orchestration
            "docker" | "docker-compose" | "podman" | "kubectl" | "helm" | "k9s" | "k3s" | "minikube" | "skaffold"
            | "kompose" | "kustomize" | "kind" | "containerd" | "nerdctl" | "buildah" | "cri-o" | "istioctl"
            | "linkerd" | "argocd" => "docker",
            // Cloud & deploy
            "terraform" | "ansible" | "pulumi" | "cdk" | "cdktf" | "fly" | "flyctl" | "vercel" | "netlify"
            | "railway" | "aws" | "gcloud" | "az" | "doctl" | "linode-cli" | "heroku" | "cf" | "ecs-cli" | "sam"
            | "serverless" | "wrangler" | "cloudflared" => "deploy",
            // Package managers
            "pip" | "pip3" | "pipx" | "uv" | "poetry" | "pdm" | "brew" | "cask" | "port" | "mas" | "apt"
            | "apt-get" | "dpkg" | "snap" | "flatpak" | "pacman" | "yay" | "paru" | "yum" | "dnf" | "rpm"
            | "zypper" | "apk" | "nix" | "nix-env" | "nix-shell" | "gem" | "bundle" | "composer" | "nuget" | "go"
                if cmd.contains("install") || cmd.contains("get") =>
            {
                "install"
            }
            // (cargo install is already handled in the "cargo" arm above)
            // Navigation & file ops
            "cd" | "ls" | "ll" | "la" | "find" | "grep" | "rg" | "fd" | "fzf" | "tree" | "cat" | "bat" | "less"
            | "more" | "head" | "tail" | "wc" | "pwd" | "which" | "where" | "file" | "stat" | "du" | "df" | "exa"
            | "eza" | "lsd" | "zoxide" | "z" | "j" | "autojump" | "cp" | "mv" | "rm" | "mkdir" | "rmdir" | "touch"
            | "ln" | "chmod" | "chown" | "xattr" | "open" | "pbcopy" | "pbpaste" | "sed" | "awk" | "sort" | "uniq"
            | "cut" | "tr" | "diff" | "patch" | "tar" | "zip" | "unzip" | "gzip" | "gunzip" | "xz" | "7z" => "navigate",
            // Debugging
            "gdb" | "lldb" | "dlv" | "pdb" | "ipdb" | "rr" | "strace" | "dtrace" | "valgrind" | "perf"
            | "instruments" | "leaks" | "sample" => "debug",
            // Network & HTTP
            "ssh" | "scp" | "rsync" | "curl" | "wget" | "httpie" | "http" | "https" | "nc" | "ncat" | "netcat"
            | "telnet" | "dig" | "nslookup" | "host" | "ping" | "traceroute" | "mtr" | "nmap" | "tcpdump"
            | "wireshark" | "ngrok" | "localtunnel" => "network",
            // Databases
            "psql" | "pg_dump" | "pg_restore" | "createdb" | "dropdb" | "mysql" | "mysqldump" | "mycli" | "sqlite3"
            | "litecli" | "redis-cli" | "redis-server" | "mongosh" | "mongo" | "mongodump" | "mongorestore"
            | "cqlsh" | "influx" | "clickhouse-client" => "database",
            // AI & ML tools
            "hf" | "huggingface-cli" => "ai",
            "ollama" => "ai",
            "claude" | "aider" | "cody" | "copilot" => "ai",
            "pi" => "ai",
            "jupyter" | "ipython" => "ai",
            "mlflow" | "wandb" | "dvc" | "bentoml" => "ai",
            // Editors & tools
            "vim" | "nvim" | "vi" | "nano" | "emacs" | "code" | "subl" | "micro" => "editor",
            "tmux" | "screen" | "zellij" | "byobu" => "multiplexer",
            "htop" | "btop" | "top" | "glances" | "nmon" | "ps" | "kill" | "killall" | "lsof" | "ss" | "netstat"
            | "iostat" | "vmstat" | "free" | "uptime" | "w" | "who" => "monitor",
            // Environment & config
            "env" | "export" | "set" | "unset" | "source" | "alias" | "echo" | "printf" | "eval" | "exec" | "xargs"
            | "direnv" | "asdf" | "mise" | "rtx" | "fnm" | "nvm" | "rbenv" | "pyenv" | "rustup" | "swiftenv"
            | "sdkman" => "env",
            // Build systems
            "mvn" | "gradle" | "ant" | "sbt" | "bazel" | "buck" | "pants" | "meson" | "autoconf" | "automake"
            | "configure" => {
                if cmd.contains("test") {
                    "test"
                } else {
                    "build"
                }
            }
            // System management
            "systemctl" | "service" | "launchctl" | "journalctl" | "crontab" | "at" | "watch" => "system",
            _ => "other",
        }
    }

    /// Insert a terminal command start event. Returns the row id.
    pub fn insert_terminal_command_start(
        &self,
        terminal_name: &str,
        command: &str,
        cwd: &str,
        started_at: u64,
        eeg_focus: Option<f64>,
        eeg_mood: Option<f64>,
    ) -> i64 {
        let category = Self::categorize_command(command);
        // Extract binary name (first word) and args (rest)
        let mut parts = command.splitn(2, char::is_whitespace);
        let binary = parts.next().unwrap_or("").trim();
        let args = parts.next().unwrap_or("").trim();
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO terminal_commands (terminal_name, command, binary, args, cwd, started_at, category, eeg_focus, eeg_mood)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                terminal_name,
                command,
                binary,
                args,
                cwd,
                started_at as i64,
                category,
                eeg_focus,
                eeg_mood
            ],
        );
        c.last_insert_rowid()
    }

    /// Update a terminal command with its end time and exit code.
    pub fn update_terminal_command_end(
        &self,
        command: &str,
        terminal_name: &str,
        exit_code: Option<i64>,
        ended_at: u64,
        eeg_focus_end: Option<f64>,
    ) {
        let c = self.conn.lock_or_recover();
        // Find the most recent matching command that hasn't ended yet
        let _ = c.execute(
            "UPDATE terminal_commands SET exit_code = ?1, ended_at = ?2,
             duration_secs = ?2 - started_at, eeg_focus_end = ?3
             WHERE id = (SELECT id FROM terminal_commands
                         WHERE command = ?4 AND terminal_name = ?5 AND ended_at IS NULL
                         ORDER BY started_at DESC LIMIT 1)",
            params![exit_code, ended_at as i64, eeg_focus_end, command, terminal_name],
        );
    }

    /// Insert a zone switch event.
    pub fn insert_zone_switch(&self, zone: &str, from_zone: &str, at: u64, eeg_focus: Option<f64>) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO zone_switches (zone, from_zone, at, eeg_focus) VALUES (?1, ?2, ?3, ?4)",
            params![zone, from_zone, at as i64, eeg_focus],
        );
    }

    /// Insert a layout snapshot.
    pub fn insert_layout_snapshot(&self, at: u64, groups: i64, visible: i64, tabs: i64, terminals: i64) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO layout_snapshots (sampled_at, editor_groups, visible_editors, open_tabs, terminals)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![at as i64, groups, visible, tabs, terminals],
        );
    }

    /// Record a browser extension activity event from raw JSON.
    /// Extracts all known fields from the event object — new fields added to
    /// the extension are automatically captured without daemon code changes.
    pub fn insert_browser_activity_json(
        &self,
        event: &serde_json::Value,
        at: u64,
        eeg_focus: Option<f64>,
        eeg_mood: Option<f64>,
    ) {
        let s = |k: &str| event.get(k).and_then(|v| v.as_str()).unwrap_or("");
        let i = |k: &str| event.get(k).and_then(|v| v.as_i64());
        let f = |k: &str| event.get(k).and_then(|v| v.as_f64());
        let b = |k: &str| event.get(k).and_then(|v| v.as_bool()).unwrap_or(false);

        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO browser_activities
                (event_type, url, domain, title, tab_id, browser_name,
                 scroll_depth, reading_time_secs, active_time_secs, idle_time_secs,
                 typing_detected, media_playing,
                 search_query, tab_count, devtools_open,
                 category, content_type, referrer_domain, nav_type,
                 click_target, click_count, mouse_distance, mouse_idle_secs,
                 has_video, has_audio, image_count, word_count, form_count,
                 video_watched_secs, video_playback_rate, copy_length, paste_length,
                 scroll_speed, scroll_direction, scroll_reversals,
                 llm_provider, llm_turn_count,
                 email_mode, email_count,
                 revisit_count, domain_visit_count,
                 visible_text, heading, page_title, download_type,
                 at, eeg_focus, eeg_mood)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36,?37,?38,?39,?40,?41,?42,?43,?44,?45,?46,?47,?48)",
            params![
                s("type"),
                s("url"),
                s("domain"),
                s("title"),
                i("tab_id"),
                s("browser_name"),
                f("scroll_depth"),
                i("reading_time_secs"),
                i("active_time_secs"),
                i("idle_time_secs"),
                b("typing_detected") as i32,
                b("media_playing") as i32,
                s("search_query"),
                i("tab_count"),
                b("devtools_open") as i32,
                s("category"),
                s("content_type"),
                s("referrer_domain"),
                s("nav_type"),
                s("click_target"),
                i("clicks_per_min"),
                i("mouse_distance"),
                i("mouse_idle_secs"),
                b("has_video") as i32,
                b("has_audio") as i32,
                i("image_count"),
                i("word_count"),
                i("form_count"),
                i("video_watched_secs"),
                f("video_playback_rate"),
                i("copy_length"),
                i("paste_length"),
                i("scroll_speed"),
                s("scroll_direction"),
                i("scroll_reversals"),
                s("llm_provider"),
                i("llm_turn_count"),
                s("email_mode"),
                i("email_count"),
                i("revisit_count"),
                i("domain_visit_count"),
                s("visible_text"),
                s("heading"),
                s("page_title"),
                s("download_type"),
                at as i64,
                eeg_focus,
                eeg_mood,
            ],
        );
    }

    /// Query recent browser activities.
    pub fn get_recent_browser_activities(&self, limit: u32, since: u64) -> Vec<BrowserActivityRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, event_type, url, domain, title, tab_id, browser_name,
                    scroll_depth, reading_time_secs, typing_detected, media_playing,
                    search_query, tab_count, devtools_open, category, referrer_domain,
                    at, eeg_focus, eeg_mood
             FROM browser_activities WHERE at >= ?1 ORDER BY at DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64, limit], |row| {
            Ok(BrowserActivityRow {
                id: row.get(0)?,
                event_type: row.get(1)?,
                url: row.get(2)?,
                domain: row.get(3)?,
                title: row.get(4)?,
                tab_id: row.get(5)?,
                browser_name: row.get(6)?,
                scroll_depth: row.get(7)?,
                reading_time_secs: row.get(8)?,
                typing_detected: row.get::<_, i32>(9)? != 0,
                media_playing: row.get::<_, i32>(10)? != 0,
                search_query: row.get(11)?,
                tab_count: row.get(12)?,
                devtools_open: row.get::<_, i32>(13)? != 0,
                category: row.get(14)?,
                referrer_domain: row.get(15)?,
                at: row.get::<_, i64>(16)? as u64,
                eeg_focus: row.get(17)?,
                eeg_mood: row.get(18)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Top domains by event count in the given time window.
    pub fn browser_domain_breakdown(&self, since: u64) -> Vec<(String, String, u64)> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT domain, category, COUNT(*) as cnt
             FROM browser_activities
             WHERE at >= ?1 AND domain != ''
             GROUP BY domain, category
             ORDER BY cnt DESC
             LIMIT 50",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u64,
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Tab switch rate: number of tab_switch events per minute in the window.
    pub fn browser_context_switch_rate(&self, start: u64, end: u64) -> f64 {
        let c = self.conn.lock_or_recover();
        let count: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities
                 WHERE event_type = 'tab_switch' AND at >= ?1 AND at <= ?2",
                params![start as i64, end as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let minutes = (end.saturating_sub(start)) as f64 / 60.0;
        if minutes > 0.0 {
            count as f64 / minutes
        } else {
            0.0
        }
    }

    // ── Browser-specific brain analytics ────────────────────────────────────

    /// Focus correlation by domain — which websites correspond to high vs low focus.
    pub fn browser_focus_by_domain(&self, since: u64, limit: u32) -> Vec<BrowserDomainFocusRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT domain, category, content_type,
                    COUNT(*) as events,
                    AVG(eeg_focus) as avg_focus,
                    SUM(CASE WHEN reading_time_secs > 0 THEN reading_time_secs ELSE 0 END) as total_reading_secs,
                    AVG(CASE WHEN scroll_depth > 0 THEN scroll_depth ELSE NULL END) as avg_scroll_depth,
                    SUM(CASE WHEN event_type = 'tab_switch' THEN 1 ELSE 0 END) as visits
             FROM browser_activities
             WHERE at >= ?1 AND domain != '' AND eeg_focus IS NOT NULL
             GROUP BY domain
             HAVING events >= 3
             ORDER BY avg_focus DESC
             LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64, limit], |row| {
            Ok(BrowserDomainFocusRow {
                domain: row.get(0)?,
                category: row.get(1)?,
                content_type: row.get(2)?,
                events: row.get::<_, i64>(3)? as u64,
                avg_focus: row.get(4)?,
                total_reading_secs: row.get::<_, i64>(5)? as u64,
                avg_scroll_depth: row.get(6)?,
                visits: row.get::<_, i64>(7)? as u64,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Distraction score — measures tab-switching frequency, social/media time,
    /// and idle browsing relative to productive browsing.
    pub fn browser_distraction_score(&self, window_secs: u64) -> BrowserDistractionScore {
        let now = now_secs();
        let since = now.saturating_sub(window_secs);
        let c = self.conn.lock_or_recover();

        let tab_switches: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities WHERE event_type = 'tab_switch' AND at >= ?1",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let social_events: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities WHERE category IN ('social', 'media') AND at >= ?1",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let productive_events: i64 = c.query_row(
            "SELECT COUNT(*) FROM browser_activities WHERE category IN ('development', 'reference', 'code') AND at >= ?1",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(0);

        let idle_secs: i64 = c.query_row(
            "SELECT COALESCE(SUM(idle_time_secs), 0) FROM browser_activities WHERE at >= ?1 AND idle_time_secs IS NOT NULL",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(0);

        let total_events = tab_switches + social_events + productive_events;
        let minutes = window_secs as f64 / 60.0;
        let switches_per_min = if minutes > 0.0 {
            tab_switches as f64 / minutes
        } else {
            0.0
        };

        // Score 0-100: higher = more distracted
        let switch_score = (switches_per_min * 10.0).min(40.0); // max 40 from switching
        let social_ratio = if total_events > 0 {
            social_events as f64 / total_events as f64
        } else {
            0.0
        };
        let social_score = social_ratio * 40.0; // max 40 from social
        let idle_score = ((idle_secs as f64 / window_secs as f64) * 20.0).min(20.0); // max 20 from idle

        let score = (switch_score + social_score + idle_score).min(100.0);

        BrowserDistractionScore {
            score,
            tab_switches_per_min: (switches_per_min * 10.0).round() / 10.0,
            social_pct: (social_ratio * 100.0).round(),
            productive_pct: if total_events > 0 {
                ((productive_events as f64 / total_events as f64) * 100.0).round()
            } else {
                0.0
            },
            idle_pct: ((idle_secs as f64 / window_secs as f64) * 100.0).round(),
            suggestion: if score > 70.0 {
                "High distraction. Close social media tabs and focus on one task.".to_string()
            } else if score > 40.0 {
                "Moderate distraction. Consider batching tab switches.".to_string()
            } else {
                "Focused browsing. Keep it up.".to_string()
            },
        }
    }

    /// Content type breakdown — how time is split across video/paper/social/code/etc.
    pub fn browser_content_breakdown(&self, since: u64) -> Vec<BrowserContentBreakdownRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT content_type,
                    COUNT(*) as events,
                    AVG(eeg_focus) as avg_focus,
                    SUM(CASE WHEN reading_time_secs > 0 THEN reading_time_secs ELSE 0 END) as total_secs
             FROM browser_activities
             WHERE at >= ?1 AND content_type != ''
             GROUP BY content_type
             ORDER BY total_secs DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64], |row| {
            Ok(BrowserContentBreakdownRow {
                content_type: row.get(0)?,
                events: row.get::<_, i64>(1)? as u64,
                avg_focus: row.get(2)?,
                total_secs: row.get::<_, i64>(3)? as u64,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// LLM usage from browser — tracks interactions with ChatGPT, Claude, Gemini, etc.
    pub fn browser_llm_usage(&self, since: u64) -> Vec<BrowserLlmUsageRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT domain, content_type,
                    COUNT(*) as interactions,
                    AVG(eeg_focus) as avg_focus,
                    MAX(CAST(json_extract(title, '$.llm_turn_count') AS INTEGER)) as max_turns
             FROM browser_activities
             WHERE at >= ?1 AND event_type = 'llm_interaction'
             GROUP BY domain
             ORDER BY interactions DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64], |row| {
            Ok(BrowserLlmUsageRow {
                domain: row.get(0)?,
                provider: row.get::<_, String>(1).unwrap_or_default(),
                interactions: row.get::<_, i64>(2)? as u64,
                avg_focus: row.get(3)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Research patterns — search frequency, refinement rate, stuck detection.
    pub fn browser_research_patterns(&self, since: u64) -> BrowserResearchPatterns {
        let c = self.conn.lock_or_recover();

        let total_searches: i64 = c.query_row(
            "SELECT COUNT(*) FROM browser_activities WHERE event_type IN ('search_query', 'search_pattern') AND at >= ?1",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(0);

        let refinements: i64 = c.query_row(
            "SELECT COUNT(*) FROM browser_activities WHERE event_type = 'search_pattern' AND search_query != '' AND at >= ?1",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(0);

        let revisits: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities WHERE event_type = 'revisit' AND at >= ?1",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let avg_focus: Option<f64> = c.query_row(
            "SELECT AVG(eeg_focus) FROM browser_activities WHERE event_type IN ('search_query', 'search_pattern') AND at >= ?1 AND eeg_focus IS NOT NULL",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(None);

        let refinement_rate = if total_searches > 0 {
            refinements as f64 / total_searches as f64
        } else {
            0.0
        };

        BrowserResearchPatterns {
            total_searches: total_searches as u64,
            refinement_rate: (refinement_rate * 100.0).round(),
            revisit_count: revisits as u64,
            avg_search_focus: avg_focus,
            stuck_indicator: refinement_rate > 0.5 && revisits > 3,
        }
    }

    // ── Advanced browser-brain insights ────────────────────────────────────

    /// Learning efficiency: deep scroll + high focus + low revisits = retaining.
    pub fn browser_learning_efficiency(&self, since: u64) -> Vec<BrowserLearningRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT domain, content_type,
                    AVG(eeg_focus) as avg_focus,
                    AVG(CASE WHEN scroll_depth > 0 THEN scroll_depth ELSE NULL END) as avg_depth,
                    AVG(CASE WHEN reading_time_secs > 0 THEN reading_time_secs ELSE NULL END) as avg_read_secs,
                    COUNT(DISTINCT url) as pages,
                    SUM(CASE WHEN event_type = 'revisit' THEN 1 ELSE 0 END) as revisits
             FROM browser_activities
             WHERE at >= ?1 AND domain != '' AND eeg_focus IS NOT NULL
                   AND event_type IN ('reading_time', 'page_profile', 'revisit')
             GROUP BY domain
             HAVING pages >= 2
             ORDER BY avg_focus DESC
             LIMIT 20",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64], |row| {
            let focus: f64 = row.get::<_, f64>(2).unwrap_or(0.0);
            let depth: f64 = row.get::<_, f64>(3).unwrap_or(0.0);
            let read_secs: f64 = row.get::<_, f64>(4).unwrap_or(0.0);
            let revisits: i64 = row.get(6)?;
            // Score: high focus + deep scroll + long read + low revisits = learning
            let score = (focus * 0.4 + depth * 100.0 * 0.3 + (read_secs / 60.0).min(10.0) * 3.0)
                * (1.0 - (revisits as f64 * 0.1).min(0.5));
            Ok(BrowserLearningRow {
                domain: row.get(0)?,
                content_type: row.get(1)?,
                avg_focus: Some(focus),
                avg_scroll_depth: Some(depth),
                avg_reading_secs: read_secs as u64,
                pages: row.get::<_, i64>(5)? as u64,
                revisits: revisits as u64,
                efficiency_score: (score.clamp(0.0, 100.0) * 10.0).round() / 10.0,
            })
        })
        .map(|r| r.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Optimal research window: which hours produce highest reading focus.
    pub fn browser_optimal_research_hours(&self, since: u64, tz_offset: i32) -> BrowserOptimalHours {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT ((at + ?2) % 86400) / 3600 as hour,
                    AVG(eeg_focus) as avg_focus,
                    COUNT(*) as events,
                    SUM(CASE WHEN reading_time_secs > 0 THEN reading_time_secs ELSE 0 END) as total_read
             FROM browser_activities
             WHERE at >= ?1 AND eeg_focus IS NOT NULL AND domain != ''
             GROUP BY hour
             ORDER BY avg_focus DESC",
        ) {
            Ok(s) => s,
            Err(_) => return BrowserOptimalHours::default(),
        };
        let rows: Vec<(u32, f64, u64, u64)> = stmt
            .query_map(params![since as i64, tz_offset as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)? as u32,
                    row.get(1)?,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, i64>(3)? as u64,
                ))
            })
            .map(|r| r.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        if rows.is_empty() {
            return BrowserOptimalHours::default();
        }
        let best: Vec<u32> = rows.iter().take(3).map(|r| r.0).collect();
        let worst: Vec<u32> = rows.iter().rev().take(3).map(|r| r.0).collect();
        BrowserOptimalHours {
            best_hours: best,
            worst_hours: worst,
            by_hour: rows
                .iter()
                .map(|(h, f, e, r)| BrowserHourRow {
                    hour: *h,
                    avg_focus: *f,
                    events: *e,
                    reading_secs: *r,
                })
                .collect(),
        }
    }

    /// AI chat effectiveness: compare focus/turns across LLM providers.
    pub fn browser_ai_effectiveness(&self, since: u64) -> Vec<BrowserAiEffectivenessRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT domain,
                    COUNT(*) as interactions,
                    AVG(eeg_focus) as avg_focus,
                    AVG(CASE WHEN reading_time_secs > 0 THEN reading_time_secs ELSE NULL END) as avg_session_secs
             FROM browser_activities
             WHERE at >= ?1 AND event_type = 'llm_interaction'
             GROUP BY domain
             ORDER BY avg_focus DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64], |row| {
            Ok(BrowserAiEffectivenessRow {
                provider: row.get(0)?,
                interactions: row.get::<_, i64>(1)? as u64,
                avg_focus: row.get(2)?,
                avg_session_secs: row.get::<_, f64>(3).ok(),
            })
        })
        .map(|r| r.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Procrastination detection: idle + low focus + same-site cycling.
    pub fn browser_procrastination_check(&self, window_secs: u64) -> BrowserProcrastination {
        let now = now_secs();
        let since = now.saturating_sub(window_secs);
        let c = self.conn.lock_or_recover();

        let idle: i64 = c.query_row(
            "SELECT COALESCE(SUM(idle_time_secs), 0) FROM browser_activities WHERE at >= ?1 AND idle_time_secs IS NOT NULL",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(0);

        let mouse_idle: i64 = c.query_row(
            "SELECT COALESCE(MAX(mouse_idle_secs), 0) FROM browser_activities WHERE at >= ?1 AND mouse_idle_secs IS NOT NULL",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(0);

        let revisits: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities WHERE event_type = 'revisit' AND at >= ?1",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let avg_focus: f64 = c
            .query_row(
                "SELECT COALESCE(AVG(eeg_focus), 50) FROM browser_activities WHERE at >= ?1 AND eeg_focus IS NOT NULL",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(50.0);

        let idle_ratio = idle as f64 / window_secs.max(1) as f64;
        let score =
            ((1.0 - avg_focus / 100.0) * 40.0 + idle_ratio * 30.0 + (revisits as f64 * 3.0).min(30.0)).min(100.0);

        BrowserProcrastination {
            score,
            idle_secs: idle as u64,
            max_mouse_idle_secs: mouse_idle as u64,
            revisit_loops: revisits as u64,
            avg_focus: Some(avg_focus),
            procrastinating: score > 60.0,
            suggestion: if score > 70.0 {
                "You're avoiding something. Commit to just 5 minutes of focused work.".into()
            } else if score > 40.0 {
                "Mild avoidance detected. Try breaking the task into smaller pieces.".into()
            } else {
                "On track.".into()
            },
        }
    }

    /// Deep reading sessions: periods of sustained, high-focus reading.
    pub fn browser_deep_reading_sessions(&self, since: u64) -> Vec<BrowserDeepReadingRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT domain, title,
                    reading_time_secs, scroll_depth, eeg_focus, at
             FROM browser_activities
             WHERE at >= ?1 AND event_type = 'reading_time'
                   AND reading_time_secs >= 300 AND scroll_depth >= 0.6
             ORDER BY eeg_focus DESC NULLS LAST
             LIMIT 20",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64], |row| {
            Ok(BrowserDeepReadingRow {
                domain: row.get(0)?,
                title: row.get(1)?,
                reading_secs: row.get::<_, i64>(2)? as u64,
                scroll_depth: row.get(3)?,
                eeg_focus: row.get(4)?,
                at: row.get::<_, i64>(5)? as u64,
            })
        })
        .map(|r| r.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Video learning ROI: focus during video vs total video time.
    pub fn browser_video_roi(&self, since: u64) -> BrowserVideoRoi {
        let c = self.conn.lock_or_recover();
        let total_watched: i64 = c.query_row(
            "SELECT COALESCE(SUM(video_watched_secs), 0) FROM browser_activities WHERE at >= ?1 AND video_watched_secs > 0",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(0);

        let focused_watched: i64 = c.query_row(
            "SELECT COALESCE(SUM(video_watched_secs), 0) FROM browser_activities WHERE at >= ?1 AND video_watched_secs > 0 AND eeg_focus > 60",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(0);

        let avg_focus: Option<f64> = c.query_row(
            "SELECT AVG(eeg_focus) FROM browser_activities WHERE at >= ?1 AND has_video = 1 AND eeg_focus IS NOT NULL",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(None);

        BrowserVideoRoi {
            total_watched_secs: total_watched as u64,
            focused_watched_secs: focused_watched as u64,
            focus_ratio: if total_watched > 0 {
                focused_watched as f64 / total_watched as f64
            } else {
                0.0
            },
            avg_focus,
        }
    }

    /// Email anxiety: focus/stress changes around email activity.
    pub fn browser_email_impact(&self, since: u64) -> BrowserEmailImpact {
        let c = self.conn.lock_or_recover();
        let email_focus: Option<f64> = c.query_row(
            "SELECT AVG(eeg_focus) FROM browser_activities WHERE at >= ?1 AND event_type = 'email_activity' AND eeg_focus IS NOT NULL",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(None);
        let non_email_focus: Option<f64> = c.query_row(
            "SELECT AVG(eeg_focus) FROM browser_activities WHERE at >= ?1 AND event_type != 'email_activity' AND eeg_focus IS NOT NULL AND domain != ''",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(None);
        let email_events: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities WHERE at >= ?1 AND event_type = 'email_activity'",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let focus_delta = match (email_focus, non_email_focus) {
            (Some(e), Some(n)) => Some(e - n),
            _ => None,
        };

        BrowserEmailImpact {
            email_sessions: email_events as u64,
            avg_focus_during_email: email_focus,
            avg_focus_outside_email: non_email_focus,
            focus_delta,
        }
    }

    /// Tab count vs cognitive load correlation.
    pub fn browser_tab_cognitive_load(&self, since: u64) -> Vec<BrowserTabLoadRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT tab_count,
                    AVG(eeg_focus) as avg_focus,
                    COUNT(*) as samples
             FROM browser_activities
             WHERE at >= ?1 AND tab_count IS NOT NULL AND eeg_focus IS NOT NULL
             GROUP BY tab_count
             ORDER BY tab_count",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64], |row| {
            Ok(BrowserTabLoadRow {
                tab_count: row.get::<_, i64>(0)? as u32,
                avg_focus: row.get(1)?,
                samples: row.get::<_, i64>(2)? as u64,
            })
        })
        .map(|r| r.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Weekend vs weekday brain: compare focus patterns.
    pub fn browser_weekday_vs_weekend(&self, since: u64) -> BrowserWeekdayComparison {
        let c = self.conn.lock_or_recover();
        // SQLite: strftime('%w', ts, 'unixepoch') gives 0=Sun, 6=Sat
        let weekday_focus: Option<f64> = c
            .query_row(
                "SELECT AVG(eeg_focus) FROM browser_activities
             WHERE at >= ?1 AND eeg_focus IS NOT NULL
                   AND CAST(strftime('%w', at, 'unixepoch') AS INTEGER) BETWEEN 1 AND 5",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(None);
        let weekend_focus: Option<f64> = c
            .query_row(
                "SELECT AVG(eeg_focus) FROM browser_activities
             WHERE at >= ?1 AND eeg_focus IS NOT NULL
                   AND CAST(strftime('%w', at, 'unixepoch') AS INTEGER) IN (0, 6)",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(None);
        let weekday_events: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities
             WHERE at >= ?1 AND CAST(strftime('%w', at, 'unixepoch') AS INTEGER) BETWEEN 1 AND 5",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let weekend_events: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities
             WHERE at >= ?1 AND CAST(strftime('%w', at, 'unixepoch') AS INTEGER) IN (0, 6)",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);

        BrowserWeekdayComparison {
            weekday_avg_focus: weekday_focus,
            weekend_avg_focus: weekend_focus,
            weekday_events: weekday_events as u64,
            weekend_events: weekend_events as u64,
            delta: match (weekday_focus, weekend_focus) {
                (Some(w), Some(e)) => Some(w - e),
                _ => None,
            },
        }
    }

    /// Night owl penalty: focus by hour bucket, detecting late-night degradation.
    pub fn browser_night_owl_analysis(&self, since: u64, tz_offset: i32) -> Vec<BrowserHourFocusRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT ((at + ?2) % 86400) / 3600 as hour,
                    AVG(eeg_focus) as avg_focus,
                    COUNT(*) as events,
                    AVG(CASE WHEN scroll_depth > 0 THEN scroll_depth ELSE NULL END) as avg_depth
             FROM browser_activities
             WHERE at >= ?1 AND eeg_focus IS NOT NULL
             GROUP BY hour
             ORDER BY hour",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64, tz_offset as i64], |row| {
            Ok(BrowserHourFocusRow {
                hour: row.get::<_, i64>(0)? as u32,
                avg_focus: row.get(1)?,
                events: row.get::<_, i64>(2)? as u64,
                avg_scroll_depth: row.get(3)?,
            })
        })
        .map(|r| r.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Copy-paste workflow: frequency and patterns of clipboard usage by domain.
    pub fn browser_copypaste_patterns(&self, since: u64) -> BrowserCopyPastePatterns {
        let c = self.conn.lock_or_recover();
        let copies: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities WHERE event_type = 'clipboard_copy' AND at >= ?1",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let pastes: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM browser_activities WHERE event_type = 'clipboard_paste' AND at >= ?1",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let avg_paste_len: Option<f64> = c.query_row(
            "SELECT AVG(paste_length) FROM browser_activities WHERE event_type = 'clipboard_paste' AND at >= ?1 AND paste_length IS NOT NULL",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(None);

        // Top domains for copy activity
        let mut stmt = match c.prepare_cached(
            "SELECT domain, COUNT(*) as cnt FROM browser_activities
             WHERE event_type IN ('clipboard_copy', 'clipboard_paste') AND at >= ?1 AND domain != ''
             GROUP BY domain ORDER BY cnt DESC LIMIT 5",
        ) {
            Ok(s) => s,
            Err(_) => {
                return BrowserCopyPastePatterns {
                    copies: copies as u64,
                    pastes: pastes as u64,
                    avg_paste_length: avg_paste_len,
                    top_domains: vec![],
                }
            }
        };
        let top: Vec<(String, u64)> = stmt
            .query_map(params![since as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })
            .map(|r| r.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        BrowserCopyPastePatterns {
            copies: copies as u64,
            pastes: pastes as u64,
            avg_paste_length: avg_paste_len,
            top_domains: top,
        }
    }

    /// Meeting → browser spiral detection: unfocused browsing after meetings.
    pub fn browser_post_meeting_spiral(&self, since: u64) -> Vec<BrowserPostMeetingRow> {
        let c = self.conn.lock_or_recover();
        // Find meetings, then check browser focus in the 30 min after each
        let mut stmt = match c.prepare_cached(
            "SELECT m.id, m.title, m.platform, m.end_at,
                    (SELECT AVG(b.eeg_focus) FROM browser_activities b
                     WHERE b.at > m.end_at AND b.at <= m.end_at + 1800 AND b.eeg_focus IS NOT NULL) as post_focus,
                    (SELECT COUNT(*) FROM browser_activities b
                     WHERE b.at > m.end_at AND b.at <= m.end_at + 1800 AND b.category IN ('social', 'media', 'news')) as distraction_events
             FROM meeting_events m
             WHERE m.end_at IS NOT NULL AND m.end_at >= ?1
             ORDER BY m.end_at DESC
             LIMIT 10",
        ) { Ok(s) => s, Err(_) => return vec![] };
        stmt.query_map(params![since as i64], |row| {
            Ok(BrowserPostMeetingRow {
                meeting_title: row.get(1)?,
                platform: row.get(2)?,
                ended_at: row.get::<_, i64>(3)? as u64,
                post_meeting_focus: row.get(4)?,
                distraction_events: row.get::<_, i64>(5)? as u64,
            })
        })
        .map(|r| r.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Context switch tax: time to recover focus after switching from browser to code.
    pub fn browser_switch_tax(&self, since: u64) -> BrowserSwitchTax {
        let c = self.conn.lock_or_recover();
        // Count browser→code switches via zone_switches
        let switches: i64 = c.query_row(
            "SELECT COUNT(*) FROM zone_switches WHERE at >= ?1 AND (zone LIKE 'browser%' OR from_zone LIKE 'browser%')",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(0);
        let avg_focus_at_switch: Option<f64> = c.query_row(
            "SELECT AVG(eeg_focus) FROM zone_switches WHERE at >= ?1 AND (zone LIKE 'browser%' OR from_zone LIKE 'browser%') AND eeg_focus IS NOT NULL",
            params![since as i64], |row| row.get(0),
        ).unwrap_or(None);

        BrowserSwitchTax {
            total_switches: switches as u64,
            avg_focus_at_switch,
            estimated_lost_minutes: (switches as f64 * 4.5).round() as u64, // ~4.5 min recovery per switch
        }
    }

    // ── Brain feedback system ─────────────────────────────────────────────

    /// Record user feedback on a brain insight (yay/nay).
    pub fn insert_brain_feedback(
        &self,
        insight: &str,
        correct: bool,
        score: Option<f64>,
        eeg_focus: Option<f64>,
        eeg_mood: Option<f64>,
        context: &str,
    ) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO brain_feedback (insight, correct, score, eeg_focus, eeg_mood, context, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                insight,
                correct as i32,
                score,
                eeg_focus,
                eeg_mood,
                context,
                now_secs() as i64
            ],
        );
    }

    /// Get accuracy stats per insight type — used to adjust scoring weights.
    pub fn brain_feedback_accuracy(&self) -> Vec<BrainFeedbackAccuracy> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT insight,
                    COUNT(*) as total,
                    SUM(CASE WHEN correct = 1 THEN 1 ELSE 0 END) as correct_count,
                    AVG(score) as avg_score,
                    AVG(eeg_focus) as avg_focus_when_correct
             FROM brain_feedback
             GROUP BY insight
             ORDER BY total DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], |row| {
            let total: i64 = row.get(1)?;
            let correct: i64 = row.get(2)?;
            Ok(BrainFeedbackAccuracy {
                insight: row.get(0)?,
                total: total as u64,
                correct: correct as u64,
                accuracy: if total > 0 { correct as f64 / total as f64 } else { 0.0 },
                avg_score: row.get(3)?,
                avg_focus_when_correct: row.get(4)?,
            })
        })
        .map(|r| r.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Get weight adjustment factor for a specific insight based on feedback.
    /// Returns 1.0 if no feedback, scales 0.5-1.5 based on accuracy.
    /// When accuracy is low, the model should reduce confidence in that signal.
    pub fn brain_feedback_weight(&self, insight: &str) -> f64 {
        let c = self.conn.lock_or_recover();
        let result: Option<(i64, i64)> = c
            .query_row(
                "SELECT COUNT(*), SUM(CASE WHEN correct = 1 THEN 1 ELSE 0 END)
             FROM brain_feedback WHERE insight = ?1",
                params![insight],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        match result {
            Some((total, correct)) if total >= 5 => {
                let accuracy = correct as f64 / total as f64;
                // Scale: 50% accuracy → 0.5 weight, 100% → 1.5 weight
                0.5 + accuracy
            }
            _ => 1.0, // Not enough data yet
        }
    }

    /// Recent feedback entries for display.
    pub fn brain_feedback_recent(&self, limit: u32) -> Vec<BrainFeedbackRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, insight, correct, score, eeg_focus, context, at
             FROM brain_feedback ORDER BY at DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![limit], |row| {
            Ok(BrainFeedbackRow {
                id: row.get(0)?,
                insight: row.get(1)?,
                correct: row.get::<_, i32>(2)? != 0,
                score: row.get(3)?,
                eeg_focus: row.get(4)?,
                context: row.get(5)?,
                at: row.get::<_, i64>(6)? as u64,
            })
        })
        .map(|r| r.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Detect and store dev loops (edit → build/test → result cycles).
    /// Called periodically by a background worker.
    pub fn detect_dev_loops(&self, window_secs: u64) -> Vec<DevLoopRow> {
        let now = now_secs();
        let since = now.saturating_sub(window_secs);
        let c = self.conn.lock_or_recover();

        // Find build/test commands in the window, ordered chronologically.
        let mut stmt = match c.prepare_cached(
            "SELECT id, command, category, exit_code, started_at, ended_at, eeg_focus, eeg_focus_end
             FROM terminal_commands
             WHERE started_at >= ?1 AND category IN ('build', 'test', 'run')
             ORDER BY started_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        struct Cmd {
            command: String,
            category: String,
            exit_code: Option<i64>,
            started_at: u64,
            focus: Option<f64>,
            focus_end: Option<f64>,
        }

        let cmds: Vec<Cmd> = stmt
            .query_map(params![since as i64], |row| {
                Ok(Cmd {
                    command: row.get(1)?,
                    category: row.get(2)?,
                    exit_code: row.get(3)?,
                    started_at: row.get::<_, i64>(4)? as u64,
                    focus: row.get(6)?,
                    focus_end: row.get(7)?,
                })
            })
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();

        if cmds.is_empty() {
            return vec![];
        }

        // Group consecutive same-command runs into loops.
        let mut loops: Vec<DevLoopRow> = vec![];
        let mut i = 0;
        while i < cmds.len() {
            let base = &cmds[i].command;
            let cat = &cmds[i].category;
            let loop_start = cmds[i].started_at;
            let mut iterations = 0u32;
            let mut passes = 0u32;
            let mut fails = 0u32;
            let mut focus_sum = 0.0f64;
            let mut focus_count = 0u32;
            let first_focus = cmds[i].focus;
            let mut last_focus = cmds[i].focus_end.or(cmds[i].focus);
            let mut loop_end = cmds[i].started_at;

            while i < cmds.len() && cmds[i].command == *base {
                iterations += 1;
                match cmds[i].exit_code {
                    Some(0) => passes += 1,
                    Some(_) => fails += 1,
                    None => {}
                }
                if let Some(f) = cmds[i].focus {
                    focus_sum += f;
                    focus_count += 1;
                }
                last_focus = cmds[i].focus_end.or(cmds[i].focus).or(last_focus);
                loop_end = cmds[i].started_at;
                i += 1;
            }

            if iterations >= 2 {
                let duration = loop_end.saturating_sub(loop_start);
                let avg_cycle = if iterations > 1 {
                    duration as f64 / (iterations - 1) as f64
                } else {
                    0.0
                };
                let avg_focus = if focus_count > 0 {
                    Some(focus_sum / focus_count as f64)
                } else {
                    None
                };
                let trend = match (first_focus, last_focus) {
                    (Some(f), Some(l)) if l > f + 5.0 => "rising",
                    (Some(f), Some(l)) if l < f - 5.0 => "falling",
                    _ => "stable",
                };
                loops.push(DevLoopRow {
                    loop_type: format!("edit-{cat}"),
                    command: base.clone(),
                    iterations,
                    passes,
                    fails,
                    started_at: loop_start,
                    ended_at: loop_end,
                    avg_cycle_secs: avg_cycle,
                    avg_focus,
                    focus_trend: trend.to_string(),
                });
            }
        }
        loops
    }

    /// Get recent terminal commands.
    pub fn get_recent_terminal_commands(&self, limit: u32, since: u64) -> Vec<TerminalCommandRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, terminal_name, command, cwd, exit_code, started_at, ended_at,
                    duration_secs, category, eeg_focus, eeg_focus_end
             FROM terminal_commands WHERE started_at >= ?1
             ORDER BY started_at DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64, limit as i64], |row| {
            Ok(TerminalCommandRow {
                id: row.get(0)?,
                terminal_name: row.get(1)?,
                command: row.get(2)?,
                cwd: row.get(3)?,
                exit_code: row.get(4)?,
                started_at: row.get::<_, i64>(5)? as u64,
                ended_at: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
                duration_secs: row.get(7)?,
                category: row.get(8)?,
                eeg_focus: row.get(9)?,
                eeg_focus_end: row.get(10)?,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Terminal impact on focus: avg focus delta by command category.
    pub fn terminal_focus_impact(&self, since: u64) -> Vec<TerminalImpactRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT category,
                    COUNT(*) as cmd_count,
                    AVG(eeg_focus_end - eeg_focus) as avg_focus_delta,
                    SUM(CASE WHEN exit_code = 0 THEN 1 ELSE 0 END) as pass_count,
                    SUM(CASE WHEN exit_code IS NOT NULL AND exit_code != 0 THEN 1 ELSE 0 END) as fail_count,
                    AVG(duration_secs) as avg_duration
             FROM terminal_commands
             WHERE started_at >= ?1 AND eeg_focus IS NOT NULL AND eeg_focus_end IS NOT NULL
             GROUP BY category
             ORDER BY cmd_count DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([since as i64], |row| {
            Ok(TerminalImpactRow {
                category: row.get(0)?,
                cmd_count: row.get(1)?,
                avg_focus_delta: row.get(2)?,
                pass_count: row.get(3)?,
                fail_count: row.get(4)?,
                avg_duration_secs: row.get(5)?,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Context switch cost: avg focus dip when switching zones.
    pub fn zone_switch_cost(&self, since: u64) -> Vec<ZoneSwitchCostRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT from_zone, zone, COUNT(*) as switches,
                    AVG(eeg_focus) as avg_focus_at_switch
             FROM zone_switches
             WHERE at >= ?1 AND eeg_focus IS NOT NULL
             GROUP BY from_zone, zone
             ORDER BY switches DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([since as i64], |row| {
            Ok(ZoneSwitchCostRow {
                from_zone: row.get(0)?,
                to_zone: row.get(1)?,
                switches: row.get(2)?,
                avg_focus_at_switch: row.get(3)?,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Correlate terminal commands with input activity.
    /// For each terminal command that was running during a time range, sum up
    /// the keystrokes and mouse events from input_buckets that occurred while
    /// that command's window was in the foreground.
    pub fn terminal_input_activity(&self, since: u64) -> Vec<TerminalInputRow> {
        let c = self.conn.lock_or_recover();
        // Get terminal commands in the window
        let mut cmd_stmt = match c.prepare_cached(
            "SELECT id, command, category, started_at,
                    COALESCE(ended_at, ?2) as end_at,
                    terminal_name
             FROM terminal_commands
             WHERE started_at >= ?1
             ORDER BY started_at DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let now = now_secs();
        let cmds: Vec<(i64, String, String, u64, u64, String)> = cmd_stmt
            .query_map(params![since as i64, now as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, i64>(4)? as u64,
                    row.get::<_, String>(5)?,
                ))
            })
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();

        let mut results = Vec::new();
        for (_id, command, category, start, end, _terminal) in &cmds {
            // Round to minute boundaries for input_buckets join
            let start_min = start - (start % 60);
            let end_min = end - (end % 60) + 60;
            let duration_secs = end.saturating_sub(*start);

            // Sum keystrokes and mouse events during this command's runtime
            let (keys, mouse): (i64, i64) = c
                .query_row(
                    "SELECT COALESCE(SUM(key_count), 0), COALESCE(SUM(mouse_count), 0)
                     FROM input_buckets
                     WHERE minute_ts >= ?1 AND minute_ts < ?2",
                    params![start_min as i64, end_min as i64],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap_or((0, 0));

            if keys > 0 || mouse > 0 || duration_secs > 60 {
                results.push(TerminalInputRow {
                    command: command.clone(),
                    category: category.clone(),
                    started_at: *start,
                    duration_secs,
                    keystrokes: keys as u64,
                    mouse_events: mouse as u64,
                    keys_per_min: if duration_secs > 0 {
                        keys as f64 / (duration_secs as f64 / 60.0)
                    } else {
                        0.0
                    },
                });
            }
        }
        results
    }

    /// Recategorize all terminal commands and backfill binary/args.
    /// Call after updating categorize_command() rules.
    pub fn recategorize_commands(&self) -> u64 {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare("SELECT id, command FROM terminal_commands") {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map(|r| r.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        let mut count = 0u64;
        for (id, cmd) in &rows {
            let category = Self::categorize_command(cmd);
            let mut parts = cmd.splitn(2, char::is_whitespace);
            let binary = parts.next().unwrap_or("").trim();
            let args = parts.next().unwrap_or("").trim();
            let _ = c.execute(
                "UPDATE terminal_commands SET category = ?1, binary = ?2, args = ?3 WHERE id = ?4",
                params![category, binary, args, id],
            );
            count += 1;
        }
        count
    }

    /// Get usage frequency per binary (for discovering uncategorized tools).
    pub fn binary_usage_stats(&self, since: u64, limit: u32) -> Vec<(String, String, i64)> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT binary, category, COUNT(*) as n FROM terminal_commands
             WHERE started_at >= ?1 AND binary != ''
             GROUP BY binary ORDER BY n DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![since as i64, limit as i64], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map(|rows| rows.filter_map(|x| x.ok()).collect())
        .unwrap_or_default()
    }

    // ── Developer Insights (EEG + activity fusion) ─────────────────────────

    /// Insight 1: Test failure rate by focus level.
    /// "Your tests fail more when focus is below X."
    pub fn insight_test_failure_by_focus(&self, since: u64) -> serde_json::Value {
        let c = self.conn.lock_or_recover();
        let rows: Vec<(String, i64, i64)> = c
            .prepare_cached(
                "SELECT
               CASE WHEN eeg_focus >= 70 THEN 'high' WHEN eeg_focus >= 40 THEN 'mid' ELSE 'low' END as level,
               SUM(CASE WHEN exit_code = 0 THEN 1 ELSE 0 END) as passes,
               SUM(CASE WHEN exit_code != 0 THEN 1 ELSE 0 END) as fails
             FROM terminal_commands
             WHERE started_at >= ?1 AND category IN ('test','build') AND exit_code IS NOT NULL AND eeg_focus IS NOT NULL
             GROUP BY level",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map(params![since as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .map(|r| r.filter_map(|x| x.ok()).collect())
                    .ok()
            })
            .unwrap_or_default();
        let arr: Vec<serde_json::Value> = rows.into_iter().map(|(level, p, f)| {
            let total = p + f;
            serde_json::json!({"focus_level": level, "passes": p, "fails": f, "fail_rate": if total > 0 { f as f64 / total as f64 } else { 0.0 }})
        }).collect();
        serde_json::json!({"test_failure_by_focus": arr})
    }

    /// Insight 2: Productivity by hour with EEG.
    /// "You write 3x more bugs after 2pm."
    pub fn insight_hourly_productivity(&self, since: u64, tz_offset: i32) -> serde_json::Value {
        let c = self.conn.lock_or_recover();
        let rows: Vec<(i64, f64, i64, i64)> = c
            .prepare_cached(
                "SELECT ((seen_at + ?2) % 86400) / 3600 as hour,
                    AVG(COALESCE(eeg_focus, 50)) as avg_focus,
                    SUM(lines_added + lines_removed) as churn,
                    SUM(undo_count) as undos
             FROM file_interactions WHERE seen_at >= ?1
             GROUP BY hour ORDER BY hour",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map(params![since as i64, tz_offset as i64], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .map(|r| r.filter_map(|x| x.ok()).collect())
                .ok()
            })
            .unwrap_or_default();
        let arr: Vec<serde_json::Value> = rows.into_iter().map(|(h, focus, churn, undos)| {
            serde_json::json!({"hour": h, "avg_focus": focus, "churn": churn, "undo_rate": if churn > 0 { undos as f64 / churn as f64 } else { 0.0 }})
        }).collect();
        serde_json::json!({"hourly_productivity": arr})
    }

    /// Insight 3: Context switch recovery time.
    /// "Switching to terminal costs you N minutes of focus recovery."
    pub fn insight_switch_recovery(&self, since: u64) -> serde_json::Value {
        let c = self.conn.lock_or_recover();
        // Get zone switches with focus before and after
        let rows: Vec<(String, String, f64)> = c
            .prepare_cached(
                "SELECT from_zone, zone, AVG(eeg_focus) FROM zone_switches
             WHERE at >= ?1 AND eeg_focus IS NOT NULL
             GROUP BY from_zone, zone",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map(params![since as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .map(|r| r.filter_map(|x| x.ok()).collect())
                    .ok()
            })
            .unwrap_or_default();
        let arr: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|(from, to, focus)| serde_json::json!({"from": from, "to": to, "avg_focus_at_switch": focus}))
            .collect();
        serde_json::json!({"switch_recovery": arr})
    }

    /// Insight 4: AI tool impact on flow.
    /// "Claude conversations drop your focus by X points on average."
    pub fn insight_ai_impact(&self, since: u64) -> serde_json::Value {
        let c = self.conn.lock_or_recover();
        let rows: Vec<(String, f64, i64)> = c
            .prepare_cached(
                "SELECT app, AVG(eeg_focus), COUNT(*) FROM conversations
             WHERE at >= ?1 AND role = 'user' AND eeg_focus IS NOT NULL
             GROUP BY app",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map(params![since as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .map(|r| r.filter_map(|x| x.ok()).collect())
                    .ok()
            })
            .unwrap_or_default();
        // Compare with overall avg focus
        let overall: f64 = c
            .query_row(
                "SELECT AVG(eeg_focus) FROM file_interactions WHERE seen_at >= ?1 AND eeg_focus IS NOT NULL",
                params![since as i64],
                |r| r.get(0),
            )
            .unwrap_or(50.0);
        let arr: Vec<serde_json::Value> = rows.into_iter().map(|(app, focus, n)| {
            serde_json::json!({"app": app, "avg_focus_during": focus, "baseline_focus": overall, "delta": focus - overall, "message_count": n})
        }).collect();
        serde_json::json!({"ai_impact": arr, "baseline_focus": overall})
    }

    /// Insight 5: Focus by language/file type.
    /// "Your best code happens in Rust, worst in CSS."
    pub fn insight_focus_by_language(&self, since: u64) -> serde_json::Value {
        let c = self.conn.lock_or_recover();
        let rows: Vec<(String, f64, i64, i64)> = c
            .prepare_cached(
                "SELECT language, AVG(eeg_focus), SUM(undo_count), COUNT(*)
             FROM file_interactions
             WHERE seen_at >= ?1 AND eeg_focus IS NOT NULL AND language != ''
             GROUP BY language HAVING COUNT(*) >= 3
             ORDER BY AVG(eeg_focus) DESC",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map(params![since as i64], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .map(|r| r.filter_map(|x| x.ok()).collect())
                .ok()
            })
            .unwrap_or_default();
        let arr: Vec<serde_json::Value> = rows.into_iter().map(|(lang, focus, undos, n)| {
            serde_json::json!({"language": lang, "avg_focus": focus, "undo_rate": undos as f64 / n.max(1) as f64, "interactions": n})
        }).collect();
        serde_json::json!({"focus_by_language": arr})
    }

    /// Insight 6: Dev loop efficiency by time of day.
    /// "Your cycle time doubles after 3pm."
    pub fn insight_loop_efficiency(&self, since: u64, tz_offset: i32) -> serde_json::Value {
        let c = self.conn.lock_or_recover();
        let rows: Vec<(i64, f64, i64, i64)> = c
            .prepare_cached(
                "SELECT ((started_at + ?2) % 86400) / 3600 as hour,
                    AVG(duration_secs) as avg_duration,
                    SUM(CASE WHEN exit_code = 0 THEN 1 ELSE 0 END) as passes,
                    SUM(CASE WHEN exit_code != 0 THEN 1 ELSE 0 END) as fails
             FROM terminal_commands
             WHERE started_at >= ?1 AND category IN ('test','build') AND exit_code IS NOT NULL
             GROUP BY hour ORDER BY hour",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map(params![since as i64, tz_offset as i64], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .map(|r| r.filter_map(|x| x.ok()).collect())
                .ok()
            })
            .unwrap_or_default();
        let arr: Vec<serde_json::Value> = rows.into_iter().map(|(h, dur, p, f)| {
            let total = p + f;
            serde_json::json!({"hour": h, "avg_cycle_secs": dur, "pass_rate": if total > 0 { p as f64 / total as f64 } else { 0.0 }, "total": total})
        }).collect();
        serde_json::json!({"loop_efficiency_by_hour": arr})
    }

    /// Insight 7: Tool impact on focus.
    /// "Docker commands correlate with your lowest focus."
    pub fn insight_tool_impact(&self, since: u64) -> serde_json::Value {
        let c = self.conn.lock_or_recover();
        let rows: Vec<(String, f64, i64)> = c
            .prepare_cached(
                "SELECT category, AVG(eeg_focus), COUNT(*) FROM terminal_commands
             WHERE started_at >= ?1 AND eeg_focus IS NOT NULL
             GROUP BY category HAVING COUNT(*) >= 2
             ORDER BY AVG(eeg_focus) ASC",
            )
            .ok()
            .and_then(|mut s| {
                s.query_map(params![since as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                    .map(|r| r.filter_map(|x| x.ok()).collect())
                    .ok()
            })
            .unwrap_or_default();
        let arr: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|(cat, focus, n)| serde_json::json!({"category": cat, "avg_focus": focus, "count": n}))
            .collect();
        serde_json::json!({"tool_focus_impact": arr})
    }

    /// All insights in one call.
    pub fn developer_insights(&self, since: u64, tz_offset: i32) -> serde_json::Value {
        serde_json::json!({
            "test_failure_by_focus": self.insight_test_failure_by_focus(since),
            "hourly_productivity": self.insight_hourly_productivity(since, tz_offset),
            "switch_recovery": self.insight_switch_recovery(since),
            "ai_impact": self.insight_ai_impact(since),
            "focus_by_language": self.insight_focus_by_language(since),
            "loop_efficiency": self.insight_loop_efficiency(since, tz_offset),
            "tool_impact": self.insight_tool_impact(since),
            "screenshot_moments": self.screenshot_analysis(since, 0),
        })
    }

    // ── Deep AI interaction analytics ──────────────────────────────────────
    //
    // 6 dimensions of how the developer works with AI, tracked over time
    // and across modalities (code edits, EEG, diagnostics, conversations).

    /// Deep AI analytics — returns 6 dimensions of AI interaction quality.
    pub fn ai_deep_analytics(&self, since: u64) -> serde_json::Value {
        let c = self.conn.lock_or_recover();

        // ── Dimension 1: AI Code Lifecycle ──
        let lifecycle: Vec<(String, i64)> = c
            .prepare(
                "SELECT event_type, COUNT(*) FROM ai_events
                 WHERE at >= ?1 AND event_type IN (
                     'suggestion_accepted', 'ai_code_refined', 'ai_code_undone', 'ai_code_deleted'
                 ) GROUP BY event_type",
            )
            .and_then(|mut s| {
                s.query_map(params![since as i64], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();

        let accepted = lifecycle
            .iter()
            .find(|(t, _)| t == "suggestion_accepted")
            .map(|(_, c)| *c)
            .unwrap_or(0);
        let refined = lifecycle
            .iter()
            .find(|(t, _)| t == "ai_code_refined")
            .map(|(_, c)| *c)
            .unwrap_or(0);
        let undone = lifecycle
            .iter()
            .find(|(t, _)| t == "ai_code_undone")
            .map(|(_, c)| *c)
            .unwrap_or(0);
        let deleted = lifecycle
            .iter()
            .find(|(t, _)| t == "ai_code_deleted")
            .map(|(_, c)| *c)
            .unwrap_or(0);
        let survival_rate = if accepted > 0 {
            (accepted - undone - deleted).max(0) as f64 / accepted as f64
        } else {
            0.0
        };
        let refinement_rate = if accepted > 0 {
            refined as f64 / accepted as f64
        } else {
            0.0
        };

        // ── Dimension 2: AI Dependency Trend (daily ratio over 30 days) ──
        let daily_trend: Vec<serde_json::Value> = c
            .prepare(
                "SELECT (at / 86400) * 86400 AS day_ts, COUNT(*) AS ai_events
                 FROM ai_events WHERE at >= ?1
                 AND event_type IN ('suggestion_accepted', 'ai_code_refined')
                 GROUP BY day_ts ORDER BY day_ts",
            )
            .and_then(|mut s| {
                s.query_map(params![since as i64], |r| {
                    Ok(serde_json::json!({
                        "day": r.get::<_, i64>(0)?,
                        "ai_events": r.get::<_, i64>(1)?,
                    }))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();

        let invocation_counts: (i64, i64) = c
            .query_row(
                "SELECT
                    SUM(CASE WHEN source = 'active' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN source = 'passive' THEN 1 ELSE 0 END)
                 FROM ai_events WHERE at >= ?1 AND event_type = 'ai_invocation'",
                params![since as i64],
                |r| Ok((r.get::<_, i64>(0).unwrap_or(0), r.get::<_, i64>(1).unwrap_or(0))),
            )
            .unwrap_or((0, 0));

        // ── Dimension 3: AI Effectiveness by Context ──
        let by_language: Vec<serde_json::Value> = c
            .prepare(
                "SELECT language,
                    SUM(CASE WHEN event_type = 'suggestion_accepted' THEN 1 ELSE 0 END) AS accepted,
                    SUM(CASE WHEN event_type IN ('ai_code_undone', 'ai_code_deleted') THEN 1 ELSE 0 END) AS rejected,
                    AVG(eeg_focus) AS avg_focus
                 FROM ai_events WHERE at >= ?1 AND language != ''
                 GROUP BY language HAVING COUNT(*) >= 2
                 ORDER BY accepted DESC LIMIT 10",
            )
            .and_then(|mut s| {
                s.query_map(params![since as i64], |r| {
                    Ok(serde_json::json!({
                        "language": r.get::<_, String>(0)?,
                        "accepted": r.get::<_, i64>(1)?,
                        "rejected": r.get::<_, i64>(2)?,
                        "avg_focus": r.get::<_, Option<f64>>(3)?,
                    }))
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();

        // ── Dimension 4: AI Conversation Quality ──
        let conversation_stats: serde_json::Value = c
            .query_row(
                "SELECT COUNT(DISTINCT session) AS sessions,
                        COUNT(*) AS total_messages,
                        AVG(CASE WHEN role = 'user' THEN eeg_focus END) AS avg_focus_during
                 FROM conversations WHERE at >= ?1 AND session != ''",
                params![since as i64],
                |r| {
                    Ok(serde_json::json!({
                        "sessions": r.get::<_, i64>(0)?,
                        "total_messages": r.get::<_, i64>(1)?,
                        "avg_focus_during": r.get::<_, Option<f64>>(2)?,
                    }))
                },
            )
            .unwrap_or(serde_json::json!({}));

        // ── Dimension 5: AI Impact on Cognition ──
        let ai_focus: Option<f64> = c
            .query_row(
                "SELECT AVG(eeg_focus) FROM ai_events
                 WHERE at >= ?1 AND eeg_focus IS NOT NULL
                 AND event_type IN ('suggestion_accepted', 'ai_code_refined')",
                params![since as i64],
                |r| r.get(0),
            )
            .unwrap_or(None);

        let human_focus: Option<f64> = c
            .query_row(
                "SELECT AVG(eeg_focus) FROM file_interactions
                 WHERE seen_at >= ?1 AND eeg_focus IS NOT NULL",
                params![since as i64],
                |r| r.get(0),
            )
            .unwrap_or(None);

        let focus_delta = match (ai_focus, human_focus) {
            (Some(ai), Some(human)) => Some(ai - human),
            _ => None,
        };

        // ── Dimension 6: AI Code Quality ──
        // Undo rate on AI events vs overall.
        let ai_undo_rate: f64 = c
            .query_row(
                "SELECT CAST(SUM(CASE WHEN event_type = 'ai_code_undone' THEN 1 ELSE 0 END) AS REAL) /
                        NULLIF(SUM(CASE WHEN event_type = 'suggestion_accepted' THEN 1 ELSE 0 END), 0)
                 FROM ai_events WHERE at >= ?1",
                params![since as i64],
                |r| r.get::<_, Option<f64>>(0),
            )
            .unwrap_or(None)
            .unwrap_or(0.0);

        serde_json::json!({
            "lifecycle": {
                "accepted": accepted,
                "refined": refined,
                "undone": undone,
                "deleted": deleted,
                "survival_rate": survival_rate,
                "refinement_rate": refinement_rate,
            },
            "dependency": {
                "daily_trend": daily_trend,
                "active_invocations": invocation_counts.0,
                "passive_invocations": invocation_counts.1,
            },
            "effectiveness": {
                "by_language": by_language,
            },
            "conversations": conversation_stats,
            "cognition": {
                "ai_focus": ai_focus,
                "human_focus": human_focus,
                "focus_delta": focus_delta,
            },
            "quality": {
                "ai_undo_rate": ai_undo_rate,
            },
        })
    }

    // ── User screenshot analysis ────────────────────────────────────────────
    //
    // User-initiated screenshots are *intentional mental actions* — the user
    // decided this moment was worth capturing.  This makes them high-signal
    // markers for EEG correlation, context analysis, and focus patterns.

    /// Return recent user screenshot events with full context.
    pub fn get_user_screenshot_events(&self, since: u64, limit: u32) -> Vec<UserScreenshotEventRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, screenshot_id, captured_at, app_name, window_title,
                    original_path, ocr_preview, eeg_focus, eeg_mood
             FROM user_screenshot_events
             WHERE captured_at >= ?1
             ORDER BY captured_at DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[activity] get_user_screenshot_events: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![since as i64, limit as i64], |row| {
            Ok(UserScreenshotEventRow {
                id: row.get(0)?,
                screenshot_id: row.get(1)?,
                captured_at: row.get::<_, i64>(2)? as u64,
                app_name: row.get(3)?,
                window_title: row.get(4)?,
                original_path: row.get(5)?,
                ocr_preview: row.get(6)?,
                eeg_focus: row.get(7)?,
                eeg_mood: row.get(8)?,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Analyze screenshot-taking behavior correlated with EEG focus.
    ///
    /// Returns:
    /// - `screenshot_count`: total screenshots in the period
    /// - `avg_focus`: average EEG focus at screenshot moments
    /// - `avg_mood`: average EEG mood at screenshot moments
    /// - `by_app`: screenshots grouped by the app that was focused
    /// - `by_hour`: screenshots grouped by hour-of-day (local time)
    /// - `high_focus_screenshots`: screenshots taken during high-focus periods (>60)
    pub fn screenshot_analysis(&self, since: u64, tz_offset: i32) -> serde_json::Value {
        let c = self.conn.lock_or_recover();

        // Aggregate counts and averages.
        let (count, avg_focus, avg_mood): (i64, Option<f64>, Option<f64>) = c
            .query_row(
                "SELECT COUNT(*), AVG(eeg_focus), AVG(eeg_mood)
                 FROM user_screenshot_events WHERE captured_at >= ?1",
                params![since as i64],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap_or((0, None, None));

        // By app.
        let by_app: Vec<serde_json::Value> = c
            .prepare(
                "SELECT app_name, COUNT(*), AVG(eeg_focus)
                 FROM user_screenshot_events
                 WHERE captured_at >= ?1 AND app_name != ''
                 GROUP BY app_name ORDER BY COUNT(*) DESC LIMIT 20",
            )
            .and_then(|mut stmt| {
                stmt.query_map(params![since as i64], |row| {
                    Ok(serde_json::json!({
                        "app": row.get::<_, String>(0)?,
                        "count": row.get::<_, i64>(1)?,
                        "avg_focus": row.get::<_, Option<f64>>(2)?,
                    }))
                })
                .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            })
            .unwrap_or_default();

        // By hour of day.
        let by_hour: Vec<serde_json::Value> = c
            .prepare(
                "SELECT ((captured_at + ?2) % 86400) / 3600 AS hour, COUNT(*), AVG(eeg_focus)
                 FROM user_screenshot_events
                 WHERE captured_at >= ?1
                 GROUP BY hour ORDER BY hour",
            )
            .and_then(|mut stmt| {
                stmt.query_map(params![since as i64, tz_offset as i64], |row| {
                    Ok(serde_json::json!({
                        "hour": row.get::<_, i64>(0)?,
                        "count": row.get::<_, i64>(1)?,
                        "avg_focus": row.get::<_, Option<f64>>(2)?,
                    }))
                })
                .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            })
            .unwrap_or_default();

        // High-focus screenshots (focus > 60).
        let high_focus_count: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM user_screenshot_events
                 WHERE captured_at >= ?1 AND eeg_focus > 60",
                params![since as i64],
                |row| row.get(0),
            )
            .unwrap_or(0);

        serde_json::json!({
            "screenshot_count": count,
            "avg_focus": avg_focus,
            "avg_mood": avg_mood,
            "by_app": by_app,
            "by_hour": by_hour,
            "high_focus_count": high_focus_count,
            "high_focus_ratio": if count > 0 { high_focus_count as f64 / count as f64 } else { 0.0 },
        })
    }

    // ── EEG Time-series ────────────────────────────────────────────────────

    /// Insert an EEG snapshot. Called periodically (every 5s) from the session runner.
    pub fn insert_eeg_sample(&self, ts: u64, metrics_json: &str) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT OR REPLACE INTO eeg_timeseries (ts, metrics) VALUES (?1, ?2)",
            params![ts as i64, metrics_json],
        );
    }

    /// Get the EEG metrics closest to a given timestamp.
    pub fn eeg_at(&self, ts: u64) -> Option<serde_json::Value> {
        let c = self.conn.lock_or_recover();
        c.query_row(
            "SELECT metrics FROM eeg_timeseries WHERE ts <= ?1 ORDER BY ts DESC LIMIT 1",
            params![ts as i64],
            |row| {
                let s: String = row.get(0)?;
                Ok(serde_json::from_str(&s).unwrap_or_default())
            },
        )
        .ok()
    }

    /// Get EEG metrics in a time range (for charts, correlation analysis).
    pub fn eeg_range(&self, from: u64, to: u64, max_points: u32) -> Vec<(u64, serde_json::Value)> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c
            .prepare_cached("SELECT ts, metrics FROM eeg_timeseries WHERE ts >= ?1 AND ts <= ?2 ORDER BY ts LIMIT ?3")
        {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![from as i64, to as i64, max_points as i64], |row| {
            let ts = row.get::<_, i64>(0)? as u64;
            let s: String = row.get(1)?;
            let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
            Ok((ts, v))
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Generic Embeddings ───────────────────────────────────────────────

    /// Store an embedding vector for any source item.
    pub fn insert_embedding(
        &self,
        source_type: &str,
        source_id: i64,
        source_text: &str,
        model: &str,
        vector: &[u8],
        created_at: u64,
    ) -> i64 {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO embeddings (source_type, source_id, source_text, model, vector, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_type, source_id, source_text, model, vector, created_at as i64],
        );
        c.last_insert_rowid()
    }

    /// Get embeddings for a source item (may have multiple models).
    pub fn get_embeddings(&self, source_type: &str, source_id: i64) -> Vec<EmbeddingRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, source_type, source_id, source_text, model, vector, created_at
             FROM embeddings WHERE source_type = ?1 AND source_id = ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![source_type, source_id], |row| {
            Ok(EmbeddingRow {
                id: row.get(0)?,
                source_type: row.get(1)?,
                source_id: row.get(2)?,
                source_text: row.get(3)?,
                model: row.get(4)?,
                vector: row.get(5)?,
                created_at: row.get::<_, i64>(6)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Get all embeddings of a given model (for building HNSW indices).
    pub fn get_embeddings_by_model(&self, model: &str, limit: u32) -> Vec<EmbeddingRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, source_type, source_id, source_text, model, vector, created_at
             FROM embeddings WHERE model = ?1 ORDER BY created_at DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![model, limit as i64], |row| {
            Ok(EmbeddingRow {
                id: row.get(0)?,
                source_type: row.get(1)?,
                source_id: row.get(2)?,
                source_text: row.get(3)?,
                model: row.get(4)?,
                vector: row.get(5)?,
                created_at: row.get::<_, i64>(6)? as u64,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    // ── Conversations ─────────────────────────────────────────────────────

    /// Insert a conversation message and update the FTS index.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_conversation(
        &self,
        app: &str,
        role: &str,
        text: &str,
        cwd: &str,
        at: u64,
        session: &str,
        eeg_focus: Option<f64>,
        eeg_mood: Option<f64>,
    ) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO conversations (app, role, text, cwd, at, session, eeg_focus, eeg_mood) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![app, role, text, cwd, at as i64, session, eeg_focus, eeg_mood],
        );
        // Keep FTS in sync
        let id = c.last_insert_rowid();
        let _ = c.execute(
            "INSERT INTO conversations_fts (rowid, text, app, role) VALUES (?1, ?2, ?3, ?4)",
            params![id, text, app, role],
        );
    }

    /// Full-text search on conversations.
    pub fn search_conversations_fts(&self, query: &str, limit: u32) -> Vec<ConversationRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT c.id, c.app, c.role, c.text, c.cwd, c.at, c.session, c.eeg_focus
             FROM conversations_fts f
             JOIN conversations c ON c.id = f.rowid
             WHERE conversations_fts MATCH ?1
             ORDER BY c.at DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![query, limit as i64], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                app: row.get(1)?,
                role: row.get(2)?,
                text: row.get(3)?,
                cwd: row.get(4)?,
                at: row.get::<_, i64>(5)? as u64,
                session: row.get(6)?,
                eeg_focus: row.get(7).ok(),
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Structured query: filter by app, role, time range.
    pub fn search_conversations_structured(
        &self,
        app: Option<&str>,
        role: Option<&str>,
        since: u64,
        until: u64,
        limit: u32,
    ) -> Vec<ConversationRow> {
        let c = self.conn.lock_or_recover();
        let mut sql =
            "SELECT id, app, role, text, cwd, at, session, eeg_focus FROM conversations WHERE at >= ?1 AND at <= ?2"
                .to_string();
        if app.is_some() {
            sql += " AND app = ?3";
        }
        if role.is_some() {
            sql += " AND role = ?4";
        }
        sql += " ORDER BY at DESC LIMIT ?5";
        let mut stmt = match c.prepare_cached(&sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let app_val = app.unwrap_or("");
        let role_val = role.unwrap_or("");
        stmt.query_map(
            params![since as i64, until as i64, app_val, role_val, limit as i64],
            |row| {
                Ok(ConversationRow {
                    id: row.get(0)?,
                    app: row.get(1)?,
                    role: row.get(2)?,
                    text: row.get(3)?,
                    cwd: row.get(4)?,
                    at: row.get::<_, i64>(5)? as u64,
                    session: row.get(6)?,
                    eeg_focus: row.get(7).ok(),
                })
            },
        )
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Fuzzy search: LIKE-based substring matching.
    pub fn search_conversations_fuzzy(&self, query: &str, limit: u32) -> Vec<ConversationRow> {
        let c = self.conn.lock_or_recover();
        let pattern = format!("%{query}%");
        let mut stmt = match c.prepare_cached(
            "SELECT id, app, role, text, cwd, at, session, eeg_focus FROM conversations
             WHERE text LIKE ?1 ORDER BY at DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                app: row.get(1)?,
                role: row.get(2)?,
                text: row.get(3)?,
                cwd: row.get(4)?,
                at: row.get::<_, i64>(5)? as u64,
                session: row.get(6)?,
                eeg_focus: row.get(7).ok(),
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
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

    // ── User screenshot events ─────────────────────────────────────────────────

    /// Record a user-initiated screenshot event with full context.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_user_screenshot_event(
        &self,
        screenshot_id: i64,
        captured_at: u64,
        app_name: &str,
        window_title: &str,
        original_path: &str,
        ocr_preview: &str,
        eeg_focus: Option<f32>,
        eeg_mood: Option<f32>,
    ) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute(
            "INSERT INTO user_screenshot_events
             (screenshot_id, captured_at, app_name, window_title, original_path, ocr_preview, eeg_focus, eeg_mood)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                screenshot_id,
                captured_at as i64,
                app_name,
                window_title,
                original_path,
                ocr_preview,
                eeg_focus,
                eeg_mood,
            ],
        );
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
        Self::get_focus_sessions_in_range_q(&c, from_ts, to_ts)
    }

    fn get_focus_sessions_in_range_q(c: &rusqlite::Connection, from_ts: u64, to_ts: u64) -> Vec<FocusSessionRow> {
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

    /// Delete terminal commands older than `cutoff`.
    pub fn prune_terminal_commands(&self, cutoff: u64) -> u64 {
        let c = self.conn.lock_or_recover();
        c.execute(
            "DELETE FROM terminal_commands WHERE started_at < ?1",
            params![cutoff as i64],
        )
        .unwrap_or(0) as u64
    }

    /// Delete AI events older than `cutoff`.
    pub fn prune_ai_events(&self, cutoff: u64) -> u64 {
        let c = self.conn.lock_or_recover();
        c.execute("DELETE FROM ai_events WHERE at < ?1", params![cutoff as i64])
            .unwrap_or(0) as u64
    }

    /// Delete zone switch records older than `cutoff`.
    pub fn prune_zone_switches(&self, cutoff: u64) -> u64 {
        let c = self.conn.lock_or_recover();
        c.execute("DELETE FROM zone_switches WHERE at < ?1", params![cutoff as i64])
            .unwrap_or(0) as u64
    }

    /// Delete layout snapshots older than `cutoff`.
    pub fn prune_layout_snapshots(&self, cutoff: u64) -> u64 {
        let c = self.conn.lock_or_recover();
        c.execute(
            "DELETE FROM layout_snapshots WHERE sampled_at < ?1",
            params![cutoff as i64],
        )
        .unwrap_or(0) as u64
    }

    /// Run SQLite's `PRAGMA optimize` to update query planner statistics.
    /// Lightweight — safe to call after pruning.
    pub fn optimize(&self) {
        let c = self.conn.lock_or_recover();
        let _ = c.execute_batch("PRAGMA optimize;");
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
    // ── Browser stats ──────────────────────────────────────────────────
    pub browser_events: u64,
    pub browser_top_domains: Vec<(String, String, u64)>,
    pub browser_content_breakdown: Vec<BrowserContentBreakdownRow>,
    pub browser_total_reading_secs: u64,
    pub browser_avg_distraction: Option<f64>,
    pub browser_video_watched_secs: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserScreenshotEventRow {
    pub id: i64,
    pub screenshot_id: i64,
    pub captured_at: u64,
    pub app_name: String,
    pub window_title: String,
    pub original_path: String,
    pub ocr_preview: String,
    pub eeg_focus: Option<f32>,
    pub eeg_mood: Option<f32>,
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
    pub eeg_focus: Option<f32>,
    pub eeg_mood: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbeddingRow {
    pub id: i64,
    pub source_type: String,
    pub source_id: i64,
    pub source_text: String,
    pub model: String,
    pub vector: Vec<u8>,
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConversationRow {
    pub id: i64,
    pub app: String,
    pub role: String,
    pub text: String,
    pub cwd: String,
    pub at: u64,
    pub session: String,
    pub eeg_focus: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DevLoopRow {
    pub loop_type: String,
    pub command: String,
    pub iterations: u32,
    pub passes: u32,
    pub fails: u32,
    pub started_at: u64,
    pub ended_at: u64,
    pub avg_cycle_secs: f64,
    pub avg_focus: Option<f64>,
    pub focus_trend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserActivityRow {
    pub id: i64,
    pub event_type: String,
    pub url: String,
    pub domain: String,
    pub title: String,
    pub tab_id: Option<i64>,
    pub browser_name: String,
    pub scroll_depth: Option<f64>,
    pub reading_time_secs: Option<i64>,
    pub typing_detected: bool,
    pub media_playing: bool,
    pub search_query: String,
    pub tab_count: Option<i64>,
    pub devtools_open: bool,
    pub category: String,
    pub referrer_domain: String,
    pub at: u64,
    pub eeg_focus: Option<f64>,
    pub eeg_mood: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserDomainFocusRow {
    pub domain: String,
    pub category: String,
    pub content_type: String,
    pub events: u64,
    pub avg_focus: Option<f64>,
    pub total_reading_secs: u64,
    pub avg_scroll_depth: Option<f64>,
    pub visits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserDistractionScore {
    pub score: f64,
    pub tab_switches_per_min: f64,
    pub social_pct: f64,
    pub productive_pct: f64,
    pub idle_pct: f64,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserContentBreakdownRow {
    pub content_type: String,
    pub events: u64,
    pub avg_focus: Option<f64>,
    pub total_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserLlmUsageRow {
    pub domain: String,
    pub provider: String,
    pub interactions: u64,
    pub avg_focus: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserResearchPatterns {
    pub total_searches: u64,
    pub refinement_rate: f64,
    pub revisit_count: u64,
    pub avg_search_focus: Option<f64>,
    pub stuck_indicator: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserLearningRow {
    pub domain: String,
    pub content_type: String,
    pub avg_focus: Option<f64>,
    pub avg_scroll_depth: Option<f64>,
    pub avg_reading_secs: u64,
    pub pages: u64,
    pub revisits: u64,
    pub efficiency_score: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrowserOptimalHours {
    pub best_hours: Vec<u32>,
    pub worst_hours: Vec<u32>,
    pub by_hour: Vec<BrowserHourRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserHourRow {
    pub hour: u32,
    pub avg_focus: f64,
    pub events: u64,
    pub reading_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserAiEffectivenessRow {
    pub provider: String,
    pub interactions: u64,
    pub avg_focus: Option<f64>,
    pub avg_session_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserProcrastination {
    pub score: f64,
    pub idle_secs: u64,
    pub max_mouse_idle_secs: u64,
    pub revisit_loops: u64,
    pub avg_focus: Option<f64>,
    pub procrastinating: bool,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserDeepReadingRow {
    pub domain: String,
    pub title: String,
    pub reading_secs: u64,
    pub scroll_depth: Option<f64>,
    pub eeg_focus: Option<f64>,
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserVideoRoi {
    pub total_watched_secs: u64,
    pub focused_watched_secs: u64,
    pub focus_ratio: f64,
    pub avg_focus: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserEmailImpact {
    pub email_sessions: u64,
    pub avg_focus_during_email: Option<f64>,
    pub avg_focus_outside_email: Option<f64>,
    pub focus_delta: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTabLoadRow {
    pub tab_count: u32,
    pub avg_focus: f64,
    pub samples: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserWeekdayComparison {
    pub weekday_avg_focus: Option<f64>,
    pub weekend_avg_focus: Option<f64>,
    pub weekday_events: u64,
    pub weekend_events: u64,
    pub delta: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserHourFocusRow {
    pub hour: u32,
    pub avg_focus: f64,
    pub events: u64,
    pub avg_scroll_depth: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCopyPastePatterns {
    pub copies: u64,
    pub pastes: u64,
    pub avg_paste_length: Option<f64>,
    pub top_domains: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserPostMeetingRow {
    pub meeting_title: String,
    pub platform: String,
    pub ended_at: u64,
    pub post_meeting_focus: Option<f64>,
    pub distraction_events: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainFeedbackAccuracy {
    pub insight: String,
    pub total: u64,
    pub correct: u64,
    pub accuracy: f64,
    pub avg_score: Option<f64>,
    pub avg_focus_when_correct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainFeedbackRow {
    pub id: i64,
    pub insight: String,
    pub correct: bool,
    pub score: Option<f64>,
    pub eeg_focus: Option<f64>,
    pub context: String,
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSwitchTax {
    pub total_switches: u64,
    pub avg_focus_at_switch: Option<f64>,
    pub estimated_lost_minutes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalInputRow {
    pub command: String,
    pub category: String,
    pub started_at: u64,
    pub duration_secs: u64,
    pub keystrokes: u64,
    pub mouse_events: u64,
    pub keys_per_min: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalCommandRow {
    pub id: i64,
    pub terminal_name: String,
    pub command: String,
    pub cwd: String,
    pub exit_code: Option<i64>,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub duration_secs: Option<i64>,
    pub category: String,
    pub eeg_focus: Option<f64>,
    pub eeg_focus_end: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalImpactRow {
    pub category: String,
    pub cmd_count: i64,
    pub avg_focus_delta: Option<f64>,
    pub pass_count: i64,
    pub fail_count: i64,
    pub avg_duration_secs: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ZoneSwitchCostRow {
    pub from_zone: String,
    pub to_zone: String,
    pub switches: i64,
    pub avg_focus_at_switch: Option<f64>,
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
        store.finalize_file_interaction(id, 120, true, 42, 8, 3, 15, 0, None, None);
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
        store.finalize_file_interaction(id1, 60, true, 10, 5, 2, 8, 0, None, None);
        let id2 = ins(&store, "/a.rs", "code", "", 200).unwrap();
        store.finalize_file_interaction(id2, 30, false, 0, 0, 0, 0, 0, None, None);
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
        store.finalize_file_interaction(1, 300, true, 100, 50, 10, 20, 3, None, None);
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
        store.finalize_file_interaction(id, 60, true, 0, 10, 5, 0, 0, None, None);
        // File edited today.
        let new_id = ins(&store, "/fresh.rs", "code", "proj", now).unwrap();
        store.finalize_file_interaction(new_id, 60, true, 0, 10, 5, 0, 0, None, None);

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
        store.finalize_file_interaction(id, 60, true, 0, 10, 5, 0, 7, None, None);
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
        store.insert_ai_event(
            "suggestion_accepted",
            "copilot",
            "/a.rs",
            "rust",
            1000,
            Some(75.0),
            Some(60.0),
        );
        store.insert_ai_event("chat_start", "claude", "/b.ts", "typescript", 2000, None, None);
        let events = store.get_recent_ai_events(10);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].source, "claude"); // newest first
        assert_eq!(events[1].event_type, "suggestion_accepted");
        // EEG data should be stored and retrieved.
        assert!((events[1].eeg_focus.unwrap() - 75.0).abs() < 0.1);
        assert!(events[0].eeg_focus.is_none());
    }

    // ── User screenshot event tests ─────────────────────────────────────

    #[test]
    fn insert_user_screenshot_event_roundtrip() {
        let store = open_temp();
        store.insert_user_screenshot_event(
            42,    // screenshot_id
            1_000, // captured_at
            "Safari",
            "GitHub - skill",
            "/Users/test/Desktop/Screenshot 2026-04-25.png",
            "some OCR text",
            Some(72.5),
            Some(65.0),
        );
        // Should appear in the activity timeline as kind="screenshot".
        let timeline = store.activity_timeline(900, 1_100, 100);
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].kind, "screenshot");
        assert_eq!(timeline[0].ts, 1_000);
        assert!(timeline[0].eeg_focus.is_some());
        assert!((timeline[0].eeg_focus.unwrap() - 72.5).abs() < 0.1);
    }

    #[test]
    fn user_screenshot_in_mixed_timeline() {
        let store = open_temp();
        // Insert a file interaction
        ins(&store, "/src/main.rs", "Code", "myproject", 1_000);
        // Insert a user screenshot event
        store.insert_user_screenshot_event(
            1,
            2_000,
            "Preview",
            "image.png",
            "/Desktop/Screenshot.png",
            "",
            Some(80.0),
            None,
        );
        // Insert a clipboard event
        store.insert_clipboard_event("Chrome", "text", 100, 3_000);

        let timeline = store.activity_timeline(500, 3_500, 100);
        assert_eq!(timeline.len(), 3);
        // Newest first
        assert_eq!(timeline[0].kind, "clipboard");
        assert_eq!(timeline[0].ts, 3_000);
        assert_eq!(timeline[1].kind, "screenshot");
        assert_eq!(timeline[1].ts, 2_000);
        assert_eq!(timeline[2].kind, "file");
        assert_eq!(timeline[2].ts, 1_000);
    }

    #[test]
    fn user_screenshot_event_without_eeg() {
        let store = open_temp();
        store.insert_user_screenshot_event(
            99,
            5_000,
            "Finder",
            "Desktop",
            "/Desktop/Screenshot.png",
            "",
            None,
            None,
        );
        let timeline = store.activity_timeline(4_900, 5_100, 10);
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].kind, "screenshot");
        assert!(timeline[0].eeg_focus.is_none());
    }

    #[test]
    fn screenshot_analysis_aggregates() {
        let store = open_temp();
        // Three screenshots from different apps, different focus levels.
        store.insert_user_screenshot_event(1, 1_000, "Safari", "GitHub", "/s1.png", "", Some(80.0), Some(70.0));
        store.insert_user_screenshot_event(2, 2_000, "Code", "main.rs", "/s2.png", "", Some(40.0), Some(50.0));
        store.insert_user_screenshot_event(3, 3_000, "Safari", "Docs", "/s3.png", "", Some(90.0), Some(75.0));

        let result = store.screenshot_analysis(500, 0);
        assert_eq!(result["screenshot_count"], 3);
        // Average focus: (80+40+90)/3 = 70
        let avg = result["avg_focus"].as_f64().unwrap();
        assert!((avg - 70.0).abs() < 0.5, "avg_focus should be ~70, got {avg}");
        // high_focus_count: 80 and 90 are > 60 → 2
        assert_eq!(result["high_focus_count"], 2);
        // by_app: Safari=2, Code=1
        let by_app = result["by_app"].as_array().unwrap();
        assert_eq!(by_app.len(), 2);
        assert_eq!(by_app[0]["app"], "Safari");
        assert_eq!(by_app[0]["count"], 2);
    }

    #[test]
    fn get_user_screenshot_events_returns_with_context() {
        let store = open_temp();
        store.insert_user_screenshot_event(
            10,
            5_000,
            "Finder",
            "Desktop — Finder",
            "/Users/me/Desktop/Screenshot.png",
            "hello world",
            Some(65.0),
            Some(55.0),
        );
        let events = store.get_user_screenshot_events(4_000, 10);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].app_name, "Finder");
        assert_eq!(events[0].window_title, "Desktop — Finder");
        assert_eq!(events[0].ocr_preview, "hello world");
        assert!((events[0].eeg_focus.unwrap() - 65.0).abs() < 0.1);
    }

    // ── Browser activity tests ──────────────────────────────────────────

    fn browser_event(event_type: &str, domain: &str) -> serde_json::Value {
        serde_json::json!({
            "type": event_type,
            "domain": domain,
            "category": "development",
            "content_type": "code",
        })
    }

    fn browser_event_full(
        event_type: &str,
        domain: &str,
        category: &str,
        content_type: &str,
        focus: Option<f64>,
        extras: serde_json::Value,
    ) -> serde_json::Value {
        let mut v = serde_json::json!({
            "type": event_type,
            "domain": domain,
            "category": category,
            "content_type": content_type,
        });
        if let serde_json::Value::Object(map) = extras {
            for (k, val) in map {
                v[k] = val;
            }
        }
        v
    }

    #[test]
    fn browser_insert_and_query() {
        let store = open_temp();
        let ev = browser_event("tab_switch", "github.com");
        store.insert_browser_activity_json(&ev, 1000, Some(75.0), None);
        store.insert_browser_activity_json(&browser_event("page_load", "docs.rs"), 1001, Some(80.0), None);

        let recent = store.get_recent_browser_activities(10, 0);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].domain, "docs.rs"); // newest first
        assert_eq!(recent[1].domain, "github.com");
    }

    #[test]
    fn browser_domain_breakdown_groups() {
        let store = open_temp();
        for i in 0..5 {
            store.insert_browser_activity_json(&browser_event("tab_switch", "github.com"), 1000 + i, None, None);
        }
        for i in 0..3 {
            store.insert_browser_activity_json(&browser_event("tab_switch", "twitter.com"), 2000 + i, None, None);
        }
        let breakdown = store.browser_domain_breakdown(0);
        assert!(breakdown.len() >= 2);
        assert_eq!(breakdown[0].0, "github.com"); // most events first
        assert_eq!(breakdown[0].2, 5);
    }

    #[test]
    fn browser_context_switch_rate_calculation() {
        let store = open_temp();
        // 10 tab switches in 5 minutes = 2/min
        for i in 0..10 {
            store.insert_browser_activity_json(&browser_event("tab_switch", "example.com"), 1000 + i * 30, None, None);
        }
        let rate = store.browser_context_switch_rate(1000, 1300); // 5 min window
        assert!(rate > 1.5 && rate < 2.5, "rate was {rate}");
    }

    #[test]
    fn browser_distraction_score_social_heavy() {
        let store = open_temp();
        let now = now_secs();
        // Insert social media events
        for i in 0..10 {
            store.insert_browser_activity_json(
                &browser_event_full(
                    "tab_switch",
                    "twitter.com",
                    "social",
                    "social",
                    None,
                    serde_json::json!({}),
                ),
                now - 200 + i,
                None,
                None,
            );
        }
        // Only 2 productive events
        store.insert_browser_activity_json(
            &browser_event_full(
                "tab_switch",
                "github.com",
                "development",
                "code",
                None,
                serde_json::json!({}),
            ),
            now - 100,
            None,
            None,
        );
        store.insert_browser_activity_json(
            &browser_event_full(
                "tab_switch",
                "docs.rs",
                "development",
                "code",
                None,
                serde_json::json!({}),
            ),
            now - 50,
            None,
            None,
        );

        let score = store.browser_distraction_score(300);
        // Social events should outnumber productive
        assert!(
            score.social_pct >= score.productive_pct,
            "social% {} should be >= productive% {}",
            score.social_pct,
            score.productive_pct
        );
    }

    #[test]
    fn browser_distraction_score_focused() {
        let store = open_temp();
        let now = now_secs();
        // All productive events
        for i in 0..10 {
            store.insert_browser_activity_json(
                &browser_event_full(
                    "tab_switch",
                    "github.com",
                    "development",
                    "code",
                    None,
                    serde_json::json!({}),
                ),
                now - 200 + i * 20,
                None,
                None,
            );
        }
        let score = store.browser_distraction_score(300);
        assert!(
            score.productive_pct > score.social_pct,
            "productive% {} should be > social% {}",
            score.productive_pct,
            score.social_pct
        );
    }

    #[test]
    fn browser_content_breakdown_groups_by_type() {
        let store = open_temp();
        store.insert_browser_activity_json(
            &browser_event_full(
                "page_profile",
                "youtube.com",
                "media",
                "video",
                None,
                serde_json::json!({"reading_time_secs": 120}),
            ),
            1000,
            None,
            None,
        );
        store.insert_browser_activity_json(
            &browser_event_full(
                "page_profile",
                "arxiv.org",
                "reference",
                "paper",
                None,
                serde_json::json!({"reading_time_secs": 300}),
            ),
            1001,
            None,
            None,
        );
        store.insert_browser_activity_json(
            &browser_event_full(
                "page_profile",
                "github.com",
                "development",
                "code",
                None,
                serde_json::json!({"reading_time_secs": 200}),
            ),
            1002,
            None,
            None,
        );

        let breakdown = store.browser_content_breakdown(0);
        assert!(breakdown.len() >= 3);
        // Paper has most reading time (300s)
        assert_eq!(breakdown[0].content_type, "paper");
    }

    #[test]
    fn browser_focus_by_domain_with_eeg() {
        let store = open_temp();
        for i in 0..5 {
            store.insert_browser_activity_json(&browser_event("reading_time", "docs.rs"), 1000 + i, Some(80.0), None);
        }
        for i in 0..5 {
            store.insert_browser_activity_json(
                &browser_event("reading_time", "reddit.com"),
                2000 + i,
                Some(30.0),
                None,
            );
        }
        let focus = store.browser_focus_by_domain(0, 10);
        assert!(focus.len() >= 2);
        // docs.rs should have higher focus
        let docs = focus.iter().find(|f| f.domain == "docs.rs").unwrap();
        let reddit = focus.iter().find(|f| f.domain == "reddit.com").unwrap();
        assert!(docs.avg_focus.unwrap() > reddit.avg_focus.unwrap());
    }

    #[test]
    fn browser_procrastination_detection() {
        let store = open_temp();
        let now = now_secs();
        // Lots of idle + revisits + low focus
        for i in 0..6 {
            store.insert_browser_activity_json(
                &browser_event_full(
                    "revisit",
                    "stackoverflow.com",
                    "development",
                    "code",
                    None,
                    serde_json::json!({"idle_time_secs": 60}),
                ),
                now - 200 + i * 30,
                Some(25.0),
                None,
            );
        }
        let result = store.browser_procrastination_check(300);
        assert!(
            result.score > 40.0,
            "procrastination score should be elevated: {}",
            result.score
        );
        assert!(result.revisit_loops >= 6);
    }

    #[test]
    fn browser_deep_reading_sessions() {
        let store = open_temp();
        // Deep read: >5min, >60% scroll
        store.insert_browser_activity_json(
            &browser_event_full(
                "reading_time",
                "rust-lang.org",
                "reference",
                "text",
                None,
                serde_json::json!({"reading_time_secs": 480, "scroll_depth": 0.85}),
            ),
            1000,
            Some(78.0),
            None,
        );
        // Shallow read: too short
        store.insert_browser_activity_json(
            &browser_event_full(
                "reading_time",
                "twitter.com",
                "social",
                "social",
                None,
                serde_json::json!({"reading_time_secs": 30, "scroll_depth": 0.2}),
            ),
            2000,
            Some(20.0),
            None,
        );

        let sessions = store.browser_deep_reading_sessions(0);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].domain, "rust-lang.org");
        assert_eq!(sessions[0].reading_secs, 480);
    }

    #[test]
    fn browser_video_roi() {
        let store = open_temp();
        // High-focus video watching
        store.insert_browser_activity_json(
            &browser_event_full(
                "media_state",
                "youtube.com",
                "media",
                "video",
                None,
                serde_json::json!({"video_watched_secs": 300, "has_video": true}),
            ),
            1000,
            Some(70.0),
            None,
        );
        // Low-focus video
        store.insert_browser_activity_json(
            &browser_event_full(
                "media_state",
                "youtube.com",
                "media",
                "video",
                None,
                serde_json::json!({"video_watched_secs": 200, "has_video": true}),
            ),
            2000,
            Some(30.0),
            None,
        );

        let roi = store.browser_video_roi(0);
        assert_eq!(roi.total_watched_secs, 500);
        assert_eq!(roi.focused_watched_secs, 300); // only the >60 focus one
        assert!((roi.focus_ratio - 0.6).abs() < 0.01);
    }

    #[test]
    fn browser_email_impact() {
        let store = open_temp();
        // Email sessions with lower focus
        for i in 0..3 {
            store.insert_browser_activity_json(
                &browser_event_full(
                    "email_activity",
                    "mail.google.com",
                    "communication",
                    "email",
                    None,
                    serde_json::json!({}),
                ),
                1000 + i,
                Some(40.0),
                None,
            );
        }
        // Non-email with higher focus
        for i in 0..5 {
            store.insert_browser_activity_json(&browser_event("tab_switch", "github.com"), 2000 + i, Some(75.0), None);
        }
        let impact = store.browser_email_impact(0);
        assert_eq!(impact.email_sessions, 3);
        assert!(impact.focus_delta.unwrap() < 0.0); // email focus < non-email
    }

    #[test]
    fn browser_tab_cognitive_load() {
        let store = open_temp();
        // Few tabs = high focus
        for i in 0..5 {
            store.insert_browser_activity_json(
                &browser_event_full(
                    "tab_snapshot",
                    "",
                    "development",
                    "",
                    None,
                    serde_json::json!({"tab_count": 5}),
                ),
                1000 + i,
                Some(80.0),
                None,
            );
        }
        // Many tabs = low focus
        for i in 0..5 {
            store.insert_browser_activity_json(
                &browser_event_full(
                    "tab_snapshot",
                    "",
                    "development",
                    "",
                    None,
                    serde_json::json!({"tab_count": 40}),
                ),
                2000 + i,
                Some(35.0),
                None,
            );
        }
        let load = store.browser_tab_cognitive_load(0);
        assert!(load.len() >= 2);
        let low_tabs = load.iter().find(|t| t.tab_count == 5).unwrap();
        let high_tabs = load.iter().find(|t| t.tab_count == 40).unwrap();
        assert!(low_tabs.avg_focus > high_tabs.avg_focus);
    }

    #[test]
    fn browser_copypaste_patterns() {
        let store = open_temp();
        for i in 0..3 {
            store.insert_browser_activity_json(
                &browser_event_full(
                    "clipboard_copy",
                    "stackoverflow.com",
                    "development",
                    "code",
                    None,
                    serde_json::json!({}),
                ),
                1000 + i,
                None,
                None,
            );
        }
        store.insert_browser_activity_json(
            &browser_event_full(
                "clipboard_paste",
                "github.com",
                "development",
                "code",
                None,
                serde_json::json!({"paste_length": 150}),
            ),
            2000,
            None,
            None,
        );

        let patterns = store.browser_copypaste_patterns(0);
        assert_eq!(patterns.copies, 3);
        assert_eq!(patterns.pastes, 1);
        assert!(patterns.top_domains.len() >= 1);
    }

    // ── Feedback system tests ───────────────────────────────────────────

    #[test]
    fn brain_feedback_insert_and_accuracy() {
        let store = open_temp();
        store.insert_brain_feedback("distraction", true, Some(75.0), Some(60.0), None, "was accurate");
        store.insert_brain_feedback("distraction", true, Some(80.0), Some(55.0), None, "correct");
        store.insert_brain_feedback("distraction", false, Some(30.0), Some(70.0), None, "was wrong");

        let acc = store.brain_feedback_accuracy();
        let d = acc.iter().find(|a| a.insight == "distraction").unwrap();
        assert_eq!(d.total, 3);
        assert_eq!(d.correct, 2);
        assert!((d.accuracy - 0.666).abs() < 0.01);
    }

    #[test]
    fn brain_feedback_weight_default_and_adjusted() {
        let store = open_temp();
        // No feedback yet — default weight 1.0
        assert!((store.brain_feedback_weight("flow_browser") - 1.0).abs() < 0.01);

        // Add 5+ feedback entries (threshold for adjustment)
        for _ in 0..4 {
            store.insert_brain_feedback("flow_browser", true, None, None, None, "");
        }
        store.insert_brain_feedback("flow_browser", false, None, None, None, "");

        // 4/5 = 80% accuracy → weight = 0.5 + 0.8 = 1.3
        let w = store.brain_feedback_weight("flow_browser");
        assert!((w - 1.3).abs() < 0.01, "weight was {w}");
    }

    #[test]
    fn brain_feedback_weight_low_accuracy() {
        let store = open_temp();
        // All wrong — 0% accuracy → weight = 0.5
        for _ in 0..5 {
            store.insert_brain_feedback("bad_signal", false, None, None, None, "");
        }
        let w = store.brain_feedback_weight("bad_signal");
        assert!((w - 0.5).abs() < 0.01, "weight was {w}");
    }

    #[test]
    fn brain_feedback_recent() {
        let store = open_temp();
        store.insert_brain_feedback("a", true, None, None, None, "");
        store.insert_brain_feedback("b", false, None, None, None, "");
        let recent = store.brain_feedback_recent(10);
        assert_eq!(recent.len(), 2);
        // Both may have same timestamp, just verify both are returned
        let insights: Vec<&str> = recent.iter().map(|r| r.insight.as_str()).collect();
        assert!(insights.contains(&"a"));
        assert!(insights.contains(&"b"));
    }

    // ── Flow state with browser integration ─────────────────────────────

    #[test]
    fn flow_state_penalized_by_tab_switching() {
        let store = open_temp();
        let now = 100_000u64;
        // Create file activity for base flow score
        store.insert_file_interaction("/a.rs", "code", "proj", "rust", "", "", now - 200, Some(80.0), None);
        store.insert_file_interaction("/a.rs", "code", "proj", "rust", "", "", now - 100, Some(80.0), None);

        // Base flow score without browser data
        let base = store.flow_state_now(300);

        // Now add rapid tab switching (>4/min)
        for i in 0..30 {
            store.insert_browser_activity_json(
                &browser_event("tab_switch", "various.com"),
                now - 250 + i * 5,
                None,
                None,
            );
        }

        let after = store.flow_state_now(300);
        assert!(
            after.score < base.score,
            "flow score should decrease with tab switching: base={}, after={}",
            base.score,
            after.score
        );
        assert!(!after.in_flow, "should not be in flow with rapid tab switching");
    }

    #[test]
    fn struggle_prediction_uses_browser_search() {
        let store = open_temp();
        let now = 100_000u64;
        // Add file interactions
        store.insert_file_interaction("/bug.rs", "code", "proj", "rust", "", "", now - 500, Some(30.0), None);

        // Add search refinements (indicating stuck)
        for i in 0..5 {
            store.insert_browser_activity_json(
                &browser_event_full(
                    "search_pattern",
                    "google.com",
                    "development",
                    "text",
                    None,
                    serde_json::json!({}),
                ),
                now - 400 + i * 50,
                Some(25.0),
                None,
            );
        }
        // Add revisits
        for i in 0..4 {
            store.insert_browser_activity_json(
                &browser_event("revisit", "stackoverflow.com"),
                now - 300 + i * 60,
                Some(30.0),
                None,
            );
        }

        let prediction = store.predict_struggle(600);
        assert!(
            prediction.score > 0.0,
            "struggle score should incorporate browser signals: {}",
            prediction.score
        );
    }

    // ── Weekly digest with browser data ─────────────────────────────────

    #[test]
    fn weekly_digest_includes_browser_stats() {
        let store = open_temp();
        let week_start = 100_000u64;
        // Add some file activity
        store.insert_file_interaction("/a.rs", "code", "proj", "rust", "", "", week_start + 100, None, None);
        // Add browser events
        for i in 0..10 {
            store.insert_browser_activity_json(
                &browser_event_full(
                    "tab_switch",
                    "github.com",
                    "development",
                    "code",
                    None,
                    serde_json::json!({"reading_time_secs": 60}),
                ),
                week_start + 200 + i,
                None,
                None,
            );
        }
        store.insert_browser_activity_json(
            &browser_event_full(
                "media_state",
                "youtube.com",
                "media",
                "video",
                None,
                serde_json::json!({"video_watched_secs": 300}),
            ),
            week_start + 500,
            None,
            None,
        );

        let digest = store.weekly_digest(week_start);
        assert!(digest.browser_events >= 11);
        assert!(digest.browser_top_domains.len() >= 1);
        assert_eq!(digest.browser_video_watched_secs, 300);
    }

    // ── Edge cases: empty tables ────────────────────────────────────────

    #[test]
    fn browser_focus_by_domain_empty_table() {
        let store = open_temp();
        let result = store.browser_focus_by_domain(0, 10);
        assert!(result.is_empty());
    }

    #[test]
    fn browser_distraction_score_empty_table() {
        let store = open_temp();
        let score = store.browser_distraction_score(300);
        assert!((score.score - 0.0).abs() < 0.01);
    }

    #[test]
    fn browser_learning_efficiency_empty_table() {
        let store = open_temp();
        assert!(store.browser_learning_efficiency(0).is_empty());
    }

    #[test]
    fn browser_all_null_eeg_returns_empty() {
        let store = open_temp();
        // Insert events with NULL eeg_focus
        for i in 0..5 {
            store.insert_browser_activity_json(&browser_event("tab_switch", "example.com"), 1000 + i, None, None);
        }
        // Focus-by-domain requires eeg_focus IS NOT NULL → should return empty
        let focus = store.browser_focus_by_domain(0, 10);
        assert!(focus.is_empty());
    }

    // ── New column storage verification ─────────────────────────────────

    #[test]
    fn browser_stores_llm_provider() {
        let store = open_temp();
        store.insert_browser_activity_json(
            &serde_json::json!({
                "type": "llm_interaction",
                "domain": "claude.ai",
                "llm_provider": "claude",
                "llm_turn_count": 5,
                "content_type": "chat",
            }),
            1000,
            Some(70.0),
            None,
        );
        let recent = store.get_recent_browser_activities(1, 0);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].event_type, "llm_interaction");
        assert_eq!(recent[0].domain, "claude.ai");
    }

    #[test]
    fn browser_stores_scroll_dynamics() {
        let store = open_temp();
        store.insert_browser_activity_json(
            &serde_json::json!({
                "type": "scroll",
                "domain": "docs.rs",
                "scroll_depth": 0.85,
                "scroll_speed": 200,
                "scroll_direction": "down",
                "scroll_reversals": 3,
            }),
            1000,
            None,
            None,
        );
        let recent = store.get_recent_browser_activities(1, 0);
        assert_eq!(recent.len(), 1);
        assert!((recent[0].scroll_depth.unwrap() - 0.85).abs() < 0.01);
    }

    #[test]
    fn browser_stores_visible_text() {
        let store = open_temp();
        store.insert_browser_activity_json(
            &serde_json::json!({
                "type": "visible_context",
                "domain": "docs.rs",
                "page_title": "tokio::runtime",
                "heading": "Runtime Configuration",
                "visible_text": "The runtime is the core of the async system...",
                "content_type": "code",
            }),
            1000,
            None,
            None,
        );
        let recent = store.get_recent_browser_activities(1, 0);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].event_type, "visible_context");
    }

    // ── Feedback weight applied correctly ────────────────────────────────

    #[test]
    fn feedback_weight_reduces_browser_penalty_in_flow() {
        let store = open_temp();
        let now = now_secs();

        // File activity for base flow score
        store.insert_file_interaction("/a.rs", "code", "proj", "rust", "", "", now - 200, Some(80.0), None);
        store.insert_file_interaction("/a.rs", "code", "proj", "rust", "", "", now - 100, Some(80.0), None);

        // Rapid tab switching
        for i in 0..30 {
            store.insert_browser_activity_json(
                &browser_event("tab_switch", "various.com"),
                now - 250 + i * 5,
                None,
                None,
            );
        }

        // Flow with default weight (1.0)
        let base = store.flow_state_now(300);

        // Now add feedback saying browser signal is wrong (low accuracy → lower weight)
        for _ in 0..5 {
            store.insert_brain_feedback("flow_browser", false, None, None, None, "");
        }

        // Flow with reduced weight (0.5)
        let after = store.flow_state_now(300);
        // The browser penalty should be reduced → higher score
        assert!(
            after.score >= base.score,
            "score should be >= with low-accuracy feedback: base={}, after={}",
            base.score,
            after.score
        );
    }

    // ── DB migration verification ───────────────────────────────────────

    #[test]
    fn db_migration_adds_new_columns() {
        // Create a store (runs DDL + ALTERs), verify new columns exist by inserting data
        let store = open_temp();
        store.insert_browser_activity_json(
            &serde_json::json!({
                "type": "llm_interaction",
                "domain": "claude.ai",
                "llm_provider": "claude",
                "llm_turn_count": 10,
                "email_mode": "",
                "scroll_speed": 150,
                "scroll_direction": "down",
                "scroll_reversals": 2,
                "visible_text": "some text",
                "heading": "Section Title",
                "page_title": "Page Title",
                "download_type": "pdf",
                "revisit_count": 3,
                "domain_visit_count": 7,
                "video_playback_rate": 1.5,
            }),
            1000,
            None,
            None,
        );
        // If any column is missing, the INSERT would fail
        let recent = store.get_recent_browser_activities(1, 0);
        assert_eq!(recent.len(), 1);
    }
}
