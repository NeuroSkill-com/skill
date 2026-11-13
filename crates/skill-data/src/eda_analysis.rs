// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Electrodermal activity (EDA) analysis.
//!
//! Skin conductance has two components with different time scales, and they
//! mean different things:
//!
//! * **Tonic level (SCL)**: the slowly drifting baseline, seconds to minutes.
//!   It tracks general arousal.
//! * **Phasic responses (SCRs)**: brief rises of a few hundredths to a few µS,
//!   one to five seconds up and several seconds back down. They follow
//!   individual events: a startle, an orienting response, a thought.
//!
//! [`EdaAnalyzer`] is a streaming, causal analyzer: feed it `(timestamp, µS)`
//! samples as they arrive and it returns an [`EdaMetrics`] each time a five
//! second epoch completes. Everything runs on the *sample timestamps*, not the
//! wall clock, so it is deterministic and behaves the same on a replayed file.
//!
//! ## Pipeline
//!
//! 1. **Artifact gate.** A reading below [`MIN_CONTACT_US`] is an open circuit
//!    (the electrodes lost the skin), and a jump steeper than
//!    [`MAX_SLOPE_US_PER_S`] is not physiology. Both are dropped and reset the
//!    response detector, so an artifact is never read as a response.
//! 2. **SCL** is the median of the epoch's valid samples, which a short peak
//!    cannot drag.
//! 3. **SCR detection**: a threshold detector on a lightly smoothed signal. It
//!    watches for a rise of at least [`SCR_THRESHOLD_US`] above the running
//!    minimum, tracks the peak, and confirms it once the signal falls back or
//!    stops climbing. A rise that takes longer than [`SCR_MAX_RISE_S`] is tonic
//!    drift and is discarded, as is one faster than [`SCR_MIN_RISE_S`], which is
//!    an artifact.
//! 4. **Arousal** is the epoch SCL as a z-score against a slowly adapting
//!    baseline learned from the session so far.
//!
//! ## What the baseline is, and is not
//!
//! The baseline is **per session**. EDA levels differ several-fold between
//! people and with skin temperature, hydration and electrode contact, so an
//! absolute µS value says little; the z-score says how far this moment is from
//! this wearer's recent norm. It is `None` until [`BASELINE_MIN_EPOCHS`] epochs
//! have been seen, and it forgets slowly ([`BASELINE_TAU_S`]), so a sustained
//! shift is eventually absorbed rather than flagged forever. A baseline that
//! persists across sessions is deliberately not attempted here.

use serde::Serialize;
use std::collections::VecDeque;

/// Length of one metrics epoch, in seconds of sample time.
pub const EPOCH_S: f64 = 5.0;

/// Below this the electrodes have lost the skin (open circuit), in µS.
pub const MIN_CONTACT_US: f64 = 0.05;
/// Faster than this is not physiology (motion, a loose electrode), in µS/s.
/// Real SCRs rise well under 1 µS/s.
pub const MAX_SLOPE_US_PER_S: f64 = 2.0;

/// Time constant of the light smoothing applied before response detection.
const SMOOTH_TAU_S: f64 = 0.3;
/// Smallest rise counted as an SCR, in µS. The literature uses 0.01 to 0.05.
pub const SCR_THRESHOLD_US: f64 = 0.03;
/// A rise quicker than this is an artifact, not a response.
pub const SCR_MIN_RISE_S: f64 = 0.3;
/// A rise slower than this is tonic drift, not a response.
pub const SCR_MAX_RISE_S: f64 = 8.0;
/// A peak is confirmed once it has not been exceeded for this long.
const PEAK_HOLD_S: f64 = 1.0;
/// ...or once the signal has fallen this fraction of the response amplitude.
const PEAK_DROP_FRACTION: f64 = 0.2;
/// The smallest drop that confirms a peak, so noise on a tiny response cannot.
const PEAK_DROP_FLOOR_US: f64 = 0.01;

/// Responses per minute are counted over this trailing window.
const RATE_WINDOW_S: f64 = 60.0;
/// Until this much data exists, a rate would be an extrapolation from too little.
const RATE_MIN_SPAN_S: f64 = 15.0;

