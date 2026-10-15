#[cfg(test)]
mod tests {
    use crate::electrode::{
        position, position_1010, position_1020, resolve_alias,
        Electrode, ElectrodeLayout, ElectrodePosition, SignalType,
        MONTAGE_1005, MONTAGE_1010, MONTAGE_1020,
        cyton_motor, cyton_daisy_standard, ganglion_default,
        positions,
    };

    // ── Montage data integrity ────────────────────────────────────────────────

    #[test]
    fn montage_1020_has_correct_count() {
        assert_eq!(MONTAGE_1020.len(), 83);
    }

    #[test]
    fn montage_1010_has_correct_count() {
        assert_eq!(MONTAGE_1010.len(), 176);
    }

    #[test]
    fn montage_1005_has_correct_count() {
        assert_eq!(MONTAGE_1005.len(), 334);
    }

    #[test]
    fn montage_1020_is_subset_of_1010() {
        let labels_1010: std::collections::HashSet<&str> =
            MONTAGE_1010.iter().map(|e| e.label).collect();
        for e in MONTAGE_1020 {
            assert!(
                labels_1010.contains(e.label),
                "10-20 label '{}' missing from 10-10",
                e.label
            );
        }
    }

    #[test]
    fn montage_1010_is_subset_of_1005() {
        let labels_1005: std::collections::HashSet<&str> =
            MONTAGE_1005.iter().map(|e| e.label).collect();
        for e in MONTAGE_1010 {
            assert!(
                labels_1005.contains(e.label),
                "10-10 label '{}' missing from 10-05",
                e.label
            );
        }
    }

    #[test]
    fn montage_1005_has_no_duplicate_labels() {
        let mut seen = std::collections::HashSet::new();
        for e in MONTAGE_1005 {
            assert!(seen.insert(e.label), "duplicate label '{}' in MONTAGE_1005", e.label);
        }
    }

    #[test]
    fn montage_1020_has_no_duplicate_labels() {
        let mut seen = std::collections::HashSet::new();
        for e in MONTAGE_1020 {
            assert!(seen.insert(e.label), "duplicate label '{}' in MONTAGE_1020", e.label);
        }
    }

    #[test]
    fn all_montage_positions_are_on_head_surface() {
        // All electrode radii should be within realistic head-surface range.
        // Min ~80 mm (temporal/peripheral positions like FFC5),
        // Max ~122 mm (inion-area positions like I1, I2).
        for e in MONTAGE_1005 {
            let r = e.radius();
            assert!(
                r > 0.070 && r < 0.130,
                "electrode '{}' has unexpected radius {:.4} m",
                e.label, r
            );
        }
    }

    #[test]
    fn midline_electrodes_have_near_zero_x() {
        let midline = ["Fz", "Cz", "Pz", "Oz", "FCz", "CPz", "AFz", "POz", "Fpz"];
        for label in midline {
            let pos = position(label).unwrap_or_else(|| panic!("'{}' not found", label));
            assert!(
                pos.x.abs() < 0.002,
                "midline electrode '{}' x={:.4} is not near zero",
                label, pos.x
            );
        }
    }

    #[test]
    fn left_right_symmetry_of_paired_electrodes() {
        // Left electrodes have negative x, right have positive x
        let pairs = [("C3", "C4"), ("F3", "F4"), ("P3", "P4"), ("T7", "T8"), ("O1", "O2")];
        for (left, right) in pairs {
            let lp = position(left).unwrap_or_else(|| panic!("'{}' not found", left));
            let rp = position(right).unwrap_or_else(|| panic!("'{}' not found", right));
            assert!(lp.x < 0.0, "left electrode '{}' should have negative x, got {}", left, lp.x);
            assert!(rp.x > 0.0, "right electrode '{}' should have positive x, got {}", right, rp.x);
            // Symmetric magnitudes (within 5 mm)
            assert!(
                (lp.x.abs() - rp.x.abs()).abs() < 0.005,
                "asymmetric pair {}/{}: |x_L|={:.4}, |x_R|={:.4}",
                left, right, lp.x.abs(), rp.x.abs()
            );
        }
    }

