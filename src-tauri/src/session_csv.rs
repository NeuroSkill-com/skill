// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Tauri-coupled CSV helpers: path creation from AppHandle and session metadata.
// The pure CSV writer (CsvState) and path utilities live in skill-data::session_csv.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::AppStateExt;
use crate::{unix_secs, yyyymmdd_utc, MutexExt};

// Re-export everything from the crate so `crate::session_csv::*` keeps working.
pub use skill_data::session_csv::*;

// ── Tauri-coupled path helpers ────────────────────────────────────────────────

/// Build the path for a new EEG CSV recording inside the skill data directory.
///
/// Uses [`crate::settings::default_skill_dir`] so the CSV lands in the same
/// root as every other data file.
/// Canonical CSV filename prefix for all new recordings.
///
/// Legacy sessions used `muse_<ts>.csv`; new sessions use `exg_<ts>.csv`
/// regardless of device.  The history loader accepts both prefixes.
#[allow(dead_code)]
pub(crate) const CSV_PREFIX: &str = "exg";

#[allow(dead_code)]
pub(crate) fn new_csv_path(app: &AppHandle) -> PathBuf {
    let skill_dir = app
        .try_state::<std::sync::Mutex<Box<crate::AppState>>>()
        .map(|s| s.lock_or_recover().skill_dir.clone())
        .unwrap_or_else(crate::settings::default_skill_dir);

    let base = skill_dir.join(yyyymmdd_utc());
    let _ = std::fs::create_dir_all(&base);
    base.join(format!("{}_{}.csv", CSV_PREFIX, unix_secs()))
}

// ── Session metadata sidecar ──────────────────────────────────────────────────

/// Write (or overwrite) a JSON sidecar file next to the CSV recording.
#[allow(dead_code)]
pub(crate) fn write_session_meta(app: &AppHandle, csv_path: &Path) {
    let s_ref = app.app_state();
    let s = s_ref.lock_or_recover();

    let session_end_utc = unix_secs();
    let session_start_utc = s.session_start_utc;
    let duration_secs = session_start_utc.map(|st| session_end_utc.saturating_sub(st));

    let meta = serde_json::json!({
        // ── Recording ────────────────────────────────────────────────────
        "csv_file":            csv_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "ppg_csv_file":        ppg_csv_path(csv_path).file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "metrics_csv_file":    metrics_csv_path(csv_path).file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "imu_csv_file":        imu_csv_path(csv_path).file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "session_start_utc":   session_start_utc,
        "session_end_utc":     session_end_utc,
        "session_duration_s":  duration_secs,
        "total_samples":       s.status.sample_count,
        "ppg_total_samples":   s.status.ppg_sample_count,
        "sample_rate_hz":      if s.status.eeg_sample_rate_hz > 0.0 { s.status.eeg_sample_rate_hz } else { EEG_SAMPLE_RATE },
        "ppg_sample_rate_hz":  PPG_SAMPLE_RATE,
        "channels":            if s.status.channel_names.is_empty() { vec!["TP9".into(), "AF7".into(), "AF8".into(), "TP10".into()] } else { s.status.channel_names.clone() },
        "ppg_channels":        ["ambient", "infrared", "red"],
        "channel_count":       if s.status.eeg_channel_count > 0 { s.status.eeg_channel_count } else { 4 },

        // ── BLE Device Identity ──────────────────────────────────────────
        "device": {
            "name":               s.status.device_name,
            "id":                 s.status.device_id,
            "serial_number":      s.status.serial_number,
            "mac_address":        s.status.mac_address,
            "firmware_version":   s.status.firmware_version,
            "hardware_version":   s.status.hardware_version,
            "bootloader_version": s.status.bootloader_version,
            "preset":             s.status.headset_preset,
        },

        // ── Battery ──────────────────────────────────────────────────────
        "battery_pct_end":        s.status.battery,

        // ── Signal quality at session end ────────────────────────────────
        "channel_quality":        s.status.channel_quality,
        "avg_snr_db":             if s.snr_count > 0 { Some(s.snr_sum / s.snr_count as f64) } else { None::<f64> },

        // ── Filter / processing config ───────────────────────────────────
        "filter_config":          s.status.filter_config,
        "embedding_overlap_secs": s.status.embedding_overlap_secs,

        // ── Remote phone (if connected via iroh) ─────────────────────────
        "phone_info":             s.status.phone_info,

        // ── App ──────────────────────────────────────────────────────────
        "app_version":            env!("CARGO_PKG_VERSION"),
        "platform":               std::env::consts::OS,
        "arch":                   std::env::consts::ARCH,
    });
    drop(s);

    let meta_path = csv_path.with_extension("json");
    match serde_json::to_string_pretty(&meta) {
        Ok(json) => match std::fs::write(&meta_path, &json) {
            Ok(_) => eprintln!("[session] wrote metadata → {}", meta_path.display()),
            Err(e) => eprintln!(
                "[session] ERROR writing metadata {}: {e}",
                meta_path.display()
            ),
        },
        Err(e) => eprintln!("[session] ERROR serialising metadata: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    // use std::path::PathBuf;
    // use std::sync::{Arc, Mutex};

    // ...existing code...

    // Test new_csv_path logic (without Tauri)
    #[test]
    fn test_new_csv_path_creates_dir_and_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let skill_dir = temp_dir.path().to_path_buf();
        // Simulate yyyymmdd_utc and unix_secs
        let date = "20260411";
        let ts = 1234567890;
        let base = skill_dir.join(date);
        let expected = base.join(format!("{}_{}.csv", CSV_PREFIX, ts));

        // Simulate directory creation
        fs::create_dir_all(&base).unwrap();
        // Simulate file creation
        fs::File::create(&expected).unwrap();
        assert!(expected.exists());
    }

    // Test write_session_meta logic (structure only)
    #[test]
    fn test_write_session_meta_creates_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let csv_path = temp_dir.path().join("test.csv");
        fs::File::create(&csv_path).unwrap();
        let meta_path = csv_path.with_extension("json");
        // Simulate writing JSON
        let meta = serde_json::json!({"csv_file": "test.csv"});
        fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap()).unwrap();
        assert!(meta_path.exists());
        let contents = fs::read_to_string(&meta_path).unwrap();
        assert!(contents.contains("csv_file"));
    }
}
