#[cfg(test)]
mod tests {
    use crate::channel_config::{
        gain_from_code, ChannelConfig, Gain, GainTracker, InputType, CHANNEL_LETTERS,
        GAIN_VALUES,
    };

    // ── Gain ──────────────────────────────────────────────────────────────────

    #[test]
    fn gain_multipliers_are_correct() {
        let cases = [
            (Gain::X1,  1.0),
            (Gain::X2,  2.0),
            (Gain::X4,  4.0),
            (Gain::X6,  6.0),
            (Gain::X8,  8.0),
            (Gain::X12, 12.0),
            (Gain::X24, 24.0),
        ];
        for (gain, expected) in cases {
            assert_eq!(gain.multiplier(), expected, "wrong multiplier for {:?}", gain);
        }
    }

    #[test]
    fn gain_codes_are_0_to_6() {
        let gains = [Gain::X1, Gain::X2, Gain::X4, Gain::X6, Gain::X8, Gain::X12, Gain::X24];
        for (i, g) in gains.iter().enumerate() {
            assert_eq!(g.code(), i as u8, "wrong code for {:?}", g);
        }
    }

    #[test]
    fn gain_default_is_24x() {
        assert_eq!(Gain::default(), Gain::X24);
    }

    #[test]
    fn gain_from_code_known_values() {
        assert_eq!(gain_from_code(0), 1.0);
        assert_eq!(gain_from_code(6), 24.0);
        assert_eq!(gain_from_code(3), 6.0);
    }

    #[test]
    fn gain_from_code_unknown_returns_1() {
        assert_eq!(gain_from_code(7), 1.0);
        assert_eq!(gain_from_code(255), 1.0);
    }

    #[test]
    fn gain_values_constant_matches_multipliers() {
        let gains = [Gain::X1, Gain::X2, Gain::X4, Gain::X6, Gain::X8, Gain::X12, Gain::X24];
        for (i, g) in gains.iter().enumerate() {
            assert_eq!(GAIN_VALUES[i], g.multiplier(), "GAIN_VALUES[{i}] mismatch");
        }
    }

    // ── ChannelConfig ─────────────────────────────────────────────────────────

    #[test]
    fn channel_config_default_values() {
        let cfg = ChannelConfig::default();
        assert!(cfg.power);
        assert_eq!(cfg.gain, Gain::X24);
        assert_eq!(cfg.input_type, InputType::Normal);
        assert!(cfg.bias);
        assert!(cfg.srb2);
        assert!(!cfg.srb1);
    }

    #[test]
    fn channel_config_builder_chain() {
        let cfg = ChannelConfig::default()
            .gain(Gain::X4)
            .input_type(InputType::Shorted)
            .power(false)
            .bias(false)
            .srb2(false)
            .srb1(true);
        assert!(!cfg.power);
        assert_eq!(cfg.gain, Gain::X4);
        assert_eq!(cfg.input_type, InputType::Shorted);
        assert!(!cfg.bias);
        assert!(!cfg.srb2);
        assert!(cfg.srb1);
    }

    #[test]
    fn channel_config_to_command_format() {
        // Default config: power=on(0), gain=24(6), input=Normal(0), bias=1, srb2=1, srb1=0
        let cmd = ChannelConfig::default().to_command('1');
        assert_eq!(cmd, "x1060110X");
    }

    #[test]
    fn channel_config_to_command_powered_off() {
        let cmd = ChannelConfig::default().power(false).to_command('1');
        // power_down byte = '1' when off
        assert_eq!(&cmd[2..3], "1", "powered-off command should have '1' in position 2");
        assert!(cmd.starts_with("x1"));
        assert!(cmd.ends_with('X'));
    }

    #[test]
    fn channel_config_to_command_gain_codes() {
        let gains_and_codes = [
            (Gain::X1,  '0'),
            (Gain::X2,  '1'),
            (Gain::X4,  '2'),
            (Gain::X6,  '3'),
            (Gain::X8,  '4'),
            (Gain::X12, '5'),
            (Gain::X24, '6'),
        ];
        for (gain, expected_code) in gains_and_codes {
            let cmd = ChannelConfig::default().gain(gain).to_command('1');
            let gain_char = cmd.chars().nth(3).unwrap();
            assert_eq!(gain_char, expected_code, "wrong gain code for {:?}", gain);
        }
    }

    #[test]
    fn channel_config_to_command_daisy_letter() {
        let cmd = ChannelConfig::default().to_command('Q');
        assert!(cmd.starts_with("xQ"), "command should start with 'xQ', got: {cmd}");
        assert!(cmd.ends_with('X'));
        assert_eq!(cmd.len(), 9);
    }

