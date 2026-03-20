// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! EEG epoch accumulator → ZUNA wgpu embedding → per-date HNSW + SQLite storage.
//!
//! ## Storage layout
//!
//! ```text
//! ~/.skill/
//!   20260223/
//!     eeg_embeddings.hnsw   ← daily HNSW approximate-NN index
//!     eeg.sqlite            ← daily SQLite database
//!   20260224/
//!     ...
//! ```
//!
//! A new date folder is created automatically at midnight (UTC).  Both files
//! are opened at worker startup and re-opened whenever the date rolls over.
//!
//! ## SQLite schema  (`embeddings` table)
//!
//! | column            | type    | notes |
//! |-------------------|---------|-------|
//! | `id`              | INTEGER | PRIMARY KEY AUTOINCREMENT |
//! | `timestamp`       | INTEGER | `YYYYMMDDHHmmss` UTC |
//! | `device_id`       | TEXT    | BLE peripheral id (nullable) |
//! | `device_name`     | TEXT    | headset display name (nullable) |
//! | `hnsw_id`         | INTEGER | zero-based row in the daily HNSW file |
//! | `eeg_embedding`   | BLOB    | `f32 LE × dim` (32 floats = 128 bytes) |
//! | `label`           | TEXT    | user-defined tag (nullable, reserved) |
//! | `extra_embedding` | BLOB    | optional second embedding (nullable) |
//! | `ppg_ambient`     | REAL    | mean PPG ambient ADC value (nullable) |
//! | `ppg_infrared`    | REAL    | mean PPG infrared ADC value (nullable) |
//! | `ppg_red`         | REAL    | mean PPG red ADC value (nullable) |

mod day_store;
mod worker;

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
};

use crate::skill_log::SkillLogger;
use crate::settings::{HookLastTrigger, HookRule};
use skill_eeg::eeg_model_config::{EegModelConfig, EegModelStatus};

use crate::constants::{
    EEG_CHANNELS, CHANNEL_NAMES, EMBEDDING_EPOCH_SAMPLES, EMBEDDING_EPOCH_SECS,
    EMBEDDING_HOP_SAMPLES,
    EMBEDDING_OVERLAP_MAX_SECS, EMBEDDING_OVERLAP_MIN_SECS,
    MUSE_SAMPLE_RATE,
};

// Re-export public items from submodules.
pub(crate) use worker::download_hf_weights;
pub(crate) use worker::luna_variant_config_path;

/// Linearly resample `src` to exactly `target_len` samples.
///
/// **Only used for ZUNA model input preparation.**  All other components
/// (CSV, DSP filter, band analyzer, quality monitor, artifact detection)
/// operate on the original device-native sample rate.
///
/// Converts device-native epoch buffers (e.g. 2500 samples at 500 Hz)
/// to the ZUNA model's expected input size (1280 samples at 256 Hz).
pub(crate) fn resample_linear(src: &[f32], target_len: usize) -> Vec<f32> {
    if src.is_empty() || target_len == 0 { return vec![0.0; target_len]; }
    if src.len() == target_len { return src.to_vec(); }
    let ratio = (src.len() - 1) as f64 / (target_len - 1).max(1) as f64;
    (0..target_len).map(|i| {
        let pos = i as f64 * ratio;
        let lo = pos.floor() as usize;
        let hi = (lo + 1).min(src.len() - 1);
        let frac = (pos - lo as f64) as f32;
        src[lo] * (1.0 - frac) + src[hi] * frac
    }).collect()
}

// ── Message sent to the background worker ─────────────────────────────────────

