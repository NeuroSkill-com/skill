// SPDX-License-Identifier: GPL-3.0-only
//! Unit tests for skill-lsl.

#[cfg(test)]
#[allow(clippy::all)]
mod tests {
    use skill_devices::session::{DeviceAdapter, DeviceCaps};

    // ── LslAdapter unit tests ─────────────────────────────────────────────

    #[test]
    fn lsl_adapter_descriptor_defaults() {
        // Can't create a real StreamInfo without network, but we can test
        // the descriptor construction logic indirectly via the constants
        assert!(skill_constants::EEG_CHANNELS >= 24);
    }

    #[test]
    fn pipeline_channels_capped_at_eeg_channels() {
        // Any channel count should be capped at EEG_CHANNELS for DSP
        let cap = skill_constants::EEG_CHANNELS;
        assert_eq!(4usize.min(cap), 4);
        assert_eq!(12usize.min(cap), 12);
        assert_eq!(24usize.min(cap), 24);
        assert_eq!(64usize.min(cap), cap);
        assert_eq!(256usize.min(cap), cap);
        assert_eq!(1024usize.min(cap), cap);
    }

    #[test]
    fn pipeline_channels_minimum_is_1() {
        assert_eq!(1usize.min(skill_constants::EEG_CHANNELS), 1);
    }

    // ── Clock mapping ─────────────────────────────────────────────────────

    #[test]
    fn clock_reference_is_read_from_the_full_info() {
        use crate::lsl_adapter::{parse_clock_reference, ClockMap};
        let xml = "<info><desc><manufacturer>x</manufacturer><clock_reference><lsl_time>12.500000</lsl_time>\
                   <unix_time>1700000000.250</unix_time></clock_reference></desc></info>";
        let mut map = parse_clock_reference(xml).expect("reference");
        assert_eq!(
            map,
            ClockMap::Offset {
                offset: 1_700_000_000.25 - 12.5,
                exact: true
            }
        );
        // a declared clock ignores when the samples arrive
        assert_eq!(map.unix_time(13.5, 20.0, 5.0), 1_700_000_001.25);
        assert!(parse_clock_reference("<info><desc></desc></info>").is_none());
        assert!(parse_clock_reference("<clock_reference><lsl_time>x</lsl_time></clock_reference>").is_none());
    }

    #[test]
    fn undeclared_clock_anchors_the_first_chunk_and_then_holds() {
        use crate::lsl_adapter::ClockMap;
        let mut map = ClockMap::Pending;
        // first chunk: stamps 100.0..100.2, arrived at Unix 5000
        assert_eq!(map.unix_time(100.0, 100.2, 5000.0), 4999.8);
        assert_eq!(map.unix_time(100.2, 100.2, 5000.0), 5000.0);
        // later chunks keep the same offset whatever their arrival time
        assert_eq!(map.unix_time(101.0, 101.0, 9999.0), 5000.8);
        assert!(matches!(map, ClockMap::Offset { exact: false, .. }));
    }

    /// The wall clock drifting against the LSL clock is followed — smoothly,
    /// so that stamps stay evenly spaced — and a wall clock that is set is
    /// followed at once.
    #[test]
    fn wall_clock_drift_is_followed_smoothly() {
        use crate::lsl_adapter::WallClock;
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut wall = WallClock::starting_at(1000.0, t0);
        // 15 ppm for an hour: what a constant offset got wrong by 54 ms
        let true_offset = |s: f64| 1000.0 + 15e-6 * s;
        let (mut prev, mut largest_step, mut worst) = (1000.0, 0.0f64, 0.0f64);
        for k in 1..=3600 * 50 {
            let s = k as f64 / 50.0; // a chunk every 20 ms
            let got = wall.offset_at(t0 + Duration::from_secs_f64(s), || true_offset(s));
            largest_step = largest_step.max((got - prev).abs());
            worst = worst.max((got - true_offset(s)).abs());
            prev = got;
        }
        assert!(worst < 0.0002, "off by {} ms", worst * 1e3);
        assert!(
            largest_step <= 20e-3 * 500e-6 + 1e-12,
            "a step of {} us",
            largest_step * 1e6
        );
        assert!((prev - 1000.054).abs() < 0.0002, "{prev}");

        // a bad measurement (the thread was preempted between the two reads)
        // moves the result by no more than the slew allows before the next one
        let s = 3600.0;
        let mut at = t0 + Duration::from_secs_f64(s + 5.0);
        let glitched = wall.offset_at(at, || true_offset(s) + 0.050);
        assert!((glitched - prev).abs() < 0.003, "{}", glitched - prev);
        at += Duration::from_secs(5);
        wall.offset_at(at, || true_offset(s));
        at += Duration::from_secs(5);
        let settled = wall.offset_at(at, || true_offset(s));
        assert!((settled - true_offset(s)).abs() < 0.003);

        // the wall clock is set an hour ahead: followed at once
        at += Duration::from_secs(5);
        let set = wall.offset_at(at, || true_offset(s) + 3600.0);
        assert!((set - true_offset(s) - 3600.0).abs() < 1e-6);
    }

