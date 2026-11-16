// SPDX-License-Identifier: GPL-3.0-only
//! A stream of integers in nanovolts (as `nesdk-lsl --int` publishes) reaches
//! the session in microvolts, exactly, and says what one count is worth; a
//! stream of floats in another unit of voltage is converted too.
#![allow(clippy::unwrap_used, clippy::panic)]

use rlsl::prelude::*;
use rlsl::types::ChannelFormat;
use skill_devices::session::{DeviceAdapter, DeviceEvent};
use std::time::{Duration, Instant};

fn describe(info: &StreamInfo, labels: &[&str], unit: &str) {
    let channels = info.desc().append_child("channels");
    for label in labels {
        let ch = channels.append_child("channel");
        ch.append_child_value("label", label);
        ch.append_child_value("unit", unit);
        ch.append_child_value("type", "EEG");
    }
}

/// The first `n` EEG frames.
fn read(adapter: &mut skill_lsl::LslAdapter, n: usize) -> Vec<Vec<f64>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let mut out = Vec::new();
        let t0 = Instant::now();
        while out.len() < n && t0.elapsed() < Duration::from_secs(20) {
            if let Ok(Some(DeviceEvent::Eeg(f))) =
                tokio::time::timeout(Duration::from_millis(100), adapter.next_event()).await
            {
                out.push(f.channels);
            }
        }
        out
    })
}

/// Sample `k`, in nanovolts: a small signal on electrode offsets that a
/// `f32` cannot hold to the nanovolt (±300 mV).
fn counts(k: i32) -> [i32; 3] {
    [300_000_000 + k, -299_999_999 - 7 * k, k % 5 - 2]
}

#[test]
fn integer_nanovolts_arrive_exactly_in_microvolts() {
    let name = "IntTest-EEG";
    let info = StreamInfo::new(name, "EEG", 3, 500.0, ChannelFormat::Int32, name);
    describe(&info, &["A", "B", "C"], "nanovolts");
    let out = StreamOutlet::new(&info, 0, 360);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let sender = std::thread::spawn(move || {
        let mut k = 0i32;
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            out.push_sample_i32(&counts(k), 0.0, true);
            k += 1;
            std::thread::sleep(Duration::from_millis(2));
        }
    });

    // resolved over the network, as a session does it: the unit comes from
    // the outlet's full description
    let found = skill_lsl::resolve_stream_by_name(name, 5.0).expect("stream resolves");
    let mut adapter = skill_lsl::LslAdapter::connect(&found).expect("connect");
    assert_eq!(adapter.eeg_scale_uv(), Some(1e-3));
    assert_eq!(adapter.descriptor().channel_names, ["A", "B", "C"]);
    let frames = read(&mut adapter, 500);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    sender.join().unwrap();

    assert_eq!(frames.len(), 500);
    let k0 = (frames[0][0] / 1e-3).round() as i32 - 300_000_000;
    for (i, f) in frames.iter().enumerate() {
        let want = counts(k0 + i as i32).map(|c| f64::from(c) * 1e-3);
        assert_eq!(f[..], want, "frame {i}");
        // and the count comes back out of it, which is what a recording stores
        assert_eq!((f[0] / 1e-3).round() as i32, counts(k0 + i as i32)[0]);
    }
}

#[test]
fn floats_in_volts_are_converted_and_floats_in_microvolts_are_not() {
    for (name, unit, scale) in [("VoltTest-EEG", "V", 1e6), ("MicroTest-EEG", "microvolts", 1.0)] {
        let info = StreamInfo::new(name, "EEG", 2, 250.0, ChannelFormat::Float32, name);
        describe(&info, &["A", "B"], unit);
        let out = StreamOutlet::new(&info, 0, 360);
        let mut adapter = skill_lsl::LslAdapter::connect(&info).expect("connect");
        // real numbers: nothing for a recording to store as integers
        assert_eq!(adapter.eeg_scale_uv(), None);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        let sender = std::thread::spawn(move || {
            while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                out.push_sample_f(&[0.5, -0.25], 0.0, true);
                std::thread::sleep(Duration::from_millis(4));
            }
        });
        let frames = read(&mut adapter, 20);
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        sender.join().unwrap();
        assert_eq!(frames.len(), 20, "{name}");
        assert!(
            frames.iter().all(|f| f[..] == [0.5 * scale, -0.25 * scale]),
            "{name}: {:?}",
            frames[0]
        );
    }
}
