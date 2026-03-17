// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Pure device-session logic extracted from `muse_session.rs`.
//!
//! Everything here is **Tauri-free**: Do Not Disturb decision engine,
//! composite EEG score computation (meditation, cognitive load, drowsiness),
//! battery EMA smoothing, and band-snapshot enrichment.

pub mod session;

pub use hermes_ble;
pub use muse_rs;
pub use mw75;
pub use openbci;
pub use emotiv;
pub use idun;

use std::collections::VecDeque;
use skill_eeg::eeg_bands::BandSnapshot;
use skill_eeg::artifact_detection::ArtifactMetrics;
use skill_eeg::head_pose::HeadPoseMetrics;
use skill_data::ppg_analysis::PpgMetrics;
use skill_data::gpu_stats::GpuStats;

// ── Band-snapshot enrichment ──────────────────────────────────────────────────

/// All the auxiliary signals that can enrich a [`BandSnapshot`].
///
/// Pass this to [`enrich_band_snapshot`] to populate PPG vitals, artifact
/// counts, head-pose metrics, composite scores, and GPU stats on the snapshot.
#[derive(Default)]
pub struct SnapshotContext {
    pub ppg:             Option<PpgMetrics>,
    pub artifacts:       Option<ArtifactMetrics>,
    pub head_pose:       Option<HeadPoseMetrics>,
    pub temperature_raw: u16,
    pub gpu:             Option<GpuStats>,
}

/// Enrich a [`BandSnapshot`] in-place with auxiliary signals and composite scores.
///
/// This is the single implementation of the enrichment logic formerly
/// duplicated across `muse_session.rs` and `openbci_session.rs`.
pub fn enrich_band_snapshot(snap: &mut BandSnapshot, ctx: &SnapshotContext) {
    // PPG vitals
    if let Some(ref ppg) = ctx.ppg {
        snap.hr               = Some(ppg.hr);
        snap.rmssd            = Some(ppg.rmssd);
        snap.sdnn             = Some(ppg.sdnn);
        snap.pnn50            = Some(ppg.pnn50);
        snap.lf_hf_ratio      = Some(ppg.lf_hf_ratio);
        snap.respiratory_rate = Some(ppg.respiratory_rate);
        snap.spo2_estimate    = Some(ppg.spo2_estimate);
        snap.perfusion_index  = Some(ppg.perfusion_index);
        snap.stress_index     = Some(ppg.stress_index);
    }

    // Artifact detection
    if let Some(ref art) = ctx.artifacts {
        snap.blink_count = Some(art.blink_count);
        snap.blink_rate  = Some(art.blink_rate);
    }

    // Head pose
    let (stillness, rmssd_opt) = if let Some(ref hp) = ctx.head_pose {
        snap.head_pitch  = Some(hp.pitch);
        snap.head_roll   = Some(hp.roll);
        snap.stillness   = Some(hp.stillness);
        snap.nod_count   = Some(hp.nod_count);
        snap.shake_count = Some(hp.shake_count);
        (hp.stillness, ctx.ppg.as_ref().map(|p| p.rmssd))
    } else {
        (0.0, ctx.ppg.as_ref().map(|p| p.rmssd))
    };

    // Temperature
    if ctx.temperature_raw > 0 {
        snap.temperature_raw = Some(ctx.temperature_raw);
    }

    // Composite scores
    let meditation = compute_meditation(snap, stillness, rmssd_opt);
    snap.meditation = Some((meditation * 10.0).round() / 10.0);

    let cognitive_load = compute_cognitive_load(snap);
    snap.cognitive_load = Some((cognitive_load * 10.0).round() / 10.0);

    let drowsiness = compute_drowsiness(snap);
    snap.drowsiness = Some((drowsiness * 10.0).round() / 10.0);

    // GPU stats
    if let Some(ref gpu) = ctx.gpu {
        snap.gpu_overall = Some(gpu.overall as f64);
        snap.gpu_render  = Some(gpu.render  as f64);
        snap.gpu_tiler   = Some(gpu.tiler   as f64);
    }
}

// ── Composite EEG scores ──────────────────────────────────────────────────────