    // Cz should be at the top of the head (highest z, near-zero x/y)
    #[test]
    fn cz_is_at_crown() {
        let cz = position("Cz").expect("Cz should be in 10-05");
        assert!(cz.z > 0.095, "Cz z={:.4} should be ~0.100 m (top of head)", cz.z);
        assert!(cz.x.abs() < 0.002, "Cz should be on midline");
    }

    // Fp electrodes are in the front
    #[test]
    fn fp_electrodes_are_frontal() {
        let fp1 = position("Fp1").expect("Fp1 must exist");
        let fp2 = position("Fp2").expect("Fp2 must exist");
        assert!(fp1.y > 0.07, "Fp1 y={:.4} should be anterior", fp1.y);
        assert!(fp2.y > 0.07, "Fp2 y={:.4} should be anterior", fp2.y);
    }

    // O electrodes are at the back
    #[test]
    fn o_electrodes_are_occipital() {
        let o1 = position("O1").expect("O1 must exist");
        assert!(o1.y < -0.09, "O1 y={:.4} should be posterior", o1.y);
    }

    // ── position() lookup ────────────────────────────────────────────────────

    #[test]
    fn position_lookup_known_labels() {
        let known = ["Fp1", "Fp2", "F3", "Fz", "F4", "C3", "Cz", "C4", "P3", "Pz", "P4",
                     "O1", "Oz", "O2", "T7", "T8", "F7", "F8"];
        for label in known {
            assert!(position(label).is_some(), "known label '{}' not found", label);
        }
    }

    #[test]
    fn position_lookup_unknown_label_returns_none() {
        assert!(position("ZZZ999").is_none());
        assert!(position("").is_none());
        assert!(position("Electrode42").is_none());
    }

    #[test]
    fn position_lookup_is_case_insensitive() {
        let p1 = position("Cz").expect("Cz must exist");
        let p2 = position("cz").expect("cz must exist (case-insensitive)");
        let p3 = position("CZ").expect("CZ must exist (case-insensitive)");
        assert_eq!(p1.x, p2.x);
        assert_eq!(p1.x, p3.x);
    }

    #[test]
    fn position_1020_returns_none_for_1010_only_label() {
        // "AFF1" is in 10-10 but not in 10-20
        assert!(
            position_1020("AFF1").is_none(),
            "AFF1 should not be in 10-20"
        );
        assert!(
            position_1010("AFF1").is_some(),
            "AFF1 should be in 10-10"
        );
    }

    #[test]
    fn position_1010_returns_none_for_1005_only_label() {
        // Labels ending in 'h' (e.g. "FCC3h") are only in 10-05
        assert!(
            position_1010("FCC3h").is_none(),
            "FCC3h should not be in 10-10"
        );
        assert!(
            position("FCC3h").is_some(),
            "FCC3h should be in 10-05"
        );
    }

    // ── resolve_alias ────────────────────────────────────────────────────────

    #[test]
    fn resolve_alias_t3_to_t7() {
        assert_eq!(resolve_alias("T3"), "T7");
    }

    #[test]
    fn resolve_alias_t4_to_t8() {
        assert_eq!(resolve_alias("T4"), "T8");
    }

    #[test]
    fn resolve_alias_t5_to_p7() {
        assert_eq!(resolve_alias("T5"), "P7");
    }

    #[test]
    fn resolve_alias_t6_to_p8() {
        assert_eq!(resolve_alias("T6"), "P8");
    }

    #[test]
    fn resolve_alias_m1_m2() {
        assert_eq!(resolve_alias("M1"), "TP9");
        assert_eq!(resolve_alias("M2"), "TP10");
    }

    #[test]
    fn resolve_alias_a1_a2() {
        assert_eq!(resolve_alias("A1"), "TP9");
        assert_eq!(resolve_alias("A2"), "TP10");
    }

    #[test]
    fn resolve_alias_non_alias_is_unchanged() {
        assert_eq!(resolve_alias("Cz"), "Cz");
        assert_eq!(resolve_alias("Fp1"), "Fp1");
        assert_eq!(resolve_alias("FCC3h"), "FCC3h");
    }

