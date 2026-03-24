// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Standalone tool-call logger with pluggable output sink.
//!
//! By default every log line is written to stderr via `eprintln!`.
//! Call [`set_log_callback`] to redirect output \u2014 for example into the
//! Tauri-side `SkillLogger` \u2014 without adding a Tauri dependency to this crate.
//!
//! ## Quick start
//!
//! ```ignore
//! use skill_tools::log::{set_log_callback, set_log_enabled};
//!
//! // Route tool-call logs through the app logger
//! set_log_callback(|tag, msg| my_logger.write(tag, msg));
//!
//! // Enable / disable at runtime
//! set_log_enabled(true);
//! ```

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

// \u2500\u2500 Types \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

/// Signature for the pluggable log sink.
///
/// `tag` is a short subsystem identifier (e.g. `"tool"`, `"tool:bash"`).
/// `msg` is the pre-formatted log message (no brackets / tag prefix).
pub type LogCallback = dyn Fn(&str, &str) + Send + Sync + 'static;

// \u2500\u2500 Statics \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

static ENABLED: AtomicBool = AtomicBool::new(true);
static CALLBACK: OnceLock<Box<LogCallback>> = OnceLock::new();

// \u2500\u2500 Public API \u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500

/// Enable or disable tool-call log output globally.
///
/// When disabled, [`tool_log!`] calls are short-circuited before formatting.
/// Enabled by default so that logs are visible during early init.
pub fn set_log_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Returns `true` if tool-call logging is currently enabled.
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

/// Write a single log line.  Prefer the [`tool_log!`] macro instead.
///
/// * If a callback was registered via [`set_log_callback`] -> delegates there.
/// * Otherwise -> `eprintln!("[{tag}] {msg}")`.
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
