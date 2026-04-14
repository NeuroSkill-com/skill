use super::*;

// ── OpenBciBoard::channel_count ───────────────────────────────────────────

#[test]
fn ganglion_has_4_channels() {
    assert_eq!(OpenBciBoard::Ganglion.channel_count(), 4);
    assert_eq!(OpenBciBoard::GanglionWifi.channel_count(), 4);
}

#[test]
fn cyton_has_8_channels() {
    assert_eq!(OpenBciBoard::Cyton.channel_count(), 8);
    assert_eq!(OpenBciBoard::CytonWifi.channel_count(), 8);
}

#[test]
fn cyton_daisy_has_16_channels() {
    assert_eq!(OpenBciBoard::CytonDaisy.channel_count(), 16);
    assert_eq!(OpenBciBoard::CytonDaisyWifi.channel_count(), 16);
}

#[test]
fn galea_has_24_channels() {
    assert_eq!(OpenBciBoard::Galea.channel_count(), 24);
}

// ── OpenBciBoard::sample_rate ─────────────────────────────────────────────

#[test]
fn ganglion_sample_rate_is_200() {
    assert!((OpenBciBoard::Ganglion.sample_rate() - 200.0).abs() < 1e-6);
    assert!((OpenBciBoard::GanglionWifi.sample_rate() - 200.0).abs() < 1e-6);
}

#[test]
fn cyton_sample_rate_is_250() {
    assert!((OpenBciBoard::Cyton.sample_rate() - 250.0).abs() < 1e-6);
    assert!((OpenBciBoard::CytonDaisy.sample_rate() - 250.0).abs() < 1e-6);
    assert!((OpenBciBoard::Galea.sample_rate() - 250.0).abs() < 1e-6);
}

#[test]
fn cyton_wifi_sample_rate_is_1000() {
    assert!((OpenBciBoard::CytonWifi.sample_rate() - 1000.0).abs() < 1e-6);
}

#[test]
fn cyton_daisy_wifi_sample_rate_is_125() {
    assert!((OpenBciBoard::CytonDaisyWifi.sample_rate() - 125.0).abs() < 1e-6);
}

// ── OpenBciBoard connection predicates ────────────────────────────────────

#[test]
fn ganglion_is_ble_only() {
    assert!(OpenBciBoard::Ganglion.is_ble());
    assert!(!OpenBciBoard::GanglionWifi.is_ble());
    assert!(!OpenBciBoard::Cyton.is_ble());
    assert!(!OpenBciBoard::Galea.is_ble());
}

#[test]
fn serial_boards_are_cyton_and_cyton_daisy() {
    assert!(OpenBciBoard::Cyton.is_serial());
    assert!(OpenBciBoard::CytonDaisy.is_serial());
    assert!(!OpenBciBoard::Ganglion.is_serial());
    assert!(!OpenBciBoard::CytonWifi.is_serial());
    assert!(!OpenBciBoard::CytonDaisyWifi.is_serial());
    assert!(!OpenBciBoard::Galea.is_serial());
}

#[test]
fn wifi_boards_are_wifi_variants() {
    assert!(OpenBciBoard::GanglionWifi.is_wifi());
    assert!(OpenBciBoard::CytonWifi.is_wifi());
    assert!(OpenBciBoard::CytonDaisyWifi.is_wifi());
    assert!(!OpenBciBoard::Ganglion.is_wifi());
    assert!(!OpenBciBoard::Cyton.is_wifi());
    assert!(!OpenBciBoard::Galea.is_wifi());
}

#[test]
fn exactly_one_connection_type_per_board() {
    for board in [
        OpenBciBoard::Ganglion,
        OpenBciBoard::GanglionWifi,
        OpenBciBoard::Cyton,
        OpenBciBoard::CytonWifi,
        OpenBciBoard::CytonDaisy,
        OpenBciBoard::CytonDaisyWifi,
        OpenBciBoard::Galea,
    ] {
        let kinds = [board.is_ble(), board.is_serial(), board.is_wifi()]
            .iter()
            .filter(|&&b| b)
            .count();
        assert!(kinds <= 1, "{board:?} reports more than one connection type");
    }
}

#[test]
fn default_board_is_ganglion() {
    assert_eq!(OpenBciBoard::default(), OpenBciBoard::Ganglion);
}

// ── CalibrationProfile defaults ───────────────────────────────────────────

#[test]
fn default_calibration_profile_has_two_actions() {
    let p = CalibrationProfile::default();
    assert_eq!(p.actions.len(), 2);
}

#[test]
fn default_calibration_profile_action_labels_match_constants() {
    let p = CalibrationProfile::default();
    assert_eq!(p.actions[0].label, skill_constants::CALIBRATION_ACTION1_LABEL);
    assert_eq!(p.actions[1].label, skill_constants::CALIBRATION_ACTION2_LABEL);
}

#[test]
fn default_calibration_profile_durations_match_constants() {
    let p = CalibrationProfile::default();
    assert_eq!(
        p.actions[0].duration_secs,
        skill_constants::CALIBRATION_ACTION_DURATION_SECS
    );
    assert_eq!(
        p.actions[1].duration_secs,
        skill_constants::CALIBRATION_ACTION_DURATION_SECS
    );
    assert_eq!(p.break_duration_secs, skill_constants::CALIBRATION_BREAK_DURATION_SECS);
    assert_eq!(p.loop_count, skill_constants::CALIBRATION_LOOP_COUNT);
    assert_eq!(p.auto_start, skill_constants::CALIBRATION_AUTO_START);
}

#[test]
fn default_calibration_profile_id_is_default() {
    assert_eq!(CalibrationProfile::default().id, "default");
}

// ── UmapUserConfig defaults ───────────────────────────────────────────────

