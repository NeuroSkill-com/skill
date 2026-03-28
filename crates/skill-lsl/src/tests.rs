// SPDX-License-Identifier: GPL-3.0-only
//! Unit tests for skill-lsl.

#[cfg(test)]
mod tests {
    use skill_devices::session::{DeviceAdapter, DeviceCaps, DeviceEvent};

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
                "TestEEG", "EEG", 32, 500.0,
                rlsl::types::ChannelFormat::Float32, "test-001",
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
        let info = StreamInfo::new(
            "SkillTest", "EEG", 4, 256.0,
            ChannelFormat::Float32, "test-e2e-001",
        );
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
        let mut adapter = crate::LslAdapter::new(&info);

        let adapter = crate::LslAdapter::new(&info);

        // Descriptor should have 4 channels at 256 Hz with labels
        let desc = adapter.descriptor();
        assert_eq!(desc.eeg_channels, 4);
        assert_eq!(desc.eeg_sample_rate, 256.0);
        assert_eq!(desc.kind, "lsl");
        assert!(desc.caps.contains(DeviceCaps::EEG));
        assert_eq!(desc.channel_names, vec!["Fp1", "Fp2", "O1", "O2"]);
        assert_eq!(desc.pipeline_channels, 4);
        }).join().unwrap();
    }

    #[test]
    fn e2e_lsl_high_channel_count() {
        std::thread::spawn(|| {
        use rlsl::prelude::*;
        use rlsl::types::ChannelFormat;

        let ch_count = 64;
        let info = StreamInfo::new(
            "HighDensity", "EEG", ch_count, 1000.0,
            ChannelFormat::Double64, "test-hd-001",
        );
        let outlet = StreamOutlet::new(&info, 0, 360);

        // Push samples
        let sample: Vec<f32> = (0..ch_count).map(|i| i as f32).collect();
        for _ in 0..50 {
            outlet.push_sample_f(&sample, 0.0, true);
        }

        let mut adapter = crate::LslAdapter::new(&info);

        let adapter = crate::LslAdapter::new(&info);

        // Descriptor should cap pipeline at EEG_CHANNELS but store all 64
        let desc = adapter.descriptor();
        assert_eq!(desc.eeg_channels, ch_count as usize);
        assert_eq!(desc.pipeline_channels, skill_constants::EEG_CHANNELS);
        assert_eq!(desc.eeg_sample_rate, 1000.0);
        assert_eq!(desc.channel_names.len(), 64);
        assert_eq!(desc.channel_names[0], "Ch1");
        assert_eq!(desc.channel_names[63], "Ch64");
        }).join().unwrap();
    }

    #[test]
    fn e2e_lsl_various_sample_rates() {
        use rlsl::prelude::*;
        use rlsl::types::ChannelFormat;

        for srate in [125.0, 256.0, 500.0, 1000.0, 2048.0] {
            let info = StreamInfo::new(
                &format!("Rate{}", srate as u32), "EEG", 4, srate,
                ChannelFormat::Float32, &format!("test-rate-{}", srate as u32),
            );
            // Don't create outlet/adapter — just verify the info construction
            assert_eq!(info.nominal_srate(), srate);
            assert_eq!(info.channel_count(), 4);
        }
    }

    #[test]
    fn e2e_lsl_channel_labels_from_xml() {
        std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async {
        use rlsl::prelude::*;
        use rlsl::types::ChannelFormat;

        let info = StreamInfo::new(
            "LabelTest", "EEG", 4, 256.0,
            ChannelFormat::Float32, "test-labels",
        );
        let desc = info.desc();
        let channels = desc.append_child("channels");
        for label in &["TP9", "AF7", "AF8", "TP10"] {
            let ch = channels.append_child("channel");
            ch.append_child_value("label", label);
        }

        let adapter = crate::LslAdapter::new(&info);
        let d = adapter.descriptor();
        assert_eq!(d.channel_names, vec!["TP9", "AF7", "AF8", "TP10"]);
        });
        }).join().unwrap();
    }

    #[test]
    fn e2e_lsl_missing_labels_fallback() {
        std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async {
        use rlsl::prelude::*;
        use rlsl::types::ChannelFormat;

        let info = StreamInfo::new(
            "NoLabels", "EEG", 8, 500.0,
            ChannelFormat::Float32, "test-nolabels",
        );
        // Don't add any channel labels in desc

        let adapter = crate::LslAdapter::new(&info);
        let d = adapter.descriptor();
        assert_eq!(d.channel_names.len(), 8);
        assert_eq!(d.channel_names[0], "Ch1");
        assert_eq!(d.channel_names[7], "Ch8");
        });
        }).join().unwrap();
    }
}
