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

use rusqlite::{Connection, params};
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
            Ok(c)  => c,
            Err(e) => { eprintln!("[activity] open {}: {e}", path.display()); return None; }
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
    pub fn insert_input_activity(
        &self,
        last_keyboard: Option<u64>,
        last_mouse:    Option<u64>,
        sampled_at:    u64,
    ) {
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
    pub fn upsert_input_bucket(
        &self,
        minute_ts:   u64,
        key_delta:   u64,
        mouse_delta: u64,
    ) {
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
            params![
                minute_ts   as i64,
                key_delta   as i64,
                mouse_delta as i64,
            ],
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
            Ok(s)  => s,
            Err(e) => { eprintln!("[activity] prepare recent_windows: {e}"); return vec![]; }
        };
        stmt.query_map([limit as i64], |row| {
            Ok(ActiveWindowRow {
                id:           row.get(0)?,
                app_name:     row.get(1)?,
                app_path:     row.get(2)?,
                window_title: row.get(3)?,
                activated_at: row.get::<_, i64>(4)? as u64,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Return the `limit` most recent input-activity samples, newest first.
    pub fn get_recent_input(&self, limit: u32) -> Vec<InputActivityRow> {
        let c = self.conn.lock_or_recover();
        let mut stmt = match c.prepare_cached(
            "SELECT id, last_keyboard, last_mouse, sampled_at
             FROM input_activity ORDER BY sampled_at DESC LIMIT ?1",
        ) {
            Ok(s)  => s,
            Err(e) => { eprintln!("[activity] prepare recent_input: {e}"); return vec![]; }
        };
        stmt.query_map([limit as i64], |row| {
            Ok(InputActivityRow {
                id:            row.get(0)?,
                last_keyboard: row.get::<_, Option<i64>>(1)?.map(|t| t as u64),
                last_mouse:    row.get::<_, Option<i64>>(2)?.map(|t| t as u64),
                sampled_at:    row.get::<_, i64>(3)? as u64,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
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
            Ok(s)  => s,
            Err(e) => { eprintln!("[activity] prepare get_input_buckets: {e}"); return vec![]; }
        };
        stmt.query_map(
            params![from_ts as i64, to_ts as i64],
            |row| Ok(InputBucketRow {
                minute_ts:   row.get::<_, i64>(0)? as u64,
                key_count:   row.get::<_, i64>(1)? as u64,
                mouse_count: row.get::<_, i64>(2)? as u64,
            }),
        )
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}

// ── Row types (returned to the frontend) ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveWindowRow {
    pub id:           i64,
    pub app_name:     String,
    pub app_path:     String,
    pub window_title: String,
    pub activated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputActivityRow {
    pub id:            i64,
    /// Unix seconds of the last keyboard event in this sampling window; `None` if absent.
    pub last_keyboard: Option<u64>,
    /// Unix seconds of the last mouse event in this sampling window; `None` if absent.
    pub last_mouse:    Option<u64>,
    /// Unix seconds when this row was written (flush time).
    pub sampled_at:    u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputBucketRow {
    /// Unix timestamp of the start of this minute (always divisible by 60).
    pub minute_ts:   u64,
    /// Total keyboard events recorded in this minute.
    pub key_count:   u64,
    /// Total mouse / scroll / click events recorded in this minute.
    pub mouse_count: u64,
}

// ── Tests ──────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_temp() -> ActivityStore {
        let dir = tempdir().unwrap();
        ActivityStore::open(dir.path()).unwrap()
    }

    fn dummy_window(ts: u64) -> ActiveWindowInfo {
        ActiveWindowInfo {
            app_name:     "TestApp".into(),
            app_path:     "/usr/bin/test".into(),
            window_title: "Test Window".into(),
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
        for i in 0..10u64 { store.insert_active_window(&dummy_window(i)); }
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
        for i in 0..10u64 { store.insert_input_activity(Some(i), Some(i), i); }
        assert_eq!(store.get_recent_input(4).len(), 4);
    }

    #[test]
    fn upsert_bucket_creates_and_increments() {
        let store = open_temp();
        let min = 1_000 * 60; // a round minute timestamp
        store.upsert_input_bucket(min, 10, 5);
        store.upsert_input_bucket(min, 3, 2);  // second flush in same minute
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
}
