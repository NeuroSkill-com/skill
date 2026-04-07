// SPDX-License-Identifier: GPL-3.0-only
//! Tests for LSL stream resolution — fast named resolve vs full discovery.
#![allow(clippy::unwrap_used)]

use rlsl::prelude::*;
use rlsl::types::ChannelFormat;
use std::time::{Duration, Instant};

/// `resolve_stream_by_name` returns a match in well under 5 s for a local outlet.
#[test]
fn resolve_by_name_is_fast() {
    let info = StreamInfo::new(
        "FastResolveTest",
        "EEG",
        4,
        256.0,
        ChannelFormat::Float32,
        "fast-resolve-001",
    );
    let _outlet = StreamOutlet::new(&info, 0, 360);

    let t0 = Instant::now();
    let result = skill_lsl::resolve_stream_by_name("FastResolveTest", 5.0);
    let elapsed = t0.elapsed();

    assert!(result.is_some(), "should resolve the local stream");
    let found = result.unwrap();
    assert_eq!(found.name(), "FastResolveTest");
    assert_eq!(found.channel_count(), 4);

    // Should resolve much faster than the 5 s timeout — typically < 1 s.
    assert!(
        elapsed < Duration::from_secs(3),
        "resolve_stream_by_name took {elapsed:?} — expected < 3s for a local stream"
    );
}

/// `resolve_stream_by_name` returns None when no matching stream exists.
#[test]
fn resolve_by_name_returns_none_for_missing() {
    let t0 = Instant::now();
    let result = skill_lsl::resolve_stream_by_name("NonexistentStream_XYZ_999", 1.0);
    let elapsed = t0.elapsed();

    assert!(result.is_none(), "should not find a nonexistent stream");
    // Should wait the full timeout
    assert!(
        elapsed >= Duration::from_millis(800),
        "should wait close to the timeout, took {elapsed:?}"
    );
}

/// `resolve_eeg_streams` only returns EEG/EXG/Biosignal types.
#[test]
fn resolve_eeg_streams_filters_by_type() {
    let eeg_info = StreamInfo::new("EegStream", "EEG", 4, 256.0, ChannelFormat::Float32, "filter-eeg-001");
    let _eeg_outlet = StreamOutlet::new(&eeg_info, 0, 360);

    let marker_info = StreamInfo::new(
        "MarkerStream",
        "Markers",
        1,
        0.0,
        ChannelFormat::String,
        "filter-marker-001",
    );
    let _marker_outlet = StreamOutlet::new(&marker_info, 0, 360);

    let streams = skill_lsl::resolve_eeg_streams(2.0);
    let names: Vec<String> = streams.iter().map(|s| s.name().to_string()).collect();

    assert!(
        names.contains(&"EegStream".to_string()),
        "should find EEG stream, got: {names:?}"
    );
    // Marker stream should be filtered out (type != eeg/exg/biosignal)
    assert!(
        !names.contains(&"MarkerStream".to_string()),
        "should not include Markers stream, got: {names:?}"
    );
}
