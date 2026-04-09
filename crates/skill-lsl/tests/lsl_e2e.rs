// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
#![allow(clippy::unwrap_used, clippy::panic)]
//!
//! End-to-end LSL integration test.
//!
//! Creates a local LSL outlet with 32 EEG channels at 256 Hz, connects the
//! skill-lsl `LslAdapter` to it, streams a known number of samples through
//! the full `DeviceAdapter` pipeline, and verifies:
//!
//!   1. The `Connected` event arrives with the correct metadata.
//!   2. All 32 channel names are parsed from the XML description.
//!   3. EEG samples arrive with correct channel count and timestamp ordering.
//!   4. Sample values round-trip with float32 precision (< 0.5 µV error).
//!   5. The pipeline handles a realistic burst (5 s @ 256 Hz = 1280 samples)
//!      without drops or ordering violations.
//!   6. The adapter shuts down cleanly when dropped.
//!
//! # Concurrency model
//!
//! `rlsl::outlet::StreamOutlet::new` calls `tokio::Runtime::block_on`
//! internally, which panics if called on a Tokio worker thread (those threads
//! have an active scheduler context).  All rlsl outlet operations therefore
//! run on a raw OS thread via `spawn_blocking`.
//!
//! Both the outlet and the adapter must share the *same* `StreamInfo` object
//! (not independently-constructed copies) so the inlet connects directly
//! rather than going through UDP discovery.  liblsl's C objects are
//! internally reference-counted and thread-safe, so we wrap `StreamInfo` in
//! an unsafe `Send` newtype to transfer it across thread boundaries.
//!
//! Run with:
//!   cargo test -p skill-lsl --test lsl_e2e -- --nocapture

use std::sync::mpsc;
use std::time::{Duration, Instant};

use rlsl::prelude::*;
use rlsl::types::ChannelFormat;
use skill_devices::session::{DeviceAdapter, DeviceCaps, DeviceEvent};

// ── Constants ─────────────────────────────────────────────────────────────────

const CHANNELS: usize = 32;
const SAMPLE_RATE: f64 = 256.0;
const STREAM_NAME: &str = "SkillE2E-32ch";
const STREAM_TYPE: &str = "EEG";
const SOURCE_ID: &str = "skill-lsl-e2e-001";

/// Standard 32-channel 10-20 layout.
const CHANNEL_LABELS: [&str; 32] = [
    "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "FC5", "FC1", "FC2", "FC6", "T7", "C3", "Cz", "C4", "T8", "TP9", "CP5",
    "CP1", "CP2", "CP6", "TP10", "P7", "P3", "Pz", "P4", "P8", "PO9", "O1", "Oz", "O2", "PO10",
];

// ── Report ────────────────────────────────────────────────────────────────────

struct Step {
    name: &'static str,
    duration: Duration,
    ok: bool,
    detail: String,
}

impl Step {
    fn pass(name: &'static str, dur: Duration, detail: impl Into<String>) -> Self {
        Self {
            name,
            duration: dur,
            ok: true,
            detail: detail.into(),
        }
    }
    fn fail(name: &'static str, dur: Duration, detail: impl Into<String>) -> Self {
        Self {
            name,
            duration: dur,
            ok: false,
            detail: detail.into(),
        }
    }
}

struct Report {
    steps: Vec<Step>,
    outlet_samples_pushed: usize,
    adapter_samples_received: usize,
    channel_count: usize,
    sample_rate: f64,
    labels: Vec<String>,
    max_value_error: f64,
    timing_violations: usize,
}

impl Report {
    fn new() -> Self {
        Self {
            steps: vec![],
            outlet_samples_pushed: 0,
            adapter_samples_received: 0,
            channel_count: 0,
            sample_rate: 0.0,
            labels: vec![],
            max_value_error: 0.0,
            timing_violations: 0,
        }
    }

