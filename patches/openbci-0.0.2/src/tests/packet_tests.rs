#[cfg(test)]
mod tests {
    use crate::channel_config::GainTracker;
    use crate::packet::{
        cast_16bit_to_i32, cast_24bit_to_i32,
        decode_cyton, decode_cyton_daisy, decode_ganglion,
        GanglionState,
        END_BYTE_ANALOG, END_BYTE_MAX, END_BYTE_STANDARD, START_BYTE,
        GANGLION_EEG_SCALE,
    };
    use crate::sample::Sample;

    // ─── Constants ───────────────────────────────────────────────────────────

    #[test]
    fn start_byte_is_0xa0() {
        assert_eq!(START_BYTE, 0xA0);
    }

    #[test]
    fn end_bytes_are_in_order() {
        assert_eq!(END_BYTE_STANDARD, 0xC0);
        assert_eq!(END_BYTE_ANALOG,   0xC1);
        assert_eq!(END_BYTE_MAX,      0xC6);
        assert!(END_BYTE_STANDARD < END_BYTE_ANALOG);
        assert!(END_BYTE_ANALOG   < END_BYTE_MAX);
    }

    #[test]
    fn ganglion_eeg_scale_reasonable() {
        // Expected ~= 0.003030 µV/LSB  (1.2e6 / (8388607 * 1.5 * 51))
        let expected = 1.2e6 / (8_388_607.0 * 1.5 * 51.0);
        assert!(
            (GANGLION_EEG_SCALE - expected).abs() < 1e-9,
            "GANGLION_EEG_SCALE={} expected={}",
            GANGLION_EEG_SCALE, expected
        );
    }

    // ─── cast_24bit_to_i32 ───────────────────────────────────────────────────

    #[test]
    fn cast_24bit_positive_value() {
        // 0x00_00_01 = 1
        assert_eq!(cast_24bit_to_i32(&[0x00, 0x00, 0x01]), 1);
    }

    #[test]
    fn cast_24bit_max_positive() {
        // 0x7F_FF_FF = 8_388_607
        assert_eq!(cast_24bit_to_i32(&[0x7F, 0xFF, 0xFF]), 8_388_607);
    }

    #[test]
    fn cast_24bit_zero() {
        assert_eq!(cast_24bit_to_i32(&[0x00, 0x00, 0x00]), 0);
    }

    #[test]
    fn cast_24bit_minus_one() {
        // 0xFF_FF_FF in two's complement 24-bit = -1
        assert_eq!(cast_24bit_to_i32(&[0xFF, 0xFF, 0xFF]), -1);
    }

    #[test]
    fn cast_24bit_min_negative() {
        // 0x80_00_00 = -8_388_608
        assert_eq!(cast_24bit_to_i32(&[0x80, 0x00, 0x00]), -8_388_608);
    }

    #[test]
    fn cast_24bit_small_negative() {
        // 0xFF_FF_FE = -2
        assert_eq!(cast_24bit_to_i32(&[0xFF, 0xFF, 0xFE]), -2);
    }

    #[test]
    fn cast_24bit_big_endian_order() {
        // 0x01_02_03
        assert_eq!(cast_24bit_to_i32(&[0x01, 0x02, 0x03]), 0x0001_0203);
    }

    // ─── cast_16bit_to_i32 ───────────────────────────────────────────────────

    #[test]
    fn cast_16bit_positive() {
        assert_eq!(cast_16bit_to_i32(&[0x00, 0x01]), 1);
    }

    #[test]
    fn cast_16bit_max_positive() {
        assert_eq!(cast_16bit_to_i32(&[0x7F, 0xFF]), 32_767);
    }

    #[test]
    fn cast_16bit_zero() {
        assert_eq!(cast_16bit_to_i32(&[0x00, 0x00]), 0);
    }

    #[test]
    fn cast_16bit_minus_one() {
        assert_eq!(cast_16bit_to_i32(&[0xFF, 0xFF]), -1);
    }

    #[test]
    fn cast_16bit_min_negative() {
        assert_eq!(cast_16bit_to_i32(&[0x80, 0x00]), -32_768);
    }

    // ─── decode_cyton ────────────────────────────────────────────────────────

