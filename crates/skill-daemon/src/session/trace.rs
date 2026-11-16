// SPDX-License-Identifier: GPL-3.0-only
//! Per-session trace: what a recording costs and whether all of it arrives.
//!
//! Once a second the session reports, for the last second and in total: the
//! samples that came in and their rate, how far behind real time they were
//! (ingest lag), breaks in their time stamps (samples a link dropped), the
//! processor time the pipeline took, what was written to disk and whether any
//! write failed, how many epochs went to the embedder and how many it had no
//! room for, and the daemon's own CPU and memory use. It goes out as a
//! `SessionTrace` event, into the log once a minute (at once when something is
//! wrong), and the totals are kept in the recording's sidecar.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;

/// Consecutive samples further apart than this many sample periods are a
/// break in the stream.
const GAP_PERIODS: f64 = 1.5;

/// One second of a session, and its running totals.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub(crate) struct TraceReport {
    /// Seconds since the session started.
    pub uptime_s: f64,
    // ── Input ────────────────────────────────────────────────────────────
    pub samples: u64,
    /// Samples per second over the last second.
    pub rate_hz: f64,
    pub nominal_rate_hz: f64,
    /// Newest sample's age when it reached the pipeline (ms); `None` when the
    /// device's time stamps are not wall-clock time.
    pub lag_ms: Option<f64>,
    /// Breaks in the time stamps, and the samples they stand for.
    pub gaps: u64,
    pub missing_samples: u64,
    /// Event markers recorded.
    pub markers: u64,
    // ── Processing ───────────────────────────────────────────────────────
    /// Share of one core the pipeline (recording + DSP) took, last second.
    pub pipeline_cpu_pct: f64,
    /// Slowest single frame of the last second (ms): a long one means the
    /// loop stalled and input was queueing meanwhile.
    pub max_frame_ms: f64,
    // ── Storage ──────────────────────────────────────────────────────────
    /// Raw EEG rows handed to storage.
    pub rows_written: u64,
    /// Failed writes. Not zero means samples are not reaching the disk.
    pub write_errors: u64,
    /// Size of the recording's files, and its growth over the last second.
    pub disk_bytes: u64,
    pub disk_bytes_per_s: f64,
    // ── Embeddings ───────────────────────────────────────────────────────
    pub epochs_sent: u64,
    /// Epochs the embedder had no room for (it re-embeds from the files when idle).
    pub epochs_dropped: u64,
    // ── Process ──────────────────────────────────────────────────────────
    /// The whole daemon, last second: percent of one core, and peak memory.
    pub process_cpu_pct: f64,
    pub process_peak_rss_mb: f64,
    // ── This computer's clock ────────────────────────────────────────────
    /// Time server minus this computer's clock at the last check (ms), and
    /// the most that figure can be wrong by; `None` when not checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_offset_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_uncertainty_ms: Option<f64>,
    /// Checks made so far.
    pub clock_checks: u64,
}

impl TraceReport {
    /// Something a person should look at now, not in a minute.
    pub(crate) fn is_alarming(&self) -> bool {
        self.write_errors > 0
    }
}

pub(crate) struct SessionTrace {
    started: Instant,
    window_start: Instant,
    nominal_rate: f64,
    samples: u64,
    gaps: u64,
    missing: u64,
    markers: u64,
    last_stamp: Option<f64>,
    w_samples: u64,
    w_pipeline: Duration,
    w_max_frame: Duration,
    w_lag: Option<f64>,
    cpu_prev: Duration,
    disk_prev: u64,
    last_logged: Option<Instant>,
    clock: Option<(f64, f64)>,
    clock_checks: u64,
}

/// Unix seconds below this are a device's own clock, not wall-clock time.
const MIN_UNIX_TS: f64 = 1_000_000_000.0;

impl SessionTrace {
    pub(crate) fn new(nominal_rate: f64) -> Self {
        let now = Instant::now();
        Self {
            started: now,
            window_start: now,
            nominal_rate,
            samples: 0,
            gaps: 0,
            missing: 0,
            markers: 0,
            last_stamp: None,
            w_samples: 0,
            w_pipeline: Duration::ZERO,
            w_max_frame: Duration::ZERO,
            w_lag: None,
            cpu_prev: process_cpu_time(),
            disk_prev: 0,
            last_logged: None,
            clock: None,
            clock_checks: 0,
        }
    }

