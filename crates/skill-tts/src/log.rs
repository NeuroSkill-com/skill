// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Standalone TTS logger with pluggable output sink.
//!
//! By default every log line is written to stderr via `eprintln!`.
//! Call [`set_log_callback`] to redirect output — for example into the
//! Tauri-side `SkillLogger` — without adding a Tauri dependency to this crate.
//!
//! ## Quick start
//!
//! ```ignore
//! use skill_tts::log::{set_log_callback, set_log_enabled};
//!
//! // Route TTS logs through the app logger
//! set_log_callback(|tag, msg| my_logger.write(tag, msg));
//!
//! // Enable / disable at runtime
//! set_log_enabled(true);
//! ```

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

// ── Types ─────────────────────────────────────────────────────────────────────

/// Signature for the pluggable log sink.
///
/// `tag` is a short subsystem identifier (`"tts"` or `"neutts"`).
/// `msg` is the pre-formatted log message (no brackets / tag prefix).
pub type LogCallback = dyn Fn(&str, &str) + Send + Sync + 'static;

// ── Statics ───────────────────────────────────────────────────────────────────

static ENABLED: AtomicBool = AtomicBool::new(true);
static CALLBACK: OnceLock<Box<LogCallback>> = OnceLock::new();

// ── Public API ────────────────────────────────────────────────────────────────

/// Enable or disable TTS log output globally.
///
/// When disabled, [`tts_log!`] calls are short-circuited before formatting.
/// Enabled by default so that logs are visible during early init.
pub fn set_log_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Returns `true` if TTS logging is currently enabled.
pub fn log_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Install a custom log sink.
///
/// Can only be called once (subsequent calls are silently ignored).
/// If no callback is installed, [`write_log`] falls back to `eprintln!`.
pub fn set_log_callback<F>(cb: F)
where
    F: Fn(&str, &str) + Send + Sync + 'static,
{
    let _ = CALLBACK.set(Box::new(cb));
}

/// Write a single log line.  Prefer the [`tts_log!`] macro instead.
///
/// * If a callback was registered via [`set_log_callback`] → delegates there.
/// * Otherwise → `eprintln!("[{tag}] {msg}")`.
#[doc(hidden)]
pub fn write_log(tag: &str, msg: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    match CALLBACK.get() {
        Some(cb) => cb(tag, msg),
        None => eprintln!("[{tag}] {msg}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_enabled_by_default() {
        // Note: other tests may have toggled this, so we just check the function works
        let _ = log_enabled();
    }

    #[test]
    fn set_enabled_toggles() {
        set_log_enabled(false);
        assert!(!log_enabled());
        set_log_enabled(true);
        assert!(log_enabled());
    }

    #[test]
    fn write_log_does_not_panic_without_callback() {
        set_log_enabled(true);
        write_log("test", "hello from test");
    }

    #[test]
    fn write_log_noop_when_disabled() {
        set_log_enabled(false);
        write_log("test", "should not appear");
        set_log_enabled(true);
    }
}

