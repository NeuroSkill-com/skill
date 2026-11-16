// SPDX-License-Identifier: GPL-3.0-only
//! Checks this computer's clock against a time server while a session runs
//! (see [`skill_data::wall_clock`] and [`skill_settings::TimeCheckConfig`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use skill_data::wall_clock::ClockCheck;

/// How soon a check that got no answer is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// A thread that asks the time at intervals, for as long as this lives.
pub(crate) struct ClockMonitor {
    rx: mpsc::Receiver<ClockCheck>,
    stop: Arc<AtomicBool>,
    pub(crate) warn_s: f64,
}

impl ClockMonitor {
    /// `None` when the setting is off. Tests never ask a server on the
    /// internet: under test only a server named in the settings is used.
    pub(crate) fn start(skill_dir: &std::path::Path) -> Option<Self> {
        let config = skill_settings::load_settings(skill_dir).time_check;
        if !config.enabled || (cfg!(test) && config.server.is_empty()) {
            return None;
        }
        let server = if config.server.trim().is_empty() {
            skill_data::wall_clock::default_server().to_string()
        } else {
            config.server.trim().to_string()
        };
        let every = Duration::from_secs(u64::from(config.interval_minutes.max(1)) * 60);
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::Builder::new()
            .name("clock-check".into())
            .spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    let wait = match skill_data::wall_clock::measure(&server) {
                        Some(check) => {
                            if tx.send(check).is_err() {
                                return;
                            }
                            every
                        }
                        None => RETRY_AFTER.min(every),
                    };
                    // Sleep in short steps so the thread ends with the session.
                    let until = Instant::now() + wait;
                    while Instant::now() < until && !stopped.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
            })
            .ok()?;
        Some(Self {
            rx,
            stop,
            warn_s: f64::from(config.warn_ms) / 1e3,
        })
    }

    /// A check made since the last call, if any.
    pub(crate) fn poll(&self) -> Option<ClockCheck> {
        self.rx.try_recv().ok()
    }
}

impl Drop for ClockMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
