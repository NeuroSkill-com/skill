use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use skill_daemon_common::{DeviceLogEntry, StatusResponse, WsClient};
use tracing::info;

use crate::state::AppState;

pub(crate) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Write JSON to `path` atomically: write to a sibling `.tmp` file then
/// rename into place.  A crash mid-write leaves the original file intact.
pub(crate) fn write_json_atomic(path: &Path, json: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

pub(crate) fn write_string_atomic(path: &Path, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        tighten_dir_permissions(parent)?;
    }

    let mut nonce = [0u8; 8];
    rand::rng().fill_bytes(&mut nonce);
    let tmp_name = format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        hex::encode(nonce)
    );
    let tmp_path = path.with_file_name(tmp_name);

    std::fs::write(&tmp_path, content)?;
    tighten_file_permissions(&tmp_path)?;
    std::fs::rename(&tmp_path, path)?;
    tighten_file_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
pub(crate) fn tighten_file_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn tighten_file_permissions(path: &Path) -> anyhow::Result<()> {
    restrict_windows_acl(path)
}

#[cfg(unix)]
pub(crate) fn tighten_dir_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn tighten_dir_permissions(path: &Path) -> anyhow::Result<()> {
    restrict_windows_acl(path)
}

/// On Windows, reset the DACL so only the current user has access.
///
/// Uses `icacls` which is available on all supported Windows versions.
/// If `icacls` is missing or fails we log a warning but do not abort —
/// the daemon should still start even if we cannot restrict permissions.
#[cfg(not(unix))]
fn restrict_windows_acl(path: &Path) -> anyhow::Result<()> {
    let path_str = path.to_string_lossy();

    // Retrieve the current user's name (e.g. "DESKTOP-X\\Alice").
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "*S-1-5-32-544".into());

    // 1. Disable inheritance and remove inherited ACEs.
    let _ = std::process::Command::new("icacls")
        .args([path_str.as_ref(), "/inheritance:r"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    // 2. Grant full control only to the current user.
    let status = std::process::Command::new("icacls")
        .args([path_str.as_ref(), "/grant:r", &format!("{user}:(F)")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            tracing::warn!(
                path = %path_str,
                code = ?s.code(),
                "icacls returned non-zero — auth token may be world-readable"
            );
            Ok(())
        }
        Err(e) => {
            tracing::warn!(
                path = %path_str,
                err = %e,
                "could not run icacls — auth token may be world-readable"
            );
            Ok(())
        }
    }
}

// ── Token management ───────────────────────────────────────────────────────

pub(crate) fn load_or_create_token() -> anyhow::Result<String> {
    let token_path = token_path()?;

    if token_path.exists() {
        let value = std::fs::read_to_string(&token_path)?;
        let token = value.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    if let Some(parent) = token_path.parent() {
        std::fs::create_dir_all(parent)?;
        tighten_dir_permissions(parent)?;
    }

    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);

    write_string_atomic(&token_path, &format!("{token}\n"))?;

    info!(path = %token_path.display(), "created daemon auth token");
    Ok(token)
}

pub(crate) fn token_path() -> anyhow::Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("unable to resolve config directory"))?;
    Ok(base.join("skill").join("daemon").join("auth.token"))
}

// ── Client / request tracking ──────────────────────────────────────────────

pub(crate) fn add_client(state: &AppState, peer: &str) {
    if let Ok(mut guard) = state.tracker.lock() {
        guard.clients.push(WsClient {
            peer: peer.to_string(),
            connected_at: now_unix_secs(),
        });
    }
}

pub(crate) fn remove_client(state: &AppState, peer: &str) {
    if let Ok(mut guard) = state.tracker.lock() {
        if let Some(idx) = guard.clients.iter().position(|c| c.peer == peer) {
            guard.clients.remove(idx);
        }
    }
}

pub(crate) fn record_request(state: &AppState, peer: String, command: String, ok: bool) {
    if let Ok(mut guard) = state.tracker.lock() {
        guard.add_request(peer, command, ok, now_unix_secs());
    }
}

pub(crate) fn push_device_log(state: &AppState, tag: &str, msg: &str) {
    const DEVICE_LOG_CAP: usize = 256;
    if let Ok(mut guard) = state.device_log.lock() {
        if guard.len() >= DEVICE_LOG_CAP {
            let _ = guard.pop_front();
        }
        guard.push_back(DeviceLogEntry {
            ts: now_unix_secs(),
            tag: tag.to_string(),
            msg: msg.to_string(),
        });
    }
}