    /// One EEG frame: its time stamp, the wall-clock time it reached the
    /// pipeline, and how long the pipeline took over it.
    pub(crate) fn on_frame(&mut self, stamp_s: f64, now_unix_s: f64, pipeline_time: Duration) {
        self.samples += 1;
        self.w_samples += 1;
        self.w_pipeline += pipeline_time;
        self.w_max_frame = self.w_max_frame.max(pipeline_time);
        if let Some(prev) = self.last_stamp {
            if self.nominal_rate > 0.0 {
                let steps = (stamp_s - prev) * self.nominal_rate;
                if steps > GAP_PERIODS {
                    self.gaps += 1;
                    self.missing += (steps.round() as u64).saturating_sub(1);
                }
            }
        }
        self.last_stamp = Some(stamp_s);
        if stamp_s >= MIN_UNIX_TS {
            self.w_lag = Some((now_unix_s - stamp_s) * 1000.0);
        }
    }

    /// One event marker recorded.
    pub(crate) fn on_marker(&mut self) {
        self.markers += 1;
    }

    /// This computer's clock was compared with a time server.
    pub(crate) fn on_clock_check(&mut self, check: &skill_data::wall_clock::ClockCheck) {
        self.clock = Some((check.offset_s * 1e3, check.uncertainty_s() * 1e3));
        self.clock_checks += 1;
    }

    /// Whether a second has passed since the last report.
    pub(crate) fn due(&self) -> bool {
        self.window_start.elapsed() >= Duration::from_secs(1)
    }

    /// Close the current one-second window.
    pub(crate) fn report(&mut self, storage: StorageCounts, epochs: (u64, u64)) -> TraceReport {
        let dt = self.window_start.elapsed().as_secs_f64().max(1e-3);
        let cpu = process_cpu_time();
        let report = TraceReport {
            uptime_s: self.started.elapsed().as_secs_f64(),
            samples: self.samples,
            rate_hz: self.w_samples as f64 / dt,
            nominal_rate_hz: self.nominal_rate,
            lag_ms: self.w_lag,
            gaps: self.gaps,
            missing_samples: self.missing,
            markers: self.markers,
            pipeline_cpu_pct: self.w_pipeline.as_secs_f64() / dt * 100.0,
            max_frame_ms: self.w_max_frame.as_secs_f64() * 1000.0,
            rows_written: storage.rows,
            write_errors: storage.errors,
            disk_bytes: storage.disk_bytes,
            disk_bytes_per_s: storage.disk_bytes.saturating_sub(self.disk_prev) as f64 / dt,
            epochs_sent: epochs.0,
            epochs_dropped: epochs.1,
            process_cpu_pct: cpu.saturating_sub(self.cpu_prev).as_secs_f64() / dt * 100.0,
            process_peak_rss_mb: process_peak_rss_mb(),
            clock_offset_ms: self.clock.map(|c| c.0),
            clock_uncertainty_ms: self.clock.map(|c| c.1),
            clock_checks: self.clock_checks,
        };
        self.cpu_prev = cpu;
        self.disk_prev = storage.disk_bytes;
        self.window_start = Instant::now();
        self.w_samples = 0;
        self.w_pipeline = Duration::ZERO;
        self.w_max_frame = Duration::ZERO;
        self.w_lag = None;
        report
    }

    /// Whether this report belongs in the log: once a minute, and right away
    /// (at most every ten seconds) while something is wrong.
    pub(crate) fn should_log(&mut self, report: &TraceReport) -> bool {
        let every = if report.is_alarming() {
            Duration::from_secs(10)
        } else {
            Duration::from_secs(60)
        };
        if self.last_logged.is_none_or(|t| t.elapsed() >= every) {
            self.last_logged = Some(Instant::now());
            return true;
        }
        false
    }
}

/// What the recording's storage says about itself.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StorageCounts {
    pub rows: u64,
    pub errors: u64,
    pub disk_bytes: u64,
}

