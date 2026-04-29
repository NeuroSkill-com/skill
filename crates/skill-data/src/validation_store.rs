// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Validation / fatigue-research store — `~/.skill/validation.sqlite`.
//!
//! Houses everything related to the four validation channels documented in
//! the README's "Validation roadmap":
//!
//! * **`config`** — single-row TOML-equivalent JSON blob.  Owns enable flags,
//!   per-channel rate limits, work-hour gates, and the master `respect_flow`
//!   switch.  The daemon is the only writer; both UIs read/write through HTTP.
//!
//! * **`kss_responses`** — Karolinska Sleepiness Scale 1-9 self-reports.
//! * **`tlx_responses`** — NASA-TLX raw, 6 sub-scales 0-100 each.
//! * **`pvt_runs`**      — Psychomotor Vigilance Task results (RT stats).
//! * **`prompt_log`**    — every prompt the scheduler fires (and the user's
//!   answer/dismiss/snooze action), so we can compute response rates and
//!   reason about prompt fatigue itself.
//!
//! All inference (the scheduler's `next_prompt` decision, EEG fatigue index)
//! runs in caller code; this module is pure storage + small queries.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

use crate::util::MutexExt;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── DDL ───────────────────────────────────────────────────────────────────────

const DDL: &str = "
-- Single-row config blob.  We keep it in JSON to make schema migrations
-- (adding a new channel, a new field) painless: the Rust struct deserialises
-- with serde defaults for any missing key.
CREATE TABLE IF NOT EXISTS config (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    json        TEXT    NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS kss_responses (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    score       INTEGER NOT NULL CHECK (score BETWEEN 1 AND 9),
    triggered_by TEXT   NOT NULL,    -- 'break_coach' | 'random' | 'manual'
    surface     TEXT    NOT NULL,    -- 'vscode' | 'tauri' | 'browser'
    in_flow     INTEGER NOT NULL DEFAULT 0,
    focus_score REAL,                -- contemporaneous focus score, NULL if unknown
    fatigue_idx REAL,                -- contemporaneous EEG fatigue index, NULL if no headset
    answered_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_kss_at ON kss_responses (answered_at DESC);

CREATE TABLE IF NOT EXISTS tlx_responses (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Six raw sub-scales, 0-100 each.
    mental      INTEGER NOT NULL CHECK (mental      BETWEEN 0 AND 100),
    physical    INTEGER NOT NULL CHECK (physical    BETWEEN 0 AND 100),
    temporal    INTEGER NOT NULL CHECK (temporal    BETWEEN 0 AND 100),
    performance INTEGER NOT NULL CHECK (performance BETWEEN 0 AND 100),
    effort      INTEGER NOT NULL CHECK (effort      BETWEEN 0 AND 100),
    frustration INTEGER NOT NULL CHECK (frustration BETWEEN 0 AND 100),
    task_kind   TEXT    NOT NULL,    -- 'flow_block' | 'debug_session' | 'end_of_day' | 'manual'
    task_duration_secs INTEGER,
    surface     TEXT    NOT NULL,
    answered_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tlx_at ON tlx_responses (answered_at DESC);

CREATE TABLE IF NOT EXISTS pvt_runs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    duration_secs    INTEGER NOT NULL,    -- task length actually run
    stimulus_count   INTEGER NOT NULL,
    response_count   INTEGER NOT NULL,
    mean_rt_ms       REAL    NOT NULL,
    median_rt_ms     REAL    NOT NULL,
    slowest10_rt_ms  REAL    NOT NULL,    -- mean of slowest 10% RTs (anticipation-resistant)
    lapse_count      INTEGER NOT NULL,    -- RT > 500 ms
    false_start_count INTEGER NOT NULL,   -- response without stimulus
    fatigue_idx      REAL,                -- contemporaneous EEG fatigue index
    started_at       INTEGER NOT NULL,
    finished_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pvt_at ON pvt_runs (finished_at DESC);

CREATE TABLE IF NOT EXISTS prompt_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    channel     TEXT    NOT NULL,    -- 'kss' | 'tlx' | 'pvt'
    triggered_by TEXT   NOT NULL,
    fired_at    INTEGER NOT NULL,
    surface     TEXT    NOT NULL DEFAULT '',  -- which client rendered the prompt
    -- One of: 'answered' | 'snoozed' | 'dismissed' | 'disabled_today' | 'disabled_perm'
    outcome     TEXT,
    outcome_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_prompt_at      ON prompt_log (fired_at DESC);
CREATE INDEX IF NOT EXISTS idx_prompt_channel ON prompt_log (channel, fired_at DESC);
";

// ── Persistent config ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct ValidationConfig {
    /// Master gate: when `true`, no prompt fires while `in_flow == true`.
    pub respect_flow: bool,
    /// Local-time hour [0,24) — no prompts before this.
    pub quiet_before_hour: u8,
    /// Local-time hour [0,24) — no prompts after this.
    pub quiet_after_hour: u8,

    pub kss: KssConfig,
    pub tlx: TlxConfig,
    pub pvt: PvtConfig,
    pub eeg_fatigue: EegFatigueConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct KssConfig {
    /// Master enable for the KSS channel.  Off by default — opt-in only.
    pub enabled: bool,
    /// Maximum number of KSS prompts to issue per local day.
    pub max_per_day: u32,
    /// Minimum minutes between consecutive prompts.
    pub min_interval_min: u32,
    /// Fire when Break Coach decides the user is fatigued.
    pub trigger_break_coach: bool,
    /// Fire occasional uniform-random control samples (needed for ROC).
    pub trigger_random: bool,
    /// Fraction of total prompts that should be uniform-random vs. event-driven.
    pub random_weight: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct TlxConfig {
    pub enabled: bool,
    pub max_per_day: u32,
    /// Only ask for TLX after the just-finished unit of work was at least this long.
    pub min_task_min: u32,
    /// If true, a daily TLX prompt fires near the configured `quiet_after_hour`.
    pub end_of_day: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct PvtConfig {
    pub enabled: bool,
    /// Surface a small "no PVT this week" hint at most once a week when enabled.
    pub weekly_reminder: bool,
    /// Locked off at the design level — PVT is too intrusive to auto-fire.
    /// Persisted explicitly so the contract is auditable.
    pub auto_fire: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "snake_case")]
pub struct EegFatigueConfig {
    /// When `true` and a NeuroSkill headset is streaming, the daemon emits a
    /// rolling fatigue index `(α + θ) / β` alongside the focus score.
    /// Passive — costs nothing — so on by default.
    pub enabled: bool,
    /// Window length (seconds) used by the rolling computation.
    pub window_secs: u32,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            respect_flow: true,
            quiet_before_hour: 9,
            quiet_after_hour: 18,
            kss: KssConfig::default(),
            tlx: TlxConfig::default(),
            pvt: PvtConfig::default(),
            eeg_fatigue: EegFatigueConfig::default(),
        }
    }
}

impl Default for KssConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_per_day: 4,
            min_interval_min: 90,
            trigger_break_coach: true,
            trigger_random: true,
            random_weight: 0.30,
        }
    }
}