// ── Paired device persistence ──────────────────────────────────────────────

/// Persist the current `status.paired_devices` list to disk.
///
/// Writes two files:
/// * `paired_devices.json` — lightweight fast-path read on daemon startup
/// * `settings.json` — kept in sync for Tauri and backward compatibility
///
/// Non-fatal: logs a warning on failure but never panics.
pub(crate) fn persist_paired_devices(state: &AppState) {
    let skill_dir = match state.skill_dir.lock() {
        Ok(g) => g.clone(),
        Err(_) => return,
    };
    let paired: Vec<skill_settings::PairedDevice> = state
        .status
        .lock()
        .map(|s| {
            s.paired_devices
                .iter()
                .map(|p| skill_settings::PairedDevice {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    last_seen: p.last_seen,
                })
                .collect()
        })
        .unwrap_or_default();

    // Fast path: paired_devices.json — written atomically so a crash mid-write
    // never leaves a truncated file.
    let paired_path = skill_dir.join(skill_constants::PAIRED_DEVICES_FILE);
    match serde_json::to_string_pretty(&paired) {
        Ok(json) => {
            if let Err(e) = write_json_atomic(&paired_path, &json) {
                tracing::warn!("persist_paired_devices: write {}: {e}", paired_path.display());
            }
        }
        Err(e) => tracing::warn!("persist_paired_devices: serialize: {e}"),
    }

    // Keep settings.json in sync (read-modify-write) for Tauri / older builds.
    // Spawned so the HTTP handler returns immediately; atomic write avoids
    // partial-file corruption when Tauri writes settings concurrently.
    let skill_dir2 = skill_dir.clone();
    let paired2 = paired.clone();
    tokio::task::spawn_blocking(move || {
        let mut settings = skill_settings::load_settings(&skill_dir2);
        settings.paired = paired2;
        let path = skill_settings::settings_path(&skill_dir2);
        if let Ok(json) = serde_json::to_string_pretty(&settings) {
            if let Err(e) = write_json_atomic(&path, &json) {
                tracing::warn!("persist_paired_devices: write settings.json: {e}");
            }
        }
    });
}

// ── Session helpers ────────────────────────────────────────────────────────

/// Spawn the appropriate session runner for the given target device.
/// Cancels any existing session first.
pub(crate) fn spawn_session_for_target(state: &AppState, target: Option<&str>) {
    let Some(t) = target else { return };

    // Idempotency guard: if we're already connecting/connected to the same
    // target and have an active session handle, do not cancel/restart.
    let same_target_active = {
        let status_same = state
            .status
            .lock()
            .ok()
            .map(|s| {
                (s.state == "connecting" || s.state == "connected")
                    && (s.target_id.as_deref() == Some(t)
                        || s.target_name.as_deref() == Some(t)
                        || s.target_display_name.as_deref() == Some(t))
            })
            .unwrap_or(false);
        let handle_active = state
            .session_handle
            .lock()
            .ok()
            .map(|slot| slot.is_some())
            .unwrap_or(false);
        status_same && handle_active
    };
    if same_target_active {
        push_device_log(
            state,
            "session",
            &format!("spawn_session_for_target noop: already active target={t}"),
        );
        return;
    }

    // Cancel any existing session.
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }

    // All devices route through the generic adapter session runner.
    let handle = crate::session::spawn_device_session(state.clone(), t.to_string());

    if let Some(h) = handle {
        if let Ok(mut slot) = state.session_handle.lock() {
            *slot = Some(h);
        }
    }
}

pub(crate) fn default_status(state: &str) -> StatusResponse {
    StatusResponse {
        state: state.to_string(),
        ..Default::default()
    }
}