struct EpochMsg {
    /// Raw µV samples: `[EEG_CHANNELS][EMBEDDING_EPOCH_SAMPLES]`.
    samples:     Vec<Vec<f32>>,
    /// `YYYYMMDDHHmmss` UTC at the epoch boundary.
    timestamp:   i64,
    device_id:   Option<String>,
    device_name: Option<String>,
    /// Channel labels from the connected device (e.g. ["TP9","AF7","AF8","TP10"] for Muse).
    channel_names: Vec<String>,
    /// Hardware sample rate (Hz) of the connected device.
    sample_rate: f32,
    /// Band powers snapshot at the moment this epoch was emitted (may be None
    /// if the band analyzer hasn't produced a result yet).
    band_snapshot: Option<skill_eeg::eeg_bands::BandSnapshot>,
    /// PPG averages for the epoch window: [ambient, infrared, red].
    /// Each value is the mean of all PPG samples received during this epoch.
    /// `None` if no PPG data was received (e.g. Muse 1 which has no PPG sensor).
    ppg_averages: Option<[f64; 3]>,
    /// Derived PPG metrics (HR, HRV, SpO2, etc.).
    ppg_metrics: Option<skill_data::ppg_analysis::PpgMetrics>,
}

// ── cubecl cache warm-up ──────────────────────────────────────────────────────

/// Pre-create the platform GPU-kernel cache directories that `cubecl` uses.
///
/// In `cubecl-common ≤ 0.9.0` the cache loader calls `.unwrap()` on
/// `std::fs::read()` without first checking whether the file exists.  On the
/// very first launch (empty cache) the read returns `Err(NotFound)` and the
/// thread panics.  Creating the directory tree ahead of the wgpu device
/// initialisation prevents that `ENOENT` by ensuring at least the *parent*
/// directory is present; cubecl can then safely stat/create individual cache
/// entries without hitting the missing-ancestor case.
/// Pre-create the cubecl kernel-cache directory tree before any wgpu operation.
///
/// cubecl-common 0.9.0 uses:
///   `dirs::home_dir().join(".cache") / "cubecl" / <crate-version> / <kernel-path>`
///
/// `CacheFile::new` calls `create_dir_all(parent).ok()` — silently swallowing
/// errors — then `File::create(&path).unwrap()`.  On macOS `~/.cache/` does not
/// exist by default (macOS uses `~/Library/Caches`), so `create_dir_all` fails
/// silently, `File::create` gets ENOENT, and the thread panics.
///
/// From a terminal the cache already exists from prior runs, so the bug is
/// invisible there.  Launching as a `.app` (fresh environment, no pre-existing
/// cache) always hits it.
///
/// We pass `skill_dir` so we can derive `$HOME` reliably — even when `$HOME`
/// is absent from the environment — because skill_dir = `$HOME/.skill`.
/// Point cubecl's autotune cache at a known-writable directory inside
/// `skill_dir`, then pre-create it so cubecl's `File::create` never fails.
///
/// ## Why this is necessary
///
/// cubecl-runtime 0.9.0 defaults to `CacheConfig::Target`, which walks up
/// from `std::env::current_dir()` looking for a `Cargo.toml` and uses
/// `<project_root>/target/` as the cache root.
///
/// • **From a terminal** inside the dev tree: `Cargo.toml` is found,
///   `target/` already exists, everything works.
/// • **From a `.app` bundle**: `current_dir()` is `/` (or the bundle path),
///   no `Cargo.toml` is ever found, so it falls back to
///   `current_dir().join("target")` = `/target/`.  macOS returns EACCES
///   when cubecl tries to create `/target/`; that error is silently
///   swallowed (`.ok()`), then `File::create` panics with ENOENT.  The
///   resulting panic poisons the wgpu device's internal mutex for the rest
///   of the process lifetime.
///
/// `GlobalConfig::set()` must be called **before** the first
/// `WgpuDevice::DefaultDevice` access (i.e. before the encoder load).
/// It panics if called a second time, so we guard with `catch_unwind`
/// to make subsequent worker restarts harmless.
use skill_exg::GPU_DEVICE_POISONED;
use skill_exg::yyyymmddhhmmss_utc;

// MUSE_SAMPLE_RATE already imported at the top of this file.

// ── EegAccumulator ────────────────────────────────────────────────────────────

/// Sliding-window EEG accumulator that triggers ZUNA embedding on 5-second
/// epochs with configurable overlap.
const PPG_CHANNELS: usize = crate::constants::PPG_CHANNELS;

