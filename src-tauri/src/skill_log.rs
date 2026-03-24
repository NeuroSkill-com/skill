// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Structured logging with per-subsystem on/off switches.
//!
//! ## How log files are populated
//!
//! Call [`tee_stderr_to_file`] once at startup.  It redirects `fd 2` (stderr)
//! through an OS pipe, then a background thread copies every byte written to
//! stderr — by **any** code, any thread, any dependency — to both:
//!
//! * the original terminal (so `cargo tauri dev` output is unchanged), and
//! * `~/.skill/YYYYMMDD/log_<unix_ts>.txt`
//!
//! [`SkillLogger::write`] simply calls `eprint!`, so `skill_log!` calls are
//! captured automatically without a separate file handle.
//!
//! ## Usage
//! ```ignore
//! skill_log!(logger, "bluetooth", "connected: {name}");
//! ```

use std::{
    path::Path,
    sync::RwLock,
};
use serde::{Deserialize, Serialize};

// ── Config ────────────────────────────────────────────────────────────────────

/// Per-subsystem logging switches persisted in `~/.skill/log_config.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// ZUNA embedding worker (epoch dispatch, encoder, HNSW inserts).
    pub embedder:  bool,
    /// Bluetooth scanner and Muse session events.
    pub bluetooth: bool,
    /// Multi-transport device scanner (BLE, USB, Cortex, WiFi).
    pub scanner: bool,
    /// WebSocket server connections and message dispatches.
    pub websocket: bool,
    /// CSV file open / flush / close events.
    pub csv:       bool,
    /// GPU EEG filter hops and band-power snapshots.
    pub filter:    bool,
    /// Band-power analysis snapshots.
    pub bands:     bool,
    /// TTS synthesis events (text, sample count, latency).
    pub tts:       bool,
    /// LLM inference engine (model load, token generation, tool calls).
    pub llm:       bool,
    /// Chat store SQLite operations (save, migrate, open).
    pub chat_store: bool,
    /// Session history loading (directory scan, sidecar parsing, orphan CSV
    /// detection).  Can be noisy when many sessions exist; off by default.
    pub history:   bool,
    /// Hook runtime in the embedding worker (matching, trigger, notifications).
    pub hooks:     bool,
    /// Tool-call execution (invocation, safety approval, results, timing).
    pub tools:     bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            embedder:  true,
            bluetooth: true,
            scanner:   true,
            websocket: false,
            csv:       false,
            filter:    false,
            bands:     false,
            tts:        false,
            llm:        false,
            chat_store: false,
            history:    false,
            hooks:     true,
            tools:     false,
        }
    }
}

// ── Logger ────────────────────────────────────────────────────────────────────

/// Thread-safe logger.  File output is handled by [`tee_stderr_to_file`];
/// this struct only holds the live per-subsystem config.
pub struct SkillLogger {
    config: RwLock<LogConfig>,
}

impl SkillLogger {
    pub fn new(config: LogConfig) -> Self {
        Self { config: RwLock::new(config) }
    }

    /// Whether `tag` is currently enabled.
    pub fn enabled(&self, tag: &str) -> bool {
        let cfg = self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        match tag {
            "embedder"  => cfg.embedder,
            "bluetooth" => cfg.bluetooth,
            "scanner"   => cfg.scanner,
            "websocket" => cfg.websocket,
            "csv"       => cfg.csv,
            "filter"    => cfg.filter,
            "bands"     => cfg.bands,
            "tts"        => cfg.tts,
            "llm"        => cfg.llm,
            "chat_store" => cfg.chat_store,
            "history"    => cfg.history,
            "hooks"      => cfg.hooks,
            t if t == "tool" || t.starts_with("tool:") => cfg.tools,
            _           => true,
        }
    }

    /// Write one log line to stderr (the fd tee copies it to the log file).
    pub fn write(&self, tag: &str, msg: &str) {
        eprintln!("[{tag}] {msg}");
    }