/// Epochs needed before an arousal z-score is reported.
pub const BASELINE_MIN_EPOCHS: usize = 6;
/// How slowly the baseline forgets, in seconds.
pub const BASELINE_TAU_S: f64 = 600.0;
/// The baseline's spread never counts as less than this, so a very steady
/// wearer's tiny variance cannot turn a small change into a huge z-score.
const BASELINE_STD_FLOOR_US: f64 = 0.05;

/// An epoch with fewer valid samples than this reports nothing.
const MIN_EPOCH_SAMPLES: usize = 3;

/// Derived EDA metrics for one epoch.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EdaMetrics {
    /// End of the epoch, seconds since the Unix epoch.
    pub timestamp_s: f64,
    /// Tonic skin conductance level: the epoch's median, in µS.
    pub scl_us: f64,
    /// Skin conductance responses confirmed during this epoch.
    pub scr_count: u32,
    /// Responses per minute over the trailing minute, once there is enough data.
    pub scr_per_min: Option<f64>,
    /// Mean response amplitude over the trailing minute, in µS, when any occurred.
    pub scr_amplitude_us: Option<f64>,
    /// SCL as a z-score against this session's baseline, once it has warmed up.
    pub arousal_z: Option<f64>,
    /// Fraction of this epoch's samples that passed the artifact gate (0 to 1).
    pub valid_fraction: f64,
}

impl EdaMetrics {
    /// The sensor channel an EDA stream must arrive on to be analysed.
    pub const SOURCE_CHANNEL: &'static str = "eda";

    /// These metrics as `(channel, unit, value)` rows for the `_sensors`
    /// sidecar, timestamped at [`Self::timestamp_s`]. Metrics that are not yet
    /// available are left out rather than written as zero.
    ///
    /// The channel ids are written into recordings, so they are never renamed.
    /// They carry an `eda_` prefix so they cannot collide with a device's own
    /// raw channels (an EmotiBit reports its firmware's `scr_*` separately).
    pub fn sensor_rows(&self) -> Vec<(&'static str, &'static str, f64)> {
        let mut rows = vec![
            ("eda_scl", "µS", self.scl_us),
            ("eda_scr_count", "count", f64::from(self.scr_count)),
            ("eda_valid_fraction", "", self.valid_fraction),
        ];
        let optional = [
            ("eda_scr_per_min", "1/min", self.scr_per_min),
            ("eda_scr_amplitude", "µS", self.scr_amplitude_us),
            ("eda_arousal", "z", self.arousal_z),
        ];
        rows.extend(optional.into_iter().filter_map(|(c, u, v)| v.map(|v| (c, u, v))));
        rows
    }
}

/// Where the response detector is in the shape of a response.
#[derive(Clone, Copy, Debug)]
enum Scr {
    /// Watching for a rise; `(t, v)` is the lowest point seen since the last response.
    Idle { min: Option<(f64, f64)> },
    /// A rise past the threshold is under way.
    Rising { onset: (f64, f64), peak: (f64, f64) },
}

/// Streaming EDA analyzer. See the [module docs](self).
#[derive(Debug)]
pub struct EdaAnalyzer {
    /// Time and value of the last sample that passed the gate.
    last_valid: Option<(f64, f64)>,
    /// Time of the last sample of any kind, to reject out-of-order input.
    last_t: Option<f64>,
    /// First valid sample's time, so an early rate is not diluted by silence.
    first_valid_t: Option<f64>,
    smooth: Option<f64>,
    scr: Scr,
    /// Confirmed responses as `(peak_time, amplitude)`, newest last.
    events: VecDeque<(f64, f64)>,

    epoch_start: Option<f64>,
    epoch_vals: Vec<f64>,
    epoch_total: usize,
    epoch_scr: u32,

    /// Exponentially weighted `(mean, variance)` of epoch SCL values.
    baseline: Option<(f64, f64)>,
    baseline_epochs: usize,
}

impl Default for EdaAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl EdaAnalyzer {
    pub fn new() -> Self {
        Self {
            last_valid: None,
            last_t: None,
            first_valid_t: None,
            smooth: None,
            scr: Scr::Idle { min: None },
            events: VecDeque::new(),
            epoch_start: None,
            epoch_vals: Vec::new(),
            epoch_total: 0,
            epoch_scr: 0,
            baseline: None,
            baseline_epochs: 0,
        }
    }