/// Total size of a recording's files: everything in its directory that
/// shares the stem of `csv_path` (`exg_<ts>.csv`, `.parquet`, `_ppg…`, …).
pub(crate) fn recording_disk_bytes(csv_path: &Path) -> u64 {
    let (Some(dir), Some(stem)) = (csv_path.parent(), csv_path.file_stem().and_then(|s| s.to_str())) else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            e.file_name().to_str().is_some_and(|n| {
                // `exg_17` must not count `exg_170…`: the stem ends the name
                // or is followed by `.` / `_`.
                n.strip_prefix(stem)
                    .is_some_and(|rest| rest.starts_with('.') || rest.starts_with('_'))
            })
        })
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// User + system CPU time of this process.
#[cfg(unix)]
fn process_cpu_time() -> Duration {
    // SAFETY: `getrusage` fills the struct it is given; zeroed is a valid start.
    let ru = unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &raw mut ru) != 0 {
            return Duration::ZERO;
        }
        ru
    };
    let tv =
        |t: libc::timeval| Duration::from_secs(t.tv_sec.max(0) as u64) + Duration::from_micros(t.tv_usec.max(0) as u64);
    tv(ru.ru_utime) + tv(ru.ru_stime)
}

#[cfg(not(unix))]
fn process_cpu_time() -> Duration {
    Duration::ZERO
}

/// Peak resident memory of this process in MB.
#[cfg(unix)]
fn process_peak_rss_mb() -> f64 {
    // SAFETY: as above.
    let max = unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &raw mut ru) != 0 {
            return 0.0;
        }
        ru.ru_maxrss as f64
    };
    // bytes on macOS, kilobytes elsewhere
    if cfg!(target_os = "macos") {
        max / 1_048_576.0
    } else {
        max / 1024.0
    }
}

#[cfg(not(unix))]
fn process_peak_rss_mb() -> f64 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_are_counted_from_the_time_stamps() {
        let mut t = SessionTrace::new(500.0);
        let t0 = 1_700_000_000.0;
        let ms = Duration::from_micros(40);
        for k in 0..100u32 {
            t.on_frame(t0 + f64::from(k) * 0.002, t0 + f64::from(k) * 0.002 + 0.015, ms);
        }
        // 10 samples missing, then one more break of a single sample
        t.on_frame(t0 + 110.0 * 0.002, t0 + 0.3, ms);
        t.on_frame(t0 + 112.0 * 0.002, t0 + 0.3, ms);
        // jitter below 1.5 periods is not a break
        t.on_frame(t0 + 113.4 * 0.002, t0 + 0.3, ms);
        let r = t.report(
            StorageCounts {
                rows: 103,
                errors: 0,
                disk_bytes: 4096,
            },
            (2, 1),
        );
        assert_eq!((r.samples, r.gaps, r.missing_samples), (103, 2, 11));
        assert_eq!((r.rows_written, r.epochs_sent, r.epochs_dropped), (103, 2, 1));
        assert!(r.lag_ms.is_some_and(|l| l > 0.0));
        assert!(r.max_frame_ms > 0.0 && r.pipeline_cpu_pct > 0.0);
        assert!(!r.is_alarming());
    }

    #[test]
    fn device_clock_stamps_have_no_lag_and_write_errors_alarm() {
        let mut t = SessionTrace::new(256.0);
        t.on_frame(12.5, 1_700_000_000.0, Duration::from_micros(10));
        let r = t.report(
            StorageCounts {
                rows: 1,
                errors: 3,
                disk_bytes: 0,
            },
            (0, 0),
        );
        assert_eq!(r.lag_ms, None);
        assert!(r.is_alarming());
        // an alarm is logged at once, then held back for ten seconds
        assert!(t.should_log(&r));
        assert!(!t.should_log(&r));
    }

    #[test]
    fn disk_bytes_counts_only_this_recording() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("exg_17.csv");
        std::fs::write(&csv, [0u8; 100]).unwrap();
        std::fs::write(dir.path().join("exg_17.parquet"), [0u8; 50]).unwrap();
        std::fs::write(dir.path().join("exg_17_ppg.csv"), [0u8; 7]).unwrap();
        std::fs::write(dir.path().join("exg_17.parquet.tail"), [0u8; 3]).unwrap();
        std::fs::write(dir.path().join("exg_170.csv"), [0u8; 1000]).unwrap();
        assert_eq!(recording_disk_bytes(&csv), 160);
    }
}
