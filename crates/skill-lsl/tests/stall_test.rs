// SPDX-License-Identifier: GPL-3.0-only
//! A session that stops reading for a while — a stalled disk, a long analysis
//! step — finds the whole stream waiting when it comes back; and a feed that
//! is compressed on request delivers the same values.
#![allow(clippy::unwrap_used, clippy::panic)]

use rlsl::prelude::*;
use rlsl::types::ChannelFormat;
use skill_devices::session::{DeviceAdapter, DeviceEvent};
use std::time::{Duration, Instant};

/// A steady 500 Hz source whose first channel counts the samples. Returns
/// the stop flag and the thread, which yields the number of samples sent.
fn source(
    name: &str,
) -> (
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    std::thread::JoinHandle<u32>,
) {
    let info = StreamInfo::new(name, "EEG", 8, 500.0, ChannelFormat::Float32, name);
    let out = StreamOutlet::new(&info, 0, 360);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let thread = std::thread::spawn(move || {
        let start = rlsl::clock::local_clock() + 0.5;
        let mut k = 0u32;
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            let due = start + f64::from(k) / 500.0;
            let wait = due - rlsl::clock::local_clock();
            if wait > 0.0 {
                std::thread::sleep(Duration::from_secs_f64(wait));
            }
            let v = k as f32;
            out.push_sample_f(&[v, v + 0.5, -v, 1.0, 2.0, 3.0, 4.0, 5.0], due, true);
            k += 1;
        }
        k
    });
    (stop, thread)
}

/// Sample numbers of the EEG frames that arrive within `secs`.
fn read(adapter: &mut skill_lsl::LslAdapter, secs: f64) -> Vec<(u32, Vec<f64>)> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let mut out = Vec::new();
        let t0 = Instant::now();
        while t0.elapsed().as_secs_f64() < secs {
            if let Ok(Some(DeviceEvent::Eeg(f))) =
                tokio::time::timeout(Duration::from_millis(100), adapter.next_event()).await
            {
                out.push((f.channels[0] as u32, f.channels));
            }
        }
        out
    })
}

/// Six seconds without a single read at 500 Hz: when the reader returns,
/// every sample since it left is there, in order.
#[test]
fn a_stalled_reader_loses_nothing() {
    let (stop, thread) = source("StallTest-EEG");
    let found = skill_lsl::resolve_stream_by_name("StallTest-EEG", 5.0).expect("stream resolves");
    let mut adapter = skill_lsl::LslAdapter::connect(&found).expect("connect");

    let before = read(&mut adapter, 1.0);
    assert!(before.len() > 200, "{} frames before the stall", before.len());
    // the stall: nothing reads the adapter
    std::thread::sleep(Duration::from_secs(6));
    let after = read(&mut adapter, 3.0);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let sent = thread.join().unwrap();

    let all: Vec<u32> = before.iter().chain(&after).map(|f| f.0).collect();
    assert!(all.len() > 4000, "{} frames of {sent} sent", all.len());
    let broken = all.windows(2).filter(|w| w[1] != w[0] + 1).count();
    assert_eq!(
        broken,
        0,
        "samples missing across the stall: {} of {sent} received",
        all.len()
    );
    assert_eq!(adapter.stats().gaps, 0);
}

/// Asked to (`SKILL_LSL_COMPRESSION=1`; by default only for sources on other
/// computers), the adapter gets a compressed feed and the same values.
#[test]
fn a_compressed_feed_delivers_the_same_values() {
    // Set before any adapter of this test connects. The other test in this
    // file reads streams of this machine, for which the variable only changes
    // how the bytes travel.
    std::env::set_var("SKILL_LSL_COMPRESSION", "1");
    let (stop, thread) = source("CompressedTest-EEG");
    let found = skill_lsl::resolve_stream_by_name("CompressedTest-EEG", 5.0).expect("stream resolves");
    let mut adapter = skill_lsl::LslAdapter::connect(&found).expect("connect");
    assert!(adapter.stats().compressed, "the outlet agreed to compress");

    let frames = read(&mut adapter, 2.0);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    thread.join().unwrap();
    assert!(frames.len() > 500, "{} frames", frames.len());
    for (k, ch) in &frames {
        let v = f64::from(*k);
        assert_eq!(ch.as_slice(), [v, v + 0.5, -v, 1.0, 2.0, 3.0, 4.0, 5.0], "sample {k}");
    }
    assert!(frames.windows(2).all(|w| w[1].0 == w[0].0 + 1), "consecutive");
}
