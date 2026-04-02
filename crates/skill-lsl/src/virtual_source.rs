// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Virtual LSL EEG source — pushes synthetic 32-channel 256 Hz data.
//!
//! Used for testing the LSL pipeline without real hardware.  The outlet
//! streams a deterministic sine-wave signal across all 32 standard 10-20
//! channels so the signal is visually inspectable in the UI.
//!
//! # Concurrency note
//! `rlsl::outlet::StreamOutlet::new` calls `tokio::Runtime::block_on`
//! internally, which panics on Tokio worker threads.  All rlsl operations
//! therefore run on a dedicated raw OS thread.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

/// Standard 32-channel 10-20 montage used by the virtual source.
pub const VIRTUAL_CHANNELS: usize = 32;
pub const VIRTUAL_SAMPLE_RATE: f64 = 256.0;
pub const VIRTUAL_STREAM_NAME: &str = "SkillVirtualEEG";
pub const VIRTUAL_STREAM_TYPE: &str = "EEG";
pub const VIRTUAL_SOURCE_ID: &str = "skill-virtual-eeg-001";

const CHANNEL_LABELS: [&str; 32] = [
    "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "FC5", "FC1", "FC2", "FC6", "T7", "C3", "Cz", "C4", "T8", "TP9", "CP5",
    "CP1", "CP2", "CP6", "TP10", "P7", "P3", "Pz", "P4", "P8", "PO9", "O1", "Oz", "O2", "PO10",
];

/// Handle to a running virtual LSL source.  Drop to stop it.
pub struct VirtualLslSource {
    shutdown: Arc<AtomicBool>,
}

impl VirtualLslSource {
    /// Start the virtual source on a dedicated OS thread.
    ///
    /// Returns immediately; the outlet thread runs in the background.
    /// Call [`VirtualLslSource::stop`] or drop this handle to shut it down.
    pub fn start() -> Result<Self, String> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown2 = shutdown.clone();

        std::thread::Builder::new()
            .name("skill-virtual-lsl".into())
            .spawn(move || run_virtual_outlet(shutdown2))
            .map_err(|e| format!("failed to spawn virtual LSL thread: {e}"))?;

        Ok(Self { shutdown })
    }

    /// Signal the outlet thread to exit.
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    /// Returns `true` while the thread is still running.
    pub fn is_running(&self) -> bool {
        !self.shutdown.load(Ordering::Acquire)
    }
}

impl Drop for VirtualLslSource {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── Outlet thread ──────────────────────────────────────────────────────────────

fn run_virtual_outlet(shutdown: Arc<AtomicBool>) {
    use rlsl::prelude::*;
    use rlsl::types::ChannelFormat;

    let info = StreamInfo::new(
        VIRTUAL_STREAM_NAME,
        VIRTUAL_STREAM_TYPE,
        VIRTUAL_CHANNELS as u32,
        VIRTUAL_SAMPLE_RATE,
        ChannelFormat::Float32,
        VIRTUAL_SOURCE_ID,
    );

    // Attach 10-20 channel labels to the XML description.
    {
        let desc = info.desc();
        let channels_node = desc.append_child("channels");
        for &label in CHANNEL_LABELS.iter() {
            let ch = channels_node.append_child("channel");
            ch.append_child_value("label", label);
            ch.append_child_value("unit", "microvolts");
            ch.append_child_value("type", "EEG");
        }
    }

    // StreamOutlet::new calls block_on internally — fine on a raw OS thread.
    let outlet = StreamOutlet::new(&info, 0, 360);
    eprintln!(
        "[lsl-virtual] outlet started — {VIRTUAL_CHANNELS} ch @ {VIRTUAL_SAMPLE_RATE} Hz \
         stream='{VIRTUAL_STREAM_NAME}'"
    );

    // Push 32-sample chunks (~125 ms each) at real-time pace.
    const CHUNK: usize = 32;
    let chunk_delay = Duration::from_secs_f64(CHUNK as f64 / VIRTUAL_SAMPLE_RATE);
    let mut t: u64 = 0; // sample counter for phase

    while !shutdown.load(Ordering::Acquire) {
        for _ in 0..CHUNK {
            let sample: Vec<f32> = (0..VIRTUAL_CHANNELS)
                .map(|ch| {
                    // Each channel gets a distinct sine frequency so they look
                    // different in any waveform viewer.
                    let freq_hz = 1.0 + ch as f64 * 0.5; // 1..16.5 Hz
                    let phase = 2.0 * std::f64::consts::PI * freq_hz * t as f64 / VIRTUAL_SAMPLE_RATE;
                    (phase.sin() * 50.0) as f32 // ±50 µV amplitude
                })
                .collect();
            outlet.push_sample_f(&sample, 0.0, true);
            t = t.wrapping_add(1);
        }
        std::thread::sleep(chunk_delay);
    }

    eprintln!("[lsl-virtual] outlet stopped");
}