/// Resolve canonical target fields for status/UI from a requested target.
/// Returns `(target_id, target_display_name)`.
pub(crate) fn resolve_target_fields(state: &AppState, target: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(t) = target else { return (None, None) };

    // ID-like targets (ble:/usb:/wifi:/...) should preserve their id and try
    // to resolve a human-friendly name from the paired list.
    if t.contains(':') {
        let display = state
            .status
            .lock()
            .ok()
            .and_then(|s| s.paired_devices.iter().find(|d| d.id == t).map(|d| d.name.clone()));
        return (Some(t.to_string()), display.or_else(|| Some(t.to_string())));
    }

    // Name-like targets: keep display name and backfill id from paired devices.
    let id = state
        .status
        .lock()
        .ok()
        .and_then(|s| s.paired_devices.iter().find(|d| d.name == t).map(|d| d.id.clone()));
    (id, Some(t.to_string()))
}

pub(crate) fn target_requires_pairing(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    // Iroh remote peers are pre-authenticated by the iroh tunnel (TOTP-paired).
    if lower.starts_with("peer:") {
        return false;
    }
    // LSL streams are paired in the LSL config, not the BLE device list.
    if lower.starts_with("lsl:") || lower == "lsl-iroh" {
        return false;
    }
    lower.contains(':') || lower == "neurosky" || lower.starts_with("muse")
}

pub(crate) fn is_paired_target(state: &AppState, target: &str) -> bool {
    state
        .status
        .lock()
        .ok()
        .map(|s| s.paired_devices.iter().any(|d| d.id == target || d.name == target))
        .unwrap_or(false)
}

