// SPDX-License-Identifier: GPL-3.0-only
//! Shared utilities for session runners and the embed pipeline.
//!
//! Extracted here to eliminate the duplication that existed across
//! `session_runner.rs`, `session/runner.rs`, and `embed/worker.rs`.

use std::path::{Path, PathBuf};

use skill_daemon_common::EventEnvelope;
use tokio::sync::broadcast;

// ── Time helpers ──────────────────────────────────────────────────────────────

pub fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn unix_secs_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── Date directory ────────────────────────────────────────────────────────────

/// Return (and create) `skill_dir/YYYYMMDD/` for today (UTC).
pub fn utc_date_dir(skill_dir: &Path) -> PathBuf {
    let secs = unix_secs();
    let days = secs / 86400;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let dir = skill_dir.join(format!("{y:04}{m:02}{d:02}"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

// ── Event broadcasting ────────────────────────────────────────────────────────

pub fn broadcast_event(tx: &broadcast::Sender<EventEnvelope>, event_type: &str, payload: &serde_json::Value) {
    let _ = tx.send(EventEnvelope {
        r#type: event_type.to_string(),
        ts_unix_ms: unix_ms(),
        correlation_id: None,
        payload: payload.clone(),
    });
}

// ── Band snapshot enrichment ──────────────────────────────────────────────────

/// Enrich a `BandSnapshot` with composite scores (focus, relaxation, engagement,
/// artifacts) and return the result as JSON.
pub fn enrich_band_snapshot(
    snap: &mut skill_eeg::eeg_bands::BandSnapshot,
    artifacts: Option<&skill_eeg::artifact_detection::ArtifactMetrics>,
) -> serde_json::Value {
    // Use skill_devices::enrich_band_snapshot for the full enrichment
    // (blink_count, blink_rate, head_pose, composite scores).
    let ctx = skill_devices::SnapshotContext {
        ppg: None,
        artifacts: artifacts.copied(),
        head_pose: None,
        temperature_raw: 0,
        gpu: skill_data::gpu_stats::read(),
    };
    skill_devices::enrich_band_snapshot(snap, &ctx);

    // Add composite scores derived from band power.
    let mut val = serde_json::to_value(&*snap).unwrap_or_default();
    if let Some(obj) = val.as_object_mut() {
        let engage_raw = skill_devices::compute_engagement_raw(snap);
        let focus = skill_devices::focus_score(engage_raw);
        let nch = snap.channels.len().max(1) as f64;
        let avg_alpha = snap.channels.iter().map(|c| c.rel_alpha as f64).sum::<f64>() / nch;
        let avg_beta = snap.channels.iter().map(|c| c.rel_beta as f64).sum::<f64>() / nch;
        let relaxation = if (avg_alpha + avg_beta) > 0.0 {
            (avg_alpha / (avg_alpha + avg_beta)) * 100.0
        } else {
            0.0
        };
        let engagement = 100.0 / (1.0 + (-2.0 * (engage_raw as f64 - 0.8)).exp());
        obj.insert("focus".into(), serde_json::json!(focus));
        obj.insert("relaxation".into(), serde_json::json!(relaxation));
        obj.insert("engagement".into(), serde_json::json!(engagement));
    }
    val
}

// ── Session metadata ──────────────────────────────────────────────────────────

/// Device identity fields bundled for session metadata.
pub struct SessionDeviceId<'a> {
    pub firmware_version: Option<&'a str>,
    pub serial_number: Option<&'a str>,
}

/// Write comprehensive session metadata JSON sidecar next to CSV.
///
/// Includes device identity, battery, signal quality, filter config, and
/// phone info from the daemon status snapshot.  Previously lived in Tauri's
/// `session_csv.rs`; now fully daemon-authoritative.
pub fn write_session_meta(
    csv_path: &Path,
    device_name: &str,
    channel_names: &[String],
    sample_rate: f64,
    start_utc: u64,
    total_samples: u64,
    device_id: &SessionDeviceId<'_>,
) {
    write_session_meta_full(
        csv_path,
        device_name,
        channel_names,
        sample_rate,
        start_utc,
        total_samples,
        device_id,
        None,
    );
}

/// Extended session metadata writer with optional status snapshot.
#[allow(clippy::too_many_arguments)]
pub fn write_session_meta_full(
    csv_path: &Path,
    device_name: &str,
    channel_names: &[String],
    sample_rate: f64,
    start_utc: u64,
    total_samples: u64,
    device_id: &SessionDeviceId<'_>,
    status: Option<&skill_daemon_common::StatusResponse>,
) {
    let session_end = unix_secs();
    let duration_secs = session_end.saturating_sub(start_utc);

    let mut meta = serde_json::json!({
        "csv_file": csv_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "session_start_utc": start_utc,
        "session_end_utc": session_end,
        "session_duration_s": duration_secs,
        "total_samples": total_samples,
        "sample_rate_hz": sample_rate,
        "device_name": device_name,
        "channel_names": channel_names,
        "channel_count": channel_names.len(),
        "firmware_version": device_id.firmware_version,
        "serial_number": device_id.serial_number,
        "daemon": true,
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    });

    // Enrich from status snapshot if available.
    if let Some(s) = status {
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("device_id".into(), serde_json::json!(s.device_id));
            obj.insert("mac_address".into(), serde_json::json!(s.mac_address));
            obj.insert("hardware_version".into(), serde_json::json!(s.hardware_version));
            obj.insert("battery_pct_end".into(), serde_json::json!(s.battery));
            obj.insert("channel_quality".into(), serde_json::json!(s.channel_quality));
            obj.insert("ppg_total_samples".into(), serde_json::json!(s.ppg_sample_count));
            obj.insert("ppg_channel_names".into(), serde_json::json!(s.ppg_channel_names));
            obj.insert("imu_channel_names".into(), serde_json::json!(s.imu_channel_names));
            obj.insert("fnirs_channel_names".into(), serde_json::json!(s.fnirs_channel_names));
            obj.insert("phone_info".into(), serde_json::json!(s.phone_info));
            obj.insert("eeg_channel_count".into(), serde_json::json!(s.eeg_channel_count));
            obj.insert("eeg_sample_rate_hz".into(), serde_json::json!(s.eeg_sample_rate_hz));
        }
    }

    if let Ok(json) = serde_json::to_string_pretty(&meta) {
        let _ = std::fs::write(csv_path.with_extension("json"), json);
    }
}
