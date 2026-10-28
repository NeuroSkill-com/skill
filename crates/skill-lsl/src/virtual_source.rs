// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Configurable virtual LSL EEG source.

//!
//! Streams synthetic EEG data so the full pipeline (LSL discovery → session
//! connect → daemon WebSocket → dashboard → embeddings → LLM) can be exercised
//! without hardware, including the ways real recordings go wrong.
//!
//! [`VirtualSourceConfig`] controls:
//! - channel count (up to 64, 10-10 labels) and sample rate;
//! - a signal **template** — brain states (`relaxed`, `focused`, `drowsy`,
//!   `sleep`, `cycle`), `artifacts` (blinks, jaw clenches), and the older
//!   `sine` / `good_quality` / `bad_quality` / `interruptions`;
//! - per-electrode **faults** (`flat`, `noisy`, `railed`, `line_noise`,
//!   `drift`, `intermittent`) by label or index, the cases a large montage
//!   lives with permanently;
//! - a simulated **battery** that drains, which the daemon reports as the
//!   device battery so low-battery handling can be tested;
//! - noise floor, global line noise and dropout probability.
//!
//! Everything except `channels` and `sample_rate` can be changed while the
//! stream runs ([`VirtualLslSource::update`]), so one session can walk through
//! a scenario: good signal → an electrode lifts → battery runs low.
//!
//! # Concurrency note
//! `rlsl::outlet::StreamOutlet::new` calls `tokio::Runtime::block_on`
//! internally, which panics on Tokio worker threads.  All rlsl operations
//! therefore run on a dedicated raw OS thread.

use anyhow::Context as _;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};
use std::time::{Duration, Instant};

use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};

// ── Public constants ──────────────────────────────────────────────────────────

pub const VIRTUAL_STREAM_NAME: &str = "SkillVirtualEEG";
pub const VIRTUAL_STREAM_TYPE: &str = "EEG";
pub const VIRTUAL_SOURCE_ID: &str = "skill-virtual-eeg-001";

/// Largest montage the source streams.
pub const VIRTUAL_MAX_CHANNELS: usize = 64;

// ── Channel label tables ──────────────────────────────────────────────────────

const LABELS_4: [&str; 4] = ["TP9", "AF7", "AF8", "TP10"];
const LABELS_8: [&str; 8] = ["Fp1", "Fp2", "F3", "F4", "C3", "C4", "O1", "O2"];
const LABELS_16: [&str; 16] = [
    "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "T7", "C3", "Cz", "C4", "T8", "P3", "Pz", "P4", "O1",
];
/// 32 standard positions, then 32 more 10-10 positions for a 64-ch cap.
const LABELS_64: [&str; 64] = [
    "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "FC5", "FC1", "FC2", "FC6", "T7", "C3", "Cz", "C4", "T8", "TP9", "CP5",
    "CP1", "CP2", "CP6", "TP10", "P7", "P3", "Pz", "P4", "P8", "PO9", "O1", "Oz", "O2", "PO10", "AF3", "AF4", "F1",
    "F2", "F5", "F6", "FC3", "FC4", "FT7", "FT8", "C1", "C2", "C5", "C6", "TP7", "TP8", "CP3", "CP4", "P1", "P2", "P5",
    "P6", "PO3", "PO4", "PO7", "PO8", "POz", "FCz", "CPz", "AFz", "Fpz", "Iz",
];

/// Channel labels for an `n`-channel montage (`n` ≤ [`VIRTUAL_MAX_CHANNELS`]).
pub fn channel_labels(n: usize) -> Vec<String> {
    let named: &[&str] = match n {
        1 | 2 | 4 => &LABELS_4[..n],
        8 => &LABELS_8,
        16 => &LABELS_16,
        _ => &LABELS_64[..n.min(VIRTUAL_MAX_CHANNELS)],
    };
    named.iter().map(|s| s.to_string()).collect()
}

/// Frontal channels — where blinks show up.
fn is_frontal(label: &str) -> bool {
    label.starts_with("Fp") || label.starts_with("AF")
}

/// Temporal channels — where jaw clenches show up.
fn is_temporal(label: &str) -> bool {
    label.starts_with('T') || label.starts_with("FT")
}