pub struct EegAccumulator {
    bufs:        [VecDeque<f32>; EEG_CHANNELS],
    since_last:  [usize; EEG_CHANNELS],
    /// Number of EEG channels the connected device actually uses.
    /// Only `bufs[0..device_channels]` are populated; the rest stay empty
    /// and are zero-filled when building the model input tensor.
    device_channels: usize,
    /// Hop in native samples (device sample rate).
    hop_samples: usize,
    /// Epoch size in native samples (sample_rate × EMBEDDING_EPOCH_SECS).
    native_epoch_samples: usize,
    device_id:   Option<String>,
    device_name: Option<String>,
    /// Channel labels from the connected device, passed to the embedding worker.
    channel_names: Vec<String>,
    /// Hardware sample rate (Hz), passed to the embedding worker.
    sample_rate: f32,
    tx:          mpsc::SyncSender<EpochMsg>,
    /// Latest band power snapshot from the GPU-based BandAnalyzer.
    /// Attached to each epoch message so the worker can store derived metrics
    /// without recomputing any FFT.
    latest_bands: Option<skill_eeg::eeg_bands::BandSnapshot>,
    /// PPG sample accumulators [ambient, infrared, red].
    /// Accumulated between epoch boundaries, averaged and attached to each epoch,
    /// then cleared.
    ppg_sums:   [f64; PPG_CHANNELS],
    ppg_counts: [u64; PPG_CHANNELS],
    /// PPG signal analyzer for HR/HRV/SpO2 computation.
    ppg_analyzer: skill_data::ppg_analysis::PpgAnalyzer,
    /// Cached latest PPG metrics (updated each epoch, read by band snapshot emitter).
    latest_ppg: Option<skill_data::ppg_analysis::PpgMetrics>,
    logger:     Arc<SkillLogger>,
    // ── Worker-restart plumbing ───────────────────────────────────────────────
    // All fields below are cloned each time we (re)spawn the background worker.
    // They must be kept in sync with the parameters passed to `embed_worker`.
    skill_dir:    PathBuf,
    config:       EegModelConfig,
    status:       Arc<Mutex<EegModelStatus>>,
    cancel:       Arc<std::sync::atomic::AtomicBool>,
    /// When set to `true` by `trigger_weights_download`, the running embed
    /// worker will exit its epoch loop and the accumulator will immediately
    /// respawn a fresh worker that re-runs `resolve_hf_weights` and loads the
    /// newly downloaded encoder — no app restart needed.
    reload_requested: Arc<std::sync::atomic::AtomicBool>,
    /// Shared reference to the persistent cross-day global HNSW index.
    /// `None` inside the Option while the startup build is still running.
    global_index: Arc<Mutex<Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>>>,
    hooks: Vec<HookRule>,
    shared_embedder: Arc<crate::label_cmds::EmbedderState>,
    label_idx: Arc<crate::label_index::LabelIndexState>,
    ws_broadcaster: crate::ws_server::WsBroadcaster,
    hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
    app: tauri::AppHandle,
}