/// Compute the meditation score (0–100) from a band snapshot and auxiliary signals.
///
/// Components:
/// - Alpha dominance (mean rel_alpha across channels, scaled to 0–40)
/// - Beta penalty (mean rel_beta, scaled and subtracted, max −20)
/// - Stillness (head-pose stillness × 0.2)
/// - HRV component (RMSSD / 100 × 20, capped at 20; defaults to 10 if no PPG)
pub fn compute_meditation(snap: &BandSnapshot, stillness: f64, rmssd: Option<f64>) -> f64 {
    let alpha_dom = snap.channels.iter()
        .map(|ch| ch.rel_alpha as f64).sum::<f64>() / snap.channels.len().max(1) as f64;
    let beta_dom = snap.channels.iter()
        .map(|ch| ch.rel_beta as f64).sum::<f64>() / snap.channels.len().max(1) as f64;
    let alpha_component = (alpha_dom * 200.0).min(40.0);
    let beta_penalty    = (beta_dom  * 100.0).min(20.0);
    let still_component = stillness * 0.2;
    let hrv_component = match rmssd {
        Some(v) => (v / 100.0 * 20.0).min(20.0),
        None    => 10.0,
    };
    (alpha_component - beta_penalty + still_component + hrv_component).clamp(0.0, 100.0)
}

/// Compute the cognitive load score (0–100) from a band snapshot.
///
/// Based on the frontal theta / parietal alpha ratio:
/// - AF7/AF8 (channels 1,2) provide frontal theta
/// - TP9/TP10 (channels 0,3) provide parietal alpha
/// - Mapped through a sigmoid: 100 / (1 + exp(−2.5 × (ratio − 1)))
pub fn compute_cognitive_load(snap: &BandSnapshot) -> f64 {
    if snap.channels.len() < 4 { return 50.0; }
    let frontal_theta  = (snap.channels[1].rel_theta as f64
                        + snap.channels[2].rel_theta as f64) / 2.0;
    let parietal_alpha = (snap.channels[0].rel_alpha as f64
                        + snap.channels[3].rel_alpha as f64) / 2.0;
    let cog_ratio = if parietal_alpha > 0.01 {
        frontal_theta / parietal_alpha
    } else { 1.0 };
    (100.0 / (1.0 + (-2.5 * (cog_ratio - 1.0)).exp())).clamp(0.0, 100.0)
}

/// Compute the drowsiness score (0–100) from a band snapshot.
///
/// Components:
/// - Theta/alpha ratio (TAR) component: TAR / 3 × 80, capped at 80
/// - Alpha spindle component: mean rel_alpha × 100, capped at 20
pub fn compute_drowsiness(snap: &BandSnapshot) -> f64 {
    let tar = snap.tar as f64;
    let alpha_dom = snap.channels.iter()
        .map(|ch| ch.rel_alpha as f64).sum::<f64>() / snap.channels.len().max(1) as f64;
    let tar_component  = (tar / 3.0 * 80.0).min(80.0);
    let alpha_spindle  = (alpha_dom * 100.0).min(20.0);
    (tar_component + alpha_spindle).clamp(0.0, 100.0)
}

/// Compute a raw engagement ratio from a band snapshot (not sigmoided).
///
/// For each channel: β / (α + θ), averaged across all channels.
/// Returns 0.5 if no channels are present.
pub fn compute_engagement_raw(snap: &BandSnapshot) -> f32 {
    if snap.channels.is_empty() { return 0.5; }
    let n = snap.channels.len() as f32;
    snap.channels.iter().map(|ch| {
        let d = ch.rel_alpha + ch.rel_theta;
        if d > 1e-6 { ch.rel_beta / d } else { 0.5 }
    }).sum::<f32>() / n
}

/// Map a raw engagement ratio to a 0–100 focus score via sigmoid.
///
/// `100 / (1 + exp(−2 × (raw − 0.8)))`
pub fn focus_score(engagement_raw: f32) -> f64 {
    (100.0_f32 / (1.0 + (-2.0 * (engagement_raw - 0.8)).exp())) as f64
}

// ── Battery EMA ───────────────────────────────────────────────────────────────

/// Exponential moving average for battery level with low-battery alerts.
pub struct BatteryEma {
    ema:   Option<f32>,
    alpha: f32,
}