    /// Build a valid 32-byte Cyton body (bytes following 0xA0).
    /// `sample_num` = body[0], EEG bytes = body[1..25], aux = body[25..31],
    /// end_byte = body[31].
    fn make_cyton_body(sample_num: u8, eeg_raw: &[[u8; 3]; 8], aux: &[u8; 6], end: u8) -> [u8; 32] {
        let mut body = [0u8; 32];
        body[0] = sample_num;
        for (i, ch) in eeg_raw.iter().enumerate() {
            body[1 + i * 3]     = ch[0];
            body[1 + i * 3 + 1] = ch[1];
            body[1 + i * 3 + 2] = ch[2];
        }
        body[25..31].copy_from_slice(aux);
        body[31] = end;
        body
    }

    fn default_gains_8ch() -> GainTracker {
        GainTracker::new(vec![24.0; 8])
    }

    #[test]
    fn decode_cyton_valid_packet_returns_some() {
        let eeg_raw = [[0u8; 3]; 8];
        let aux = [0u8; 6];
        let body = make_cyton_body(1, &eeg_raw, &aux, END_BYTE_STANDARD);
        let result = decode_cyton(&body, &default_gains_8ch(), 8);
        assert!(result.is_some(), "should decode a valid packet");
    }

    #[test]
    fn decode_cyton_invalid_end_byte_returns_none() {
        let eeg_raw = [[0u8; 3]; 8];
        let aux = [0u8; 6];
        let body = make_cyton_body(1, &eeg_raw, &aux, 0xFF); // invalid end byte
        let result = decode_cyton(&body, &default_gains_8ch(), 8);
        assert!(result.is_none(), "invalid end byte should return None");
    }

    #[test]
    fn decode_cyton_end_byte_c6_is_valid() {
        let body = make_cyton_body(1, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_MAX);
        assert!(decode_cyton(&body, &default_gains_8ch(), 8).is_some());
    }

    #[test]
    fn decode_cyton_all_end_bytes_c0_to_c6_are_valid() {
        for end in 0xC0u8..=0xC6 {
            let body = make_cyton_body(1, &[[0u8; 3]; 8], &[0u8; 6], end);
            assert!(
                decode_cyton(&body, &default_gains_8ch(), 8).is_some(),
                "end byte 0x{end:02X} should be valid"
            );
        }
    }