    /// Replace the live config and persist to `config_path`.
    pub fn set_config(&self, cfg: LogConfig, config_path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            let _ = std::fs::write(config_path, json);
        }
        *self.config.write().unwrap_or_else(std::sync::PoisonError::into_inner) = cfg;
    }

    pub fn get_config(&self) -> LogConfig {
        self.config.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

// ── Conditional logging macro ─────────────────────────────────────────────────

#[macro_export]
macro_rules! skill_log {
    ($logger:expr, $tag:literal, $($arg:tt)*) => {
        if $logger.enabled($tag) {
            $logger.write($tag, &format!($($arg)*));
        }
    };
}

// ── Stderr tee ────────────────────────────────────────────────────────────────

/// Redirect `stderr` through an OS pipe so every `eprintln!` / `eprint!` from
/// any thread or dependency is copied to both the original terminal stderr and
/// `log_path`.
///
/// Must be called **once**, as early as possible.  Non-fatal: if the pipe or
/// file cannot be created the function returns silently and stderr is
/// unchanged.
///
/// Not available on non-Unix targets (Windows stub below).
#[cfg(unix)]
pub fn tee_stderr_to_file(log_path: &Path) {
    use std::fs::OpenOptions;
    use std::io::{Read, Write};
    use std::os::unix::io::FromRawFd;
    use std::sync::atomic::{AtomicBool, Ordering};

    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) { return; } // already set up

    // Ensure the parent directory exists.
    if let Some(dir) = log_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    let log_file = match OpenOptions::new().create(true).append(true).open(log_path) {
        Ok(f)  => f,
        Err(e) => {
            eprintln!("[logger] cannot open log file {}: {e}", log_path.display());
            return;
        }
    };

    // SAFETY: All libc calls below (pipe, dup2, read, write) operate on
    // valid file descriptors created/duplicated in this block.  The reader
    // thread owns read_fd and the duplicated stderr owns write_fd.
    unsafe {
        // Create pipe: read_fd ← write_fd
        let mut fds: [libc::c_int; 2] = [-1; 2];
        if libc::pipe(fds.as_mut_ptr()) != 0 { return; }
        let (read_fd, write_fd) = (fds[0], fds[1]);

        // Preserve the original stderr so the tee thread can still write to it.
        let orig_stderr = libc::dup(2);
        if orig_stderr < 0 {
            libc::close(read_fd); libc::close(write_fd);
            return;
        }

        // Point fd 2 → write end of the pipe.
        if libc::dup2(write_fd, 2) < 0 {
            libc::close(read_fd); libc::close(write_fd); libc::close(orig_stderr);
            return;
        }
        // fd 2 now owns the write end; close the spare descriptor.
        libc::close(write_fd);

        // Tee thread: read from pipe, write to both original stderr and file.
        let _ = std::thread::Builder::new()
            .name("stderr-tee".into())
            .spawn(move || {
                let mut reader = std::fs::File::from_raw_fd(read_fd);
                let mut orig   = std::fs::File::from_raw_fd(orig_stderr);
                let mut file   = log_file;
                let mut buf    = vec![0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,          // EOF or error → done
                        Ok(n) => {
                            let _ = orig.write_all(&buf[..n]);
                            let _ = file.write_all(&buf[..n]);
                        }
                    }
                }
            });
    }
}

#[cfg(not(unix))]
pub fn tee_stderr_to_file(_log_path: &Path) {
    // Windows: no-op for now.  Consider using SetStdHandle + a named pipe.
}

// ── Config file helpers ───────────────────────────────────────────────────────

pub fn load_log_config(skill_dir: &Path) -> LogConfig {
    skill_data::util::load_json_or_default(&skill_dir.join(crate::constants::LOG_CONFIG_FILE))
}

/// Write default `LogConfig` to disk if the file does not exist yet.
pub fn ensure_log_config(skill_dir: &Path) {
    let path = skill_dir.join(crate::constants::LOG_CONFIG_FILE);
    if !path.exists() {
        skill_data::util::save_json(&path, &LogConfig::default());
    }
}