    #[test]
    fn alias_position_lookup_resolves_automatically() {
        // T3 is an old name for T7; they should resolve to the same position
        let pos_t3 = position("T3").expect("T3 alias should resolve");
        let pos_t7 = position("T7").expect("T7 must exist");
        assert_eq!(pos_t3.x, pos_t7.x);
        assert_eq!(pos_t3.y, pos_t7.y);
        assert_eq!(pos_t3.z, pos_t7.z);
    }

    // ── positions constants module ────────────────────────────────────────────

    #[test]
    fn positions_cz_constant_is_correct_string() {
        assert_eq!(positions::CZ, "Cz");
    }

    #[test]
    fn positions_fp1_fp2_are_correct() {
        assert_eq!(positions::FP1, "Fp1");
        assert_eq!(positions::FP2, "Fp2");
    }

    #[test]
    fn positions_t7_t8_are_preferred_names() {
        // The canonical name in 10-05 is T7/T8, not T3/T4
        assert_eq!(positions::T7, "T7");
        assert_eq!(positions::T8, "T8");
    }

    #[test]
    fn positions_legacy_aliases_point_to_canonical() {
        // Old names should resolve to the new canonical label
        assert_eq!(positions::T3, "T7");
        assert_eq!(positions::T4, "T8");
        assert_eq!(positions::T5, "P7");
        assert_eq!(positions::T6, "P8");
    }

    #[test]
    fn positions_h_suffix_labels_exist() {
        // 10-05 half-step positions
        let labels = [positions::FCC3H, positions::CCP3H, positions::FFC3H];
        for label in labels {
            assert!(
                position(label).is_some(),
                "10-05 position '{}' should exist",
                label
            );
        }
    }

    // ── ElectrodePosition ────────────────────────────────────────────────────

    #[test]
    fn electrode_position_radius() {
        let ep = ElectrodePosition { label: "test", x: 0.03, y: 0.04, z: 0.0 };
        assert!((ep.radius() - 0.05).abs() < 1e-10, "expected 0.05, got {}", ep.radius());
    }

    #[test]
    fn electrode_position_copy_semantics() {
        let ep = ElectrodePosition { label: "Cz", x: 0.0, y: -0.01, z: 0.1 };
        let ep2 = ep; // copy, not move
        assert_eq!(ep.label, ep2.label);
        assert_eq!(ep.z, ep2.z);
    }

    // ── Electrode ────────────────────────────────────────────────────────────

    #[test]
    fn electrode_eeg_constructor() {
        let e = Electrode::eeg("Cz");
        assert_eq!(e.label, "Cz");
        assert_eq!(e.signal_type, SignalType::Eeg);
        assert!(e.note.is_none());
    }

    #[test]
    fn electrode_emg_constructor() {
        let e = Electrode::emg("Left Bicep");
        assert_eq!(e.signal_type, SignalType::Emg);
    }

    #[test]
    fn electrode_with_note() {
        let e = Electrode::eeg("Cz").with_note("motor cortex");
        assert_eq!(e.note.as_deref(), Some("motor cortex"));
    }

    #[test]
    fn electrode_position_method_resolves_standard_label() {
        let e = Electrode::eeg("Cz");
        let pos = e.position().expect("Cz should have a position");
        assert!(pos.z > 0.09);
    }

    #[test]
    fn electrode_position_method_returns_none_for_custom_label() {
        let e = Electrode::eeg("MyCustomElectrode");
        assert!(e.position().is_none());
    }

    #[test]
    fn electrode_display_shows_label() {
        let e = Electrode::eeg("T7");
        assert_eq!(format!("{e}"), "T7");
    }

    // ── SignalType ────────────────────────────────────────────────────────────

    #[test]
    fn signal_type_display() {
        assert_eq!(format!("{}", SignalType::Eeg), "EEG");
        assert_eq!(format!("{}", SignalType::Emg), "EMG");
        assert_eq!(format!("{}", SignalType::Eog), "EOG");
        assert_eq!(format!("{}", SignalType::Ecg), "ECG");
        assert_eq!(format!("{}", SignalType::Reference), "REF");
        assert_eq!(format!("{}", SignalType::Other("custom".to_string())), "custom");
    }

    // ── ElectrodeLayout ───────────────────────────────────────────────────────