impl EegAccumulator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        skill_dir:        PathBuf,
        config:           EegModelConfig,
        status:           Arc<Mutex<EegModelStatus>>,
        cancel:           Arc<std::sync::atomic::AtomicBool>,
        reload_requested: Arc<std::sync::atomic::AtomicBool>,
        logger:           Arc<SkillLogger>,
        global_index:     Arc<Mutex<Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>>>,
        hooks:            Vec<HookRule>,
        shared_embedder:  Arc<crate::label_cmds::EmbedderState>,
        label_idx:        Arc<crate::label_index::LabelIndexState>,
        ws_broadcaster:   crate::ws_server::WsBroadcaster,
        hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
        app: tauri::AppHandle,
    ) -> Self {
        let tx = Self::spawn_worker(
            skill_dir.clone(), config.clone(),
            status.clone(), cancel.clone(), reload_requested.clone(),
            logger.clone(), global_index.clone(),
            hooks.clone(), shared_embedder.clone(), label_idx.clone(), ws_broadcaster.clone(),
            hook_runtime.clone(), app.clone(),
        );
        Self {
            bufs:        std::array::from_fn(|_| VecDeque::new()),
            since_last:  [0; EEG_CHANNELS],
            device_channels: CHANNEL_NAMES.len(),
            hop_samples: EMBEDDING_HOP_SAMPLES,
            native_epoch_samples: EMBEDDING_EPOCH_SAMPLES,
            device_id:    None,
            device_name:  None,
            channel_names: CHANNEL_NAMES.iter().map(|s: &&str| s.to_string()).collect(),
            sample_rate: MUSE_SAMPLE_RATE,
            tx,
            latest_bands: None,
            ppg_sums:   [0.0; PPG_CHANNELS],
            ppg_counts: [0; PPG_CHANNELS],
            ppg_analyzer: skill_data::ppg_analysis::PpgAnalyzer::new(10.0),
            latest_ppg: None,
            logger,
            skill_dir,
            config,
            status,
            cancel,
            reload_requested,
            global_index,
            hooks,
            shared_embedder,
            label_idx,
            ws_broadcaster,
            hook_runtime,
            app,
        }
    }

    /// Spawn a fresh `eeg-embed` worker thread and return the sender half of
    /// its channel.  Called both at construction time and whenever `push()`
    /// detects that the previous worker exited (e.g. after a cubecl panic).
    #[allow(clippy::too_many_arguments)]
    fn spawn_worker(
        skill_dir:        PathBuf,
        config:           EegModelConfig,
        status:           Arc<Mutex<EegModelStatus>>,
        cancel:           Arc<std::sync::atomic::AtomicBool>,
        reload_requested: Arc<std::sync::atomic::AtomicBool>,
        logger:           Arc<SkillLogger>,
        global_index:     Arc<Mutex<Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>>>,
        hooks:            Vec<HookRule>,
        shared_embedder:  Arc<crate::label_cmds::EmbedderState>,
        label_idx:        Arc<crate::label_index::LabelIndexState>,
        ws_broadcaster:   crate::ws_server::WsBroadcaster,
        hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
        app: tauri::AppHandle,
    ) -> mpsc::SyncSender<EpochMsg> {
        let (tx, rx) = mpsc::sync_channel::<EpochMsg>(4);
        std::thread::Builder::new()
            .name("eeg-embed".into())
            .spawn(move || worker::embed_worker(
                rx, skill_dir, config, status, cancel, reload_requested, logger, global_index,
                hooks, shared_embedder, label_idx, ws_broadcaster,
                hook_runtime, app,
            ))
            .expect("[embed] failed to spawn background thread");
        tx
    }

    /// Restart the background worker after it has exited unexpectedly (or
    /// after a deliberate reload request from `trigger_weights_download`).
    fn restart_worker(&mut self) {
        skill_log!(self.logger, "embedder", "restarting embed worker");
        self.tx = Self::spawn_worker(
            self.skill_dir.clone(),
            self.config.clone(),
            self.status.clone(),
            self.cancel.clone(),
            self.reload_requested.clone(),
            self.logger.clone(),
            self.global_index.clone(),
            self.hooks.clone(),
            self.shared_embedder.clone(),
            self.label_idx.clone(),
            self.ws_broadcaster.clone(),
            self.hook_runtime.clone(),
            self.app.clone(),
        );
    }

    /// Update the latest band snapshot (called from lib.rs whenever the
    /// GPU-based BandAnalyzer produces a new result, ~4 Hz).
    pub fn update_bands(&mut self, snap: skill_eeg::eeg_bands::BandSnapshot) {
        self.latest_bands = Some(snap);
    }

    /// Update device info included in every subsequent epoch message.
    pub fn update_device(&mut self, id: Option<String>, name: Option<String>) {
        self.device_id   = id;
        self.device_name = name;
    }

    /// Update channel names and sample rate for the connected device.
    ///
    /// Called by the session runner after a device connects so the embedding
    /// worker receives the correct channel labels and sample rate.
    pub fn set_device_channels(&mut self, names: Vec<String>, sample_rate: f32) {
        self.device_channels = names.len().min(EEG_CHANNELS);
        self.channel_names = names;
        self.sample_rate   = sample_rate;
        // Recompute native epoch/hop for the new sample rate.
        self.native_epoch_samples = (sample_rate * EMBEDDING_EPOCH_SECS).round() as usize;
        // Preserve the current overlap in seconds.
        let epoch_native = self.native_epoch_samples;
        let hop_frac = self.hop_samples as f32 / EMBEDDING_EPOCH_SAMPLES as f32;
        self.hop_samples = (epoch_native as f32 * hop_frac).round().max(1.0) as usize;
        // Clear buffers for the new channel configuration.
        for b in &mut self.bufs { b.clear(); }
        self.since_last = [0; EEG_CHANNELS];
    }

    /// Update the overlap between consecutive epochs (seconds).
    pub fn set_overlap_secs(&mut self, secs: f32) {
        let clamped       = secs.clamp(EMBEDDING_OVERLAP_MIN_SECS, EMBEDDING_OVERLAP_MAX_SECS);
        let overlap_samps = (clamped * self.sample_rate).round() as usize;
        self.hop_samples  = self.native_epoch_samples.saturating_sub(overlap_samps).max(1);
        self.since_last   = [0; EEG_CHANNELS];
        skill_log!(self.logger, "embedder", "overlap set to {clamped:.2} s → hop={} samples", self.hop_samples);
    }

    /// Replace hook configuration and restart the worker so it picks up changes.
    pub fn set_hooks(&mut self, hooks: Vec<HookRule>) {
        self.hooks = hooks;
        self.restart_worker();
    }

    /// Accumulate PPG samples for `channel` (0=ambient, 1=infrared, 2=red).
    /// These are averaged over the epoch window and stored alongside EEG embeddings.
    pub fn push_ppg(&mut self, channel: usize, samples: &[f64]) {
        if channel >= PPG_CHANNELS { return; }
        for &v in samples {
            self.ppg_sums[channel] += v;
            self.ppg_counts[channel] += 1;
        }
        // Also feed the PPG analyzer for HR/HRV computation
        self.ppg_analyzer.push(channel, samples);
    }

    /// Push raw µV samples at the device's native sample rate.
    ///
    /// Samples are accumulated at native rate.  When a full epoch
    /// (`EMBEDDING_EPOCH_SECS` seconds) is collected, the data is resampled
    /// to `EMBEDDING_EPOCH_SAMPLES` (1280 @ 256 Hz) for the ZUNA model.
    /// This is the **only** place resampling occurs — all other DSP
    /// (filter, bands, quality, CSV) uses native-rate data.
    pub fn push(&mut self, electrode: usize, samples: &[f32]) {
        if electrode >= EEG_CHANNELS { return; }

        self.bufs[electrode].extend(samples.iter().copied());
        self.since_last[electrode] += samples.len();

        let n_ch = self.device_channels;
        let native_epoch = self.native_epoch_samples;

        // Only check active device channels (0..n_ch) — inactive channels
        // (n_ch..EEG_CHANNELS) are never pushed and would block forever.
        let min_buf        = self.bufs[..n_ch].iter().map(|b| b.len()).min().unwrap_or(0);
        let min_since_last = self.since_last[..n_ch].iter().copied().min().unwrap_or(0);

        if min_buf < native_epoch || min_since_last < self.hop_samples {
            return;
        }

        // Build epoch: extract `native_epoch` samples from each active channel,
        // resample to EMBEDDING_EPOCH_SAMPLES (1280) for the ZUNA model, and
        // zero-fill inactive channels so the tensor is always EEG_CHANNELS wide.
        let epoch: Vec<Vec<f32>> = (0..EEG_CHANNELS)
            .map(|ch| {
                let b = &self.bufs[ch];
                if ch >= n_ch || b.len() < native_epoch {
                    // Inactive or under-filled channel → zero-fill.
                    vec![0.0f32; EMBEDDING_EPOCH_SAMPLES]
                } else {
                    let raw: Vec<f32> = b.iter()
                        .skip(b.len() - native_epoch)
                        .copied()
                        .collect();
                    if native_epoch == EMBEDDING_EPOCH_SAMPLES {
                        raw // already 256 Hz — no resampling needed
                    } else {
                        resample_linear(&raw, EMBEDDING_EPOCH_SAMPLES)
                    }
                }
            })
            .collect();

        // Only drain active channel buffers.
        for b in &mut self.bufs[..n_ch] { b.drain(..self.hop_samples); }
        self.since_last = [0; EEG_CHANNELS];

        // Compute PPG averages for this epoch, then reset accumulators.
        let ppg_averages = if self.ppg_counts.iter().any(|&c| c > 0) {
            let avgs = std::array::from_fn(|i| {
                if self.ppg_counts[i] > 0 {
                    self.ppg_sums[i] / self.ppg_counts[i] as f64
                } else {
                    0.0
                }
            });
            self.ppg_sums   = [0.0; PPG_CHANNELS];
            self.ppg_counts = [0; PPG_CHANNELS];
            Some(avgs)
        } else {
            None
        };

        // Compute PPG-derived metrics (HR, HRV, SpO2, etc.)
        let ppg_epoch_samples = (crate::constants::EMBEDDING_EPOCH_SECS as f64 * 64.0) as usize;
        let ppg_metrics = self.ppg_analyzer.compute_epoch(ppg_epoch_samples);
        if let Some(ref pm) = ppg_metrics {
            self.latest_ppg = Some(pm.clone());
        }

        let msg = EpochMsg {
            samples:       epoch,
            timestamp:     yyyymmddhhmmss_utc(),
            device_id:     self.device_id.clone(),
            device_name:   self.device_name.clone(),
            channel_names: self.channel_names.clone(),
            sample_rate:   self.sample_rate,
            band_snapshot: self.latest_bands.clone(),
            ppg_averages,
            ppg_metrics,
        };
        if let Err(e) = self.tx.try_send(msg) {
            match e {
                mpsc::TrySendError::Full(_) => {
                    skill_log!(self.logger, "embedder", "epoch dropped — worker busy (channel full)");
                }
                mpsc::TrySendError::Disconnected(_) => {
                    // The worker thread exited unexpectedly.  If the wgpu
                    // device was permanently poisoned by a cubecl panic, DO
                    // NOT respawn — the new worker would hit the same poisoned
                    // mutex immediately.  Let the accumulator go quiet instead;
                    // the next app restart will get a fresh process with clean
                    // device state.
                    if GPU_DEVICE_POISONED.load(std::sync::atomic::Ordering::Relaxed) {
                        skill_log!(self.logger, "embedder",
                            "worker exited and wgpu device is poisoned — NOT respawning; \
                             GPU embeddings disabled until app restart");
                    } else {
                        skill_log!(self.logger, "embedder",
                            "worker thread exited unexpectedly — respawning");
                        self.restart_worker();
                    }
                }
            }
        }
    }

    /// Return the most recently computed PPG metrics (if any).
    pub fn latest_ppg(&self) -> Option<&skill_data::ppg_analysis::PpgMetrics> {
        self.latest_ppg.as_ref()
    }

}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_identity() {
        let src: Vec<f32> = (0..1280).map(|i| i as f32).collect();
        let out = resample_linear(&src, 1280);
        assert_eq!(out.len(), 1280);
        assert_eq!(out, src);
    }

    #[test]
    fn resample_upsample() {
        // 640 samples → 1280 (doubles)
        let src: Vec<f32> = (0..640).map(|i| i as f32).collect();
        let out = resample_linear(&src, 1280);
        assert_eq!(out.len(), 1280);
        // First and last should be preserved exactly.
        assert!((out[0] - 0.0).abs() < 1e-5);
        assert!((out[1279] - 639.0).abs() < 1e-3);
    }

    #[test]
    fn resample_downsample() {
        // 2500 samples (500 Hz × 5 s) → 1280
        let src: Vec<f32> = (0..2500).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = resample_linear(&src, 1280);
        assert_eq!(out.len(), 1280);
        // Endpoints preserved.
        assert!((out[0] - src[0]).abs() < 1e-5);
        assert!((out[1279] - src[2499]).abs() < 1e-3);
    }

    #[test]
    fn resample_empty_source() {
        let out = resample_linear(&[], 1280);
        assert_eq!(out.len(), 1280);
        assert!(out.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn resample_zero_target() {
        let out = resample_linear(&[1.0, 2.0, 3.0], 0);
        assert!(out.is_empty());
    }
}

