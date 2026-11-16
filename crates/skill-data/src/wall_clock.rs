// SPDX-License-Identifier: GPL-3.0-only
//! How this computer's clock stands against a time server, and what to do
//! about it afterwards.
//!
//! A recording's time stamps are this computer's wall clock. Within a session
//! that does not matter — its streams are aligned with each other by other
//! means — but two recordings made on two computers line up only as well as
//! the two clocks agree, and a clock that is not being kept in step drifts by
//! tens of milliseconds an hour.
//!
//! The clock is not corrected while recording: one answer from a time server
//! over a home network is good to several milliseconds at best, which is the
//! size of the error it would correct, and stepping a clock in mid-recording
//! would break the even spacing of the samples. Instead the difference is
//! *measured* ([`query`], [`measure`]) at intervals and kept beside the
//! recording ([`append_check`]: `exg_<ts>_clock.csv`), so that it can be
//! applied later, when all of a session's measurements are there to average
//! ([`ClockCorrection`]).

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Seconds between the NTP era (1900) and the Unix epoch (1970).
const NTP_UNIX_OFFSET: f64 = 2_208_988_800.0;

/// One comparison of this computer's clock with a time server.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClockCheck {
    /// When it was made, Unix seconds on this computer's clock.
    pub unix_s: f64,
    /// Server time minus this computer's time, seconds: what to add to a time
    /// stamp taken here to get the server's time. Positive: this clock is slow.
    pub offset_s: f64,
    /// Time the question and its answer spent on the network, seconds. The
    /// offset cannot be wrong by more than half of this.
    pub round_trip_s: f64,
    pub server: String,
}

impl ClockCheck {
    /// The most the offset can be wrong by, seconds.
    pub fn uncertainty_s(&self) -> f64 {
        self.round_trip_s / 2.0
    }
}

/// The time server this system's own clock service uses by default, so that
/// asking it tells no one anything the operating system does not already.
pub fn default_server() -> &'static str {
    if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        "time.apple.com"
    } else if cfg!(target_os = "windows") {
        "time.windows.com"
    } else {
        "pool.ntp.org"
    }
}

fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

fn ntp_time(bytes: &[u8]) -> f64 {
    let secs = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let frac = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    f64::from(secs) + f64::from(frac) / 4_294_967_296.0 - NTP_UNIX_OFFSET
}

/// Ask `server` (`host` or `host:port`, port 123 by default) the time once
/// (SNTP, RFC 4330).
pub fn query(server: &str, timeout: Duration) -> std::io::Result<ClockCheck> {
    let bad = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_string());
    let target = if server.rsplit(':').next().is_some_and(|p| p.parse::<u16>().is_ok()) && !server.ends_with(']') {
        server.to_string()
    } else {
        format!("{server}:123")
    };
    let addr: SocketAddr = target
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| bad("the time server's name has no address"))?;
    let socket = UdpSocket::bind(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })?;
    socket.set_read_timeout(Some(timeout))?;
    socket.connect(addr)?;

    // Version 4, mode 3 (client). The transmit time is ours to choose; the
    // server sends it back, which is how its answer is told from a stray one.
    let mut request = [0u8; 48];
    request[0] = 0x23;
    let sent = unix_now();
    let stamp = sent + NTP_UNIX_OFFSET;
    request[40..44].copy_from_slice(&(stamp as u32).to_be_bytes());
    request[44..48].copy_from_slice(&((stamp.fract() * 4_294_967_296.0) as u32).to_be_bytes());
    socket.send(&request)?;

    let mut reply = [0u8; 48];
    let n = socket.recv(&mut reply)?;
    let received = unix_now();
    if n < 48 {
        return Err(bad("short answer from the time server"));
    }
    let (mode, stratum) = (reply[0] & 7, reply[1]);
    if mode != 4 || stratum == 0 || stratum > 15 {
        return Err(bad("the time server has no time to give"));
    }
    if reply[24..32] != request[40..48] {
        return Err(bad("an answer to another question"));
    }
    let (server_received, server_sent) = (ntp_time(&reply[32..40]), ntp_time(&reply[40..48]));
    Ok(ClockCheck {
        unix_s: received,
        offset_s: ((server_received - sent) + (server_sent - received)) / 2.0,
        round_trip_s: ((received - sent) - (server_sent - server_received)).max(0.0),
        server: server.to_string(),
    })
}