    /// Forget everything, as at the start of a new recording.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Feed one sample: seconds since the Unix epoch and skin conductance in µS.
    ///
    /// Returns the metrics of the epoch this sample completed, if it completed
    /// one. Non-finite values and samples that are not later than the previous
    /// one are ignored.
    pub fn push(&mut self, t: f64, us: f64) -> Option<EdaMetrics> {
        if !t.is_finite() || !us.is_finite() || self.last_t.is_some_and(|last| t <= last) {
            return None;
        }
        self.last_t = Some(t);

        let finished = self.roll_epoch(t);
        self.epoch_total += 1;

        if self.passes_gate(t, us) {
            self.accept(t, us);
        } else {
            // An artifact must not be read as the start or end of a response.
            self.scr = Scr::Idle { min: None };
            self.smooth = None;
        }
        finished
    }

    /// Close the current epoch if `t` has moved past it, returning its metrics.
    ///
    /// Epochs stay aligned to a grid while data is continuous. After a gap of
    /// two epochs or more they restart at `t` instead, so a silence does not
    /// produce a run of empty epochs.
    fn roll_epoch(&mut self, t: f64) -> Option<EdaMetrics> {
        let start = *self.epoch_start.get_or_insert(t);
        if t - start < EPOCH_S {
            return None;
        }
        let end = start + EPOCH_S;
        let metrics = self.finish_epoch(end);
        self.epoch_start = Some(if t - start < 2.0 * EPOCH_S { end } else { t });
        metrics
    }

    fn passes_gate(&self, t: f64, us: f64) -> bool {
        if us < MIN_CONTACT_US {
            return false;
        }
        match self.last_valid {
            // Slope against the last *valid* sample: while a jump is still
            // being rejected the gap keeps growing, so the slope falls and a
            // genuine re-contact at a new level is accepted after a moment.
            Some((lt, lv)) => (us - lv).abs() / (t - lt) <= MAX_SLOPE_US_PER_S,
            None => true,
        }
    }

    fn accept(&mut self, t: f64, us: f64) {
        let dt = self.last_valid.map_or(0.0, |(lt, _)| t - lt);
        self.last_valid = Some((t, us));
        self.first_valid_t.get_or_insert(t);
        self.epoch_vals.push(us);

        let smoothed = match self.smooth {
            Some(prev) => prev + (1.0 - (-dt / SMOOTH_TAU_S).exp()) * (us - prev),
            None => us,
        };
        self.smooth = Some(smoothed);
        self.step_detector(t, smoothed);
    }

    fn step_detector(&mut self, t: f64, s: f64) {
        match self.scr {
            Scr::Idle { min } => match min {
                Some((_, mv)) if s - mv >= SCR_THRESHOLD_US => {
                    self.scr = Scr::Rising {
                        onset: min.unwrap_or((t, s)),
                        peak: (t, s),
                    };
                }
                Some((_, mv)) if s >= mv => {}
                _ => self.scr = Scr::Idle { min: Some((t, s)) },
            },
            Scr::Rising { onset, mut peak } => {
                if s > peak.1 {
                    peak = (t, s);
                }
                // Still climbing after this long is a drift, not a response.
                if peak.0 - onset.0 > SCR_MAX_RISE_S {
                    self.scr = Scr::Idle { min: Some((t, s)) };
                    return;
                }
                let amplitude = peak.1 - onset.1;
                let drop = (PEAK_DROP_FRACTION * amplitude).max(PEAK_DROP_FLOOR_US);
                if t - peak.0 >= PEAK_HOLD_S || s <= peak.1 - drop {
                    let rise = peak.0 - onset.0;
                    if amplitude >= SCR_THRESHOLD_US && rise >= SCR_MIN_RISE_S {
                        self.events.push_back((peak.0, amplitude));
                        self.epoch_scr += 1;
                    }
                    // The next response is measured from where this one ends.
                    self.scr = Scr::Idle { min: Some((t, s)) };
                } else {
                    self.scr = Scr::Rising { onset, peak };
                }
            }
        }
    }

    /// Summarise and clear the epoch that ended at `end`.
    fn finish_epoch(&mut self, end: f64) -> Option<EdaMetrics> {
        let vals = std::mem::take(&mut self.epoch_vals);
        let total = std::mem::take(&mut self.epoch_total);
        let scr_count = std::mem::take(&mut self.epoch_scr);
        if vals.len() < MIN_EPOCH_SAMPLES {
            return None;
        }

        let scl = median(&vals);
        let (scr_per_min, scr_amplitude_us) = self.rate_and_amplitude(end);
        let arousal_z = self.update_baseline(scl);

        Some(EdaMetrics {
            timestamp_s: end,
            scl_us: scl,
            scr_count,
            scr_per_min,
            scr_amplitude_us,
            arousal_z,
            valid_fraction: vals.len() as f64 / total.max(1) as f64,
        })
    }