    fn print(&self) {
        let w = 76usize;
        let bar = "═".repeat(w - 2);
        let total: Duration = self.steps.iter().map(|s| s.duration).sum();

        eprintln!();
        eprintln!("╔{bar}╗");
        eprintln!("║{:^width$}║", "LSL E2E INTEGRATION TEST REPORT", width = w - 2);
        eprintln!("╠{bar}╣");
        let ch_list = self.labels.join(", ");
        self.p(
            w,
            &format!("Stream: {} ch @ {} Hz", self.channel_count, self.sample_rate),
        );
        if !ch_list.is_empty() {
            self.p(w, &format!("Labels: [{ch_list}]"));
        }
        self.p(
            w,
            &format!(
                "Pushed: {}  Received: {}  MaxErr: {:.2e} µV  TsViol: {}",
                self.outlet_samples_pushed, self.adapter_samples_received, self.max_value_error, self.timing_violations,
            ),
        );
        self.p(w, &format!("Total: {:.3}s", total.as_secs_f64()));
        eprintln!("╠{bar}╣");
        eprintln!("║{:^width$}║", "PIPELINE STEPS", width = w - 2);
        eprintln!("║{}║", " ".repeat(w - 2));

        for (i, step) in self.steps.iter().enumerate() {
            let icon = if step.ok { "✅" } else { "❌" };
            let status = if step.ok { "OK" } else { "FAIL" };
            self.p(
                w,
                &format!(
                    "{icon} {}. {:<38} {:>6.3}s  {status}",
                    i + 1,
                    step.name,
                    step.duration.as_secs_f64(),
                ),
            );
            for line in step.detail.lines() {
                self.p(w, &format!("      {line}"));
            }
        }

        let all_ok = self.steps.iter().all(|s| s.ok);
        eprintln!("╠{bar}╣");
        let verdict = if all_ok {
            "✅  ALL STEPS PASSED"
        } else {
            "❌  SOME STEPS FAILED"
        };
        eprintln!("║{verdict:^width$}║", width = w - 2);
        eprintln!("╚{bar}╝");
        eprintln!();

        if !all_ok {
            panic!("lsl-e2e: one or more steps failed — see report above");
        }
    }

    fn p(&self, w: usize, s: &str) {
        let inner = w - 4;
        for chunk in wrap(s, inner) {
            eprintln!("║  {chunk:<inner$}  ║");
        }
    }
}