/// Ask a few times and keep the answer that spent least time on the network:
/// the one whose two trips were most nearly equal, which is what the offset
/// assumes. `None` when the server cannot be reached.
pub fn measure(server: &str) -> Option<ClockCheck> {
    (0..4)
        .filter_map(|k| {
            if k > 0 {
                std::thread::sleep(Duration::from_millis(100));
            }
            query(server, Duration::from_secs(1)).ok()
        })
        .min_by(|a, b| a.round_trip_s.total_cmp(&b.round_trip_s))
}

// ── Kept with the recording ───────────────────────────────────────────────────

/// `exg_<ts>_clock.csv` beside a recording.
pub fn clock_csv_path(eeg_csv_path: &Path) -> PathBuf {
    let stem = eeg_csv_path.file_stem().and_then(|s| s.to_str()).unwrap_or("exg");
    eeg_csv_path.with_file_name(format!("{stem}_clock.csv"))
}

/// Add one check to a recording's `_clock.csv` (`unix_s,offset_ms,
/// round_trip_ms,server`), on disk before this returns.
pub fn append_check(eeg_csv_path: &Path, check: &ClockCheck) -> std::io::Result<()> {
    let file = std::fs::File::options()
        .create(true)
        .append(true)
        .open(clock_csv_path(eeg_csv_path))?;
    let new = file.metadata()?.len() == 0;
    let mut wtr = csv::Writer::from_writer(file);
    if new {
        wtr.write_record(["unix_s", "offset_ms", "round_trip_ms", "server"])?;
    }
    wtr.write_record([
        format!("{:.3}", check.unix_s).as_str(),
        format!("{:.3}", check.offset_s * 1e3).as_str(),
        format!("{:.3}", check.round_trip_s * 1e3).as_str(),
        check.server.as_str(),
    ])?;
    wtr.flush()?;
    wtr.into_inner()
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .sync_data()
}

/// The checks kept with a recording, oldest first; empty when there are none.
pub fn read_checks(eeg_csv_path: &Path) -> Vec<ClockCheck> {
    let Ok(mut rdr) = csv::Reader::from_path(clock_csv_path(eeg_csv_path)) else {
        return Vec::new();
    };
    rdr.records()
        .flatten()
        .filter_map(|r| {
            Some(ClockCheck {
                unix_s: r.get(0)?.parse().ok()?,
                offset_s: r.get(1)?.parse::<f64>().ok()? / 1e3,
                round_trip_s: r.get(2)?.parse::<f64>().ok()? / 1e3,
                server: r.get(3)?.to_string(),
            })
        })
        .collect()
}

// ── Applied afterwards ────────────────────────────────────────────────────────

/// What a set of checks says this computer's clock was off by at any moment
/// of the time they cover.
///
/// One check is good to half its round trip; many are better. The checks are
/// fitted with a straight line — an offset and a drift — each weighted by how
/// little time it spent on the network; with too few of them, or all at
/// nearly the same moment, the drift is left out and their weighted mean is
/// the offset.
#[derive(Debug, Clone, PartialEq)]
pub struct ClockCorrection {
    /// Offset at `at_s`, seconds (server minus this computer).
    offset_s: f64,
    at_s: f64,
    /// Seconds of offset gained per second: the clock's rate error.
    drift: f64,
    /// Spread of the checks about the fit, seconds.
    pub residual_s: f64,
    pub checks: usize,
    /// The time the checks cover; outside it the nearest end is used.
    from_s: f64,
    to_s: f64,
}

impl ClockCorrection {
    /// Checks needed, and seconds they must span, before a drift is fitted.
    const DRIFT_MIN_CHECKS: usize = 3;
    const DRIFT_MIN_SPAN_S: f64 = 600.0;