// ── Configuration ─────────────────────────────────────────────────────────────

/// Signal template — determines spectral content and artefact pattern.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalTemplate {
    /// Pure sine waves — one distinct frequency per channel.
    Sine,
    /// Realistic resting-state EEG: dominant alpha + delta/beta/theta mix.
    #[default]
    GoodQuality,
    /// Noisy signal with muscle artefacts and optional line noise.
    BadQuality,
    /// Good signal with random per-channel dropouts (electrode lift-off sim).
    Interruptions,
    /// Eyes-closed rest: strong posterior alpha (~10 Hz).
    Relaxed,
    /// Concentration: beta (~20 Hz) up, alpha suppressed, a little gamma.
    Focused,
    /// Drowsiness: theta (~6 Hz) dominant, alpha fading, slow drift.
    Drowsy,
    /// NREM sleep: high-amplitude delta (~1.5 Hz) with sleep-spindle bursts.
    Sleep,
    /// Relaxed signal plus periodic blinks (frontal) and jaw clenches (temporal).
    Artifacts,
    /// Relaxed → focused → drowsy, `cycle_secs` each, repeating.
    Cycle,
}

/// What is wrong with one electrode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultKind {
    /// Disconnected / broken lead: nothing but a flat line.
    Flat,
    /// Bad contact: large broadband noise.
    Noisy,
    /// Saturated amplifier: the channel sits on the rails and jumps between them.
    Railed,
    /// Strong mains pickup (60 Hz) on this lead.
    LineNoise,
    /// Drying gel / sweat: slow, large baseline wander over the signal.
    Drift,
    /// Loose electrode: drops out for 3 s of every 6 s.
    Intermittent,
    /// Loose contact picking up mains: 120 µV of 60 Hz over a normal signal
    /// (normal amplitude overall, ~90 % hum) → fair.
    Hum,
    /// Muscle tension / dry sensor: broadband noise on a normal signal → fair.
    Emg,
}

/// A fault on one electrode, addressed by label (`"AF7"`) or 0-based index.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ChannelFault {
    pub channel: ChannelRef,
    pub kind: FaultKind,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum ChannelRef {
    Index(usize),
    Label(String),
}

impl ChannelRef {
    fn resolve(&self, labels: &[String]) -> Option<usize> {
        match self {
            Self::Index(i) => (*i < labels.len()).then_some(*i),
            Self::Label(l) => labels.iter().position(|x| x.eq_ignore_ascii_case(l)),
        }
    }
}

/// A battery that starts at `percent` and loses `drain_per_min` points a minute.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct BatterySim {
    pub percent: f64,
    #[serde(default)]
    pub drain_per_min: f64,
}

/// SNR multiplier per quality tier (matches the TypeScript QUALITY_SNR map).
fn quality_snr(q: &str) -> f64 {
    match q {
        "poor" => 0.5,
        "fair" => 2.0,
        "good" => 5.0,
        "excellent" => 20.0,
        _ => 5.0,
    }
}

/// All parameters accepted by `/v1/lsl/virtual-source/start` (and, except
/// `channels` / `sample_rate`, by `/v1/lsl/virtual-source/config`).
/// Unknown fields are ignored; missing fields use their defaults so that a
/// plain `{}` body still produces a valid 32-ch 256 Hz source.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct VirtualSourceConfig {
    /// EEG channel count (default 32, max 64).
    #[serde(default = "default_channels")]
    pub channels: usize,
    /// Samples per second per channel (default 256).
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
    /// Signal template (default "good_quality").
    #[serde(default)]
    pub template: SignalTemplate,
    /// Signal quality tier: "poor" | "fair" | "good" | "excellent" (default "good").
    #[serde(default = "default_quality")]
    pub quality: String,
    /// Peak signal amplitude in µV (default 50).
    #[serde(default = "default_amplitude")]
    pub amplitude_uv: f64,
    /// RMS noise floor in µV (default 5).
    #[serde(default = "default_noise")]
    pub noise_uv: f64,
    /// Line-noise injection on every channel: "none" | "50hz" | "60hz" (default "none").
    #[serde(default = "default_line_noise")]
    pub line_noise: String,
    /// Per-second dropout probability 0–1 (default 0).
    #[serde(default)]
    pub dropout_prob: f64,
    /// Per-electrode faults (default none).
    #[serde(default)]
    pub faults: Vec<ChannelFault>,
    /// Simulated battery reported as the device battery (default none: 0%,
    /// like any LSL stream).
    #[serde(default)]
    pub battery: Option<BatterySim>,
    /// Seconds per state for the `cycle` template (default 60).
    #[serde(default = "default_cycle_secs")]
    pub cycle_secs: f64,
}