    /// Responses per minute and their mean amplitude over the trailing window.
    fn rate_and_amplitude(&mut self, end: f64) -> (Option<f64>, Option<f64>) {
        while self
            .events
            .front()
            .is_some_and(|&(peak_t, _)| peak_t <= end - RATE_WINDOW_S)
        {
            self.events.pop_front();
        }
        let span = self.first_valid_t.map_or(0.0, |first| (end - first).min(RATE_WINDOW_S));
        let rate = (span >= RATE_MIN_SPAN_S).then(|| self.events.len() as f64 * 60.0 / span);
        let amplitude = (!self.events.is_empty())
            .then(|| self.events.iter().map(|&(_, a)| a).sum::<f64>() / self.events.len() as f64);
        (rate, amplitude)
    }

    /// Score `scl` against the baseline so far, then fold it into the baseline.
    ///
    /// Scored first, so a level is never compared with a baseline that already
    /// contains it. Early epochs average equally, so the baseline converges
    /// quickly; after that it forgets with time constant [`BASELINE_TAU_S`].
    fn update_baseline(&mut self, scl: f64) -> Option<f64> {
        let z = match self.baseline {
            Some((mean, var)) if self.baseline_epochs >= BASELINE_MIN_EPOCHS => {
                Some((scl - mean) / var.sqrt().max(BASELINE_STD_FLOOR_US))
            }
            _ => None,
        };

        self.baseline_epochs += 1;
        let slow = 1.0 - (-EPOCH_S / BASELINE_TAU_S).exp();
        let a = slow.max(1.0 / self.baseline_epochs as f64);
        self.baseline = Some(match self.baseline {
            None => (scl, 0.0),
            Some((mean, var)) => {
                let d = scl - mean;
                (mean + a * d, (1.0 - a) * (var + a * d * d))
            }
        });
        z
    }
}