    /// `None` without any check.
    pub fn from_checks(checks: &[ClockCheck]) -> Option<Self> {
        let (first, last) = (checks.first()?, checks.last()?);
        // A round trip of zero would outweigh everything: no trip is that good.
        let weight = |c: &ClockCheck| 1.0 / c.round_trip_s.max(1e-3).powi(2);
        let sum: f64 = checks.iter().map(weight).sum();
        let at_s = checks.iter().map(|c| weight(c) * c.unix_s).sum::<f64>() / sum;
        let mean = checks.iter().map(|c| weight(c) * c.offset_s).sum::<f64>() / sum;
        let spread: f64 = checks.iter().map(|c| weight(c) * (c.unix_s - at_s).powi(2)).sum();
        let span = last.unix_s - first.unix_s;
        let drift = if checks.len() >= Self::DRIFT_MIN_CHECKS && span >= Self::DRIFT_MIN_SPAN_S && spread > 0.0 {
            checks
                .iter()
                .map(|c| weight(c) * (c.unix_s - at_s) * (c.offset_s - mean))
                .sum::<f64>()
                / spread
        } else {
            0.0
        };
        let residual = (checks
            .iter()
            .map(|c| weight(c) * (c.offset_s - mean - drift * (c.unix_s - at_s)).powi(2))
            .sum::<f64>()
            / sum)
            .sqrt();
        Some(Self {
            offset_s: mean,
            at_s,
            drift,
            residual_s: residual,
            checks: checks.len(),
            from_s: first.unix_s,
            to_s: last.unix_s,
        })
    }

    /// The correction for a recording, from the checks kept with it.
    pub fn for_recording(eeg_csv_path: &Path) -> Option<Self> {
        Self::from_checks(&read_checks(eeg_csv_path))
    }

    /// Server time minus this computer's time at `unix_s`, seconds.
    pub fn offset_at(&self, unix_s: f64) -> f64 {
        self.offset_s + self.drift * (unix_s.clamp(self.from_s, self.to_s) - self.at_s)
    }

    /// A time stamp taken on this computer, on the server's clock.
    pub fn corrected(&self, unix_s: f64) -> f64 {
        unix_s + self.offset_at(unix_s)
    }