fn default_channels() -> usize {
    32
}
fn default_sample_rate() -> f64 {
    256.0
}
fn default_quality() -> String {
    "good".into()
}
fn default_amplitude() -> f64 {
    50.0
}
fn default_noise() -> f64 {
    5.0
}
fn default_line_noise() -> String {
    "none".into()
}
fn default_cycle_secs() -> f64 {
    60.0
}

impl Default for VirtualSourceConfig {
    fn default() -> Self {
        Self {
            channels: default_channels(),
            sample_rate: default_sample_rate(),
            template: SignalTemplate::default(),
            quality: default_quality(),
            amplitude_uv: default_amplitude(),
            noise_uv: default_noise(),
            line_noise: default_line_noise(),
            dropout_prob: 0.0,
            faults: Vec::new(),
            battery: None,
            cycle_secs: default_cycle_secs(),
        }
    }
}

// ── Handle ────────────────────────────────────────────────────────────────────

/// Live settings shared with the outlet thread.
struct Shared {
    config: RwLock<VirtualSourceConfig>,
    /// When the current battery simulation started.
    battery_since: RwLock<Instant>,
}

/// Handle to a running virtual LSL source.  Drop to stop it.
pub struct VirtualLslSource {
    shutdown: Arc<AtomicBool>,
    shared: Arc<Shared>,
}

impl VirtualLslSource {
    /// Start the virtual source on a dedicated OS thread and return immediately.
    pub fn start(mut config: VirtualSourceConfig) -> anyhow::Result<Self> {
        config.channels = config.channels.clamp(1, VIRTUAL_MAX_CHANNELS);
        config.sample_rate = config.sample_rate.max(1.0);
        let shutdown = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            config: RwLock::new(config),
            battery_since: RwLock::new(Instant::now()),
        });
        let (shutdown2, shared2) = (shutdown.clone(), shared.clone());

        std::thread::Builder::new()
            .name("skill-virtual-lsl".into())
            .spawn(move || run_virtual_outlet(shutdown2, shared2))
            .context("failed to spawn virtual LSL thread")?;

        Ok(Self { shutdown, shared })
    }

    /// Current configuration.
    pub fn config(&self) -> VirtualSourceConfig {
        self.shared.config.read().map(|c| c.clone()).unwrap_or_default()
    }

    /// Replace the live configuration. `channels` and `sample_rate` are fixed
    /// by the stream and kept; a new `battery` restarts its drain clock.
    pub fn update(&self, mut config: VirtualSourceConfig) {
        let Ok(mut cur) = self.shared.config.write() else {
            return;
        };
        config.channels = cur.channels;
        config.sample_rate = cur.sample_rate;
        if config.battery != cur.battery {
            if let Ok(mut since) = self.shared.battery_since.write() {
                *since = Instant::now();
            }
        }
        *cur = config;
    }

    /// Simulated battery percentage now, if a battery is configured.
    pub fn battery_percent(&self) -> Option<f64> {
        let battery = self.shared.config.read().ok()?.battery?;
        let mins = self.shared.battery_since.read().ok()?.elapsed().as_secs_f64() / 60.0;
        Some((battery.percent - battery.drain_per_min * mins).clamp(0.0, 100.0))
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    pub fn is_running(&self) -> bool {
        !self.shutdown.load(Ordering::Acquire)
    }
}