pub(crate) fn preferred_peer_target(
    activity: &[skill_iroh::PeerActivityView],
    recent_peer_ids: &[String],
) -> Option<String> {
    activity
        .iter()
        .find(|p| p.tunnel_connected && p.remote_device_connected && p.streaming_active)
        .map(|p| format!("peer:{}", p.peer_id))
        .or_else(|| {
            activity
                .iter()
                .find(|p| p.tunnel_connected && p.remote_device_connected)
                .map(|p| format!("peer:{}", p.peer_id))
        })
        .or_else(|| {
            activity
                .iter()
                .find(|p| p.tunnel_connected)
                .map(|p| format!("peer:{}", p.peer_id))
        })
        .or_else(|| recent_peer_ids.first().map(|p| format!("peer:{p}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_json_atomic_creates_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.json");
        let data = r#"[{"id":"ble:abc","name":"Muse-1234","last_seen":1000}]"#;
        write_json_atomic(&path, data).expect("write failed");
        let read_back = std::fs::read_to_string(&path).unwrap();
        assert_eq!(read_back, data);
        // No .tmp file left behind
        assert!(!dir.path().join("test.tmp").exists());
    }

    #[test]
    fn request_log_is_capped_under_abuse() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".to_string(), td.path().to_path_buf());

        for i in 0..1200 {
            record_request(&state, "127.0.0.1".into(), format!("/bad/{i}"), false);
        }

        let guard = state.tracker.lock().unwrap();
        assert_eq!(guard.requests.len(), 500);
        assert_eq!(guard.requests.first().map(|r| r.command.as_str()), Some("/bad/700"));
        assert_eq!(guard.requests.last().map(|r| r.command.as_str()), Some("/bad/1199"));
    }

    #[test]
    fn device_log_is_capped_at_256_entries() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".to_string(), td.path().to_path_buf());

        for i in 0..400 {
            push_device_log(&state, "test", &format!("msg-{i}"));
        }

        let guard = state.device_log.lock().unwrap();
        assert_eq!(guard.len(), 256);
        assert_eq!(guard.front().map(|e| e.msg.as_str()), Some("msg-144"));
        assert_eq!(guard.back().map(|e| e.msg.as_str()), Some("msg-399"));
    }

    #[test]
    fn preferred_peer_target_prioritizes_remote_ble_stream() {
        let peers = vec![
            skill_iroh::PeerActivityView {
                peer_id: "p1".into(),
                tunnel_connected: true,
                remote_device_connected: false,
                streaming_active: false,
                eeg_streaming_active: false,
                last_seen_unix: 1,
            },
            skill_iroh::PeerActivityView {
                peer_id: "p2".into(),
                tunnel_connected: true,
                remote_device_connected: true,
                streaming_active: true,
                eeg_streaming_active: true,
                last_seen_unix: 2,
            },
        ];
        let chosen = preferred_peer_target(&peers, &[]);
        assert_eq!(chosen.as_deref(), Some("peer:p2"));
    }

    #[test]
    fn preferred_peer_target_falls_back_to_recent_when_no_live_peer() {
        let peers: Vec<skill_iroh::PeerActivityView> = vec![];
        let recent = vec!["abc".to_string()];
        let chosen = preferred_peer_target(&peers, &recent);
        assert_eq!(chosen.as_deref(), Some("peer:abc"));
    }

    #[test]
    fn now_unix_ms_returns_plausible_value() {
        let ms = now_unix_ms();
        // Should be after 2024-01-01 in milliseconds
        assert!(ms > 1_704_067_200_000);
    }

    #[test]
    fn now_unix_secs_returns_plausible_value() {
        let secs = now_unix_secs();
        assert!(secs > 1_704_067_200);
    }

    #[test]
    fn default_status_sets_state_field() {
        let s = default_status("connecting");
        assert_eq!(s.state, "connecting");
        let s2 = default_status("disconnected");
        assert_eq!(s2.state, "disconnected");
    }

    #[test]
    fn target_requires_pairing_for_ble_devices() {
        assert!(target_requires_pairing("ble:abc123"));
        assert!(target_requires_pairing("usb:/dev/ttyUSB0"));
        assert!(target_requires_pairing("neurosky"));
        assert!(target_requires_pairing("muse-1234"));
    }

    #[test]
    fn target_does_not_require_pairing_for_peers() {
        assert!(!target_requires_pairing("peer:abc123"));
        assert!(!target_requires_pairing("Peer:XYZ"));
    }

    #[test]
    fn is_paired_target_checks_id_and_name() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        // Add a paired device
        if let Ok(mut status) = state.status.lock() {
            status.paired_devices.push(skill_daemon_common::PairedDeviceResponse {
                id: "ble:abc".into(),
                name: "Muse-1234".into(),
                last_seen: 0,
            });
        }
        assert!(is_paired_target(&state, "ble:abc"));
        assert!(is_paired_target(&state, "Muse-1234"));
        assert!(!is_paired_target(&state, "ble:unknown"));
    }

    #[test]
    fn resolve_target_fields_id_like_target() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        // Add paired device for name resolution
        if let Ok(mut status) = state.status.lock() {
            status.paired_devices.push(skill_daemon_common::PairedDeviceResponse {
                id: "ble:abc".into(),
                name: "Muse-1234".into(),
                last_seen: 0,
            });
        }
        let (id, display) = resolve_target_fields(&state, Some("ble:abc"));
        assert_eq!(id, Some("ble:abc".into()));
        assert_eq!(display, Some("Muse-1234".into()));
    }

    #[test]
    fn resolve_target_fields_name_like_target() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        if let Ok(mut status) = state.status.lock() {
            status.paired_devices.push(skill_daemon_common::PairedDeviceResponse {
                id: "ble:abc".into(),
                name: "Muse-1234".into(),
                last_seen: 0,
            });
        }
        let (id, display) = resolve_target_fields(&state, Some("Muse-1234"));
        assert_eq!(id, Some("ble:abc".into()));
        assert_eq!(display, Some("Muse-1234".into()));
    }

    #[test]
    fn resolve_target_fields_none_target() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        let (id, display) = resolve_target_fields(&state, None);
        assert!(id.is_none());
        assert!(display.is_none());
    }

    #[test]
    fn resolve_target_fields_unknown_id_uses_id_as_display() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        let (id, display) = resolve_target_fields(&state, Some("ble:unknown"));
        assert_eq!(id, Some("ble:unknown".into()));
        assert_eq!(display, Some("ble:unknown".into()));
    }

    #[test]
    fn write_string_atomic_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        write_string_atomic(&path, "hello\n").expect("write failed");
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello\n");
    }

    #[tokio::test]
    async fn persist_paired_devices_writes_json_file() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        // Add paired devices
        if let Ok(mut status) = state.status.lock() {
            status.paired_devices.push(skill_daemon_common::PairedDeviceResponse {
                id: "ble:test".into(),
                name: "TestDevice".into(),
                last_seen: 12345,
            });
        }
        persist_paired_devices(&state);
        // Allow the spawned blocking task to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        // Check the fast-path file was written
        let paired_path = td.path().join(skill_constants::PAIRED_DEVICES_FILE);
        assert!(paired_path.exists());
        let content = std::fs::read_to_string(&paired_path).unwrap();
        assert!(content.contains("TestDevice"));
    }
}