    /// The clock's rate error in parts per million (positive: it runs slow);
    /// zero when there was too little to tell.
    pub fn drift_ppm(&self) -> f64 {
        self.drift * 1e6
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// A time server on this machine whose clock is `ahead_s` ahead of ours
    /// and which takes `think` to answer.
    fn server(ahead_s: f64, think: Duration) -> String {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 48];
            while let Ok((48, from)) = socket.recv_from(&mut buf) {
                let stamp = |t: f64| {
                    let t = t + NTP_UNIX_OFFSET;
                    let mut b = [0u8; 8];
                    b[..4].copy_from_slice(&(t as u32).to_be_bytes());
                    b[4..].copy_from_slice(&((t.fract() * 4_294_967_296.0) as u32).to_be_bytes());
                    b
                };
                let received = unix_now() + ahead_s;
                std::thread::sleep(think);
                let mut reply = [0u8; 48];
                reply[0] = 0x24; // version 4, mode 4 (server)
                reply[1] = 2; // stratum
                reply[24..32].copy_from_slice(&buf[40..48]);
                reply[32..40].copy_from_slice(&stamp(received));
                reply[40..48].copy_from_slice(&stamp(unix_now() + ahead_s));
                let _ = socket.send_to(&reply, from);
            }
        });
        addr.to_string()
    }

    #[test]
    fn a_clock_that_is_slow_is_measured() {
        // 36 ms: what an unsynchronised laptop was found to be off by
        let addr = server(0.036, Duration::from_millis(20));
        let check = measure(&addr).expect("an answer");
        assert!((check.offset_s - 0.036).abs() < 0.005, "{check:?}");
        // the server's own time to answer is not network time
        assert!(check.round_trip_s < 0.015, "{check:?}");
        assert_eq!(check.server, addr);
        // and one that is fast
        let check = measure(&server(-1.5, Duration::ZERO)).expect("an answer");
        assert!((check.offset_s + 1.5).abs() < 0.005, "{check:?}");
    }

    #[test]
    fn no_answer_is_no_measurement() {
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = silent.local_addr().unwrap().to_string();
        assert!(query(&addr, Duration::from_millis(100)).is_err());
        assert!(query("no-such-host.invalid", Duration::from_millis(100)).is_err());
    }

    #[test]
    fn checks_are_kept_with_the_recording() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("exg_1700000000.csv");
        assert!(read_checks(&csv).is_empty());
        assert!(ClockCorrection::for_recording(&csv).is_none());
        let checks: Vec<ClockCheck> = (0..3)
            .map(|k| ClockCheck {
                unix_s: 1_700_000_000.0 + f64::from(k) * 300.0,
                offset_s: 0.030 + f64::from(k) * 0.0015,
                round_trip_s: 0.012,
                server: "time.example".into(),
            })
            .collect();
        for c in &checks {
            append_check(&csv, c).unwrap();
        }
        assert_eq!(clock_csv_path(&csv).file_name().unwrap(), "exg_1700000000_clock.csv");
        assert_eq!(read_checks(&csv), checks);
    }

    #[test]
    fn many_noisy_checks_give_the_offset_and_the_drift() {
        // an hour of a clock 30 ms slow and losing 19 ms more, checked every
        // five minutes over a network that adds up to ±8 ms to an answer
        let truth = |t: f64| 0.030 + 0.019 * t / 3600.0;
        let checks: Vec<ClockCheck> = (0..13)
            .map(|k| {
                let t = f64::from(k) * 300.0;
                let noise = (f64::from((k * 37) % 17) / 16.0 - 0.5) * 0.016;
                ClockCheck {
                    unix_s: 1_700_000_000.0 + t,
                    offset_s: truth(t) + noise,
                    round_trip_s: 0.004 + noise.abs() * 2.0,
                    server: "s".into(),
                }
            })
            .collect();
        let fit = ClockCorrection::from_checks(&checks).unwrap();
        let single_worst = checks
            .iter()
            .map(|c| (c.offset_s - truth(c.unix_s - 1_700_000_000.0)).abs())
            .fold(0.0, f64::max);
        for minute in [0.0, 17.0, 30.0, 60.0] {
            let t = 1_700_000_000.0 + minute * 60.0;
            let err = (fit.offset_at(t) - truth(minute * 60.0)).abs();
            assert!(err < 0.002, "{} ms off at minute {minute}", err * 1e3);
            assert!(err < single_worst / 3.0);
        }
        assert!((fit.drift_ppm() - 5.28).abs() < 1.0, "{}", fit.drift_ppm());
        assert!((fit.corrected(1_700_000_000.0) - 1_700_000_000.030).abs() < 0.002);
        // outside the time covered: the nearest end, not the line run on
        assert_eq!(
            fit.offset_at(1_700_000_000.0 - 86_400.0),
            fit.offset_at(1_700_000_000.0)
        );
    }

    #[test]
    fn a_few_checks_give_an_offset_and_no_drift() {
        let check = |t: f64, off: f64, trip: f64| ClockCheck {
            unix_s: t,
            offset_s: off,
            round_trip_s: trip,
            server: "s".into(),
        };
        let one = ClockCorrection::from_checks(&[check(100.0, 0.02, 0.01)]).unwrap();
        assert_eq!((one.offset_at(0.0), one.drift_ppm(), one.checks), (0.02, 0.0, 1));
        // the answer that travelled fastest counts for most
        let two = ClockCorrection::from_checks(&[check(100.0, 0.020, 0.004), check(160.0, 0.060, 0.080)]).unwrap();
        assert!((two.offset_at(130.0) - 0.020).abs() < 0.001, "{two:?}");
        assert_eq!(two.drift_ppm(), 0.0);
        assert!(ClockCorrection::from_checks(&[]).is_none());
    }
}