/// Result of a battery update — tells the caller what alert (if any) to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BatteryAlert {
    /// No alert needed.
    None,
    /// Battery dropped below 20% for the first time.
    Low(f32),
    /// Battery dropped below 10% for the first time.
    Critical(f32),
}

impl BatteryEma {
    pub fn new(alpha: f32) -> Self { Self { ema: None, alpha } }

    /// Push a new raw battery reading, return smoothed value and alert.
    pub fn update(&mut self, raw: f32) -> (f32, BatteryAlert) {
        let prev = self.ema.unwrap_or(raw);
        let smoothed = match self.ema {
            None    => raw,
            Some(v) => self.alpha * raw + (1.0 - self.alpha) * v,
        };
        self.ema = Some(smoothed);

        let alert = if smoothed < 10.0 && prev >= 10.0 {
            BatteryAlert::Critical(smoothed)
        } else if smoothed < 20.0 && prev >= 20.0 {
            BatteryAlert::Low(smoothed)
        } else {
            BatteryAlert::None
        };
        (smoothed, alert)
    }

    pub fn current(&self) -> Option<f32> { self.ema }
    pub fn is_first_reading(&self) -> bool { self.ema.is_none() }
}

// ── DND Focus-Mode Decision Engine ────────────────────────────────────────────

// Re-export SNR thresholds from the canonical constants crate.
pub use skill_constants::SNR_LOW_DB;
pub use skill_constants::SNR_LOW_TICKS;

/// Configuration for the DND decision engine (read from app settings).
#[derive(Debug, Clone)]
pub struct DndConfig {
    pub enabled:             bool,
    pub focus_threshold:     f64,
    pub duration_secs:       u32,
    pub exit_duration_secs:  u32,
    pub focus_lookback_secs: u32,
    pub exit_notification:   bool,
    pub focus_mode_identifier: String,
    /// SNR threshold (dB) below which focus mode is forcibly deactivated.
    pub snr_exit_db:         f32,
}

/// Mutable state tracked across ticks by the DND engine.
#[derive(Debug, Clone)]
pub struct DndState {
    pub active:        bool,
    pub focus_samples: VecDeque<f64>,
    pub score_history: VecDeque<f64>,
    pub below_ticks:   u32,
    pub snr_low_ticks: u32,
    pub os_active:     Option<bool>,
}

impl DndState {
    pub fn new() -> Self {
        Self {
            active: false,
            focus_samples: VecDeque::new(),
            score_history: VecDeque::new(),
            below_ticks: 0,
            snr_low_ticks: 0,
            os_active: None,
        }
    }
}

impl Default for DndState {
    fn default() -> Self { Self::new() }
}

/// Outcome of one tick of the DND decision engine.
#[derive(Debug, Clone)]
pub struct DndDecision {
    /// Whether DND automation is enabled.
    pub enabled:               bool,
    pub threshold:             f64,
    pub exit_duration_secs:    u32,
    pub focus_lookback_secs:   u32,
    pub window:                usize,
    pub exit_window:           usize,
    pub sample_count:          usize,
    pub avg_score:             f64,
    /// Whether DND should appear active to the user.
    pub emit_active:           bool,
    pub below_ticks:           u32,
    pub exit_held:             bool,
    pub os_active:             Option<bool>,
    /// `Some((enable, mode_id))` → caller should invoke the OS DND toggle.
    pub set_dnd_to:            Option<(bool, String)>,
    /// Whether to send a native exit notification after the OS call.
    pub send_exit_notification: bool,
    /// Human-readable exit reason for the notification body.
    pub exit_body:             &'static str,
}