    #[test]
    fn channel_config_command_length_is_always_9() {
        for &letter in &CHANNEL_LETTERS {
            let cmd = ChannelConfig::default().to_command(letter);
            assert_eq!(cmd.len(), 9, "command for '{letter}' should be 9 chars: {cmd}");
        }
    }

    // ── InputType ────────────────────────────────────────────────────────────

    #[test]
    fn input_type_default_is_normal() {
        assert_eq!(InputType::default(), InputType::Normal);
    }

    #[test]
    fn input_type_codes_are_contiguous_from_zero() {
        let types = [
            InputType::Normal, InputType::Shorted, InputType::BiasMeas,
            InputType::Mvdd, InputType::Temp, InputType::TestSig,
            InputType::BiasDrp, InputType::BiasDrn,
        ];
        for (i, t) in types.iter().enumerate() {
            assert_eq!(*t as u8, i as u8, "wrong code for {:?}", t);
        }
    }

    // ── CHANNEL_LETTERS ───────────────────────────────────────────────────────

    #[test]
    fn channel_letters_cyton_are_1_to_8() {
        for (i, &letter) in CHANNEL_LETTERS[0..8].iter().enumerate() {
            let expected = char::from_digit(i as u32 + 1, 10).unwrap();
            assert_eq!(letter, expected, "Cyton channel {i} should map to '{expected}'");
        }
    }

    #[test]
    fn channel_letters_daisy_are_qwertyui() {
        let expected = ['Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I'];
        assert_eq!(&CHANNEL_LETTERS[8..16], &expected, "Daisy letters mismatch");
    }

    #[test]
    fn channel_letters_are_all_distinct() {
        let mut seen = std::collections::HashSet::new();
        for &c in &CHANNEL_LETTERS {
            assert!(seen.insert(c), "duplicate channel letter: '{c}'");
        }
    }

    // ── GainTracker ───────────────────────────────────────────────────────────

    #[test]
    fn gain_tracker_returns_default_gain() {
        let tracker = GainTracker::new(vec![24.0; 8]);
        for i in 0..8 {
            assert_eq!(tracker.gain_for(i), 24.0, "default gain wrong at ch {i}");
        }
    }

    #[test]
    fn gain_tracker_out_of_range_returns_1() {
        let tracker = GainTracker::new(vec![24.0; 8]);
        assert_eq!(tracker.gain_for(8), 1.0);
        assert_eq!(tracker.gain_for(100), 1.0);
    }

    #[test]
    fn gain_tracker_apply_command_updates_gain() {
        let mut tracker = GainTracker::new(vec![24.0; 8]);
        // "x1020110X" → channel '1' (idx 0), gain code 2 (= 4×)
        let cmd = ChannelConfig::default().gain(Gain::X4).to_command('1');
        tracker.apply_command(&cmd);
        assert_eq!(tracker.gain_for(0), 4.0, "gain not updated to X4");
        // other channels untouched
        assert_eq!(tracker.gain_for(1), 24.0);
    }

    #[test]
    fn gain_tracker_apply_command_daisy_channel() {
        let mut tracker = GainTracker::new(vec![24.0; 16]);
        let cmd = ChannelConfig::default().gain(Gain::X8).to_command('Q'); // channel 8
        tracker.apply_command(&cmd);
        assert_eq!(tracker.gain_for(8), 8.0);
        assert_eq!(tracker.gain_for(0), 24.0); // untouched
    }

    #[test]
    fn gain_tracker_reset_command_sets_all_to_24() {
        let mut tracker = GainTracker::new(vec![1.0, 2.0, 4.0, 6.0, 8.0, 12.0, 24.0, 1.0]);
        tracker.apply_command("d");
        for i in 0..8 {
            assert_eq!(tracker.gain_for(i), 24.0, "reset failed for ch {i}");
        }
    }

    #[test]
    fn gain_tracker_gains_slice_matches_individual() {
        let gains_in = vec![1.0, 2.0, 4.0, 6.0, 8.0, 12.0, 24.0, 24.0];
        let tracker = GainTracker::new(gains_in.clone());
        assert_eq!(tracker.gains(), gains_in.as_slice());
    }

    #[test]
    fn gain_tracker_apply_multiple_commands_in_string() {
        // Two x..X commands back-to-back with no separator
        let mut tracker = GainTracker::new(vec![24.0; 8]);
        let cmd1 = ChannelConfig::default().gain(Gain::X1).to_command('1'); // ch0 → 1×
        let cmd2 = ChannelConfig::default().gain(Gain::X2).to_command('2'); // ch1 → 2×
        let combined = format!("{cmd1}{cmd2}");
        tracker.apply_command(&combined);
        assert_eq!(tracker.gain_for(0), 1.0);
        assert_eq!(tracker.gain_for(1), 2.0);
        assert_eq!(tracker.gain_for(2), 24.0); // untouched
    }
}