    #[test]
    fn decode_cyton_sample_num_preserved() {
        let body = make_cyton_body(42, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        assert_eq!(sample.sample_num, 42);
    }

    #[test]
    fn decode_cyton_end_byte_preserved() {
        let body = make_cyton_body(1, &[[0u8; 3]; 8], &[0u8; 6], 0xC3);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        assert_eq!(sample.end_byte, 0xC3);
    }

    #[test]
    fn decode_cyton_zero_eeg_gives_zero_uv() {
        let body = make_cyton_body(0, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        for (i, &v) in sample.eeg.iter().enumerate() {
            assert_eq!(v, 0.0, "channel {i} should be 0 µV for zero input");
        }
    }

    #[test]
    fn decode_cyton_eeg_scaling_with_gain24() {
        // channel 0: raw = 0x000001 = 1 LSB
        let mut eeg_raw = [[0u8; 3]; 8];
        eeg_raw[0] = [0x00, 0x00, 0x01];
        let body = make_cyton_body(0, &eeg_raw, &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        // expected: 4.5 / 8_388_607.0 * 1_000_000.0 / 24.0 ≈ 0.02235 µV
        let expected = 4.5e6 / 8_388_607.0 / 24.0;
        let diff = (sample.eeg[0] - expected).abs();
        assert!(diff < 1e-9, "eeg[0]={} expected={} diff={}", sample.eeg[0], expected, diff);
    }

    #[test]
    fn decode_cyton_eeg_scaling_gain1_is_24x_larger() {
        let mut eeg_raw = [[0u8; 3]; 8];
        eeg_raw[0] = [0x00, 0x00, 0x01];
        let body = make_cyton_body(0, &eeg_raw, &[0u8; 6], END_BYTE_STANDARD);
        let gains_24 = default_gains_8ch();
        let gains_1  = GainTracker::new(vec![1.0; 8]);
        let s24 = decode_cyton(&body, &gains_24, 8).unwrap();
        let s1  = decode_cyton(&body, &gains_1,  8).unwrap();
        let ratio = s1.eeg[0] / s24.eeg[0];
        assert!(
            (ratio - 24.0).abs() < 1e-9,
            "gain=1 should give 24× more µV than gain=24, got ratio={ratio}"
        );
    }

    #[test]
    fn decode_cyton_max_positive_raw_is_positive_uv() {
        let mut eeg_raw = [[0u8; 3]; 8];
        eeg_raw[0] = [0x7F, 0xFF, 0xFF]; // max positive 24-bit
        let body = make_cyton_body(0, &eeg_raw, &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        assert!(sample.eeg[0] > 0.0, "max positive raw should be positive µV");
    }

    #[test]
    fn decode_cyton_max_negative_raw_is_negative_uv() {
        let mut eeg_raw = [[0u8; 3]; 8];
        eeg_raw[0] = [0x80, 0x00, 0x00]; // min (most negative) 24-bit
        let body = make_cyton_body(0, &eeg_raw, &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        assert!(sample.eeg[0] < 0.0, "max negative raw should be negative µV");
    }

    #[test]
    fn decode_cyton_minus_one_raw() {
        let mut eeg_raw = [[0u8; 3]; 8];
        eeg_raw[0] = [0xFF, 0xFF, 0xFF]; // -1 in 24-bit two's complement
        let body = make_cyton_body(0, &eeg_raw, &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        let expected = 4.5e6 / 8_388_607.0 / 24.0 * -1.0;
        let diff = (sample.eeg[0] - expected).abs();
        assert!(diff < 1e-9, "eeg[0]={} expected={}", sample.eeg[0], expected);
    }

    #[test]
    fn decode_cyton_eeg_has_8_channels() {
        let body = make_cyton_body(0, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        assert_eq!(sample.eeg.len(), 8);
    }

    #[test]
    fn decode_cyton_standard_end_byte_decodes_accel() {
        // aux bytes: X=0x0100 (256 LSB), Y=0, Z=0
        let aux = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
        let body = make_cyton_body(0, &[[0u8; 3]; 8], &aux, END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        let accel = sample.accel.expect("standard packet should have accel");
        // ACCEL_SCALE = 0.002/16 = 0.000125 g/LSB; 256 LSB → 0.032 g
        assert!((accel[0] - 0.032).abs() < 1e-9, "ax={}", accel[0]);
        assert_eq!(accel[1], 0.0);
        assert_eq!(accel[2], 0.0);
    }

    #[test]
    fn decode_cyton_zero_accel_is_none() {
        // All-zero aux bytes → accel should be None (ax==0 check)
        let body = make_cyton_body(0, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        assert!(sample.accel.is_none(), "zero aux should give None accel");
    }

    #[test]
    fn decode_cyton_analog_end_byte_decodes_analog_pins() {
        let aux = [0x00, 0x0A, 0x00, 0x14, 0x00, 0x1E]; // 10, 20, 30
        let body = make_cyton_body(0, &[[0u8; 3]; 8], &aux, END_BYTE_ANALOG);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        let analog = sample.analog.expect("analog packet should have analog field");
        assert_eq!(analog[0], 10.0);
        assert_eq!(analog[1], 20.0);
        assert_eq!(analog[2], 30.0);
        assert!(sample.accel.is_none(), "analog mode should not have accel");
    }

    #[test]
    fn decode_cyton_aux_bytes_preserved_raw() {
        let aux = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let body = make_cyton_body(0, &[[0u8; 3]; 8], &aux, END_BYTE_STANDARD);
        let sample = decode_cyton(&body, &default_gains_8ch(), 8).unwrap();
        assert_eq!(sample.aux_bytes, aux);
    }

    // ─── decode_cyton_daisy ──────────────────────────────────────────────────

    #[test]
    fn decode_daisy_odd_packet_alone_returns_none() {
        // Cyton (odd sample_num) with no prior Daisy packet → None
        let body = make_cyton_body(1, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        let mut gains = GainTracker::new(vec![24.0; 16]);
        let mut buf: Option<Sample> = None;
        let result = decode_cyton_daisy(&body, &mut gains, &mut buf);
        assert!(result.is_none(), "odd (Cyton) packet without prior Daisy should return None");
        assert!(buf.is_none(), "buffer should remain None after odd-only packet");
    }

    #[test]
    fn decode_daisy_even_packet_alone_buffers_not_emits() {
        // Daisy (even sample_num) → goes into buffer, returns None
        let body = make_cyton_body(0, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        let mut gains = GainTracker::new(vec![24.0; 16]);
        let mut buf: Option<Sample> = None;
        let result = decode_cyton_daisy(&body, &mut gains, &mut buf);
        assert!(result.is_none(), "Daisy packet alone should not emit a sample");
        assert!(buf.is_some(), "buffer should hold the partial sample");
    }

    #[test]
    fn decode_daisy_pair_even_then_odd_emits_sample() {
        let mut gains = GainTracker::new(vec![24.0; 16]);
        let mut buf: Option<Sample> = None;

        // Daisy packet (even)
        let body_even = make_cyton_body(0, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        let r1 = decode_cyton_daisy(&body_even, &mut gains, &mut buf);
        assert!(r1.is_none());

        // Cyton packet (odd)
        let body_odd = make_cyton_body(1, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        let r2 = decode_cyton_daisy(&body_odd, &mut gains, &mut buf);
        assert!(r2.is_some(), "even+odd pair should emit a sample");
        assert!(buf.is_none(), "buffer should be empty after emit");
    }

    #[test]
    fn decode_daisy_emitted_sample_has_16_channels() {
        let mut gains = GainTracker::new(vec![24.0; 16]);
        let mut buf: Option<Sample> = None;

        let body_even = make_cyton_body(0, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        decode_cyton_daisy(&body_even, &mut gains, &mut buf);

        let body_odd = make_cyton_body(1, &[[0u8; 3]; 8], &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton_daisy(&body_odd, &mut gains, &mut buf).unwrap();
        assert_eq!(sample.eeg.len(), 16, "merged sample should have 16 channels");
    }

    #[test]
    fn decode_daisy_channel_assignment_is_correct() {
        // Daisy body with channel 0 = 0x000001 → should appear as eeg[8] in merged sample
        // Cyton body with channel 0 = 0x000002 → should appear as eeg[0]
        let mut gains = GainTracker::new(vec![1.0; 16]); // gain=1 for easy math
        let mut buf: Option<Sample> = None;

        let mut daisy_eeg = [[0u8; 3]; 8];
        daisy_eeg[0] = [0x00, 0x00, 0x01]; // ch8 raw = 1

        let body_even = make_cyton_body(0, &daisy_eeg, &[0u8; 6], END_BYTE_STANDARD);
        decode_cyton_daisy(&body_even, &mut gains, &mut buf);

        let mut cyton_eeg = [[0u8; 3]; 8];
        cyton_eeg[0] = [0x00, 0x00, 0x02]; // ch0 raw = 2

        let body_odd = make_cyton_body(1, &cyton_eeg, &[0u8; 6], END_BYTE_STANDARD);
        let sample = decode_cyton_daisy(&body_odd, &mut gains, &mut buf).unwrap();

        let scale = 4.5e6 / 8_388_607.0; // gain=1
        assert!((sample.eeg[0] - scale * 2.0).abs() < 1e-9, "Cyton ch0 wrong: {}", sample.eeg[0]);
        assert!((sample.eeg[8] - scale * 1.0).abs() < 1e-9, "Daisy ch8 wrong: {}", sample.eeg[8]);
    }

    #[test]
    fn decode_daisy_invalid_end_byte_returns_none() {
        let body = make_cyton_body(0, &[[0u8; 3]; 8], &[0u8; 6], 0xBF); // too low
        let mut gains = GainTracker::new(vec![24.0; 16]);
        let mut buf: Option<Sample> = None;
        assert!(decode_cyton_daisy(&body, &mut gains, &mut buf).is_none());
    }

    // ─── decode_ganglion ─────────────────────────────────────────────────────

    fn make_ganglion_18bit_packet(id: u8) -> Vec<u8> {
        // 20 bytes: [id, then 19 bytes for bits + 1 accel byte]
        // Pad with zeros → 8 zero-deltas, which means 0 µV output
        vec![id, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    }

    #[test]
    fn decode_ganglion_empty_data_returns_empty() {
        let mut state = GanglionState::default();
        let result = decode_ganglion(&[], &mut state, 3);
        assert!(result.is_empty());
    }

    #[test]
    fn decode_ganglion_one_byte_returns_empty() {
        let mut state = GanglionState::default();
        let result = decode_ganglion(&[0x01], &mut state, 3);
        assert!(result.is_empty());
    }

    #[test]
    fn decode_ganglion_fw3_packet_id_0_returns_2_samples() {
        let pkt = make_ganglion_18bit_packet(0);
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        assert_eq!(samples.len(), 2, "18-bit packet should yield 2 samples");
    }

    #[test]
    fn decode_ganglion_fw3_packet_id_99_returns_2_samples() {
        let pkt = make_ganglion_18bit_packet(99);
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        assert_eq!(samples.len(), 2);
    }

    #[test]
    fn decode_ganglion_fw3_packet_id_100_returns_2_samples_19bit() {
        let pkt = make_ganglion_18bit_packet(100);
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        assert_eq!(samples.len(), 2);
    }

    #[test]
    fn decode_ganglion_samples_have_4_channels() {
        let pkt = make_ganglion_18bit_packet(1);
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        for (i, s) in samples.iter().enumerate() {
            assert_eq!(s.eeg.len(), 4, "sample {i} should have 4 channels");
        }
    }

    #[test]
    fn decode_ganglion_samples_have_accel_field() {
        let pkt = make_ganglion_18bit_packet(1);
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        for (i, s) in samples.iter().enumerate() {
            assert!(s.accel.is_some(), "sample {i} should have accel");
        }
    }

    #[test]
    fn decode_ganglion_impedance_packet_201_to_205_returns_1_sample() {
        for id in 201u8..=205 {
            // "Z" terminator at byte 2
            let pkt = vec![id, b'1', b'2', b'3', b'Z', 0, 0, 0, 0, 0,
                           0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let mut state = GanglionState::default();
            let samples = decode_ganglion(&pkt, &mut state, 3);
            assert_eq!(
                samples.len(), 1,
                "impedance packet id={id} should yield 1 sample"
            );
        }
    }

    #[test]
    fn decode_ganglion_impedance_packet_parses_value() {
        // packet id=201 (channel 1), value "5000Z"
        let pkt: Vec<u8> = vec![201, b'5', b'0', b'0', b'0', b'Z', 0, 0, 0, 0,
                                  0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        assert_eq!(samples.len(), 1);
        let resist = samples[0].resistance.as_ref().expect("impedance sample should have resistance");
        // id % 10 = 1 → slot 0 (channel 1 impedance)
        assert_eq!(resist[0], 5000.0, "resistance[0] should be 5000 Ω");
    }

    #[test]
    fn decode_ganglion_packet_id_200_is_treated_as_normal_eeg() {
        // id=200: not >200 so not "unknown"; falls through to normal EEG path
        let pkt = make_ganglion_18bit_packet(200);
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        // Normal 19-bit path (id >= 100): returns 2 samples
        assert_eq!(samples.len(), 2, "packet id=200 is treated as 19-bit EEG packet");
    }

    #[test]
    fn decode_ganglion_packet_id_206_returns_empty() {
        // id=206: >200 but not in impedance range (201-205) → unknown, returns empty
        let pkt = make_ganglion_18bit_packet(206);
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        assert!(samples.is_empty(), "packet id=206 (unknown) should return empty");
    }

    #[test]
    fn decode_ganglion_fw2_packet_id_0_returns_sample() {
        // Firmware 2, packet id=0: uncompressed init packet
        // data[1..13] = 4 × 24-bit values
        let mut pkt = vec![0u8; 20];
        pkt[0] = 0;
        // channel 0: 0x000001
        pkt[1] = 0x00; pkt[2] = 0x00; pkt[3] = 0x01;
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 2);
        assert_eq!(samples.len(), 1, "fw2 init packet should yield 1 sample");
    }

    #[test]
    fn decode_ganglion_fw2_packet_id_1_to_100_yields_2_samples() {
        for id in [1u8, 50, 100] {
            let pkt = make_ganglion_18bit_packet(id);
            let mut state = GanglionState::default();
            let samples = decode_ganglion(&pkt, &mut state, 2);
            assert_eq!(samples.len(), 2, "fw2 packet id={id} should yield 2 samples");
        }
    }

    #[test]
    fn decode_ganglion_fw3_accel_updated_on_mod10_match() {
        // id=0 (mod 10 = 0) → updates accel[2] (z axis)
        // Byte 19 = 0x08 (signed = 8) → z = -GANGLION_ACCEL_SCALE * 8 = -0.128 g
        let mut pkt = make_ganglion_18bit_packet(0);
        pkt[19] = 0x08u8; // 8 as i8
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        assert_eq!(samples.len(), 2);
        let accel = samples[0].accel.unwrap();
        assert!(
            (accel[2] - (-0.016 * 8.0)).abs() < 1e-9,
            "accel[2]={} should be -0.128 g",
            accel[2]
        );
    }

    #[test]
    fn decode_ganglion_fw3_accel_y_updated_on_mod10_1() {
        // id=1 (mod 10 = 1) → updates accel[1] (y axis)
        let mut pkt = make_ganglion_18bit_packet(1);
        pkt[19] = 0x05u8; // 5 as i8 → y = +0.016*5 = 0.08 g
        let mut state = GanglionState::default();
        let samples = decode_ganglion(&pkt, &mut state, 3);
        let accel = samples[0].accel.unwrap();
        assert!(
            (accel[1] - 0.016 * 5.0).abs() < 1e-9,
            "accel[1]={} should be 0.08 g",
            accel[1]
        );
    }

    // ─── round-trip numerical consistency ────────────────────────────────────

    #[test]
    fn cyton_eeg_positive_negative_symmetry() {
        // +1 and -1 raw values should give equal-magnitude, opposite-sign µV
        let mut gains = default_gains_8ch();

        let mut pos_eeg = [[0u8; 3]; 8];
        pos_eeg[0] = [0x00, 0x00, 0x01];
        let pos_body = make_cyton_body(0, &pos_eeg, &[0u8; 6], END_BYTE_STANDARD);
        let pos_sample = decode_cyton(&pos_body, &gains, 8).unwrap();

        let mut neg_eeg = [[0u8; 3]; 8];
        neg_eeg[0] = [0xFF, 0xFF, 0xFF];
        let neg_body = make_cyton_body(0, &neg_eeg, &[0u8; 6], END_BYTE_STANDARD);
        let neg_sample = decode_cyton(&neg_body, &gains, 8).unwrap();

        assert!(
            (pos_sample.eeg[0] + neg_sample.eeg[0]).abs() < 1e-15,
            "+1 and -1 raw should give equal-magnitude µV"
        );
    }

    #[test]
    fn cyton_eeg_full_scale_range() {
        // Max positive: 0x7FFFFF; max negative: 0x800000
        let gains = default_gains_8ch();

        let mut max_eeg = [[0u8; 3]; 8];
        max_eeg[0] = [0x7F, 0xFF, 0xFF];
        let pos_body = make_cyton_body(0, &max_eeg, &[0u8; 6], END_BYTE_STANDARD);
        let pos_s = decode_cyton(&pos_body, &gains, 8).unwrap();

        let mut min_eeg = [[0u8; 3]; 8];
        min_eeg[0] = [0x80, 0x00, 0x00];
        let neg_body = make_cyton_body(0, &min_eeg, &[0u8; 6], END_BYTE_STANDARD);
        let neg_s = decode_cyton(&neg_body, &gains, 8).unwrap();

        // With gain=24: range ≈ ±187 µV
        assert!(pos_s.eeg[0] > 180.0, "max positive µV={}", pos_s.eeg[0]);
        assert!(neg_s.eeg[0] < -180.0, "max negative µV={}", neg_s.eeg[0]);
    }
}
