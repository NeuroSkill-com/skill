// SPDX-License-Identifier: GPL-3.0-only
//! Time stamps leave the adapter in Unix time, and a source's accelerometer
//! stream comes along with its EEG stream.
#![allow(clippy::unwrap_used, clippy::panic)]

use rlsl::prelude::*;
use rlsl::types::ChannelFormat;
use skill_devices::session::{DeviceAdapter, DeviceCaps, DeviceEvent};
use std::time::{Duration, Instant};

fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

/// A UDP port where nobody answers: a stream pointed at it has no time service.
fn silent_port() -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    // keep it bound (and silent) for the rest of the test run
    std::mem::forget(socket);
    port
}

/// A time service whose clock is `ahead` seconds ahead of this machine's:
/// answers `LSL:timedata` like an outlet. Returns its port.
fn fake_time_service(ahead: f64) -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        while let Ok((n, from)) = socket.recv_from(&mut buf) {
            let text = String::from_utf8_lossy(&buf[..n]).to_string();
            let mut lines = text.lines();
            if lines.next().map(str::trim) != Some("LSL:timedata") {
                continue;
            }
            let params: Vec<&str> = lines.next().unwrap_or("").split_whitespace().collect();
            let (wave, t0) = (params.first().unwrap_or(&"0"), params.get(1).unwrap_or(&"0"));
            let remote_now = rlsl::clock::local_clock() + ahead;
            let _ = socket.send_to(format!(" {wave} {t0} {remote_now} {remote_now}").as_bytes(), from);
        }
    });
    port
}

/// Collect events for `secs` on a plain thread (rlsl has its own runtime).
/// EEG frames as `(timestamp, channels)`, and accelerometer readings.
type Collected = (Vec<(f64, Vec<f64>)>, Vec<[f32; 3]>);

fn collect(mut adapter: skill_lsl::LslAdapter, secs: f64) -> Collected {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let (mut eeg, mut imu) = (Vec::new(), Vec::new());
        let t0 = Instant::now();
        while t0.elapsed().as_secs_f64() < secs {
            match tokio::time::timeout(Duration::from_millis(200), adapter.next_event()).await {
                Ok(Some(DeviceEvent::Eeg(f))) => eeg.push((f.timestamp_s, f.channels)),
                Ok(Some(DeviceEvent::Imu(f))) => imu.push(f.accel),
                Ok(Some(_)) | Err(_) => {}
                Ok(None) => break,
            }
        }
        (eeg, imu)
    })
}

/// Without a time service to synchronise with, a stream that declares its
/// wall-clock reference is stamped with exactly that time; its accelerometer
/// sibling is delivered as IMU frames in g.
#[test]
fn declared_clock_reference_gives_exact_unix_time_and_accelerometer_comes_along() {
    // A reference far from "now" proves the declaration is what is used.
    const UNIX0: f64 = 1_600_000_000.0;
    let lsl0 = rlsl::clock::local_clock();

    let info = StreamInfo::new("ClockRef-EEG", "EEG", 4, 250.0, ChannelFormat::Float32, "clock-ref-src");
    let desc = info.desc();
    let channels = desc.append_child("channels");
    for label in ["Fz", "Cz", "Pz", "Oz"] {
        channels.append_child("channel").append_child_value("label", label);
    }
    let reference = desc.append_child("clock_reference");
    reference.append_child_value("lsl_time", &format!("{lsl0:.6}"));
    reference.append_child_value("unix_time", &format!("{UNIX0:.3}"));
    let eeg_out = StreamOutlet::new(&info, 0, 360);

    let acc_info = StreamInfo::new(
        "ClockRef-Accelerometer",
        "Accelerometer",
        3,
        50.0,
        ChannelFormat::Float32,
        "clock-ref-src",
    );
    let ch = acc_info.desc().append_child("channel");
    ch.append_child_value("name", "X");
    ch.append_child_value("unit", "mm/s2");
    let acc_out = StreamOutlet::new(&acc_info, 0, 360);

    // Resolve over the network, as the daemon does: the description (labels,
    // clock reference) then has to come from the outlet's full info.
    let found = skill_lsl::resolve_stream_by_name("ClockRef-EEG", 5.0).expect("stream resolves");
    found.set_v4service_port(silent_port());
    let adapter = skill_lsl::LslAdapter::connect(&found).expect("connect");
    let d = adapter.descriptor().clone();
    assert_eq!(d.channel_names, ["Fz", "Cz", "Pz", "Oz"]);
    assert!(d.caps.contains(DeviceCaps::IMU), "accelerometer sibling found");
    assert_eq!(d.imu_channel_names.len(), 3);

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let feeder = std::thread::spawn(move || {
        let mut k = 0u64;
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            // sample k was taken at lsl0 + 1 + k/250 on the sender's clock
            let v = k as f32;
            eeg_out.push_sample_f(&[v, v + 0.25, v + 0.5, v + 0.75], lsl0 + 1.0 + k as f64 / 250.0, true);
            if k % 5 == 0 {
                acc_out.push_sample_f(&[0.0, 4903.325, 9806.65], 0.0, true);
            }
            k += 1;
            std::thread::sleep(Duration::from_millis(4));
        }
    });
    let stats_probe = adapter.stats();
    assert!(stats_probe.exact_clock && !stats_probe.lsl_synced);
    let (eeg, imu) = collect(adapter, 2.5);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    feeder.join().unwrap();

    assert!(eeg.len() > 200, "{} EEG frames", eeg.len());
    for (ts, ch) in &eeg {
        // channel 0 carries the sample index k: its time is UNIX0 + 1 + k/250
        let want = UNIX0 + 1.0 + ch[0] / 250.0;
        assert!((ts - want).abs() < 0.002, "sample {} stamped {ts}, want {want}", ch[0]);
        assert_eq!(ch[1], ch[0] + 0.25);
    }
    assert!(imu.len() > 20, "{} IMU frames", imu.len());
    assert!(
        imu.iter()
            .all(|a| a[0] == 0.0 && (a[1] - 0.5).abs() < 1e-6 && (a[2] - 1.0).abs() < 1e-6),
        "{:?}",
        imu[0]
    );
}

