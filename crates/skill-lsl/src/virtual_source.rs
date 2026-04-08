// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Configurable virtual LSL EEG source.

//!
//! Streams synthetic EEG data so the full pipeline (LSL discovery → session
//! connect → daemon WebSocket → dashboard) can be exercised without hardware.
//!
//! The `VirtualSourceConfig` controls channel count, sample rate, signal
//! template (sine / good_quality / bad_quality / interruptions), noise floor,
//! line-noise injection, and per-sample dropout probability.
//!
//! # Concurrency note
//! `rlsl::outlet::StreamOutlet::new` calls `tokio::Runtime::block_on`
//! internally, which panics on Tokio worker threads.  All rlsl operations
//! therefore run on a dedicated raw OS thread.

use anyhow::Context as _;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

// ── Public constants ──────────────────────────────────────────────────────────

pub const VIRTUAL_STREAM_NAME: &str = "SkillVirtualEEG";
pub const VIRTUAL_STREAM_TYPE: &str = "EEG";
pub const VIRTUAL_SOURCE_ID: &str = "skill-virtual-eeg-001";

// ── Channel label tables ──────────────────────────────────────────────────────

const LABELS_4: [&str; 4] = ["TP9", "AF7", "AF8", "TP10"];
const LABELS_8: [&str; 8] = ["Fp1", "Fp2", "F3", "F4", "C3", "C4", "O1", "O2"];
const LABELS_16: [&str; 16] = [
    "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "T7", "C3", "Cz", "C4", "T8", "P3", "Pz", "P4", "O1",
];
const LABELS_32: [&str; 32] = [
    "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "FC5", "FC1", "FC2", "FC6", "T7", "C3", "Cz", "C4", "T8", "TP9", "CP5",
    "CP1", "CP2", "CP6", "TP10", "P7", "P3", "Pz", "P4", "P8", "PO9", "O1", "Oz", "O2", "PO10",
];

fn channel_labels(n: usize) -> Vec<String> {
    let named: Vec<&str> = match n {
        1 => LABELS_4[..1].to_vec(),
        2 => LABELS_4[..2].to_vec(),
        4 => LABELS_4.to_vec(),
        8 => LABELS_8.to_vec(),
        16 => LABELS_16.to_vec(),
        32 => LABELS_32.to_vec(),
        _ => {
            // For other counts slice from LABELS_32, appending generic names if needed.
            let mut out: Vec<&str> = LABELS_32[..n.min(32)].to_vec();
            if n > 32 {
                // Caller shouldn't go beyond 32 but be safe.
                out.truncate(n);
            }
            out
        }
    };
    named.iter().take(n).map(|s| s.to_string()).collect()
}

// ── Configuration ─────────────────────────────────────────────────────────────

/// Signal template — determines spectral content and artefact pattern.
#[derive(Debug, Clone, Default, serde::Deserialize)]
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

/// All parameters accepted by `/v1/lsl/virtual-source/start`.
/// Unknown fields are ignored; missing fields use their defaults so that a
/// plain `{}` body still produces a valid 32-ch 256 Hz source.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct VirtualSourceConfig {
    /// EEG channel count (default 32).
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
    /// Line-noise injection: "none" | "50hz" | "60hz" (default "none").
    #[serde(default = "default_line_noise")]
    pub line_noise: String,
    /// Per-second dropout probability 0–1 (default 0).
    #[serde(default)]
    pub dropout_prob: f64,
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
        }
    }
}

// ── Handle ────────────────────────────────────────────────────────────────────

/// Handle to a running virtual LSL source.  Drop to stop it.
pub struct VirtualLslSource {
    shutdown: Arc<AtomicBool>,
    pub config: VirtualSourceConfig,
}

impl VirtualLslSource {
    /// Start the virtual source on a dedicated OS thread and return immediately.
    pub fn start(config: VirtualSourceConfig) -> anyhow::Result<Self> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown2 = shutdown.clone();
        let cfg_clone = config.clone();

        std::thread::Builder::new()
            .name("skill-virtual-lsl".into())
            .spawn(move || run_virtual_outlet(shutdown2, cfg_clone))
            .context("failed to spawn virtual LSL thread")?;