fn median(vals: &[f64]) -> f64 {
    let mut v = vals.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const HZ: f64 = 15.0;
    const T0: f64 = 1_700_000_000.0;

    /// A response of amplitude `a` starting at `at`: a smooth half-cosine rise
    /// over `rise` seconds, then an exponential recovery.
    fn scr(t: f64, at: f64, a: f64, rise: f64, tau: f64) -> f64 {
        let x = t - at;
        if x <= 0.0 {
            0.0
        } else if x < rise {
            a * 0.5 * (1.0 - (std::f64::consts::PI * x / rise).cos())
        } else {
            a * (-(x - rise) / tau).exp()
        }
    }

    /// Run `f(seconds_since_start)` for `secs` seconds and collect every epoch.
    fn run(secs: f64, f: impl Fn(f64) -> f64) -> Vec<EdaMetrics> {
        let mut a = EdaAnalyzer::new();
        feed(&mut a, 0.0, secs, f)
    }

    fn feed(a: &mut EdaAnalyzer, from: f64, secs: f64, f: impl Fn(f64) -> f64) -> Vec<EdaMetrics> {
        let n = (secs * HZ) as usize;
        (0..n)
            .filter_map(|i| {
                let s = from + i as f64 / HZ;
                a.push(T0 + s, f(s))
            })
            .collect()
    }

    fn total_scr(m: &[EdaMetrics]) -> u32 {
        m.iter().map(|e| e.scr_count).sum()
    }

    #[test]
    fn a_flat_signal_has_a_level_and_no_responses() {
        let m = run(60.0, |_| 2.0);
        assert!(m.len() >= 10, "about one epoch per 5 s, got {}", m.len());
        assert!(m.iter().all(|e| (e.scl_us - 2.0).abs() < 1e-9));
        assert_eq!(total_scr(&m), 0);
        let last = m.last().unwrap();
        assert_eq!(last.scr_per_min, Some(0.0));
        assert_eq!(last.scr_amplitude_us, None, "no responses, no amplitude");
        assert_eq!(last.valid_fraction, 1.0);
    }

    #[test]
    fn each_response_is_counted_once_with_about_its_amplitude() {
        // Three well-separated 0.3 µS responses on a 2 µS level.
        let m = run(70.0, |s| {
            2.0 + scr(s, 10.0, 0.3, 1.5, 3.0) + scr(s, 30.0, 0.3, 1.5, 3.0) + scr(s, 50.0, 0.3, 1.5, 3.0)
        });
        assert_eq!(total_scr(&m), 3);
        let amp = m.last().unwrap().scr_amplitude_us.unwrap();
        assert!((amp - 0.3).abs() < 0.06, "amplitude {amp}");
    }

    #[test]
    fn the_rate_is_per_minute_over_the_trailing_window() {
        // Four responses in the first minute, none in the next.
        let m = run(120.0, |s| {
            [5.0, 15.0, 25.0, 40.0]
                .iter()
                .map(|&at| scr(s, at, 0.25, 1.5, 3.0))
                .sum::<f64>()
                + 2.0
        });
        let at_60 = m.iter().find(|e| e.timestamp_s - T0 >= 59.0).unwrap();
        assert!(
            (at_60.scr_per_min.unwrap() - 4.0).abs() < 0.5,
            "{:?}",
            at_60.scr_per_min
        );
        let last = m.last().unwrap();
        assert_eq!(last.scr_per_min, Some(0.0), "old responses fall out of the window");
    }

    #[test]
    fn a_rise_below_the_threshold_is_not_a_response() {
        let m = run(40.0, |s| 2.0 + scr(s, 10.0, 0.01, 1.5, 3.0));
        assert_eq!(total_scr(&m), 0);
    }

    /// A slow ramp is tonic drift. Counting it would turn every slow rise in
    /// level into a run of "responses".
    #[test]
    fn slow_drift_is_not_a_response() {
        let m = run(90.0, |s| 2.0 + 0.02 * s);
        assert_eq!(total_scr(&m), 0);
        // ...and the level it drifts to is still reported.
        assert!(m.last().unwrap().scl_us > 3.0);
    }

    /// A steep step is a motion or contact artifact, never a response.
    #[test]
    fn an_instant_jump_is_rejected_as_an_artifact() {
        let m = run(40.0, |s| if (20.0..20.2).contains(&s) { 7.0 } else { 2.0 });
        assert_eq!(total_scr(&m), 0, "a spike is not a response");
        let hit = m.iter().find(|e| (e.timestamp_s - T0 - 25.0).abs() < 0.1).unwrap();
        assert!(hit.valid_fraction < 1.0, "the rejected samples are visible");
        assert!((hit.scl_us - 2.0).abs() < 1e-9, "and do not move the level");
    }

    #[test]
    fn losing_contact_reports_nothing_rather_than_a_zero_level() {
        let m = run(30.0, |s| if s < 10.0 { 2.0 } else { 0.0 });
        assert!(
            m.iter().all(|e| e.scl_us > 1.0),
            "an open circuit must not be reported as a level of zero"
        );
        assert!(
            m.iter().all(|e| e.timestamp_s - T0 <= 15.0 + 1e-6),
            "no epochs after contact is lost"
        );
    }

    #[test]
    fn a_new_level_after_contact_returns_is_accepted() {
        let mut a = EdaAnalyzer::new();
        feed(&mut a, 0.0, 10.0, |_| 2.0);
        feed(&mut a, 10.0, 10.0, |_| 0.0);
        let m = feed(&mut a, 20.0, 30.0, |_| 5.0);
        assert!(
            m.iter().any(|e| (e.scl_us - 5.0).abs() < 1e-9),
            "the gate must not reject the new level forever"
        );
    }

    #[test]
    fn arousal_is_absent_until_the_baseline_has_warmed_up() {
        let m = run(60.0, |_| 2.0);
        assert!(m[..BASELINE_MIN_EPOCHS].iter().all(|e| e.arousal_z.is_none()));
        assert!(m[BASELINE_MIN_EPOCHS + 1].arousal_z.is_some());
    }

    #[test]
    fn a_rise_in_level_scores_as_high_arousal_against_the_baseline() {
        let m = run(150.0, |s| if s < 90.0 { 2.0 + 0.02 * (s * 1.7).sin() } else { 3.5 });
        let before = m.iter().rev().find(|e| e.timestamp_s - T0 < 90.0).unwrap();
        assert!(before.arousal_z.unwrap().abs() < 1.0, "{:?}", before.arousal_z);
        let after = m.iter().find(|e| e.timestamp_s - T0 > 95.0).unwrap();
        assert!(after.arousal_z.unwrap() > 3.0, "{:?}", after.arousal_z);
    }

    /// A perfectly steady wearer has almost no variance; without a floor a
    /// small, ordinary change would score as enormous.
    #[test]
    fn the_baseline_spread_is_floored() {
        let m = run(120.0, |s| if s < 60.0 { 2.0 } else { 2.1 });
        let z = m.last().unwrap().arousal_z.unwrap();
        assert!(z <= 0.1 / BASELINE_STD_FLOOR_US + 1e-9, "{z}");
    }

    /// The epoch in progress when the silence began is real data and is emitted
    /// with its own timestamp once data resumes. What must not happen is epochs
    /// timestamped *inside* the silence, or a backlog of empty ones.
    #[test]
    fn a_long_gap_restarts_the_epochs_instead_of_emitting_empty_ones() {
        let mut a = EdaAnalyzer::new();
        let first = feed(&mut a, 0.0, 20.0, |_| 2.0);
        let second = feed(&mut a, 600.0, 20.0, |_| 2.0);
        assert!(!first.is_empty() && !second.is_empty());
        let all: Vec<f64> = first.iter().chain(&second).map(|e| e.timestamp_s - T0).collect();
        assert!(
            all.iter().all(|&t| t <= 25.0 + 1e-6 || t >= 600.0),
            "an epoch was timestamped inside the silence: {all:?}"
        );
        assert!(second.len() <= 5, "no backlog of empty epochs, got {}", second.len());
        assert!(all.windows(2).all(|w| w[0] < w[1]), "epochs stay in order: {all:?}");
    }

    #[test]
    fn non_finite_and_out_of_order_samples_are_ignored() {
        let mut a = EdaAnalyzer::new();
        assert!(a.push(T0, f64::NAN).is_none());
        assert!(a.push(f64::INFINITY, 2.0).is_none());
        let _ = a.push(T0 + 1.0, 2.0);
        // Not later than the previous sample: must not rewind or double-count.
        assert!(a.push(T0 + 1.0, 9.0).is_none());
        assert!(a.push(T0 + 0.5, 9.0).is_none());
        assert_eq!(a.epoch_total, 1);
    }

    #[test]
    fn reset_forgets_the_session() {
        let mut a = EdaAnalyzer::new();
        feed(&mut a, 0.0, 60.0, |_| 2.0);
        a.reset();
        let m = feed(&mut a, 0.0, 30.0, |_| 4.0);
        assert!(m
            .iter()
            .all(|e| e.arousal_z.is_none() || e.arousal_z.unwrap().abs() < 1.0));
        assert!((m[0].scl_us - 4.0).abs() < 1e-9);
    }

    #[test]
    fn metrics_serialise_with_stable_field_names() {
        let m = &run(30.0, |_| 2.0)[1];
        let j = serde_json::to_value(m).unwrap();
        for key in [
            "timestamp_s",
            "scl_us",
            "scr_count",
            "scr_per_min",
            "scr_amplitude_us",
            "arousal_z",
            "valid_fraction",
        ] {
            assert!(j.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn sensor_rows_omit_metrics_that_are_not_available_yet() {
        let m = &run(30.0, |_| 2.0)[0];
        let channels: Vec<&str> = m.sensor_rows().iter().map(|r| r.0).collect();
        assert_eq!(channels, ["eda_scl", "eda_scr_count", "eda_valid_fraction"]);

        let warm = run(120.0, |_| 2.0).pop().unwrap();
        let channels: Vec<&str> = warm.sensor_rows().iter().map(|r| r.0).collect();
        assert!(channels.contains(&"eda_scr_per_min"));
        assert!(channels.contains(&"eda_arousal"));
        assert!(
            !channels.contains(&"eda_scr_amplitude"),
            "no responses, no amplitude row"
        );
    }

    /// Derived channels must never collide with a device's raw ones.
    #[test]
    fn derived_channel_ids_are_prefixed() {
        let warm = run(120.0, |s| 2.0 + scr(s, 20.0, 0.3, 1.5, 3.0)).pop().unwrap();
        assert!(warm.sensor_rows().iter().all(|r| r.0.starts_with("eda_")));
    }

    #[test]
    fn median_handles_odd_even_and_unsorted() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
    }
}