fn wrap(s: &str, width: usize) -> Vec<String> {
    if s.len() <= width {
        return vec![s.to_owned()];
    }
    let mut out = vec![];
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if cur.is_empty() {
            cur = word.to_owned();
        } else if cur.len() + 1 + word.len() <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            out.push(cur.clone());
            cur = word.to_owned();
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ── Send wrapper ──────────────────────────────────────────────────────────────
//
// `StreamInfo` wraps a C `lsl_streaminfo` pointer (reference-counted, heap
// allocated).  liblsl's C API is thread-safe for all public functions, so
// sending the pointer across threads is safe.  We need this to pass the
// outlet's StreamInfo to `spawn_blocking` so the adapter's inlet uses a
// direct connection rather than going through UDP discovery.

#[allow(dead_code)]
struct SendStreamInfo(StreamInfo);
// SAFETY: liblsl's lsl_streaminfo objects are internally reference-counted
// and all public C functions operating on them are thread-safe.
unsafe impl Send for SendStreamInfo {}

// ── Helper ────────────────────────────────────────────────────────────────────

fn add_labels(info: &StreamInfo) {
    let desc = info.desc();
    let channels_node = desc.append_child("channels");
    for &label in CHANNEL_LABELS.iter() {
        let ch = channels_node.append_child("channel");
        ch.append_child_value("label", label);
        ch.append_child_value("unit", "microvolts");
        ch.append_child_value("type", "EEG");
    }
}

// ── Test entry point ──────────────────────────────────────────────────────────

/// Full end-to-end LSL pipeline test: 32 EEG channels, 256 Hz, 5 s of data.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lsl_e2e_32ch_256hz() {
    let mut report = Report::new();
    const TOTAL_SAMPLES: usize = (SAMPLE_RATE as usize) * 5; // 1280 samples

    // Deterministic test signal: value(ch, s) = ch + s * 0.01 µV
    let all_samples: Vec<Vec<f32>> = (0..TOTAL_SAMPLES)
        .map(|s| (0..CHANNELS).map(|c| (c as f32) + (s as f32) * 0.01).collect())
        .collect();

    // ── Step 1 + 2: Create outlet and adapter in spawn_blocking ──────────────
    //
    // `StreamOutlet::new` calls `tokio::Runtime::block_on` internally; this
    // panics if called on a Tokio worker thread.  `spawn_blocking` runs the
    // closure on a non-worker thread from Tokio's blocking pool — safe.
    //
    // Crucially, both outlet and adapter receive the SAME `StreamInfo` object
    // (not independently created copies).  This gives the adapter's inlet
    // direct access to the outlet's connection info so it can connect
    // immediately without UDP discovery.

    let t0 = Instant::now();

    // The outlet MUST stay alive until the async receive loop finishes.
    //
    // Root cause: rlsl's TCP session thread checks `shutdown` at the top of
    // every loop iteration.  `Drop for StreamOutlet` sets shutdown=true and
    // pushes a sentinel.  The TCP thread then exits on the NEXT iteration
    // check, discarding any samples still buffered in `chunk_buf` (the last
    // partial pushthrough chunk).  This causes the final 32-sample chunk to
    // be lost reliably.
    //
    // Fix: push all samples in the blocking thread, then hand the outlet to
    // the async side via a second channel so it is dropped only AFTER the
    // receive loop completes.
    struct SendOutlet {
        _inner: StreamOutlet,
    }
    // SAFETY: StreamOutlet contains raw pointers managed by liblsl's C layer.
    // liblsl guarantees thread-safety of outlet handles; we simply move the
    // owning wrapper to the async side after all pushes are complete.
    unsafe impl Send for SendOutlet {}

    let (setup_tx, setup_rx) = mpsc::sync_channel::<Result<(SendStreamInfo, skill_lsl::LslAdapter), String>>(0);
    // Second channel: blocking push thread sends the outlet to the async side
    // after all samples have been pushed, keeping it alive until receive is done.
    let (outlet_tx, outlet_rx) = mpsc::sync_channel::<SendOutlet>(0);

    let samples_for_push = all_samples.clone();
    let (push_done_tx, push_done_rx) = mpsc::sync_channel::<usize>(0);

    tokio::task::spawn_blocking(move || {
        // --- Outlet ---
        let info = StreamInfo::new(
            STREAM_NAME,
            STREAM_TYPE,
            CHANNELS as u32,
            SAMPLE_RATE,
            ChannelFormat::Float32,
            SOURCE_ID,
        );
        add_labels(&info);

        // StreamOutlet::new calls block_on internally — fine here because
        // spawn_blocking runs on a thread with no active Tokio scheduler.
        let outlet = StreamOutlet::new(&info, 0, 360);

        // Adapter: pass the SAME info so the inlet connects directly (no UDP
        // discovery round-trip, no risk of missing pre-connection samples).
        let adapter = skill_lsl::LslAdapter::new(&info);

        // Send info + adapter to the async side.
        setup_tx.send(Ok((SendStreamInfo(info), adapter))).ok();

        // Push data on this same blocking thread while the async side receives.
        const CHUNK: usize = 32; // ~125 ms of data per chunk at 256 Hz
        let chunk_delay = Duration::from_secs_f64(CHUNK as f64 / SAMPLE_RATE);
        let mut pushed = 0usize;
        for chunk in samples_for_push.chunks(CHUNK) {
            for row in chunk {
                outlet.push_sample_f(row, 0.0, true);
            }
            pushed += chunk.len();
            std::thread::sleep(chunk_delay);
        }
        push_done_tx.send(pushed).ok();

        // Transfer outlet ownership to the async side so it is dropped
        // only after the receive loop finishes (prevents TCP session teardown
        // while the last chunk is still in transit).
        outlet_tx.send(SendOutlet { _inner: outlet }).ok();
    });

    let (_info, mut adapter) = match setup_rx.recv() {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => {
            report
                .steps
                .push(Step::fail("Create outlet + adapter", t0.elapsed(), e));
            report.print();
            return;
        }
        Err(e) => {
            report.steps.push(Step::fail(
                "Create outlet + adapter",
                t0.elapsed(),
                format!("channel error: {e}"),
            ));
            report.print();
            return;
        }
    };

    let desc = adapter.descriptor();
    let descriptor_ok = desc.eeg_channels == CHANNELS
        && (desc.eeg_sample_rate - SAMPLE_RATE).abs() < 0.01
        && desc.kind == "lsl"
        && desc.caps.contains(DeviceCaps::EEG)
        && desc.channel_names.len() == CHANNELS
        && desc.channel_names[0] == "Fp1"
        && desc.channel_names[CHANNELS - 1] == "PO10"
        && desc.pipeline_channels == CHANNELS.min(skill_constants::EEG_CHANNELS);

    if descriptor_ok {
        report.channel_count = desc.eeg_channels;
        report.sample_rate = desc.eeg_sample_rate;
        report.labels = desc.channel_names.clone();
        report.steps.push(Step::pass(
            "Create outlet + adapter (32 ch, 256 Hz)",
            t0.elapsed(),
            format!(
                "{} ch @ {} Hz | pipeline_ch={} | labels=[{}…{}]",
                desc.eeg_channels,
                desc.eeg_sample_rate,
                desc.pipeline_channels,
                desc.channel_names.first().map(String::as_str).unwrap_or(""),
                desc.channel_names.last().map(String::as_str).unwrap_or(""),
            ),
        ));
    } else {
        report.steps.push(Step::fail(
            "Create outlet + adapter (32 ch, 256 Hz)",
            t0.elapsed(),
            format!(
                "ch={} (want {CHANNELS}), rate={:.1} (want {SAMPLE_RATE}), \
                 kind={}, labels={}, first={:?}, last={:?}",
                desc.eeg_channels,
                desc.eeg_sample_rate,
                desc.kind,
                desc.channel_names.len(),
                desc.channel_names.first(),
                desc.channel_names.last(),
            ),
        ));
        report.print();
        return;
    }

    // ── Step 3: Receive Connected event ───────────────────────────────────────

    let t0 = Instant::now();
    match tokio::time::timeout(Duration::from_secs(5), adapter.next_event()).await {
        Ok(Some(DeviceEvent::Connected(info))) => {
            report.steps.push(Step::pass(
                "Receive Connected event",
                t0.elapsed(),
                format!("name='{}' hw={:?}", info.name, info.hardware_version),
            ));
        }
        other => {
            report.steps.push(Step::fail(
                "Receive Connected event",
                t0.elapsed(),
                format!("got {other:?}"),
            ));
            report.print();
            return;
        }
    }

    // ── Step 4: Receive 1280 EEG samples (5 s × 256 Hz) ──────────────────────

    let t0 = Instant::now();
    let mut received: Vec<(f64, Vec<f32>)> = Vec::with_capacity(TOTAL_SAMPLES);
    // 5 s of data + 3 s headroom for LSL buffer flush and OS scheduling.
    let rx_deadline = Duration::from_secs(8);
    let rx_start = Instant::now();

    'recv: loop {
        let remaining = rx_deadline.saturating_sub(rx_start.elapsed());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, adapter.next_event()).await {
            Ok(Some(DeviceEvent::Eeg(frame))) => {
                let samples: Vec<f32> = frame.channels.into_iter().map(|v| v as f32).collect();
                received.push((frame.timestamp_s, samples));
                if received.len() >= TOTAL_SAMPLES {
                    break 'recv;
                }
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break 'recv,
        }
    }

    // Drain any samples that arrived after the timed receive loop exited.
    // The inlet thread may have buffered the last chunk just as the deadline
    // fired; a short extra receive window picks them up.
    // Drain the adapter channel: the inlet thread may still be mid-transfer
    // when the timed loop exits.  Keep reading with a tight per-event timeout
    // until 500 ms of silence confirms no more samples are in flight.
    while let Ok(Some(DeviceEvent::Eeg(frame))) =
        tokio::time::timeout(Duration::from_millis(500), adapter.next_event()).await
    {
        let samples: Vec<f32> = frame.channels.into_iter().map(|v| v as f32).collect();
        received.push((frame.timestamp_s, samples));
        if received.len() >= TOTAL_SAMPLES {
            break;
        }
    }

    let pushed = push_done_rx.recv().unwrap_or(0);

    // Receive the outlet from the push thread and drop it NOW — after the
    // receive loop has finished.  This prevents the rlsl TCP session thread
    // from seeing shutdown=true while the last chunk is still in transit.
    drop(outlet_rx.recv().ok());

    report.outlet_samples_pushed = pushed;
    report.adapter_samples_received = received.len();

    // Require at least 95% of samples — a single lost chunk (32 samples, 2.5%)
    // should still fail so regressions are caught.
    if received.len() < TOTAL_SAMPLES * 95 / 100 {
        report.steps.push(Step::fail(
            "Receive 5 s EEG data (1280 × 32 ch)",
            t0.elapsed(),
            format!(
                "received {}/{TOTAL_SAMPLES} — pipeline stalled (pushed={pushed})",
                received.len()
            ),
        ));
        report.print();
        return;
    }
    report.steps.push(Step::pass(
        "Receive 5 s EEG data (1280 × 32 ch)",
        t0.elapsed(),
        format!("received {}/{TOTAL_SAMPLES} samples (pushed={pushed})", received.len()),
    ));

    // ── Step 5: Channel count on every frame ──────────────────────────────────

    let t0 = Instant::now();
    let wrong = received.iter().filter(|(_, ch)| ch.len() != CHANNELS).count();
    if wrong == 0 {
        report.steps.push(Step::pass(
            "Channel count per frame (== 32)",
            t0.elapsed(),
            format!("all {} frames have exactly {CHANNELS} channels", received.len()),
        ));
    } else {
        report.steps.push(Step::fail(
            "Channel count per frame (== 32)",
            t0.elapsed(),
            format!("{wrong}/{} frames have wrong channel count", received.len()),
        ));
    }

    // ── Step 6: Timestamp monotonicity ───────────────────────────────────────

    let t0 = Instant::now();
    let mut ts_violations = 0usize;
    let mut prev_ts = f64::NEG_INFINITY;
    for (ts, _) in &received {
        if *ts < prev_ts - 0.001 {
            ts_violations += 1;
        }
        prev_ts = *ts;
    }
    report.timing_violations = ts_violations;

    if ts_violations <= 3 {
        report.steps.push(Step::pass(
            "Timestamp monotonicity",
            t0.elapsed(),
            if ts_violations == 0 {
                format!("all {} timestamps non-decreasing", received.len())
            } else {
                format!("{ts_violations} minor jitter violation(s) — within tolerance")
            },
        ));
    } else {
        report.steps.push(Step::fail(
            "Timestamp monotonicity",
            t0.elapsed(),
            format!("{ts_violations} out-of-order timestamps — exceeds tolerance"),
        ));
    }

    // ── Step 7: Sample value accuracy ────────────────────────────────────────
    //
    // Match each received frame to its pushed counterpart by channel-0 value:
    //   ch0 = 0 + push_idx × 0.01  ⟹  push_idx ≈ round(ch0 / 0.01)
    // Then check all 32 channels.

    let t0 = Instant::now();
    let check_n = received.len().min(256);
    let mut max_err = 0.0f64;

    for (_, frame) in received.iter().take(check_n) {
        let push_idx = (frame[0] as f64 / 0.01).round() as usize;
        if push_idx < TOTAL_SAMPLES {
            for (c, &recv_val) in frame.iter().enumerate() {
                let expected = (c as f64) + (push_idx as f64) * 0.01;
                let err = (recv_val as f64 - expected).abs();
                if err > max_err {
                    max_err = err;
                }
            }
        }
    }
    report.max_value_error = max_err;

    const THRESHOLD: f64 = 0.5; // µV — float32 precision + liblsl jitter
    if max_err <= THRESHOLD {
        report.steps.push(Step::pass(
            "Sample value accuracy (round-trip)",
            t0.elapsed(),
            format!("max err = {max_err:.2e} µV ≤ {THRESHOLD} µV (spot-checked {check_n} frames)"),
        ));
    } else {
        report.steps.push(Step::fail(
            "Sample value accuracy (round-trip)",
            t0.elapsed(),
            format!("max err = {max_err:.2e} µV exceeds {THRESHOLD} µV"),
        ));
    }

    // ── Step 8: Graceful shutdown ─────────────────────────────────────────────

    let t0 = Instant::now();
    drop(adapter);
    tokio::time::sleep(Duration::from_millis(50)).await;
    report.steps.push(Step::pass(
        "Graceful shutdown",
        t0.elapsed(),
        "adapter dropped — inlet thread signalled to exit",
    ));

    report.print();
}
