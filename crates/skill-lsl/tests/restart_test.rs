// SPDX-License-Identifier: GPL-3.0-only
//! A source that is restarted while a session runs — its program crashed and
//! was started again — is another LSL outlet. The adapter finds it and goes
//! on: the session keeps recording, with the break counted and marked.
#![allow(clippy::unwrap_used, clippy::panic)]

use rlsl::prelude::*;
use rlsl::types::ChannelFormat;
use skill_devices::session::{DeviceAdapter, DeviceEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A steady 250 Hz source; channel 0 is `base` + the sample number. The outlet
/// goes when the returned flag is set and the thread has ended.
fn source(name: &str, base: f32, history: f64) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let info = StreamInfo::new(name, "EEG", 4, 250.0, ChannelFormat::Float32, "restart-test-device");
    let out = StreamOutlet::new(&info, 0, 360);
    // (what it has sent is kept this long for a consumer that comes late and asks)
    out.set_history(history);
    // A second stream of the same device, as a device with an accelerometer has: the same
    // source id. Looking for the restarted stream by source id alone finds this one as often
    // as the other.
    let sibling = StreamOutlet::new(
        &StreamInfo::new(&format!("{name}-Accelerometer"), "Accelerometer", 3, 0.0, ChannelFormat::Float32, "restart-test-device"),
        0,
        360,
    );
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let thread = std::thread::spawn(move || {
        let start = rlsl::clock::local_clock();
        let mut k = 0u32;
        let _sibling = sibling;
        while !stop2.load(Ordering::Relaxed) {
            let due = start + f64::from(k) / 250.0;
            let wait = due - rlsl::clock::local_clock();
            if wait > 0.0 {
                std::thread::sleep(Duration::from_secs_f64(wait));
            }
            out.push_sample_f(&[base + k as f32, 1.0, 2.0, 3.0], due, true);
            k += 1;
        }
    });
    (stop, thread)
}

/// Events of the next `secs`: EEG (channel 0, time stamp) and marker texts.
fn read(adapter: &mut skill_lsl::LslAdapter, secs: f64) -> (Vec<(f64, f64)>, Vec<String>) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let (mut eeg, mut markers) = (Vec::new(), Vec::new());
        let t0 = Instant::now();
        while t0.elapsed().as_secs_f64() < secs {
            match tokio::time::timeout(Duration::from_millis(100), adapter.next_event()).await {
                Ok(Some(DeviceEvent::Eeg(f))) => eeg.push((f.channels[0], f.timestamp_s)),
                Ok(Some(DeviceEvent::Marker(m))) if m.source == skill_lsl::RESTART_MARKER_SOURCE => {
                    markers.push(m.text);
                }
                _ => {}
            }
        }
        (eeg, markers)
    })
}

#[test]
fn a_restarted_source_is_found_again() {
    let name = format!("RestartTest-{}-EEG", std::process::id());
    let (stop, thread) = source(&name, 0.0, 0.0);
    let found = skill_lsl::resolve_stream_by_name(&name, 10.0).expect("the stream resolves");
    let mut adapter = skill_lsl::LslAdapter::connect(&found).expect("connect");
    let (before, markers) = read(&mut adapter, 2.0);
    assert!(before.len() > 250, "samples of the first outlet: {}", before.len());
    assert!(markers.is_empty());

    // the sender dies, and is started again a moment later: another outlet
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let (stop, thread) = source(&name, 1_000_000.0, 0.0);

    let (after, markers) = read(&mut adapter, 12.0);
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    let again: Vec<&(f64, f64)> = after.iter().filter(|(v, _)| *v >= 1_000_000.0).collect();
    assert!(
        again.len() > 250,
        "the session goes on with the outlet that took the first one's place: {} samples",
        again.len()
    );
    assert_eq!(markers, ["source restarted"]);
    let stats = adapter.stats();
    assert_eq!(stats.restarts, 1);
    assert!(
        // (the sender was gone for half a second: 125 samples at least)
        stats.gaps >= 1 && stats.missing_samples > 100,
        "the break is counted: {stats:?}"
    );
    // still on this machine's clock, and after what came before
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let last_before = before.last().unwrap().1;
    assert!(again.iter().all(|(_, t)| *t > last_before && (*t - now).abs() < 30.0));
}

/// A sender that keeps its last seconds is asked for them when it is gone over to: what it
/// sent before the session found it again is in the recording too, once, from its first
/// sample on.
#[test]
fn nothing_a_restarted_source_has_sent_is_missing_when_it_keeps_its_last_seconds() {
    let name = format!("RestartKept-{}-EEG", std::process::id());
    let (stop, thread) = source(&name, 0.0, 5.0);
    let found = skill_lsl::resolve_stream_by_name(&name, 10.0).expect("the stream resolves");
    let mut adapter = skill_lsl::LslAdapter::connect(&found).expect("connect");
    let (before, _) = read(&mut adapter, 2.0);
    assert!(before.len() > 250);

    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let (stop, thread) = source(&name, 1_000_000.0, 5.0);
    let (after, markers) = read(&mut adapter, 10.0);
    stop.store(true, Ordering::Relaxed);
    thread.join().unwrap();

    let again: Vec<f64> = after.iter().map(|(v, _)| *v).filter(|v| *v >= 1_000_000.0).collect();
    assert_eq!(markers, ["source restarted"]);
    assert!(again.len() > 1000, "{} samples of the new sender", again.len());
    assert_eq!(again[0], 1_000_000.0, "from the first sample the new sender sent");
    assert!(
        again.windows(2).all(|w| w[1] - w[0] == 1.0),
        "every sample once, in order: {:?}",
        again.windows(2).find(|w| w[1] - w[0] != 1.0)
    );
    // and their times are the sender's own, in order, not the moment they were handed over
    let stamps: Vec<f64> = after.iter().filter(|(v, _)| *v >= 1_000_000.0).map(|(_, t)| *t).collect();
    assert!(stamps.windows(2).all(|w| w[1] > w[0] && w[1] - w[0] < 0.02), "4 ms apart, the replayed ones too");
}