    #[test]
    fn layout_new_has_correct_length() {
        let layout = ElectrodeLayout::new(8);
        assert_eq!(layout.len(), 8);
        assert!(!layout.is_empty());
    }

    #[test]
    fn layout_empty() {
        let layout = ElectrodeLayout::new(0);
        assert!(layout.is_empty());
        assert_eq!(layout.len(), 0);
    }

    #[test]
    fn layout_default_is_empty() {
        let layout = ElectrodeLayout::default();
        assert!(layout.is_empty());
    }

    #[test]
    fn layout_unassigned_channel_label_is_ch_n() {
        let layout = ElectrodeLayout::new(4);
        assert_eq!(layout.label(0), "Ch1");
        assert_eq!(layout.label(3), "Ch4");
    }

    #[test]
    fn layout_from_labels_assigns_correctly() {
        let layout = ElectrodeLayout::from_labels(&["Fp1", "Fp2", "Cz"]);
        assert_eq!(layout.label(0), "Fp1");
        assert_eq!(layout.label(1), "Fp2");
        assert_eq!(layout.label(2), "Cz");
        assert_eq!(layout.len(), 3);
    }

    #[test]
    fn layout_from_labels_resolves_aliases() {
        let layout = ElectradeLayout_from_aliases();
        // T3 → T7 alias should be resolved
        assert_eq!(layout.label(0), "T7", "T3 alias should resolve to T7");
        assert_eq!(layout.label(1), "T8", "T4 alias should resolve to T8");
    }

    fn ElectradeLayout_from_aliases() -> ElectrodeLayout {
        ElectrodeLayout::from_labels(&["T3", "T4"])
    }

    #[test]
    fn layout_get_returns_electrode_for_assigned_channel() {
        let layout = ElectrodeLayout::from_labels(&["Cz"]);
        let e = layout.get(0).expect("channel 0 should be assigned");
        assert_eq!(e.label, "Cz");
    }

    #[test]
    fn layout_get_returns_none_for_unassigned_channel() {
        let layout = ElectrodeLayout::new(4);
        assert!(layout.get(0).is_none());
        assert!(layout.get(3).is_none());
    }

    #[test]
    fn layout_get_returns_none_for_out_of_range() {
        let layout = ElectrodeLayout::from_labels(&["Cz"]);
        assert!(layout.get(1).is_none());
        assert!(layout.get(99).is_none());
    }

    #[test]
    fn layout_with_electrode_builder() {
        let layout = ElectrodeLayout::new(2)
            .with_electrode(0, Electrode::eeg("Fp1"))
            .with_electrode(1, Electrode::emg("Deltoid"));
        assert_eq!(layout.label(0), "Fp1");
        assert_eq!(layout.label(1), "Deltoid");
    }

    #[test]
    fn layout_set_electrode_mutates_in_place() {
        let mut layout = ElectrodeLayout::new(2);
        layout.set_electrode(0, Electrode::eeg("C3"));
        assert_eq!(layout.label(0), "C3");
        assert_eq!(layout.label(1), "Ch2"); // still unassigned
    }

    #[test]
    fn layout_set_electrode_out_of_range_is_no_op() {
        let mut layout = ElectrodeLayout::new(2);
        layout.set_electrode(5, Electrode::eeg("C3")); // out of range, should not panic
        assert_eq!(layout.len(), 2);
    }