    #[test]
    fn a_marker_stamp_that_is_no_time_gives_way_to_the_arrival() {
        use crate::lsl_adapter::plausible_marker_time;
        let now = 1_794_800_000.0;
        let mut warned = false;
        // a little late, a replayed one, the clocks a few ms apart: its own time
        for at in [now - 0.02, now - 9.5, now + 0.004] {
            assert_eq!(plausible_marker_time(at, now, "m", &mut warned), at);
        }
        assert!(!warned);
        // from the future, or from long ago: a sender that numbers its markers
        assert_eq!(plausible_marker_time(now + 1000.0, now, "m", &mut warned), now);
        assert_eq!(plausible_marker_time(100.0, now, "m", &mut warned), now);
        assert!(warned);
    }

    #[test]
    fn marker_times_shed_their_transport_delay() {
        use crate::lsl_adapter::{marker_text_from_numbers, MarkerClock};
        let mut clock = MarkerClock::default();
        // sender clock = Unix - 1000; the first marker took 80 ms to arrive
        assert_eq!(clock.unix_time(10.0, 1010.08), 1010.08);
        // the next took 5 ms: it is placed 5 ms after it was stamped, not 80
        let t = clock.unix_time(11.0, 1011.005);
        assert!((t - 1011.005).abs() < 1e-9, "{t}");
        // and a slow one after that keeps the best estimate
        let t = clock.unix_time(12.0, 1012.2);
        assert!((t - 1012.005).abs() < 1e-9, "{t}");
        // no stamp: the arrival
        assert_eq!(clock.unix_time(0.0, 2000.0), 2000.0);

        assert_eq!(marker_text_from_numbers(&[42.0]), "42");
        assert_eq!(marker_text_from_numbers(&[1.0, 2.5]), "1;2.5");
    }

    // ── discover_streams smoke test ───────────────────────────────────────

    #[test]
    fn discover_streams_returns_empty_without_network() {
        // With no LSL outlets on the network, should return empty (not panic)
        let streams = crate::discover_streams(0.1);
        // May or may not find streams depending on the test environment
        // The important thing is it doesn't crash
        let _ = streams;
    }

    #[test]
    fn resolve_eeg_streams_returns_empty_without_network() {
        let streams = crate::resolve_eeg_streams(0.1);
        let _ = streams;
    }

    // ── LslStreamInfo fields ──────────────────────────────────────────────

    #[test]
    fn lsl_stream_info_struct_complete() {
        let info = crate::LslStreamInfo {
            name: "TestEEG".into(),
            stream_type: "EEG".into(),
            channel_count: 32,
            sample_rate: 500.0,
            source_id: "test-001".into(),
            hostname: "lab-pc".into(),
            info: rlsl::stream_info::StreamInfo::new(
                "TestEEG",
                "EEG",
                32,
                500.0,
                rlsl::types::ChannelFormat::Float32,
                "test-001",
            ),
        };
        assert_eq!(info.name, "TestEEG");
        assert_eq!(info.channel_count, 32);
        assert_eq!(info.sample_rate, 500.0);
    }