/// Run one tick of the DND focus-mode decision engine.
///
/// Pure function: reads `config` and mutates `state`, returns a `DndDecision`
/// that tells the caller what OS calls and UI updates to perform.
pub fn dnd_tick(
    config:      &DndConfig,
    state:       &mut DndState,
    focus_score: f64,
    snr_db:      f32,
) -> DndDecision {
    let window      = (config.duration_secs as usize * 4).max(8);
    let exit_window = (config.exit_duration_secs as usize * 4).max(4);
    let lookback_window = (config.focus_lookback_secs as usize * 4).max(4);

    // Update rolling windows.
    state.focus_samples.push_back(focus_score);
    while state.focus_samples.len() > window { state.focus_samples.pop_front(); }
    let sample_count = state.focus_samples.len();
    let avg_score = state.focus_samples.iter().sum::<f64>() / sample_count as f64;

    state.score_history.push_back(focus_score);
    while state.score_history.len() > lookback_window { state.score_history.pop_front(); }

    // SNR low-signal tracking.
    let snr_threshold = config.snr_exit_db;
    if snr_db < snr_threshold {
        state.snr_low_ticks = state.snr_low_ticks.saturating_add(1);
    } else {
        state.snr_low_ticks = 0;
    }
    let snr_forced_exit = config.enabled
        && state.active
        && state.snr_low_ticks >= SNR_LOW_TICKS;

    let mut emit_active = state.active;
    let mut below_ticks = state.below_ticks;
    let mut exit_held   = false;
    let mut set_dnd_to: Option<(bool, String)> = None;
    let mut send_exit_notification = false;
    let mut exit_body: &'static str = "";

    if snr_forced_exit {
        state.below_ticks = exit_window as u32;
        below_ticks       = exit_window as u32;
        emit_active       = false;
        set_dnd_to        = Some((false, String::new()));
        send_exit_notification = config.exit_notification;
        exit_body = "Signal quality (SNR) dropped below threshold for 1 minute. Focus mode deactivated.";
    } else if config.enabled {
        if avg_score >= config.focus_threshold {
            state.below_ticks = 0;
            below_ticks       = 0;
            if !state.active && snr_db >= snr_threshold && sample_count >= window {
                set_dnd_to = Some((true, config.focus_mode_identifier.clone()));
            }
        } else if state.active {
            let recent_had_focus = state.score_history.iter().any(|&v| v >= config.focus_threshold);
            if recent_had_focus {
                state.below_ticks = 0; below_ticks = 0; exit_held = true;
            } else {
                state.below_ticks += 1;
                below_ticks        = state.below_ticks;
                if state.below_ticks as usize >= exit_window {
                    state.below_ticks      = exit_window as u32;
                    emit_active            = false;
                    set_dnd_to             = Some((false, String::new()));
                    send_exit_notification = config.exit_notification;
                    exit_body = "Your focus score dropped. Focus mode has been deactivated.";
                }
            }
        } else {
            state.below_ticks = 0; below_ticks = 0;
        }
    } else if state.active {
        state.below_ticks  = 0;
        below_ticks        = 0;
        emit_active        = false;
        set_dnd_to         = Some((false, String::new()));
        send_exit_notification = config.exit_notification;
        exit_body = "Do Not Disturb automation was disabled. Focus mode deactivated.";
    }

    DndDecision {
        enabled: config.enabled,
        threshold: config.focus_threshold,
        exit_duration_secs: config.exit_duration_secs,
        focus_lookback_secs: config.focus_lookback_secs,
        window,
        exit_window,
        sample_count,
        avg_score,
        emit_active,
        below_ticks,
        exit_held,
        os_active: state.os_active,
        set_dnd_to,
        send_exit_notification,
        exit_body,
    }
}