    #[test]
    fn layout_iter_yields_only_assigned() {
        let layout = ElectrodeLayout::new(4)
            .with_electrode(1, Electrode::eeg("C3"))
            .with_electrode(3, Electrode::eeg("C4"));
        let items: Vec<(usize, &Electrode)> = layout.iter().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, 1);
        assert_eq!(items[1].0, 3);
    }

    #[test]
    fn layout_iter_labels_yields_all_channels() {
        let layout = ElectrodeLayout::new(3)
            .with_electrode(0, Electrode::eeg("Fp1"));
        let labels: Vec<(usize, String)> = layout.iter_labels().collect();
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0], (0, "Fp1".to_string()));
        assert_eq!(labels[1], (1, "Ch2".to_string()));
        assert_eq!(labels[2], (2, "Ch3".to_string()));
    }

    #[test]
    fn layout_labels_returns_vec_of_all_labels() {
        let layout = ElectrodeLayout::from_labels(&["Fp1", "Fp2"]);
        let labels = layout.labels();
        assert_eq!(labels, vec!["Fp1".to_string(), "Fp2".to_string()]);
    }

    #[test]
    fn layout_labels_unassigned_channels_get_default_names() {
        let layout = ElectrodeLayout::new(3);
        assert_eq!(layout.labels(), vec!["Ch1", "Ch2", "Ch3"]);
    }

    #[test]
    fn layout_position_returns_known_position() {
        let layout = ElectrodeLayout::from_labels(&["Cz"]);
        let pos = layout.position(0).expect("Cz should have a position");
        assert!(pos.z > 0.09);
    }

    #[test]
    fn layout_position_returns_none_for_unassigned() {
        let layout = ElectrodeLayout::new(2);
        assert!(layout.position(0).is_none());
    }

    #[test]
    fn layout_position_returns_none_for_custom_label() {
        let layout = ElectrodeLayout::from_labels(&["MySensor"]);
        assert!(layout.position(0).is_none());
    }

    #[test]
    fn layout_subset_1020_filters_correctly() {
        // Mix of 10-20, 10-10-only, and custom electrodes.
        // "AFF1" is in 10-10 but NOT in 10-20.
        let layout = ElectrodeLayout::from_labels(&[
            "Cz",        // in 10-20
            "AFF1",      // in 10-10, not 10-20
            "MySensor",  // custom
            "Fp1",       // in 10-20
        ]);
        let subset = layout.subset_1020();
        let labels: Vec<&str> = subset.iter().map(|(_, e)| e.label.as_str()).collect();
        assert!(labels.contains(&"Cz"),    "Cz should be in 10-20 subset");
        assert!(labels.contains(&"Fp1"),   "Fp1 should be in 10-20 subset");
        assert!(!labels.contains(&"AFF1"), "AFF1 should NOT be in 10-20 subset");
        assert!(!labels.contains(&"MySensor"), "custom label should not be in subset");
    }

    #[test]
    fn layout_subset_1010_includes_aff1() {
        let layout = ElectrodeLayout::from_labels(&["Cz", "AFF1", "MySensor"]);
        let subset = layout.subset_1010();
        let labels: Vec<&str> = subset.iter().map(|(_, e)| e.label.as_str()).collect();
        assert!(labels.contains(&"Cz"),    "Cz should be in 10-10 subset");
        assert!(labels.contains(&"AFF1"),  "AFF1 should be in 10-10 subset");
        assert!(!labels.contains(&"MySensor"), "custom label should not be in subset");
    }

    // ── Pre-built layouts ─────────────────────────────────────────────────────

    #[test]
    fn cyton_motor_layout_has_8_channels() {
        let layout = cyton_motor();
        assert_eq!(layout.len(), 8);
    }

    #[test]
    fn cyton_motor_all_positions_known() {
        let layout = cyton_motor();
        for (i, e) in layout.iter() {
            assert!(
                e.position().is_some(),
                "cyton_motor() channel {i} label '{}' has no known position",
                e.label
            );
        }
    }

    #[test]
    fn cyton_daisy_standard_layout_has_16_channels() {
        let layout = cyton_daisy_standard();
        assert_eq!(layout.len(), 16);
    }

    #[test]
    fn cyton_daisy_all_labels_assigned() {
        let layout = cyton_daisy_standard();
        // All 16 channels should be assigned (no "Ch{n}")
        for i in 0..16 {
            let label = layout.label(i);
            assert!(
                !label.starts_with("Ch"),
                "cyton_daisy_standard() channel {i} is unassigned (label: {label})"
            );
        }
    }

    #[test]
    fn ganglion_default_layout_has_4_channels() {
        let layout = ganglion_default();
        assert_eq!(layout.len(), 4);
    }

    #[test]
    fn ganglion_default_all_positions_known() {
        let layout = ganglion_default();
        for (i, e) in layout.iter() {
            assert!(
                e.position().is_some(),
                "ganglion_default() channel {i} label '{}' has no known position",
                e.label
            );
        }
    }
}