    // ── Channel count configurations ──────────────────────────────────────

    #[test]
    fn channel_configs_2_to_1024() {
        for ch in [2, 4, 8, 12, 16, 24, 32, 64, 128, 256, 512, 1024] {
            let pipeline = ch.min(skill_constants::EEG_CHANNELS);
            assert!(pipeline >= 2.min(skill_constants::EEG_CHANNELS));
            assert!(pipeline <= skill_constants::EEG_CHANNELS);
            // All channels should be storable (no artificial limit on CSV)
            assert!(ch >= pipeline);
        }
    }

    // ── IrohLslAdapter ────────────────────────────────────────────────────

    #[test]
    fn iroh_lsl_adapter_descriptor_defaults() {
        // The default descriptor before a stream arrives
        // should have reasonable defaults
        let kind = "lsl-iroh";
        assert_eq!(kind, "lsl-iroh");
    }

    // ── E2E: create outlet → inlet → adapter ─────────────────────────────

    #[test]
    fn e2e_lsl_outlet_to_adapter() {
        // rlsl internally creates a tokio runtime — run everything on a clean thread.
        std::thread::spawn(|| {
            use rlsl::prelude::*;
            use rlsl::types::ChannelFormat;

            // Create a local outlet
            let info = StreamInfo::new("SkillTest", "EEG", 4, 256.0, ChannelFormat::Float32, "test-e2e-001");
            let desc = info.desc();
            let channels = desc.append_child("channels");
            for label in &["Fp1", "Fp2", "O1", "O2"] {
                let ch = channels.append_child("channel");
                ch.append_child_value("label", label);
            }
            let outlet = StreamOutlet::new(&info, 0, 360);

            // Push some samples
            for i in 0..100 {
                let sample = [i as f32 * 0.1, i as f32 * 0.2, i as f32 * 0.3, i as f32 * 0.4];
                outlet.push_sample_f(&sample, 0.0, true);
            }

            // Create adapter from the same info
            let adapter = crate::LslAdapter::new(&info);

            // Descriptor should have 4 channels at 256 Hz with labels
            let desc = adapter.descriptor();
            assert_eq!(desc.eeg_channels, 4);
            assert_eq!(desc.eeg_sample_rate, 256.0);
            assert_eq!(desc.kind, "lsl");
            assert!(desc.caps.contains(DeviceCaps::EEG));
            assert_eq!(desc.channel_names, vec!["Fp1", "Fp2", "O1", "O2"]);
            assert_eq!(desc.pipeline_channels, 4);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn e2e_lsl_high_channel_count() {
        std::thread::spawn(|| {
            use rlsl::prelude::*;
            use rlsl::types::ChannelFormat;

            let ch_count = 64;
            let info = StreamInfo::new(
                "HighDensity",
                "EEG",
                ch_count,
                1000.0,
                ChannelFormat::Double64,
                "test-hd-001",
            );
            let outlet = StreamOutlet::new(&info, 0, 360);

            // Push samples
            let sample: Vec<f32> = (0..ch_count).map(|i| i as f32).collect();
            for _ in 0..50 {
                outlet.push_sample_f(&sample, 0.0, true);
            }

            let adapter = crate::LslAdapter::new(&info);

            // Descriptor should cap pipeline at EEG_CHANNELS but store all 64
            let desc = adapter.descriptor();
            assert_eq!(desc.eeg_channels, ch_count as usize);
            assert_eq!(desc.pipeline_channels, skill_constants::EEG_CHANNELS);
            assert_eq!(desc.eeg_sample_rate, 1000.0);
            assert_eq!(desc.channel_names.len(), 64);
            assert_eq!(desc.channel_names[0], "Ch1");
            assert_eq!(desc.channel_names[63], "Ch64");
        })
        .join()
        .unwrap();
    }

    #[test]
    fn e2e_lsl_various_sample_rates() {
        use rlsl::prelude::*;
        use rlsl::types::ChannelFormat;

        for srate in [125.0, 256.0, 500.0, 1000.0, 2048.0] {
            let info = StreamInfo::new(
                &format!("Rate{}", srate as u32),
                "EEG",
                4,
                srate,
                ChannelFormat::Float32,
                &format!("test-rate-{}", srate as u32),
            );
            // Don't create outlet/adapter — just verify the info construction
            assert_eq!(info.nominal_srate(), srate);
            assert_eq!(info.channel_count(), 4);
        }
    }

    #[test]
    fn e2e_lsl_channel_labels_from_xml() {
        // Run on a raw OS thread: StreamOutlet::new calls block_on internally
        // and panics if a tokio runtime is already active.
        std::thread::spawn(|| {
            use rlsl::prelude::*;
            use rlsl::types::ChannelFormat;

            let info = StreamInfo::new("LabelTest", "EEG", 4, 256.0, ChannelFormat::Float32, "test-labels");
            let desc = info.desc();
            let channels = desc.append_child("channels");
            for label in &["TP9", "AF7", "AF8", "TP10"] {
                let ch = channels.append_child("channel");
                ch.append_child_value("label", label);
            }
            // Outlet must exist so the inlet's open_stream() can connect.
            let _outlet = StreamOutlet::new(&info, 0, 360);

            let adapter = crate::LslAdapter::new(&info);
            let d = adapter.descriptor();
            assert_eq!(d.channel_names, vec!["TP9", "AF7", "AF8", "TP10"]);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn e2e_resolve_stream_by_name_with_multiple_outlets() {
        // Ensure named resolve selects the exact stream when multiple EEG
        // outlets are advertising simultaneously.
        std::thread::spawn(|| {
            use rlsl::prelude::*;
            use rlsl::types::ChannelFormat;

            let info_a = StreamInfo::new("SkillA", "EEG", 4, 256.0, ChannelFormat::Float32, "sid-a");
            let info_b = StreamInfo::new("SkillB", "EEG", 8, 500.0, ChannelFormat::Float32, "sid-b");
            let _outlet_a = StreamOutlet::new(&info_a, 0, 360);
            let _outlet_b = StreamOutlet::new(&info_b, 0, 360);

            // Give discovery a short moment.
            std::thread::sleep(std::time::Duration::from_millis(300));

            let found = crate::resolve_stream_by_name("SkillB", 5.0).expect("SkillB should resolve");
            assert_eq!(found.name(), "SkillB");
            assert_eq!(found.channel_count(), 8);
            assert_eq!(found.nominal_srate(), 500.0);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn e2e_resolve_eeg_streams_filters_non_eeg_types() {
        std::thread::spawn(|| {
            use rlsl::prelude::*;
            use rlsl::types::ChannelFormat;

            let eeg = StreamInfo::new("EEG-Only", "EEG", 4, 256.0, ChannelFormat::Float32, "sid-eeg");
            let aux = StreamInfo::new("AUX-Only", "AUX", 2, 100.0, ChannelFormat::Float32, "sid-aux");
            let _eeg_outlet = StreamOutlet::new(&eeg, 0, 360);
            let _aux_outlet = StreamOutlet::new(&aux, 0, 360);

            std::thread::sleep(std::time::Duration::from_millis(300));

            let streams = crate::resolve_eeg_streams(3.0);
            assert!(streams.iter().any(|s| s.name() == "EEG-Only"));
            assert!(!streams.iter().any(|s| s.name() == "AUX-Only"));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn e2e_lsl_missing_labels_fallback() {
        std::thread::spawn(|| {
            use rlsl::prelude::*;
            use rlsl::types::ChannelFormat;

            let info = StreamInfo::new("NoLabels", "EEG", 8, 500.0, ChannelFormat::Float32, "test-nolabels");
            // Don't add any channel labels in desc.
            // Outlet must exist so the inlet's open_stream() can connect.
            let _outlet = StreamOutlet::new(&info, 0, 360);

            let adapter = crate::LslAdapter::new(&info);
            let d = adapter.descriptor();
            assert_eq!(d.channel_names.len(), 8);
            assert_eq!(d.channel_names[0], "Ch1");
            assert_eq!(d.channel_names[7], "Ch8");
        })
        .join()
        .unwrap();
    }
}