/// A stream with neither a time service nor a declared reference is anchored
/// to its arrival: Unix time, now.
#[test]
fn undeclared_clock_is_anchored_to_arrival() {
    let info = StreamInfo::new("AnchorTest", "EEG", 2, 100.0, ChannelFormat::Float32, "anchor-src");
    let out = StreamOutlet::new(&info, 0, 360);
    let found = skill_lsl::resolve_stream_by_name("AnchorTest", 5.0).expect("stream resolves");
    found.set_v4service_port(silent_port());
    let adapter = skill_lsl::LslAdapter::connect(&found).expect("connect");
    assert!(!adapter.stats().exact_clock && !adapter.stats().lsl_synced);
    assert!(!adapter.descriptor().caps.contains(DeviceCaps::IMU));

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let feeder = std::thread::spawn(move || {
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            out.push_sample_f(&[1.0, 2.0], 0.0, true);
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let (eeg, _) = collect(adapter, 1.5);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    feeder.join().unwrap();

    assert!(eeg.len() > 50, "{} frames", eeg.len());
    let now = unix_now();
    assert!(
        eeg.iter().all(|(ts, _)| (now - ts).abs() < 5.0),
        "first {} now {now}",
        eeg[0].0
    );
    assert!(eeg.windows(2).all(|w| w[1].0 > w[0].0), "increasing");
    let step = (eeg[eeg.len() - 1].0 - eeg[0].0) / (eeg.len() - 1) as f64;
    assert!((step - 0.01).abs() < 0.003, "mean step {step}");
}

/// Marker streams on the network — text and numeric, one of them appearing
/// after the session started — reach the session as marker events in Unix time.
#[test]
fn marker_streams_are_followed() {
    // Names of this run alone: the same test running elsewhere on the network (another
    // checkout, another session) has streams of its own, and theirs are not ours.
    let id = std::process::id();
    let (host, tasks, triggers) = (
        format!("MarkerHost-{id}-EEG"),
        format!("TaskEvents-{id}"),
        format!("TriggerCodes-{id}"),
    );
    let info = StreamInfo::new(&host, "EEG", 2, 100.0, ChannelFormat::Float32, "marker-host");
    let eeg = StreamOutlet::new(&info, 0, 360);
    let text_info = StreamInfo::new(&tasks, "Markers", 1, 0.0, ChannelFormat::String, "task-events");
    let text_out = StreamOutlet::new(&text_info, 0, 360);

    let found = skill_lsl::resolve_stream_by_name(&host, 5.0).expect("stream resolves");
    let mut adapter = skill_lsl::LslAdapter::connect(&found).expect("connect");

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    // when each text marker was sent, on this machine's Unix clock
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, f64)>::new()));
    let sent2 = sent.clone();
    let triggers2 = triggers.clone();
    let feeder = std::thread::spawn(move || {
        // a second marker stream, numeric, that only starts now
        let code_info = StreamInfo::new(&triggers2, "Markers", 1, 0.0, ChannelFormat::Int32, "trigger-codes");
        let code_out = StreamOutlet::new(&code_info, 0, 360);
        let mut k = 0u32;
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            eeg.push_sample_f(&[1.0, 2.0], 0.0, true);
            if k % 50 == 49 {
                let text = format!("stimulus {}", k / 50);
                sent2.lock().unwrap().push((text.clone(), unix_now()));
                text_out.push_sample_str(&[text], 0.0, true);
                code_out.push_sample_f(&[(100 + k / 50) as f32], 0.0, true);
            }
            k += 1;
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let markers: Vec<(f64, String, String)> = rt.block_on(async {
        let mut out = Vec::new();
        let t0 = Instant::now();
        // long enough for the rescan to find the late stream, also on a machine that is
        // busy with other things (it ends as soon as both streams have been heard)
        while t0.elapsed() < Duration::from_secs(40) {
            if let Ok(Some(DeviceEvent::Marker(m))) =
                tokio::time::timeout(Duration::from_millis(200), adapter.next_event()).await
            {
                out.push((m.timestamp_s, m.source, m.text));
            }
            let has = |src: &str| out.iter().filter(|m| m.1 == src).count() >= 2;
            if has(&tasks) && has(&triggers) {
                break;
            }
        }
        out
    });
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    feeder.join().unwrap();

    let of = |src: &str| -> Vec<&(f64, String, String)> { markers.iter().filter(|m| m.1 == src).collect() };
    let (text, codes) = (of(&tasks), of(&triggers));
    assert!(text.len() >= 2 && codes.len() >= 2, "{markers:?}");
    assert!(text.iter().all(|m| m.2.starts_with("stimulus ")), "{text:?}");
    assert!(
        codes.iter().all(|m| m.2.parse::<u32>().is_ok_and(|c| c >= 100)),
        "{codes:?}"
    );
    let now = unix_now();
    assert!(
        text.iter().chain(&codes).all(|m| (now - m.0).abs() < 30.0),
        "Unix time: {markers:?}"
    );
    // each marker at the moment it was sent — whatever the pace the sender kept (it means
    // to send one every half second, and on a busy machine does not)
    let sent = sent.lock().unwrap();
    for m in &text {
        let at = sent.iter().find(|s| s.0 == m.2).expect("a marker that was sent").1;
        assert!(
            (m.0 - at).abs() < 0.05,
            "marker {:?} sent at {at}, recorded at {}",
            m.2,
            m.0
        );
    }
}

/// A source on a computer whose clock is 100 s ahead of ours — and whose
/// declared wall-clock reference is therefore 100 s off too — is recorded on
/// *our* clock: each sample at the moment it was taken, as this machine tells
/// time. That is what puts it on the same timeline as markers and labels.
#[test]
fn samples_from_another_clock_land_on_this_machine_s_time() {
    const AHEAD: f64 = 100.0;
    let info = StreamInfo::new("OtherClock-EEG", "EEG", 2, 250.0, ChannelFormat::Float32, "other-clock");
    let remote_now = || rlsl::clock::local_clock() + AHEAD;
    // the source's own idea of the time: its clock, and a wall clock that is off by as much
    let reference = info.desc().append_child("clock_reference");
    reference.append_child_value("lsl_time", &format!("{:.6}", remote_now()));
    reference.append_child_value("unix_time", &format!("{:.3}", unix_now() + AHEAD));
    let out = StreamOutlet::new(&info, 0, 360);

    let found = skill_lsl::resolve_stream_by_name("OtherClock-EEG", 5.0).expect("stream resolves");
    found.set_v4address("127.0.0.1");
    found.set_v4service_port(fake_time_service(AHEAD));
    let adapter = skill_lsl::LslAdapter::connect(&found).expect("connect");
    assert!(adapter.stats().lsl_synced && adapter.stats().exact_clock);

    // Sample k is taken at `start + k / 250` (a steady 250 Hz, like an amplifier's), stamped on
    // the source's clock, and carries k.
    let (start_unix, start_lsl) = (unix_now() + 0.2, rlsl::clock::local_clock() + 0.2);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop2 = stop.clone();
    let feeder = std::thread::spawn(move || {
        let mut k = 0u32;
        while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
            let due = start_lsl + f64::from(k) / 250.0;
            let wait = due - rlsl::clock::local_clock();
            if wait > 0.0 {
                std::thread::sleep(Duration::from_secs_f64(wait));
            }
            out.push_sample_f(&[k as f32, 1.0], due + AHEAD, true);
            k += 1;
        }
    });
    let (eeg, _) = collect(adapter, 3.0);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    feeder.join().unwrap();

    assert!(eeg.len() > 300, "{} frames", eeg.len());
    let mut errors_ms: Vec<f64> = eeg
        .iter()
        .map(|(ts, ch)| (ts - (start_unix + ch[0] / 250.0)) * 1000.0)
        .collect();
    errors_ms.sort_by(|a, b| a.total_cmp(b));
    let median = errors_ms[errors_ms.len() / 2];
    // on our clock, not 100 s (or 100 000 ms) away on the source's
    assert!(
        median.abs() < 20.0,
        "median error {median} ms; range {} .. {}",
        errors_ms[0],
        errors_ms[errors_ms.len() - 1]
    );
}
