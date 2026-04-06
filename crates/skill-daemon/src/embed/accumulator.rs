// SPDX-License-Identifier: GPL-3.0-only
//! Sliding-window EEG epoch accumulator.
//!
//! Collects raw samples at the device's native sample rate.  When a full
//! epoch (5 s) has been collected across all active channels, the data is
//! resampled to the model's expected rate and sent to the embed worker.

use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::Instant;

use skill_constants::{EEG_CHANNELS, EMBEDDING_EPOCH_SAMPLES, EMBEDDING_EPOCH_SECS, EMBEDDING_HOP_SAMPLES};
use tracing::info;

/// Message sent to the background embed worker.
pub(crate) struct EpochMsg {
    /// Raw µV samples: `[n_channels][EMBEDDING_EPOCH_SAMPLES]`.
    /// Already resampled to MUSE_SAMPLE_RATE (256 Hz).
    pub samples: Vec<Vec<f32>>,
    /// Timestamp (YYYYMMDDHHmmss UTC) at the epoch boundary.
    pub timestamp: i64,
    pub device_name: Option<String>,
    pub channel_names: Vec<String>,
    #[allow(dead_code)]
    pub sample_rate: f32,
    /// Band snapshot at the moment this epoch was emitted.
    pub band_snapshot: Option<skill_eeg::eeg_bands::BandSnapshot>,
}

/// Sliding-window accumulator that fires epoch messages when enough data
/// has been collected.
pub struct EpochAccumulator {
    bufs: [VecDeque<f32>; EEG_CHANNELS],
    since_last: [usize; EEG_CHANNELS],
    last_push_at: Instant,
    device_channels: usize,
    hop_samples: usize,
    native_epoch_samples: usize,
    device_name: Option<String>,
    channel_names: Vec<String>,
    sample_rate: f32,
    latest_bands: Option<skill_eeg::eeg_bands::BandSnapshot>,
    pub(crate) tx: mpsc::SyncSender<EpochMsg>,
}

impl EpochAccumulator {
    pub fn new(
        tx: mpsc::SyncSender<EpochMsg>,
        device_channels: usize,
        sample_rate: f32,
        channel_names: Vec<String>,
    ) -> Self {
        let native_epoch = (sample_rate * EMBEDDING_EPOCH_SECS).round() as usize;
        // Preserve default overlap ratio.
        let hop_frac = EMBEDDING_HOP_SAMPLES as f32 / EMBEDDING_EPOCH_SAMPLES as f32;
        let hop = (native_epoch as f32 * hop_frac).round().max(1.0) as usize;

        Self {
            bufs: std::array::from_fn(|_| VecDeque::new()),
            since_last: [0; EEG_CHANNELS],
            last_push_at: Instant::now(),
            device_channels: device_channels.min(EEG_CHANNELS),
            hop_samples: hop,
            native_epoch_samples: native_epoch,
            device_name: None,
            channel_names,
            sample_rate,
            latest_bands: None,
            tx,
        }
    }

    pub fn set_device_name(&mut self, name: String) {
        self.device_name = Some(name);
    }

    pub fn update_bands(&mut self, snap: skill_eeg::eeg_bands::BandSnapshot) {
        self.latest_bands = Some(snap);
    }

    /// Push raw µV samples for one electrode at the device's native rate.
    pub fn push(&mut self, electrode: usize, samples: &[f32]) {
        if electrode >= EEG_CHANNELS || samples.is_empty() {
            return;
        }

        // Discard stale data after long gap.
        let now = Instant::now();
        if now.duration_since(self.last_push_at).as_secs() > 30 {
            let has_data = self.bufs[..self.device_channels].iter().any(|b| !b.is_empty());
            if has_data {
                info!("discarding stale epoch data");
                for b in &mut self.bufs {
                    b.clear();
                }
                self.since_last = [0; EEG_CHANNELS];
            }
        }
        self.last_push_at = now;

        self.bufs[electrode].extend(samples.iter().copied());
        self.since_last[electrode] += samples.len();

        let n_ch = self.device_channels;
        let native_epoch = self.native_epoch_samples;

        let min_buf = self.bufs[..n_ch].iter().map(VecDeque::len).min().unwrap_or(0);
        let min_since = self.since_last[..n_ch].iter().copied().min().unwrap_or(0);

        if min_buf < native_epoch || min_since < self.hop_samples {
            return;
        }

        // Build epoch: extract native_epoch samples, resample to EMBEDDING_EPOCH_SAMPLES.
        let epoch: Vec<Vec<f32>> = (0..EEG_CHANNELS)
            .map(|ch| {
                let b = &self.bufs[ch];
                if ch >= n_ch || b.len() < native_epoch {
                    vec![0.0f32; EMBEDDING_EPOCH_SAMPLES]
                } else {
                    let raw: Vec<f32> = b.iter().skip(b.len() - native_epoch).copied().collect();
                    if native_epoch == EMBEDDING_EPOCH_SAMPLES {
                        raw
                    } else {
                        resample_linear(&raw, EMBEDDING_EPOCH_SAMPLES)
                    }
                }
            })
            .collect();

        // Drain hop from active channels.
        for b in &mut self.bufs[..n_ch] {
            let drain = self.hop_samples.min(b.len());
            b.drain(..drain);
        }
        self.since_last = [0; EEG_CHANNELS];

        let msg = EpochMsg {
            samples: epoch,
            timestamp: skill_exg::yyyymmddhhmmss_utc(),
            device_name: self.device_name.clone(),
            channel_names: self.channel_names.clone(),
            sample_rate: self.sample_rate,
            band_snapshot: self.latest_bands.clone(),
        };
        if let Err(e) = self.tx.try_send(msg) {
            match e {
                mpsc::TrySendError::Full(_) => {
                    info!("epoch dropped — embed worker busy");
                }
                mpsc::TrySendError::Disconnected(_) => {
                    info!("embed worker disconnected — epochs will be dropped");
                }
            }
        }
    }
}

/// Linearly resample `src` to `target_len` samples.
fn resample_linear(src: &[f32], target_len: usize) -> Vec<f32> {
    if src.is_empty() || target_len == 0 {
        return vec![0.0; target_len];
    }
    if src.len() == target_len {
        return src.to_vec();
    }
    let ratio = (src.len() - 1) as f64 / (target_len - 1).max(1) as f64;
    (0..target_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let lo = pos.floor() as usize;
            let hi = (lo + 1).min(src.len() - 1);
            let frac = (pos - lo as f64) as f32;
            src[lo] * (1.0 - frac) + src[hi] * frac
        })
        .collect()
}
