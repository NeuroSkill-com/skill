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
///
/// `samples` / `channel_names` are consumed only by the encoder functions,
/// which are all feature-gated — so with no encoder compiled in they are
/// genuinely unread. Gated precisely rather than blanket-`allow`ed so the
/// lint still fires if a shipped configuration stops using them.
#[cfg_attr(
    not(any(
        feature = "embed-zuna",
        feature = "embed-luna",
        feature = "embed-reve",
        feature = "embed-tribev2",
        feature = "embed-neurorvq",
        feature = "embed-eegdino",
        feature = "embed-lumamba",
    )),
    allow(dead_code)
)]
pub(crate) struct EpochMsg {
    /// Raw µV samples: `[n_channels][EMBEDDING_EPOCH_SAMPLES]`.
    /// Already resampled to MUSE_SAMPLE_RATE (256 Hz).
    pub samples: Vec<Vec<f32>>,
    /// Timestamp (YYYYMMDDHHmmss UTC) at the epoch boundary.
    pub timestamp: i64,
    pub device_name: Option<String>,
    pub channel_names: Vec<String>,
    /// Sample rate of `samples` (the embedding rate after resampling, not the
    /// device's native rate).
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
    max_buf_samples: usize,
    device_name: Option<String>,
    channel_names: Vec<String>,
    latest_bands: Option<skill_eeg::eeg_bands::BandSnapshot>,
    pub(crate) tx: mpsc::SyncSender<EpochMsg>,
    /// Epochs handed to the embed worker, and epochs it had no room for.
    epochs_sent: u64,
    epochs_dropped: u64,
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
            max_buf_samples: native_epoch * 4,
            device_name: None,
            channel_names,
            latest_bands: None,
            tx,
            epochs_sent: 0,
            epochs_dropped: 0,
        }
    }

    /// `(sent, dropped)`: epochs handed to the embed worker, and epochs it
    /// had no room for (or was gone for).
    pub fn epoch_counts(&self) -> (u64, u64) {
        (self.epochs_sent, self.epochs_dropped)
    }

    pub fn set_device_name(&mut self, name: String) {
        self.device_name = Some(name);
    }

    pub fn update_bands(&mut self, snap: skill_eeg::eeg_bands::BandSnapshot) {
        self.latest_bands = Some(snap);
    }

    /// Push one sample of every channel (a whole frame). Same result as
    /// `push` once per electrode, with the epoch check done once.
    pub fn push_frame(&mut self, channels: &[f64]) {
        let n = channels.len().min(EEG_CHANNELS);
        if n == 0 {
            return;
        }
        // The first electrode goes through `push` (which discards stale data
        // after a long pause, before anything of this frame is queued), the
        // last one too (which checks for a complete epoch once the frame is
        // in); the ones between only queue.
        let direct = n > 2 && self.bufs[..n].iter().all(|b| b.len() < self.max_buf_samples);
        if !direct {
            for (el, &v) in channels[..n].iter().enumerate() {
                self.push(el, &[v as f32]);
            }
            return;
        }
        self.push(0, &[channels[0] as f32]);
        for (el, &v) in channels[..n - 1].iter().enumerate().skip(1) {
            self.bufs[el].push_back(v as f32);
            self.since_last[el] += 1;
        }
        self.push(n - 1, &[channels[n - 1] as f32]);
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

        // Per-channel cap: when other electrodes stall, this channel would
        // otherwise grow without bound (the drain step requires min_buf across
        // all channels). Drop oldest down to one epoch's worth.
        if self.bufs[electrode].len() > self.max_buf_samples {
            let drop = self.bufs[electrode].len() - self.native_epoch_samples;
            self.bufs[electrode].drain(..drop);
            self.since_last[electrode] = self.since_last[electrode].min(self.native_epoch_samples);
            info!(
                channel = electrode,
                dropped = drop,
                "epoch buf overflow — channel imbalance, dropping oldest samples"
            );
        }

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
            // The rate of `epoch` as sent — it was resampled to
            // EMBEDDING_EPOCH_SAMPLES above. This used to be the device's
            // native rate, so a 250/500 Hz device told the encoder the wrong
            // rate for 256 Hz data.
            sample_rate: EMBEDDING_EPOCH_SAMPLES as f32 / EMBEDDING_EPOCH_SECS,
            band_snapshot: self.latest_bands.clone(),
        };
        if let Err(e) = self.tx.try_send(msg) {
            self.epochs_dropped += 1;
            match e {
                mpsc::TrySendError::Full(_) => {
                    info!("epoch dropped — embed worker busy");
                }
                mpsc::TrySendError::Disconnected(_) => {
                    info!("embed worker disconnected — epochs will be dropped");
                }
            }
        } else {
            self.epochs_sent += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Whole frames produce the same epochs as one electrode at a time.
    #[test]
    fn frames_give_the_same_epochs_as_per_electrode_pushes() {
        let run = |frames: bool| -> Vec<Vec<Vec<f32>>> {
            let (tx, rx) = mpsc::sync_channel::<EpochMsg>(64);
            let mut acc = EpochAccumulator::new(tx, 8, 500.0, (0..8).map(|i| format!("C{i}")).collect());
            for k in 0..(500 * 12) {
                let frame: Vec<f64> = (0..8).map(|c| f64::from(k) + f64::from(c) / 10.0).collect();
                if frames {
                    acc.push_frame(&frame);
                } else {
                    for (el, &v) in frame.iter().enumerate() {
                        acc.push(el, &[v as f32]);
                    }
                }
            }
            let (sent, dropped) = acc.epoch_counts();
            let epochs: Vec<Vec<Vec<f32>>> = rx.try_iter().map(|m| m.samples).collect();
            assert_eq!((sent as usize, dropped), (epochs.len(), 0));
            epochs
        };
        let (a, b) = (run(false), run(true));
        assert!(a.len() >= 2, "{} epochs in 12 s", a.len());
        assert_eq!(a, b);
    }

    #[test]
    fn epoch_reports_the_rate_it_was_resampled_to() {
        let (tx, rx) = mpsc::sync_channel(4);
        let mut acc = EpochAccumulator::new(tx, 2, 500.0, vec!["C3".into(), "C4".into()]);
        for _ in 0..(500 * 6) {
            acc.push(0, &[1.0]);
            acc.push(1, &[2.0]);
        }
        let msg = rx.try_recv().expect("an epoch after 6 s at 500 Hz");
        assert_eq!(msg.samples[0].len(), EMBEDDING_EPOCH_SAMPLES);
        assert_eq!(msg.sample_rate, EMBEDDING_EPOCH_SAMPLES as f32 / EMBEDDING_EPOCH_SECS);
    }
}