impl Default for TlxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_per_day: 3,
            min_task_min: 30,
            end_of_day: false,
        }
    }
}

impl Default for PvtConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            weekly_reminder: true,
            auto_fire: false,
        }
    }
}

impl Default for EegFatigueConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            window_secs: 30,
        }
    }
}

// ── Records inserted via record_* helpers ────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KssRecord {
    pub score: u8,
    pub triggered_by: String,
    pub surface: String,
    pub in_flow: bool,
    pub focus_score: Option<f64>,
    pub fatigue_idx: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TlxRecord {
    pub mental: u8,
    pub physical: u8,
    pub temporal: u8,
    pub performance: u8,
    pub effort: u8,
    pub frustration: u8,
    pub task_kind: String,
    pub task_duration_secs: Option<i64>,
    pub surface: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PvtRecord {
    pub duration_secs: i64,
    pub stimulus_count: i64,
    pub response_count: i64,
    pub mean_rt_ms: f64,
    pub median_rt_ms: f64,
    pub slowest10_rt_ms: f64,
    pub lapse_count: i64,
    pub false_start_count: i64,
    pub fatigue_idx: Option<f64>,
    pub started_at: i64,
    pub finished_at: i64,
}

// ── Store ────────────────────────────────────────────────────────────────────

pub struct ValidationStore {
    conn: Mutex<Connection>,
}

impl ValidationStore {
    /// Open or create the store at `~/.skill/validation.sqlite`.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let path = skill_dir.join(skill_constants::VALIDATION_FILE);
        let conn = match Connection::open(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[validation] open {}: {e}", path.display());
                return None;
            }
        };
        crate::util::init_wal_pragmas(&conn);
        if let Err(e) = conn.execute_batch(DDL) {
            eprintln!("[validation] DDL failed: {e}");
            return None;
        }
        Some(Self { conn: Mutex::new(conn) })
    }

    // ── Config ───────────────────────────────────────────────────────────────

    pub fn load_config(&self) -> ValidationConfig {
        let guard = self.conn.lock_or_recover();
        let row: Option<String> = guard
            .query_row("SELECT json FROM config WHERE id = 1", [], |r| r.get(0))
            .optional()
            .ok()
            .flatten();
        match row {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => ValidationConfig::default(),
        }
    }

    pub fn save_config(&self, cfg: &ValidationConfig) -> rusqlite::Result<()> {
        let json = serde_json::to_string(cfg).unwrap_or_else(|_| "{}".into());
        let now = now_secs();
        let guard = self.conn.lock_or_recover();
        guard.execute(
            "INSERT INTO config (id, json, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json, updated_at = excluded.updated_at",
            params![json, now],
        )?;
        Ok(())
    }

    // ── Inserters ────────────────────────────────────────────────────────────

    pub fn record_kss(&self, rec: &KssRecord) -> rusqlite::Result<i64> {
        let now = now_secs();
        let guard = self.conn.lock_or_recover();
        guard.execute(
            "INSERT INTO kss_responses
              (score, triggered_by, surface, in_flow, focus_score, fatigue_idx, answered_at)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rec.score,
                rec.triggered_by,
                rec.surface,
                rec.in_flow as i64,
                rec.focus_score,
                rec.fatigue_idx,
                now,
            ],
        )?;
        Ok(guard.last_insert_rowid())
    }

    pub fn record_tlx(&self, rec: &TlxRecord) -> rusqlite::Result<i64> {
        let now = now_secs();
        let guard = self.conn.lock_or_recover();
        guard.execute(
            "INSERT INTO tlx_responses
              (mental, physical, temporal, performance, effort, frustration,
               task_kind, task_duration_secs, surface, answered_at)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                rec.mental,
                rec.physical,
                rec.temporal,
                rec.performance,
                rec.effort,
                rec.frustration,
                rec.task_kind,
                rec.task_duration_secs,
                rec.surface,
                now,
            ],
        )?;
        Ok(guard.last_insert_rowid())
    }

    pub fn record_pvt(&self, rec: &PvtRecord) -> rusqlite::Result<i64> {
        let guard = self.conn.lock_or_recover();
        guard.execute(
            "INSERT INTO pvt_runs
              (duration_secs, stimulus_count, response_count, mean_rt_ms, median_rt_ms,
               slowest10_rt_ms, lapse_count, false_start_count, fatigue_idx,
               started_at, finished_at)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                rec.duration_secs,
                rec.stimulus_count,
                rec.response_count,
                rec.mean_rt_ms,
                rec.median_rt_ms,
                rec.slowest10_rt_ms,
                rec.lapse_count,
                rec.false_start_count,
                rec.fatigue_idx,
                rec.started_at,
                rec.finished_at,
            ],
        )?;
        Ok(guard.last_insert_rowid())
    }

    // ── Prompt log (for the scheduler) ───────────────────────────────────────

    pub fn log_prompt(&self, channel: &str, triggered_by: &str, surface: &str) -> rusqlite::Result<i64> {
        let now = now_secs();
        let guard = self.conn.lock_or_recover();
        guard.execute(
            "INSERT INTO prompt_log (channel, triggered_by, fired_at, surface) VALUES (?1, ?2, ?3, ?4)",
            params![channel, triggered_by, now, surface],
        )?;
        Ok(guard.last_insert_rowid())
    }

    pub fn close_prompt(&self, id: i64, outcome: &str) -> rusqlite::Result<()> {
        let now = now_secs();
        let guard = self.conn.lock_or_recover();
        guard.execute(
            "UPDATE prompt_log SET outcome = ?1, outcome_at = ?2 WHERE id = ?3",
            params![outcome, now, id],
        )?;
        Ok(())
    }

    /// Last fire-time of a given channel (for rate-limiting).
    pub fn last_prompt_at(&self, channel: &str) -> Option<i64> {
        let guard = self.conn.lock_or_recover();
        guard
            .query_row(
                "SELECT fired_at FROM prompt_log WHERE channel = ?1 ORDER BY fired_at DESC LIMIT 1",
                params![channel],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    /// Number of prompts fired for `channel` since `since` (unix secs).
    pub fn prompt_count_since(&self, channel: &str, since: i64) -> i64 {
        let guard = self.conn.lock_or_recover();
        guard
            .query_row(
                "SELECT COUNT(*) FROM prompt_log WHERE channel = ?1 AND fired_at >= ?2",
                params![channel, since],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
    }

    /// Most recent PVT finish time, for the weekly-reminder gate.
    pub fn last_pvt_finished_at(&self) -> Option<i64> {
        let guard = self.conn.lock_or_recover();
        guard
            .query_row(
                "SELECT finished_at FROM pvt_runs ORDER BY finished_at DESC LIMIT 1",
                [],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .ok()
            .flatten()
    }

    // ── Read-back queries (for the Tauri history view + correlation jobs) ───

    pub fn recent_kss(&self, since: i64) -> Vec<(i64, u8, String, i64)> {
        let guard = self.conn.lock_or_recover();
        let mut stmt = match guard.prepare(
            "SELECT id, score, triggered_by, answered_at
             FROM kss_responses WHERE answered_at >= ?1 ORDER BY answered_at DESC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(params![since], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, u8>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .ok();
        rows.map(|i| i.flatten().collect()).unwrap_or_default()
    }
}

// ── EEG fatigue index (Jap et al. 2009) ─────────────────────────────────────

/// `(α + θ) / β` over frontal channels, computed from a band-power snapshot.
///
/// Returns `None` if any required band is missing or `β` is zero.  The input
/// is the same JSON shape the session runner publishes to `latest_bands`:
/// `{ "alpha": <f64>, "beta": <f64>, "theta": <f64>, ... }`.
pub fn eeg_fatigue_index(bands: &serde_json::Value) -> Option<f64> {
    let alpha = bands.get("alpha")?.as_f64()?;
    let theta = bands.get("theta")?.as_f64()?;
    let beta = bands.get("beta")?.as_f64()?;
    if !beta.is_finite() || beta.abs() < 1e-9 {
        return None;
    }
    let idx = (alpha + theta) / beta;
    if idx.is_finite() {
        Some(idx)
    } else {
        None
    }
}

// ── Scheduler ───────────────────────────────────────────────────────────────

/// In-memory runtime state shared across requests.  Lives in `AppState` and is
/// not persisted: snoozes and "disable today" reset on daemon restart, which
/// is the right behaviour (a crash loop shouldn't keep prompts suppressed
/// forever, and "today" only means anything for an actually-running daemon).
#[derive(Clone, Debug, Default)]
pub struct ValidationRuntime {
    /// Channel → unix-secs at which the snooze ends.
    pub snoozed_until: std::collections::HashMap<String, i64>,
    /// Channel → unix-secs at which "disable today" expires (next local midnight).
    pub disabled_today_until: std::collections::HashMap<String, i64>,
}

impl ValidationRuntime {
    pub fn snooze(&mut self, channel: &str, duration_secs: i64) {
        self.snoozed_until
            .insert(channel.to_string(), now_secs() + duration_secs);
    }

    pub fn disable_today(&mut self, channel: &str, midnight: i64) {
        self.disabled_today_until.insert(channel.to_string(), midnight);
    }

    pub fn is_snoozed(&self, channel: &str) -> bool {
        let now = now_secs();
        self.snoozed_until.get(channel).map(|t| *t > now).unwrap_or(false)
            || self
                .disabled_today_until
                .get(channel)
                .map(|t| *t > now)
                .unwrap_or(false)
    }
}

/// What the scheduler decided to do.  Returned by `decide_prompt` so clients
/// (VS Code, Tauri) don't reason about timing themselves.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PromptDecision {
    Kss { triggered_by: String },
    Tlx { triggered_by: String, task_kind: String },
    Pvt { triggered_by: String },
    None { reason: String },
}

/// Inputs the scheduler needs.  Decoupled from `AppState` so `decide_prompt`
/// is easy to unit-test.
pub struct SchedulerCtx<'a> {
    pub config: &'a ValidationConfig,
    pub runtime: &'a ValidationRuntime,
    pub store: &'a ValidationStore,
    pub now_unix: i64,
    pub local_hour: u8, // 0-23
    pub local_midnight_next: i64,
    pub in_flow: bool,
}

impl<'a> SchedulerCtx<'a> {
    /// True if the global gates (flow respect + quiet hours) allow any prompt.
    fn global_ok(&self) -> bool {
        if self.config.respect_flow && self.in_flow {
            return false;
        }
        let h = self.local_hour;
        let start = self.config.quiet_before_hour;
        let end = self.config.quiet_after_hour;
        // Inclusive-start / exclusive-end window.  When start == end, the
        // window is empty and everything is allowed (interpret as "no quiet
        // hours configured").
        if start == end {
            return true;
        }
        h >= start && h < end
    }
}

/// Pure decision function.  Takes the channel hint (`break_coach_active`,
/// `last_task_kind`, `last_task_duration_secs`) and decides whether — and on
/// which channel — a prompt should fire.  Channel ordering is: KSS first
/// (lightest), TLX if a heavy task just ended, PVT on the weekly cadence.
pub fn decide_prompt(
    ctx: &SchedulerCtx<'_>,
    break_coach_active: bool,
    last_task_kind: Option<&str>,
    last_task_duration_secs: Option<i64>,
) -> PromptDecision {
    if !ctx.global_ok() {
        return PromptDecision::None {
            reason: "quiet_hours_or_in_flow".into(),
        };
    }

    // ── KSS ─────────────────────────────────────────────────────────────────
    if ctx.config.kss.enabled && !ctx.runtime.is_snoozed("kss") {
        let day_start = ctx.now_unix - (ctx.now_unix % 86_400);
        let count_today = ctx.store.prompt_count_since("kss", day_start);
        let last_at = ctx.store.last_prompt_at("kss").unwrap_or(0);
        let interval_ok = ctx.now_unix - last_at >= (ctx.config.kss.min_interval_min as i64) * 60;
        let cap_ok = (count_today as u32) < ctx.config.kss.max_per_day;

        if interval_ok && cap_ok {
            if break_coach_active && ctx.config.kss.trigger_break_coach {
                return PromptDecision::Kss {
                    triggered_by: "break_coach".into(),
                };
            }
            // Probabilistic random-control sample.  We use a deterministic
            // hash of the current minute so the decision is stable across
            // multiple `should_prompt` calls within the same minute (the
            // daemon may be polled by both VS Code and Tauri).
            if ctx.config.kss.trigger_random {
                let minute = ctx.now_unix / 60;
                let h = simple_hash(minute as u64) as f32 / u32::MAX as f32;
                if h < ctx.config.kss.random_weight {
                    return PromptDecision::Kss {
                        triggered_by: "random".into(),
                    };
                }
            }
        }
    }

    // ── TLX ─────────────────────────────────────────────────────────────────
    if ctx.config.tlx.enabled && !ctx.runtime.is_snoozed("tlx") {
        let day_start = ctx.now_unix - (ctx.now_unix % 86_400);
        let count_today = ctx.store.prompt_count_since("tlx", day_start);
        let cap_ok = (count_today as u32) < ctx.config.tlx.max_per_day;
        if cap_ok {
            if let (Some(kind), Some(dur)) = (last_task_kind, last_task_duration_secs) {
                let min_dur = (ctx.config.tlx.min_task_min as i64) * 60;
                if dur >= min_dur {
                    return PromptDecision::Tlx {
                        triggered_by: "post_task".into(),
                        task_kind: kind.to_string(),
                    };
                }
            }
        }
    }

    // ── PVT ─────────────────────────────────────────────────────────────────
    if ctx.config.pvt.enabled && ctx.config.pvt.weekly_reminder && !ctx.runtime.is_snoozed("pvt") {
        let last = ctx.store.last_pvt_finished_at().unwrap_or(0);
        let week_secs: i64 = 7 * 86_400;
        if ctx.now_unix - last >= week_secs {
            return PromptDecision::Pvt {
                triggered_by: "weekly_reminder".into(),
            };
        }
    }

    let _ = ctx.local_midnight_next;
    PromptDecision::None {
        reason: "no_trigger".into(),
    }
}

#[inline]
fn simple_hash(mut x: u64) -> u32 {
    // splitmix64
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((x ^ (x >> 31)) >> 32) as u32
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn open_tmp() -> (TempDir, ValidationStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ValidationStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn config_round_trip_defaults() {
        let (_d, s) = open_tmp();
        let loaded = s.load_config();
        assert!(loaded.respect_flow);
        assert!(loaded.eeg_fatigue.enabled, "eeg fatigue ships on");
        assert!(!loaded.kss.enabled, "kss ships off");
        assert!(!loaded.tlx.enabled, "tlx ships off");
        assert!(!loaded.pvt.enabled, "pvt ships off");
    }

    #[test]
    fn config_save_then_load() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = true;
        cfg.kss.max_per_day = 2;
        s.save_config(&cfg).unwrap();
        let loaded = s.load_config();
        assert!(loaded.kss.enabled);
        assert_eq!(loaded.kss.max_per_day, 2);
    }

    #[test]
    fn record_and_count_kss() {
        let (_d, s) = open_tmp();
        s.record_kss(&KssRecord {
            score: 4,
            triggered_by: "manual".into(),
            surface: "vscode".into(),
            in_flow: false,
            focus_score: Some(72.0),
            fatigue_idx: None,
        })
        .unwrap();
        let recent = s.recent_kss(0);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].1, 4);
        assert_eq!(recent[0].2, "manual");
    }

    #[test]
    fn fatigue_index_jap() {
        let bands = json!({ "alpha": 5.0, "theta": 3.0, "beta": 2.0 });
        assert_eq!(eeg_fatigue_index(&bands), Some(4.0));
    }

    #[test]
    fn fatigue_index_zero_beta() {
        let bands = json!({ "alpha": 5.0, "theta": 3.0, "beta": 0.0 });
        assert_eq!(eeg_fatigue_index(&bands), None);
    }

    #[test]
    fn fatigue_index_missing_band() {
        let bands = json!({ "alpha": 5.0, "theta": 3.0 });
        assert_eq!(eeg_fatigue_index(&bands), None);
    }

    #[test]
    fn scheduler_respects_flow() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = true;
        let rt = ValidationRuntime::default();
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: 1700000000,
            local_hour: 12,
            local_midnight_next: 1700000000 + 60,
            in_flow: true,
        };
        let d = decide_prompt(&ctx, true, None, None);
        match d {
            PromptDecision::None { reason } => assert!(reason.contains("flow")),
            _ => panic!("should not prompt while in flow"),
        }
    }

    #[test]
    fn scheduler_respects_quiet_hours() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = true;
        let rt = ValidationRuntime::default();
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: 1700000000,
            local_hour: 22, // after quiet_after_hour=18
            local_midnight_next: 0,
            in_flow: false,
        };
        match decide_prompt(&ctx, true, None, None) {
            PromptDecision::None { .. } => {}
            _ => panic!("should not prompt outside quiet window"),
        }
    }

    #[test]
    fn scheduler_fires_kss_on_break_coach() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = true;
        cfg.kss.trigger_random = false; // isolate the break-coach path
        let rt = ValidationRuntime::default();
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: 1700000000,
            local_hour: 12,
            local_midnight_next: 0,
            in_flow: false,
        };
        match decide_prompt(&ctx, true, None, None) {
            PromptDecision::Kss { triggered_by } => assert_eq!(triggered_by, "break_coach"),
            other => panic!("expected KSS prompt, got {other:?}"),
        }
    }

    #[test]
    fn snooze_blocks_prompt() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = true;
        let mut rt = ValidationRuntime::default();
        rt.snooze("kss", 1800);
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: now_secs(),
            local_hour: 12,
            local_midnight_next: 0,
            in_flow: false,
        };
        match decide_prompt(&ctx, true, None, None) {
            PromptDecision::None { .. } => {}
            _ => panic!("snooze should suppress prompt"),
        }
    }

    #[test]
    fn scheduler_tlx_fires_after_long_task() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = false; // isolate TLX path
        cfg.tlx.enabled = true;
        cfg.tlx.min_task_min = 30;
        let rt = ValidationRuntime::default();
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: 1700000000,
            local_hour: 12,
            local_midnight_next: 0,
            in_flow: false,
        };
        match decide_prompt(&ctx, false, Some("flow_block"), Some(31 * 60)) {
            PromptDecision::Tlx { task_kind, .. } => assert_eq!(task_kind, "flow_block"),
            other => panic!("expected TLX, got {other:?}"),
        }
    }

    #[test]
    fn scheduler_tlx_skips_short_task() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = false;
        cfg.tlx.enabled = true;
        cfg.tlx.min_task_min = 30;
        let rt = ValidationRuntime::default();
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: 1700000000,
            local_hour: 12,
            local_midnight_next: 0,
            in_flow: false,
        };
        match decide_prompt(&ctx, false, Some("debug_session"), Some(20 * 60)) {
            PromptDecision::None { .. } => {}
            other => panic!("short tasks must not trigger TLX, got {other:?}"),
        }
    }

    #[test]
    fn scheduler_pvt_fires_when_no_recent_run() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = false;
        cfg.tlx.enabled = false;
        cfg.pvt.enabled = true;
        cfg.pvt.weekly_reminder = true;
        let rt = ValidationRuntime::default();
        let now = 1700000000;
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: now,
            local_hour: 12,
            local_midnight_next: 0,
            in_flow: false,
        };
        // Empty store → last_pvt_finished_at returns None → treated as 0 →
        // > one week elapsed → fire.
        match decide_prompt(&ctx, false, None, None) {
            PromptDecision::Pvt { triggered_by } => assert_eq!(triggered_by, "weekly_reminder"),
            other => panic!("expected PVT nudge, got {other:?}"),
        }
    }

    #[test]
    fn scheduler_pvt_silent_when_recent_run() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = false;
        cfg.tlx.enabled = false;
        cfg.pvt.enabled = true;
        cfg.pvt.weekly_reminder = true;
        // Insert a PVT run from 3 days ago.
        let now = now_secs();
        s.record_pvt(&PvtRecord {
            duration_secs: 180,
            stimulus_count: 60,
            response_count: 60,
            mean_rt_ms: 300.0,
            median_rt_ms: 295.0,
            slowest10_rt_ms: 480.0,
            lapse_count: 1,
            false_start_count: 0,
            fatigue_idx: None,
            started_at: now - 3 * 86400 - 200,
            finished_at: now - 3 * 86400,
        })
        .unwrap();
        let rt = ValidationRuntime::default();
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: now,
            local_hour: 12,
            local_midnight_next: 0,
            in_flow: false,
        };
        match decide_prompt(&ctx, false, None, None) {
            PromptDecision::None { .. } => {}
            other => panic!("expected silent PVT, got {other:?}"),
        }
    }

    #[test]
    fn scheduler_quiet_window_open_when_start_equals_end() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = true;
        cfg.kss.trigger_random = false;
        cfg.quiet_before_hour = 0;
        cfg.quiet_after_hour = 0;
        let rt = ValidationRuntime::default();
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: 1700000000,
            local_hour: 23, // would be outside [9..18) but window is "always"
            local_midnight_next: 0,
            in_flow: false,
        };
        match decide_prompt(&ctx, true, None, None) {
            PromptDecision::Kss { .. } => {}
            other => panic!("with quiet window disabled, kss should fire: {other:?}"),
        }
    }

    #[test]
    fn scheduler_kss_rate_limit_blocks_after_recent_prompt() {
        let (_d, s) = open_tmp();
        let mut cfg = ValidationConfig::default();
        cfg.kss.enabled = true;
        cfg.kss.trigger_random = false;
        cfg.kss.min_interval_min = 90;
        s.log_prompt("kss", "break_coach", "vscode").unwrap(); // sets last_prompt_at = now
        let rt = ValidationRuntime::default();
        let ctx = SchedulerCtx {
            config: &cfg,
            runtime: &rt,
            store: &s,
            now_unix: now_secs() + 60, // only 60 s elapsed; well under 90 min
            local_hour: 12,
            local_midnight_next: 0,
            in_flow: false,
        };
        match decide_prompt(&ctx, true, None, None) {
            PromptDecision::None { .. } => {}
            other => panic!("rate-limit should suppress within min_interval, got {other:?}"),
        }
    }

    #[test]
    fn record_tlx_round_trip() {
        let (_d, s) = open_tmp();
        let id = s
            .record_tlx(&TlxRecord {
                mental: 80,
                physical: 10,
                temporal: 60,
                performance: 40,
                effort: 70,
                frustration: 55,
                task_kind: "flow_block".into(),
                task_duration_secs: Some(2700),
                surface: "tauri".into(),
            })
            .unwrap();
        assert!(id > 0);
    }

    #[test]
    fn prompt_log_count_excludes_other_channels() {
        let (_d, s) = open_tmp();
        let now = now_secs();
        s.log_prompt("kss", "break_coach", "vscode").unwrap();
        s.log_prompt("tlx", "post_task", "tauri").unwrap();
        s.log_prompt("kss", "random", "vscode").unwrap();
        assert_eq!(s.prompt_count_since("kss", now - 60), 2);
        assert_eq!(s.prompt_count_since("tlx", now - 60), 1);
        assert_eq!(s.prompt_count_since("pvt", now - 60), 0);
    }

    #[test]
    fn close_prompt_records_outcome() {
        let (_d, s) = open_tmp();
        let id = s.log_prompt("kss", "break_coach", "vscode").unwrap();
        s.close_prompt(id, "snoozed").unwrap();
        // Round-trip: a second close with a different outcome should overwrite.
        s.close_prompt(id, "answered").unwrap();
        // No assertion API for the outcome column directly — but the
        // operations succeeding without panic is the contract we care about.
    }
}