#[test]
fn default_umap_config_n_neighbors_is_15() {
    assert_eq!(UmapUserConfig::default().n_neighbors, 15);
}

#[test]
fn default_umap_config_n_epochs_is_500() {
    assert_eq!(UmapUserConfig::default().n_epochs, 500);
}

#[test]
fn default_umap_config_timeout_is_120s() {
    assert_eq!(UmapUserConfig::default().timeout_secs, 120);
}

// ── tilde_path ────────────────────────────────────────────────────────────

#[test]
fn tilde_path_contracts_home() {
    if let Ok(home) = std::env::var("HOME") {
        let p = std::path::Path::new(&home).join(".skill").join("settings.json");
        let result = tilde_path(&p);
        assert!(result.starts_with("~/"), "expected '~/...' got '{result}'");
    }
}

#[test]
fn tilde_path_leaves_non_home_path_unchanged() {
    let p = std::path::Path::new("/tmp/some/path.json");
    assert_eq!(tilde_path(p), "/tmp/some/path.json");
}

// ── OpenBciConfig defaults ────────────────────────────────────────────────

#[test]
fn default_openbci_config_scan_timeout_is_10() {
    assert_eq!(OpenBciConfig::default().scan_timeout_secs, 10);
}

#[test]
fn default_openbci_config_wifi_port_is_3000() {
    assert_eq!(OpenBciConfig::default().wifi_local_port, 3000);
}

#[test]
fn default_openbci_config_has_empty_serial_port() {
    assert!(OpenBciConfig::default().serial_port.is_empty());
}

// ── new_profile_id ────────────────────────────────────────────────────────

#[test]
fn new_profile_id_starts_with_cal_prefix() {
    let id = new_profile_id();
    assert!(id.starts_with("cal_"), "expected 'cal_...', got '{id}'");
}

#[test]
fn new_profile_id_is_unique_across_calls() {
    let a = new_profile_id();
    let b = new_profile_id();
    assert!(a.starts_with("cal_"));
    assert!(b.starts_with("cal_"));
    assert!(!a.is_empty());
    assert!(!b.is_empty());
}

// ── parse_hhmm ──────────────────────────────────────────────────────────

#[test]
fn parse_hhmm_valid() {
    assert_eq!(parse_hhmm("08:30"), (8, 30));
    assert_eq!(parse_hhmm("23:59"), (23, 59));
    assert_eq!(parse_hhmm("00:00"), (0, 0));
}

#[test]
fn parse_hhmm_clamps_overflow() {
    assert_eq!(parse_hhmm("25:70"), (23, 59));
    assert_eq!(parse_hhmm("99:99"), (23, 59));
}

#[test]
fn parse_hhmm_bad_input() {
    assert_eq!(parse_hhmm(""), (0, 0));
    assert_eq!(parse_hhmm("garbage"), (0, 0));
    assert_eq!(parse_hhmm(":"), (0, 0));
}

// ── SleepConfig::duration_minutes ───────────────────────────────────────

#[test]
fn sleep_duration_normal() {
    let cfg = SleepConfig {
        bedtime: "23:00".into(),
        wake_time: "07:00".into(),
        preset: SleepPreset::Default,
    };
    assert_eq!(cfg.duration_minutes(), 480); // 8 hours
}

#[test]
fn sleep_duration_same_day() {
    let cfg = SleepConfig {
        bedtime: "01:00".into(),
        wake_time: "09:00".into(),
        preset: SleepPreset::Default,
    };
    assert_eq!(cfg.duration_minutes(), 480); // 8 hours
}

#[test]
fn sleep_duration_overnight() {
    let cfg = SleepConfig {
        bedtime: "22:00".into(),
        wake_time: "06:00".into(),
        preset: SleepPreset::Default,
    };
    assert_eq!(cfg.duration_minutes(), 480); // 8 hours
}

#[test]
fn sleep_duration_equal_times() {
    let cfg = SleepConfig {
        bedtime: "08:00".into(),
        wake_time: "08:00".into(),
        preset: SleepPreset::Default,
    };
    assert_eq!(cfg.duration_minutes(), 0);
}

// ── UserSettings serde ──────────────────────────────────────────────────

#[test]
fn user_settings_default_serde_roundtrip() {
    let s = UserSettings::default();
    let json = serde_json::to_string(&s).unwrap();
    let back: UserSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(back.ws_host, s.ws_host);
    assert_eq!(back.ws_port, s.ws_port);
    assert_eq!(back.theme, s.theme);
}

#[test]
fn user_settings_from_empty_json() {
    let s: UserSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(s.ws_port, default_ws_port());
    assert_eq!(s.theme, default_theme());
    assert_eq!(s.accent_color, default_accent_color());
    assert_eq!(s.daily_goal_min, default_daily_goal_min());
}

#[test]
fn umap_user_config_default_roundtrip() {
    let cfg = UmapUserConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let back: UmapUserConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.n_epochs, 500);
    assert_eq!(back.n_neighbors, 15);
    assert_eq!(back.timeout_secs, 120);
}

#[test]
fn settings_path_builds_correctly() {
    let p = settings_path(std::path::Path::new("/home/user/.skill"));
    assert!(p.to_str().unwrap().contains("settings"));
}

// ── default_* functions ─────────────────────────────────────────────────

#[test]
fn default_values_are_sensible() {
    assert!(!default_ws_host().is_empty());
    assert!(default_ws_port() > 0);
    assert!(!default_theme().is_empty());
    assert!(!default_accent_color().is_empty());
    assert!(default_daily_goal_min() > 0);
    assert!(!default_embedding_model().is_empty());
    assert!(default_overlap_secs() >= 0.0);
    assert!(default_update_check_interval() > 0);
    assert!(!default_hf_endpoint().is_empty());
}