/// Apply the result of a successful OS DND state change back to the engine state.
pub fn dnd_apply_os_result(state: &mut DndState, enabled: bool) {
    state.active        = enabled;
    state.below_ticks   = 0;
    state.snr_low_ticks = 0;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use skill_eeg::eeg_bands::{BandSnapshot, BandPowers};

    fn test_ch() -> BandPowers {
        BandPowers {
            channel: "TP9".into(),
            delta: 10.0, theta: 5.0, alpha: 8.0, beta: 4.0, gamma: 2.0, high_gamma: 1.0,
            rel_delta: 0.33, rel_theta: 0.17, rel_alpha: 0.27,
            rel_beta: 0.13, rel_gamma: 0.07, rel_high_gamma: 0.03,
            dominant: "delta".into(), dominant_symbol: "δ".into(), dominant_color: "#888".into(),
        }
    }

    fn test_snap() -> BandSnapshot {
        let ch = test_ch();
        BandSnapshot {
            channels: vec![ch.clone(), ch.clone(), ch.clone(), ch.clone()],
            tar: 0.5, bar: 0.4, dtr: 1.2, pse: 0.7, apf: 10.0, bps: -1.5,
            snr: 12.0, coherence: 0.5, mu_suppression: 0.1, mood: 60.0,
            tbr: 0.8, sef95: 22.0, spectral_centroid: 15.0,
            hjorth_activity: 0.1, hjorth_mobility: 0.2, hjorth_complexity: 0.3,
            permutation_entropy: 0.6, higuchi_fd: 1.5, dfa_exponent: 0.7,
            sample_entropy: 0.4, pac_theta_gamma: 0.1, laterality_index: 0.05,
            headache_index: 10.0, migraine_index: 5.0,
            consciousness_lzc: 50.0, consciousness_wakefulness: 70.0,
            consciousness_integration: 60.0,
            hr: None, rmssd: None, sdnn: None, pnn50: None,
            lf_hf_ratio: None, respiratory_rate: None,
            spo2_estimate: None, perfusion_index: None, stress_index: None,
            blink_count: None, blink_rate: None,
            head_pitch: None, head_roll: None, stillness: None,
            nod_count: None, shake_count: None,
            meditation: None, cognitive_load: None, drowsiness: None,
            temperature_raw: None,
            gpu_overall: None, gpu_render: None, gpu_tiler: None,
            faa: 0.05, timestamp: 0.0,
        }
    }

    #[test]
    fn meditation_in_range() {
        let m = compute_meditation(&test_snap(), 0.8, Some(40.0));
        assert!(m >= 0.0 && m <= 100.0, "meditation={m}");
    }

    #[test]
    fn cognitive_load_in_range() {
        let c = compute_cognitive_load(&test_snap());
        assert!(c >= 0.0 && c <= 100.0, "cognitive_load={c}");
    }

    #[test]
    fn drowsiness_in_range() {
        let d = compute_drowsiness(&test_snap());
        assert!(d >= 0.0 && d <= 100.0, "drowsiness={d}");
    }

    #[test]
    fn focus_score_range() {
        assert!(focus_score(0.0) >= 0.0);
        assert!(focus_score(1.0) <= 100.0);
    }

    #[test]
    fn battery_ema_first_reading() {
        let mut b = BatteryEma::new(0.1);
        assert!(b.is_first_reading());
        let (v, _) = b.update(80.0);
        assert!((v - 80.0).abs() < 0.01);
        assert!(!b.is_first_reading());
    }

    #[test]
    fn battery_low_alert() {
        let mut b = BatteryEma::new(1.0); // alpha=1 → no smoothing
        let (_, a1) = b.update(25.0);
        assert_eq!(a1, BatteryAlert::None);
        let (_, a2) = b.update(18.0);
        assert_eq!(a2, BatteryAlert::Low(18.0));
    }

    #[test]
    fn battery_critical_alert() {
        let mut b = BatteryEma::new(1.0);
        b.update(15.0);
        let (_, a) = b.update(8.0);
        assert_eq!(a, BatteryAlert::Critical(8.0));
    }

    #[test]
    fn dnd_tick_activates_above_threshold() {
        let cfg = DndConfig {
            enabled: true, focus_threshold: 60.0, duration_secs: 2,
            exit_duration_secs: 2, focus_lookback_secs: 2,
            exit_notification: false, focus_mode_identifier: "test".into(),
            snr_exit_db: 5.0,
        };
        let mut st = DndState::new();
        // Fill the window (2s × 4Hz = 8 samples).
        for _ in 0..8 { dnd_tick(&cfg, &mut st, 70.0, 10.0); }
        let d = dnd_tick(&cfg, &mut st, 70.0, 10.0);
        assert!(d.set_dnd_to.is_some());
        assert_eq!(d.set_dnd_to.unwrap().0, true);
    }

    #[test]
    fn dnd_tick_no_activate_below_threshold() {
        let cfg = DndConfig {
            enabled: true, focus_threshold: 60.0, duration_secs: 2,
            exit_duration_secs: 2, focus_lookback_secs: 2,
            exit_notification: false, focus_mode_identifier: "test".into(),
            snr_exit_db: 5.0,
        };
        let mut st = DndState::new();
        for _ in 0..10 { dnd_tick(&cfg, &mut st, 30.0, 10.0); }
        let d = dnd_tick(&cfg, &mut st, 30.0, 10.0);
        assert!(d.set_dnd_to.is_none());
    }
}