        Ok(Self { shutdown, config })
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

/// Generate one sample value for channel `ch` at time index `t`.
fn generate_sample(ch: usize, t: u64, cfg: &VirtualSourceConfig, rng: &mut SmallRng, in_dropout: bool) -> f32 {
    if in_dropout {
        return 0.0;
    }

    let sr = cfg.sample_rate;
    let t_s = t as f64 / sr;
    let amp = cfg.amplitude_uv;

    // Phase offset per channel so channels look different
    let ch_offset = ch as f64 * (TWO_PI / cfg.channels.max(1) as f64);

    let base = match &cfg.template {
        SignalTemplate::Sine => {
            // Each channel a distinct frequency: 1 Hz + 0.5 Hz steps
            let freq = 1.0 + ch as f64 * 0.5;
            (TWO_PI * freq * t_s + ch_offset).sin() * amp
        }

        SignalTemplate::GoodQuality => {
            // Dominant alpha (10 Hz) with a small amount of delta/theta/beta
            let alpha = (TWO_PI * 10.1 * t_s + ch_offset).sin() * amp * 0.65;
            let delta = (TWO_PI * 2.0 * t_s).sin() * amp * 0.15;
            let theta = (TWO_PI * 6.0 * t_s + ch_offset).sin() * amp * 0.10;
            let beta = (TWO_PI * 20.0 * t_s).sin() * amp * 0.10;
            alpha + delta + theta + beta
        }

        SignalTemplate::BadQuality => {
            // Low alpha + high-frequency muscle artefact bursts
            let alpha = (TWO_PI * 9.0 * t_s + ch_offset).sin() * amp * 0.3;
            // Muscle: 50–150 Hz – use a mix of high-freq sinusoids
            let muscle = (TWO_PI * 80.0 * t_s).sin() * amp * 0.5 + (TWO_PI * 120.0 * t_s).sin() * amp * 0.3;
            alpha + muscle
        }

        SignalTemplate::Interruptions => {
            // Good alpha signal — dropout is handled by `in_dropout` flag above
            (TWO_PI * 10.0 * t_s + ch_offset).sin() * amp * 0.7
        }
    };

    // Gaussian noise scaled to noise_uv (approximate: σ ≈ noise_uv)
    let noise = if cfg.noise_uv > 0.0 {
        // Box–Muller approximate Gaussian
        let u: f64 = rng.random::<f64>().max(1e-10);
        let v: f64 = rng.random::<f64>();
        let z = (-2.0 * u.ln()).sqrt() * (TWO_PI * v).cos();
        z * cfg.noise_uv
    } else {
        0.0
    };

    // Line-noise injection (50 or 60 Hz)
    let line = match cfg.line_noise.as_str() {
        "50hz" => (TWO_PI * 50.0 * t_s).sin() * amp * 0.25,
        "60hz" => (TWO_PI * 60.0 * t_s).sin() * amp * 0.25,
        _ => 0.0,
    };

    // SNR: scale signal component by quality snr (higher = less noise effect)
    // We already baked noise into the sample so just add components.
    let snr = quality_snr(&cfg.quality);
    let signal_scale = snr / (snr + 1.0); // approaches 1 for excellent, 0.33 for poor

    ((base * signal_scale) + noise + line) as f32
}

// ── Outlet thread ─────────────────────────────────────────────────────────────

fn run_virtual_outlet(shutdown: Arc<AtomicBool>, cfg: VirtualSourceConfig) {
    use rlsl::prelude::*;
    use rlsl::types::ChannelFormat;

    let n_ch = cfg.channels.clamp(1, 64);
    let sr = cfg.sample_rate.max(1.0);
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
         template={:?} quality={} stream='{VIRTUAL_STREAM_NAME}'",
        cfg.template, cfg.quality,
    );

    // Push chunks of ~32 samples at real-time pace.
    let chunk: usize = (sr / 8.0).ceil() as usize; // ~32 samples at 256 Hz
    let chunk_delay = Duration::from_secs_f64(chunk as f64 / sr);

    let mut t: u64 = 0;
    let mut rng = SmallRng::seed_from_u64(42);
    let dropout_rate = cfg.dropout_prob.clamp(0.0, 1.0);
    // Per-channel dropout state (each channel can independently be in dropout)
    let mut dropout_timer: Vec<u64> = vec![0u64; n_ch];

    while !shutdown.load(Ordering::Acquire) {
        for _ in 0..chunk {
            let sample: Vec<f32> = (0..n_ch)
                .map(|ch| {
                    // Check / update dropout state for this channel
                    if dropout_timer[ch] > 0 {
                        dropout_timer[ch] -= 1;
                        generate_sample(ch, t, &cfg, &mut rng, true)
                    } else {
                        // Probabilistic dropout onset (~dropout_rate events/sec)
                        let p_onset = dropout_rate / sr;
                        if rng.random::<f64>() < p_onset {
                            // Dropout lasts 0.05–0.5 s
                            let dur_samples = (rng.random::<f64>() * 0.45 * sr + 0.05 * sr) as u64;
                            dropout_timer[ch] = dur_samples;
                        }
                        generate_sample(ch, t, &cfg, &mut rng, false)
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