impl Drop for VirtualLslSource {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── Signal generation ─────────────────────────────────────────────────────────

const TWO_PI: f64 = std::f64::consts::PI * 2.0;

fn sine(freq: f64, t_s: f64, phase: f64) -> f64 {
    (TWO_PI * freq * t_s + phase).sin()
}

fn gaussian(rng: &mut SmallRng) -> f64 {
    // Box–Muller
    let u: f64 = rng.random::<f64>().max(1e-10);
    let v: f64 = rng.random::<f64>();
    (-2.0 * u.ln()).sqrt() * (TWO_PI * v).cos()
}

/// Brain-state signal for one channel (before noise and faults).
fn template_signal(template: &SignalTemplate, ch: usize, label: &str, t_s: f64, cfg: &VirtualSourceConfig) -> f64 {
    let amp = cfg.amplitude_uv;
    // Phase offset per channel so channels look different
    let ph = ch as f64 * (TWO_PI / cfg.channels.max(1) as f64);
    match template {
        SignalTemplate::Sine => {
            // Each channel a distinct frequency: 1 Hz + 0.5 Hz steps
            sine(1.0 + ch as f64 * 0.5, t_s, ph) * amp
        }
        SignalTemplate::GoodQuality | SignalTemplate::Relaxed => {
            // Dominant alpha (10 Hz) with a small amount of delta/theta/beta
            sine(10.1, t_s, ph) * amp * 0.65
                + sine(2.0, t_s, 0.0) * amp * 0.15
                + sine(6.0, t_s, ph) * amp * 0.10
                + sine(20.0, t_s, 0.0) * amp * 0.10
        }
        SignalTemplate::BadQuality => {
            // Low alpha + high-frequency muscle artefact bursts (50–150 Hz)
            sine(9.0, t_s, ph) * amp * 0.3 + sine(80.0, t_s, 0.0) * amp * 0.5 + sine(120.0, t_s, 0.0) * amp * 0.3
        }
        SignalTemplate::Interruptions => {
            // Good alpha signal — dropout is handled by the dropout state
            sine(10.0, t_s, ph) * amp * 0.7
        }
        SignalTemplate::Focused => {
            sine(20.0, t_s, ph) * amp * 0.55
                + sine(24.0, t_s, ph * 0.5) * amp * 0.20
                + sine(10.0, t_s, ph) * amp * 0.15
                + sine(40.0, t_s, 0.0) * amp * 0.08
        }
        SignalTemplate::Drowsy => {
            sine(6.0, t_s, ph) * amp * 0.70
                + sine(9.0, t_s, ph) * amp * 0.20
                + sine(2.5, t_s, 0.0) * amp * 0.25
                + sine(0.3, t_s, ph) * amp * 0.20
        }
        SignalTemplate::Sleep => {
            // Spindles: 13 Hz bursts ~1 s long every ~8 s.
            let spindle_env = if (t_s % 8.0) < 1.0 {
                (std::f64::consts::PI * (t_s % 8.0)).sin()
            } else {
                0.0
            };
            sine(1.5, t_s, ph) * amp * 1.4
                + sine(0.8, t_s, 0.0) * amp * 0.6
                + sine(13.0, t_s, ph) * amp * 0.5 * spindle_env
        }
        SignalTemplate::Artifacts => {
            let base = template_signal(&SignalTemplate::Relaxed, ch, label, t_s, cfg);
            // Blink: ~250 ms, ~150 µV frontal deflection every 4 s.
            let blink = if is_frontal(label) && (t_s % 4.0) < 0.25 {
                (std::f64::consts::PI * (t_s % 4.0) / 0.25).sin() * 150.0
            } else {
                0.0
            };
            // Jaw clench: 1 s of EMG on temporal channels every 10 s.
            let clench = if is_temporal(label) && (t_s % 10.0) < 1.0 {
                sine(70.0, t_s, ph) * 60.0 + sine(95.0, t_s, 0.0) * 40.0
            } else {
                0.0
            };
            base + blink + clench
        }
        SignalTemplate::Cycle => {
            let states = [SignalTemplate::Relaxed, SignalTemplate::Focused, SignalTemplate::Drowsy];
            let idx = (t_s / cfg.cycle_secs.max(1.0)) as usize % states.len();
            template_signal(&states[idx], ch, label, t_s, cfg)
        }
    }
}

/// Apply an electrode fault to a clean sample. `None` faults pass through.
fn apply_fault(kind: FaultKind, clean: f64, t_s: f64, ch: usize, rng: &mut SmallRng) -> f64 {
    match kind {
        FaultKind::Flat => 0.0,
        FaultKind::Noisy => clean + gaussian(rng) * 500.0,
        // Square wave between the rails, flipping every 2 s.
        FaultKind::Railed => {
            if (t_s / 2.0) as u64 % 2 == 0 {
                1800.0
            } else {
                -1800.0
            }
        }
        FaultKind::LineNoise => clean + sine(60.0, t_s, 0.0) * 600.0,
        FaultKind::Drift => clean + sine(0.5, t_s, ch as f64) * 450.0,
        FaultKind::Hum => clean + sine(60.0, t_s, 0.0) * 120.0,
        FaultKind::Emg => clean + gaussian(rng) * 15.0,
        // Out for 3 s of every 6 s (offset per channel).
        FaultKind::Intermittent => {
            if ((t_s + ch as f64 * 0.7) % 6.0) < 3.0 {
                0.0
            } else {
                clean
            }
        }
    }
}

/// Generate one sample value for channel `ch` at time index `t`.
fn generate_sample(
    ch: usize,
    label: &str,
    t: u64,
    cfg: &VirtualSourceConfig,
    fault: Option<FaultKind>,
    rng: &mut SmallRng,
    in_dropout: bool,
) -> f32 {
    if in_dropout {
        return 0.0;
    }
    let t_s = t as f64 / cfg.sample_rate;
    let base = template_signal(&cfg.template, ch, label, t_s, cfg);

    // Gaussian noise scaled to noise_uv (approximate: σ ≈ noise_uv)
    let noise = if cfg.noise_uv > 0.0 {
        gaussian(rng) * cfg.noise_uv
    } else {
        0.0
    };

    // Line-noise injection (50 or 60 Hz)
    let line = match cfg.line_noise.as_str() {
        "50hz" => sine(50.0, t_s, 0.0) * cfg.amplitude_uv * 0.25,
        "60hz" => sine(60.0, t_s, 0.0) * cfg.amplitude_uv * 0.25,
        _ => 0.0,
    };

    // SNR: scale the signal component by the quality tier.
    let snr = quality_snr(&cfg.quality);
    let signal_scale = snr / (snr + 1.0); // approaches 1 for excellent, 0.33 for poor

    let clean = (base * signal_scale) + noise + line;
    match fault {
        Some(kind) => apply_fault(kind, clean, t_s, ch, rng) as f32,
        None => clean as f32,
    }
}

/// Fault per channel index for the current config.
fn fault_map(cfg: &VirtualSourceConfig, labels: &[String]) -> Vec<Option<FaultKind>> {
    let mut map = vec![None; labels.len()];
    for f in &cfg.faults {
        if let Some(i) = f.channel.resolve(labels) {
            map[i] = Some(f.kind);
        }
    }
    map
}

// ── Outlet thread ─────────────────────────────────────────────────────────────

fn run_virtual_outlet(shutdown: Arc<AtomicBool>, shared: Arc<Shared>) {
    use rlsl::prelude::*;
    use rlsl::types::ChannelFormat;

    let initial = shared.config.read().map(|c| c.clone()).unwrap_or_default();
    let n_ch = initial.channels;
    let sr = initial.sample_rate;
    let labels = channel_labels(n_ch);

    let info = StreamInfo::new(
        VIRTUAL_STREAM_NAME,
        VIRTUAL_STREAM_TYPE,
        n_ch as u32,
        sr,
        ChannelFormat::Float32,
        VIRTUAL_SOURCE_ID,
    );

    // Attach channel labels to the stream XML descriptor.
    {
        let desc = info.desc();
        let channels = desc.append_child("channels");
        for label in &labels {
            let ch = channels.append_child("channel");
            ch.append_child_value("label", label);
            ch.append_child_value("unit", "microvolts");
            ch.append_child_value("type", "EEG");
        }
    }

    // StreamOutlet::new calls block_on — fine on a raw OS thread.
    let outlet = StreamOutlet::new(&info, 0, 360);
    eprintln!(
        "[lsl-virtual] outlet started — {n_ch} ch @ {sr} Hz \
         template={:?} quality={} faults={} stream='{VIRTUAL_STREAM_NAME}'",
        initial.template,
        initial.quality,
        initial.faults.len(),
    );

    // Push chunks of ~32 samples at real-time pace.
    let chunk: usize = (sr / 8.0).ceil() as usize; // ~32 samples at 256 Hz
    let chunk_delay = Duration::from_secs_f64(chunk as f64 / sr);

    let mut t: u64 = 0;
    let mut rng = SmallRng::seed_from_u64(42);
    // Per-channel dropout state (each channel can independently be in dropout)
    let mut dropout_timer: Vec<u64> = vec![0u64; n_ch];

    while !shutdown.load(Ordering::Acquire) {
        // Re-read once per chunk so live updates apply within ~125 ms.
        let cfg = shared.config.read().map(|c| c.clone()).unwrap_or_default();
        let faults = fault_map(&cfg, &labels);
        let dropout_rate = cfg.dropout_prob.clamp(0.0, 1.0);
        for _ in 0..chunk {
            let sample: Vec<f32> = (0..n_ch)
                .map(|ch| {
                    // Check / update dropout state for this channel
                    if dropout_timer[ch] > 0 {
                        dropout_timer[ch] -= 1;
                        generate_sample(ch, &labels[ch], t, &cfg, faults[ch], &mut rng, true)
                    } else {
                        // Probabilistic dropout onset (~dropout_rate events/sec)
                        let p_onset = dropout_rate / sr;
                        if rng.random::<f64>() < p_onset {
                            // Dropout lasts 0.05–0.5 s
                            let dur_samples = (rng.random::<f64>() * 0.45 * sr + 0.05 * sr) as u64;
                            dropout_timer[ch] = dur_samples;
                        }
                        generate_sample(ch, &labels[ch], t, &cfg, faults[ch], &mut rng, false)
                    }
                })
                .collect();
            outlet.push_sample_f(&sample, 0.0, true);
            t = t.wrapping_add(1);
        }
        std::thread::sleep(chunk_delay);
    }

    eprintln!("[lsl-virtual] outlet stopped");
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use skill_eeg::eeg_quality::{QualityMonitor, SignalQuality};

    /// Run one channel through the real quality classifier for 1 s.
    fn quality_of(cfg: &VirtualSourceConfig, ch: usize, start_s: f64) -> SignalQuality {
        let labels = channel_labels(cfg.channels);
        let faults = fault_map(cfg, &labels);
        let mut rng = SmallRng::seed_from_u64(7);
        let sr = cfg.sample_rate as usize;
        let t0 = (start_s * cfg.sample_rate) as u64;
        let samples: Vec<f64> = (0..sr as u64)
            .map(|i| generate_sample(ch, &labels[ch], t0 + i, cfg, faults[ch], &mut rng, false) as f64)
            .collect();
        let mut m = QualityMonitor::with_window(1, sr);
        m.push(0, &samples);
        m.all_qualities()[0].clone()
    }

    /// Like `quality_of`, but over `secs` seconds (the noise check uses a
    /// multi-second median).
    fn quality_over(cfg: &VirtualSourceConfig, ch: usize, secs: u64) -> SignalQuality {
        let labels = channel_labels(cfg.channels);
        let faults = fault_map(cfg, &labels);
        let mut rng = SmallRng::seed_from_u64(7);
        let sr = cfg.sample_rate as usize;
        let samples: Vec<f64> = (0..sr as u64 * secs)
            .map(|i| generate_sample(ch, &labels[ch], i, cfg, faults[ch], &mut rng, false) as f64)
            .collect();
        let mut m = QualityMonitor::with_window(1, sr);
        m.push(0, &samples);
        m.all_qualities()[0].clone()
    }

    fn with_fault(kind: FaultKind) -> VirtualSourceConfig {
        VirtualSourceConfig {
            channels: 4,
            faults: vec![ChannelFault {
                channel: ChannelRef::Label("af7".into()),
                kind,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn faults_land_in_the_intended_quality_class() {
        // Clean neighbour stays good.
        assert_eq!(quality_of(&with_fault(FaultKind::Flat), 0, 0.0), SignalQuality::Good);
        assert_eq!(
            quality_of(&with_fault(FaultKind::Flat), 1, 0.0),
            SignalQuality::NoSignal
        );
        assert_eq!(quality_of(&with_fault(FaultKind::Noisy), 1, 0.0), SignalQuality::Poor);
        assert_eq!(quality_of(&with_fault(FaultKind::Railed), 1, 1.5), SignalQuality::Poor);
        assert_eq!(
            quality_of(&with_fault(FaultKind::LineNoise), 1, 0.0),
            SignalQuality::Poor
        );
        assert_eq!(quality_of(&with_fault(FaultKind::Drift), 1, 1.0), SignalQuality::Fair);
        // Intermittent: out for 3 s of every 6 s, offset 0.7 s for ch 1.
        assert_eq!(
            quality_of(&with_fault(FaultKind::Intermittent), 1, 0.0),
            SignalQuality::NoSignal
        );
        assert_eq!(
            quality_of(&with_fault(FaultKind::Intermittent), 1, 3.0),
            SignalQuality::Good
        );
        // Normal amplitude, but hum- or noise-dominated: only the noise check
        // catches these. Judged over 4 s like the live monitor.
        assert_eq!(quality_over(&with_fault(FaultKind::Hum), 1, 4), SignalQuality::Fair);
        assert_eq!(quality_over(&with_fault(FaultKind::Emg), 1, 4), SignalQuality::Fair);
        assert_eq!(quality_over(&with_fault(FaultKind::Emg), 0, 4), SignalQuality::Good);
    }

    #[test]
    fn every_brain_state_template_reads_as_good_signal() {
        for template in [
            SignalTemplate::Relaxed,
            SignalTemplate::Focused,
            SignalTemplate::Drowsy,
            SignalTemplate::Sleep,
            SignalTemplate::Cycle,
        ] {
            let cfg = VirtualSourceConfig {
                channels: 8,
                template: template.clone(),
                ..Default::default()
            };
            for ch in 0..8 {
                assert_eq!(quality_of(&cfg, ch, 0.5), SignalQuality::Good, "{template:?} ch{ch}");
            }
        }
    }

    #[test]
    fn labels_cover_a_64_channel_cap_without_duplicates() {
        let l = channel_labels(64);
        assert_eq!(l.len(), 64);
        let unique: std::collections::HashSet<_> = l.iter().collect();
        assert_eq!(unique.len(), 64);
        assert_eq!(channel_labels(4), ["TP9", "AF7", "AF8", "TP10"]);
        assert_eq!(channel_labels(20).len(), 20);
    }

    #[test]
    fn faults_resolve_by_label_or_index_and_ignore_unknowns() {
        let labels = channel_labels(4);
        let cfg = VirtualSourceConfig {
            channels: 4,
            faults: vec![
                ChannelFault {
                    channel: ChannelRef::Index(3),
                    kind: FaultKind::Noisy,
                },
                ChannelFault {
                    channel: ChannelRef::Label("Cz".into()),
                    kind: FaultKind::Flat,
                },
            ],
            ..Default::default()
        };
        assert_eq!(fault_map(&cfg, &labels), [None, None, None, Some(FaultKind::Noisy)]);
    }

    #[test]
    fn config_parses_faults_and_battery_from_json() {
        let cfg: VirtualSourceConfig = serde_json::from_value(serde_json::json!({
            "channels": 64,
            "template": "drowsy",
            "faults": [{"channel": "T7", "kind": "flat"}, {"channel": 5, "kind": "line_noise"}],
            "battery": {"percent": 30, "drain_per_min": 2},
        }))
        .unwrap();
        assert_eq!(cfg.template, SignalTemplate::Drowsy);
        assert_eq!(cfg.faults.len(), 2);
        assert_eq!(cfg.faults[1].channel, ChannelRef::Index(5));
        assert_eq!(cfg.battery.map(|b| b.drain_per_min), Some(2.0));
    }
}
