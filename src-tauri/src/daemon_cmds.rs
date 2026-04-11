// Helper for erased_serde trait object serialization
pub fn push_event_to_daemon_erased(event_type: &str, payload: &dyn erased_serde::Serialize) {
    let mut buf = Vec::new();
    {
        let mut ser = serde_json::Serializer::new(&mut buf);
        erased_serde::serialize(payload, &mut ser).unwrap();
    }
    let payload_json = String::from_utf8(buf).unwrap();
    crate::ws_server::LAST_EVENT.with(|cell| {
        *cell.borrow_mut() = Some((event_type.to_string(), payload_json));
    });
}
// SPDX-License-Identifier: GPL-3.0-only

use serde::Serialize;
use skill_daemon_common::{
    DiscoveredDeviceResponse, ForgetDeviceRequest, ScannerStateResponse, ScannerWifiConfigRequest,
    SessionControlRequest, SetPreferredDeviceRequest, StatusResponse, VersionResponse,
    WsPortResponse, PROTOCOL_VERSION,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Serialize)]
pub struct DaemonStatus {
    pub base_url: String,
    pub reachable: bool,
    pub authenticated: bool,
    pub compatible_protocol: bool,
    pub daemon_required: bool,
    pub version: Option<VersionResponse>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DaemonBootstrap {
    pub port: u16,
    pub token: String,
    pub compatible_protocol: bool,
    pub daemon_version: Option<String>,
    pub protocol_version: Option<u32>,
}

#[tauri::command]
pub fn get_daemon_bootstrap() -> Result<DaemonBootstrap, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let port = fetch_daemon_ws_port().unwrap_or(18444);
    let version = fetch_version(&base_url, &token).ok();
    let compatible_protocol = version
        .as_ref()
        .map(|v| v.protocol_version == PROTOCOL_VERSION)
        .unwrap_or(true);

    Ok(DaemonBootstrap {
        port,
        token,
        compatible_protocol,
        daemon_version: version.as_ref().map(|v| v.daemon_version.clone()),
        protocol_version: version.as_ref().map(|v| v.protocol_version),
    })
}

#[tauri::command]
pub fn get_daemon_status() -> DaemonStatus {
    let base_url = daemon_base_url();
    let token = load_daemon_token().ok();
    let daemon_required = daemon_required_env();

    let Some(token) = token else {
        return DaemonStatus {
            base_url,
            reachable: false,
            authenticated: false,
            compatible_protocol: false,
            daemon_required,
            version: None,
            error: Some("daemon auth token not found".to_string()),
        };
    };

    match fetch_version(&base_url, &token) {
        Ok(version) => DaemonStatus {
            base_url,
            reachable: true,
            authenticated: true,
            compatible_protocol: version.protocol_version == PROTOCOL_VERSION,
            daemon_required,
            version: Some(version),
            error: None,
        },
        Err(err) => DaemonStatus {
            base_url,
            reachable: false,
            authenticated: false,
            compatible_protocol: false,
            daemon_required,
            version: None,
            error: Some(err),
        },
    }
}

#[tauri::command]
pub fn get_daemon_token_path() -> String {
    token_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "<unresolved>".to_string())
}

fn resolve_daemon_bin_path() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Production: sidecar next to app binary (Tauri bundles it)
            for name in &["skill-daemon", "skill-daemon.exe"] {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return candidate.display().to_string();
                }
            }
            // macOS .app bundle: inside Contents/MacOS/
            let mac_candidate = dir.join("../MacOS/skill-daemon");
            if mac_candidate.exists() {
                return mac_candidate
                    .canonicalize()
                    .unwrap_or(mac_candidate)
                    .display()
                    .to_string();
            }
        }
    }

    // Dev: look in target dir
    let target_candidates = [
        "src-tauri/target/debug/skill-daemon",
        "src-tauri/target/debug/skill-daemon.exe",
        "src-tauri/target/aarch64-apple-darwin/debug/skill-daemon",
        "src-tauri/target/x86_64-pc-windows-msvc/debug/skill-daemon.exe",
        "target/debug/skill-daemon",
        "target/debug/skill-daemon.exe",
    ];
    for c in &target_candidates {
        if std::path::Path::new(c).exists() {
            return c.to_string();
        }
    }

    if cfg!(target_os = "windows") {
        "skill-daemon.exe".to_string()
    } else {
        "skill-daemon".to_string()
    }
}

fn daemon_rollback_bin_path() -> Result<PathBuf, String> {
    let base =
        dirs::config_dir().ok_or_else(|| "unable to resolve config directory".to_string())?;
    let mut p = base.join("skill").join("daemon").join("bin");
    let name = if cfg!(target_os = "windows") {
        "skill-daemon.rollback.exe"
    } else {
        "skill-daemon.rollback"
    };
    p.push(name);
    Ok(p)
}

fn update_daemon_rollback_snapshot_best_effort() {
    let src = std::env::var("SKILL_DAEMON_BIN").unwrap_or_else(|_| resolve_daemon_bin_path());
    let src_path = PathBuf::from(&src);
    if !src_path.exists() {
        return;
    }

    let Ok(dst_path) = daemon_rollback_bin_path() else {
        return;
    };

    if src_path == dst_path {
        return;
    }

    if let (Ok(src_meta), Ok(dst_meta)) =
        (std::fs::metadata(&src_path), std::fs::metadata(&dst_path))
    {
        if src_meta.len() == dst_meta.len() {
            return;
        }
    }

    if let Some(parent) = dst_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::copy(&src_path, &dst_path).is_ok() {
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dst_path, std::fs::Permissions::from_mode(0o755));
        }
        eprintln!("[daemon] updated rollback snapshot: {}", dst_path.display());
    }
}

/// Ensure the daemon process is running.  If it's not reachable, attempt to
/// spawn it.  Called once during `setup_app`.
pub(crate) fn ensure_daemon_running() {
    let base_url = daemon_base_url();
    let addr: std::net::SocketAddr = std::env::var("SKILL_DAEMON_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:18444".to_string())
        .parse()
        .unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], 18444)));

    // Quick health check — if the daemon is already up, nothing to do.
    if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
        eprintln!("[daemon] already running at {base_url}");
        return;
    }

    // The daemon may have been spawned by tauri-build.js moments ago but not
    // yet bound its port (slow first-run: model probe, HF cache scan, etc.).
    // Poll for up to 4 s before concluding it is absent and spawning a second
    // instance.  Spawning two daemons simultaneously causes a CPU spike
    // because both start BLE scanning and activity-monitor threads before the
    // loser fails to bind the port and exits.
    eprintln!(
        "[daemon] not yet reachable at {base_url} — waiting up to 4 s for in-progress start…"
    );
    let poll_deadline = std::time::Instant::now() + Duration::from_secs(4);
    let already_running = loop {
        std::thread::sleep(Duration::from_millis(200));
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            break true;
        }
        if std::time::Instant::now() >= poll_deadline {
            break false;
        }
    };
    if already_running {
        eprintln!("[daemon] became ready during wait — skipping spawn");
        return;
    }

    // Try to spawn the daemon binary.
    //
    // On Windows executables must end in `.exe`; the candidates below
    // include both the bare name and the `.exe` variant so the lookup
    // works on all platforms.
    let bin = std::env::var("SKILL_DAEMON_BIN").unwrap_or_else(|_| resolve_daemon_bin_path());

    eprintln!("[daemon] not reachable at {base_url}, spawning: {bin}");
    match std::process::Command::new(&bin)
        .env(
            "SKILL_DAEMON_ADDR",
            std::env::var("SKILL_DAEMON_ADDR").unwrap_or_else(|_| "127.0.0.1:18444".to_string()),
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
    {
        Ok(_) => {
            eprintln!("[daemon] spawned, waiting for readiness...");
            // Wait up to 5 seconds for the daemon to become ready.
            for _ in 0..50 {
                std::thread::sleep(Duration::from_millis(100));
                if std::net::TcpStream::connect_timeout(
                    &std::env::var("SKILL_DAEMON_ADDR")
                        .unwrap_or_else(|_| "127.0.0.1:18444".to_string())
                        .parse()
                        .unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], 18444))),
                    Duration::from_millis(200),
                )
                .is_ok()
                {
                    eprintln!("[daemon] ready");
                    return;
                }
            }
            eprintln!("[daemon] spawned but not ready after 5 s — continuing anyway");
        }
        Err(e) => {
            eprintln!("[daemon] failed to spawn: {e} — features requiring daemon will degrade");
        }
    }
}

#[tauri::command]
pub fn start_daemon_dev() -> Result<(), String> {
    let bin = std::env::var("SKILL_DAEMON_BIN").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "skill-daemon.exe".to_string()
        } else {
            "skill-daemon".to_string()
        }
    });
    let addr = std::env::var("SKILL_DAEMON_ADDR").unwrap_or_else(|_| "127.0.0.1:18444".to_string());

    std::process::Command::new(bin)
        .env("SKILL_DAEMON_ADDR", addr)
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn daemon_install_service() -> Result<serde_json::Value, String> {
    crate::daemon_cmds::install_daemon_service()
}

#[tauri::command]
pub fn daemon_uninstall_service() -> Result<serde_json::Value, String> {
    crate::daemon_cmds::uninstall_daemon_service()
}

#[tauri::command]
pub fn get_daemon_service_status() -> Result<serde_json::Value, String> {
    crate::daemon_cmds::daemon_service_status()
}

pub(crate) fn fetch_daemon_ws_port() -> Result<u16, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let body: WsPortResponse = fetch_json_with_auth(&base_url, &token, "/v1/ws-port")?;
    Ok(body.port)
}

/// Fetch daemon log lines that have a sequence number >= `since`.
/// Returns `(next_seq, lines)` on success.
pub(crate) fn fetch_daemon_log_recent(since: u64) -> Result<(u64, Vec<String>), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let path = format!("/v1/log/recent?since={since}");
    let body: serde_json::Value = fetch_json_with_auth(&base_url, &token, &path)?;
    let next_seq = body["next_seq"].as_u64().unwrap_or(0);
    let lines = body["lines"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_owned()))
                .collect()
        })
        .unwrap_or_default();
    Ok((next_seq, lines))
}

pub(crate) fn fetch_daemon_status() -> Result<StatusResponse, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/status")
}

pub(crate) fn set_preferred_device(id: String) -> Result<Vec<DiscoveredDeviceResponse>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/devices/set-preferred",
        &SetPreferredDeviceRequest { id },
    )
}

pub(crate) fn forget_device(id: String) -> Result<Vec<DiscoveredDeviceResponse>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/devices/forget",
        &ForgetDeviceRequest { id },
    )
}

pub(crate) fn retry_connect() -> Result<StatusResponse, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/control/retry-connect",
        &serde_json::json!({}),
    )
}

pub(crate) fn cancel_retry() -> Result<StatusResponse, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/control/cancel-retry",
        &serde_json::json!({}),
    )
}

pub(crate) fn enable_reconnect() -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth(
        &base_url,
        &token,
        "/v1/control/enable-reconnect",
        &serde_json::json!({}),
    )
}

pub(crate) fn disable_reconnect() -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth(
        &base_url,
        &token,
        "/v1/control/disable-reconnect",
        &serde_json::json!({}),
    )
}

#[allow(dead_code)]
pub(crate) fn cancel_session_sync() -> Result<StatusResponse, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/control/cancel-session",
        &serde_json::json!({}),
    )
}

/// Blocking version for internal callers (lifecycle, session_connect).
pub(crate) fn start_session_sync(target: Option<String>) -> Result<StatusResponse, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/control/start-session",
        &SessionControlRequest { target },
    )
}

#[tauri::command]
pub async fn start_session(target: Option<String>) -> Result<StatusResponse, String> {
    tokio::task::spawn_blocking(move || start_session_sync(target))
        .await
        .map_err(|e| e.to_string())?
}

#[allow(dead_code)]
pub(crate) fn scanner_start() -> Result<ScannerStateResponse, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/control/scanner/start",
        &serde_json::json!({}),
    )
}

#[allow(dead_code)]
pub(crate) fn scanner_stop() -> Result<ScannerStateResponse, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/control/scanner/stop",
        &serde_json::json!({}),
    )
}

#[allow(dead_code)]
pub(crate) fn scanner_state() -> Result<ScannerStateResponse, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/control/scanner/state")
}

pub(crate) fn fetch_history_sessions() -> Result<Vec<serde_json::Value>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/history/sessions")
}

pub(crate) fn set_notch_preset(
    preset: Option<skill_eeg::eeg_filter::PowerlineFreq>,
) -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/settings/notch-preset",
        &serde_json::json!({"value": preset}),
    )?;
    Ok(())
}

pub(crate) fn fetch_update_check_interval() -> Result<u64, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value =
        fetch_json_with_auth(&base_url, &token, "/v1/settings/update-check-interval")?;
    Ok(v.get("value")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(3600))
}

pub(crate) fn set_update_check_interval(secs: u64) -> Result<u64, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/settings/update-check-interval",
        &serde_json::json!({"value": secs}),
    )?;
    Ok(v.get("value")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(secs))
}

pub(crate) fn test_location() -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/settings/location-test",
        &serde_json::json!({}),
    )
}

pub(crate) fn fetch_accent_color() -> Result<String, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value = fetch_json_with_auth(&base_url, &token, "/v1/ui/accent-color")?;
    Ok(v.get("value")
        .and_then(|x| x.as_str())
        .unwrap_or("blue")
        .to_string())
}

pub(crate) fn set_accent_color(accent: String) -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/ui/accent-color",
        &serde_json::json!({"value": accent}),
    )?;
    Ok(())
}

pub(crate) fn fetch_recent_active_windows(
    limit: Option<u32>,
) -> Result<Vec<skill_data::activity_store::ActiveWindowRow>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/activity/recent-windows",
        &serde_json::json!({"limit": limit}),
    )
}

pub(crate) fn fetch_recent_input_activity(
    limit: Option<u32>,
) -> Result<Vec<skill_data::activity_store::InputActivityRow>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/activity/recent-input",
        &serde_json::json!({"limit": limit}),
    )
}

pub(crate) fn fetch_input_buckets(
    from_ts: Option<u64>,
    to_ts: Option<u64>,
) -> Result<Vec<skill_data::activity_store::InputBucketRow>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/activity/input-buckets",
        &serde_json::json!({"from_ts": from_ts, "to_ts": to_ts}),
    )
}

#[allow(dead_code)]
pub(crate) fn fetch_hooks() -> Result<Vec<skill_settings::HookRule>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/hooks")
}

pub(crate) fn llm_server_start() -> Result<String, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/server/start",
        &serde_json::json!({}),
    )?;
    Ok(v.get("result")
        .and_then(|x| x.as_str())
        .unwrap_or("starting")
        .to_string())
}

pub(crate) fn llm_server_stop() -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/server/stop",
        &serde_json::json!({}),
    )?;
    Ok(())
}

pub(crate) fn llm_server_status() -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/llm/server/status")
}

pub(crate) fn llm_server_logs() -> Result<Vec<serde_json::Value>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/llm/server/logs")
}

pub(crate) fn llm_server_switch_model(filename: String) -> Result<String, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/server/switch-model",
        &serde_json::json!({"filename": filename}),
    )?;
    Ok(v.get("result")
        .and_then(|x| x.as_str())
        .unwrap_or("switching")
        .to_string())
}

pub(crate) fn llm_server_switch_mmproj(filename: String) -> Result<String, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/server/switch-mmproj",
        &serde_json::json!({"filename": filename}),
    )?;
    Ok(v.get("result")
        .and_then(|x| x.as_str())
        .unwrap_or("switching")
        .to_string())
}

pub(crate) fn llm_get_catalog() -> Result<skill_llm::catalog::LlmCatalog, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/llm/catalog")
}

pub(crate) fn llm_refresh_catalog() -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/catalog/refresh",
        &serde_json::json!({}),
    )?;
    Ok(())
}

pub(crate) fn llm_add_model(
    repo: String,
    filename: String,
    size_gb: Option<f32>,
    mmproj: Option<String>,
    download: Option<bool>,
) -> Result<String, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/catalog/add-model",
        &serde_json::json!({"repo":repo,"filename":filename,"size_gb":size_gb,"mmproj":mmproj,"download":download}),
    )?;
    Ok(v.get("filename")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string())
}

pub(crate) fn llm_get_downloads() -> Result<Vec<serde_json::Value>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/llm/downloads")
}

pub(crate) fn llm_download_action(path: &str, filename: String) -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        path,
        &serde_json::json!({"filename": filename}),
    )?;
    Ok(())
}

pub(crate) fn llm_set_active_model(filename: String) -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/selection/active-model",
        &serde_json::json!({"filename": filename}),
    )?;
    Ok(())
}

pub(crate) fn llm_set_active_mmproj(filename: String) -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/selection/active-mmproj",
        &serde_json::json!({"filename": filename}),
    )?;
    Ok(())
}

pub(crate) fn llm_set_autoload_mmproj(enabled: bool) -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/selection/autoload-mmproj",
        &serde_json::json!({"value": enabled}),
    )?;
    Ok(())
}

pub(crate) fn llm_chat_completions(
    messages: Vec<serde_json::Value>,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/chat-completions",
        &serde_json::json!({"messages": messages, "params": params}),
    )
}

pub(crate) fn llm_abort_stream() -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/abort-stream",
        &serde_json::json!({}),
    )?;
    Ok(())
}

pub(crate) fn llm_cancel_tool_call(tool_call_id: String) -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let _: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/llm/cancel-tool-call",
        &serde_json::json!({"tool_call_id": tool_call_id}),
    )?;
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn fetch_skills_refresh_interval() -> Result<u64, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value =
        fetch_json_with_auth(&base_url, &token, "/v1/skills/refresh-interval")?;
    Ok(v.get("value")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0))
}

#[allow(dead_code)]
pub(crate) fn fetch_skills_sync_on_launch() -> Result<bool, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value =
        fetch_json_with_auth(&base_url, &token, "/v1/skills/sync-on-launch")?;
    Ok(v.get("value")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
}

#[allow(dead_code)]
pub(crate) fn get_disabled_skills() -> Result<Vec<String>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value = fetch_json_with_auth(&base_url, &token, "/v1/skills/disabled")?;
    Ok(serde_json::from_value(v.get("value").cloned().unwrap_or_default()).unwrap_or_default())
}

#[allow(dead_code)]
pub(crate) fn fetch_lsl_config() -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/lsl/config")
}

#[allow(dead_code)]
pub(crate) fn get_lsl_idle_timeout() -> Result<Option<u64>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value = fetch_json_with_auth(&base_url, &token, "/v1/lsl/idle-timeout")?;
    Ok(v.get("secs").and_then(serde_json::Value::as_u64))
}

pub(crate) fn find_history_session(timestamp_utc: u64) -> Result<Option<String>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let val: serde_json::Value = post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/history/find-session",
        &serde_json::json!({"timestamp_utc": timestamp_utc}),
    )?;
    Ok(val
        .get("csv_path")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string))
}

#[allow(dead_code)]
pub(crate) fn fetch_daemon_estimate_reembed() -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/models/estimate-reembed")
}

#[allow(dead_code)]
pub(crate) fn scanner_set_wifi_config(
    wifi_shield_ip: String,
    galea_ip: String,
) -> Result<ScannerWifiConfigRequest, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/control/scanner/wifi-config",
        &ScannerWifiConfigRequest {
            wifi_shield_ip,
            galea_ip,
        },
    )
}

// Mirror state functions removed — daemon is sole authority for device/status state.

fn fetch_version(base_url: &str, token: &str) -> Result<VersionResponse, String> {
    fetch_json_with_auth(base_url, token, "/v1/version")
}

fn fetch_json_with_auth<T: serde::de::DeserializeOwned>(
    base_url: &str,
    token: &str,
    path: &str,
) -> Result<T, String> {
    let url = format!("{base_url}{path}");

    let mut response = ureq::get(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|err| err.to_string())?;

    response
        .body_mut()
        .read_json::<T>()
        .map_err(|err| err.to_string())
}

/// Forward an EEG sample batch to the daemon for WS broadcast.
#[allow(
    dead_code,
    reason = "reserved for upcoming direct EEG fan-out integration"
)]
pub(crate) fn push_eeg_samples_to_daemon(electrode: usize, samples: &[f64], timestamp: f64) {
    push_event_to_daemon(
        "EegSample",
        &serde_json::json!({
            "electrode": electrode,
            "samples": samples,
            "timestamp": timestamp,
        }),
    );
}

/// Forward band power snapshot to the daemon for WS broadcast.
#[allow(
    dead_code,
    reason = "reserved for upcoming direct EEG fan-out integration"
)]
pub(crate) fn push_bands_to_daemon(bands: &impl serde::Serialize) {
    push_event_to_daemon("EegBands", bands);
}

pub(crate) fn push_event_to_daemon(event_type: &str, payload: &impl serde::Serialize) {
    let Ok(payload_val) = serde_json::to_value(payload) else {
        return;
    };
    let envelope = skill_daemon_common::EventEnvelope {
        r#type: event_type.to_string(),
        ts_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        correlation_id: None,
        payload: payload_val,
    };
    let Ok(body) = serde_json::to_string(&envelope) else {
        return;
    };
    let base_url = daemon_base_url();
    let Ok(token) = load_daemon_token() else {
        return;
    };
    // Fire-and-forget push via POST to a daemon events endpoint.
    let _ = ureq::post(&format!("{base_url}/v1/events/push"))
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .send(body.as_str());
}

#[allow(dead_code)]
pub(crate) fn post_json_with_auth_pub<T: Serialize>(path: &str, body: &T) -> Result<(), String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth(&base_url, &token, path, body)
}

pub(crate) fn post_json_value_with_auth(
    path: &str,
    body: &impl Serialize,
) -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(&base_url, &token, path, body)
}

pub(crate) fn fetch_json_value_with_auth(path: &str) -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, path)
}

fn post_json_with_auth<T: Serialize>(
    base_url: &str,
    token: &str,
    path: &str,
    body: &T,
) -> Result<(), String> {
    let url = format!("{base_url}{path}");
    let payload = serde_json::to_string(body).map_err(|err| err.to_string())?;

    ureq::post(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .send(payload.as_str())
        .map_err(|err| err.to_string())?;

    Ok(())
}

fn post_json_with_auth_response<TReq: Serialize, TResp: serde::de::DeserializeOwned>(
    base_url: &str,
    token: &str,
    path: &str,
    body: &TReq,
) -> Result<TResp, String> {
    let url = format!("{base_url}{path}");
    let payload = serde_json::to_string(body).map_err(|err| err.to_string())?;

    let mut response = ureq::post(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .send(payload.as_str())
        .map_err(|err| err.to_string())?;

    response
        .body_mut()
        .read_json::<TResp>()
        .map_err(|err| err.to_string())
}

fn daemon_required_env() -> bool {
    std::env::var("SKILL_DAEMON_REQUIRED")
        .map(|v| {
            let v = v.to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        })
        .unwrap_or(false)
}

pub(crate) fn install_daemon_service() -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let mut resp = ureq::post(&format!("{base_url}/service/install"))
        .send("")
        .map_err(|e| e.to_string())?;
    resp.body_mut()
        .read_json::<serde_json::Value>()
        .map_err(|e| e.to_string())
}

pub(crate) fn uninstall_daemon_service() -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let mut resp = ureq::post(&format!("{base_url}/service/uninstall"))
        .send("")
        .map_err(|e| e.to_string())?;
    resp.body_mut()
        .read_json::<serde_json::Value>()
        .map_err(|e| e.to_string())
}

pub(crate) fn daemon_service_status() -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let mut resp = ureq::get(&format!("{base_url}/service/status"))
        .call()
        .map_err(|e| e.to_string())?;
    resp.body_mut()
        .read_json::<serde_json::Value>()
        .map_err(|e| e.to_string())
}

fn wait_for_protocol_compatibility(timeout: Duration) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    let mut last_err: Option<String> = None;

    while Instant::now() < deadline {
        match load_daemon_token().and_then(|token| fetch_version(&daemon_base_url(), &token)) {
            Ok(v) => return Ok(v.protocol_version == PROTOCOL_VERSION),
            Err(e) => last_err = Some(e),
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    Err(last_err.unwrap_or_else(|| "timed out waiting for daemon version".to_string()))
}

fn restart_daemon_process_best_effort() {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/IM", "skill-daemon.exe", "/F"])
            .output();
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "skill-daemon"])
            .output();
    }
}

pub(crate) fn ensure_daemon_runtime_ready() {
    ensure_daemon_running();

    let mut compatible = false;
    match wait_for_protocol_compatibility(Duration::from_secs(5)) {
        Ok(true) => {
            compatible = true;
        }
        Ok(false) => {
            eprintln!("[daemon] protocol mismatch detected — attempting recovery restart");
            restart_daemon_process_best_effort();
            std::thread::sleep(Duration::from_millis(300));
            ensure_daemon_running();
            match wait_for_protocol_compatibility(Duration::from_secs(5)) {
                Ok(true) => {
                    compatible = true;
                    eprintln!("[daemon] protocol compatibility restored after restart");
                }
                Ok(false) => {
                    eprintln!(
                        "[daemon] protocol still incompatible after restart — attempting rollback binary"
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[daemon] protocol check failed after restart: {e} — attempting rollback binary"
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("[daemon] protocol check unavailable: {e}");
        }
    }

    if !compatible {
        if let Ok(rollback_bin) = daemon_rollback_bin_path() {
            if rollback_bin.exists() {
                eprintln!(
                    "[daemon] attempting rollback daemon: {}",
                    rollback_bin.display()
                );
                restart_daemon_process_best_effort();
                std::thread::sleep(Duration::from_millis(300));

                let prev = std::env::var("SKILL_DAEMON_BIN").ok();
                std::env::set_var("SKILL_DAEMON_BIN", rollback_bin.display().to_string());
                ensure_daemon_running();
                if let Some(v) = prev {
                    std::env::set_var("SKILL_DAEMON_BIN", v);
                } else {
                    std::env::remove_var("SKILL_DAEMON_BIN");
                }

                match wait_for_protocol_compatibility(Duration::from_secs(5)) {
                    Ok(true) => {
                        compatible = true;
                        eprintln!("[daemon] rollback daemon restored compatibility");
                    }
                    Ok(false) => eprintln!("[daemon] rollback daemon still incompatible — continuing degraded"),
                    Err(e) => eprintln!("[daemon] rollback daemon failed readiness check: {e} — continuing degraded"),
                }
            } else {
                eprintln!(
                    "[daemon] no rollback daemon snapshot found at {}",
                    rollback_bin.display()
                );
            }
        }
    }

    if compatible {
        update_daemon_rollback_snapshot_best_effort();
    }

    ensure_daemon_background_service();
}

#[cfg(test)]
fn ensure_daemon_runtime_ready_with_hooks<
    FEnsure,
    FWait,
    FRestart,
    FRollbackExists,
    FRollbackLaunch,
    FSnapshot,
    FService,
>(
    mut ensure_running: FEnsure,
    mut wait: FWait,
    mut restart: FRestart,
    rollback_exists: FRollbackExists,
    mut launch_rollback: FRollbackLaunch,
    mut snapshot: FSnapshot,
    mut service: FService,
) -> bool
where
    FEnsure: FnMut(),
    FWait: FnMut() -> Result<bool, String>,
    FRestart: FnMut(),
    FRollbackExists: Fn() -> bool,
    FRollbackLaunch: FnMut(),
    FSnapshot: FnMut(),
    FService: FnMut(),
{
    ensure_running();

    let mut compatible = false;
    match wait() {
        Ok(true) => compatible = true,
        Ok(false) => {
            restart();
            ensure_running();
            if let Ok(true) = wait() {
                compatible = true;
            }
        }
        Err(_) => {}
    }

    if !compatible && rollback_exists() {
        restart();
        launch_rollback();
        if let Ok(true) = wait() {
            compatible = true;
        }
    }

    if compatible {
        snapshot();
    }
    service();
    compatible
}

pub(crate) fn ensure_daemon_background_service() {
    let autoinstall_enabled = std::env::var("SKILL_DAEMON_SERVICE_AUTOINSTALL")
        .map(|v| {
            let v = v.to_ascii_lowercase();
            !(v == "0" || v == "false" || v == "no" || v == "off")
        })
        .unwrap_or(true);

    if !autoinstall_enabled {
        eprintln!("[daemon] background service auto-install disabled by env");
        return;
    }

    let status = match daemon_service_status() {
        Ok(v) => v
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string(),
        Err(e) => {
            eprintln!("[daemon] service status unavailable: {e}");
            return;
        }
    };

    match status.as_str() {
        "running" => {
            // already good
        }
        "not_installed" | "stopped" => match install_daemon_service() {
            Ok(_) => eprintln!("[daemon] background service installed/enabled ({status})"),
            Err(e) => eprintln!("[daemon] background service install failed: {e}"),
        },
        _ => {
            eprintln!("[daemon] background service status: {status}");
        }
    }
}

fn daemon_base_url() -> String {
    let addr = std::env::var("SKILL_DAEMON_ADDR").unwrap_or_else(|_| "127.0.0.1:18444".to_string());
    format!("http://{addr}")
}

fn load_daemon_token() -> Result<String, String> {
    let path = token_path().map_err(|err| err.to_string())?;
    let token = std::fs::read_to_string(path)
        .map_err(|err| err.to_string())?
        .trim()
        .to_string();

    if token.is_empty() {
        return Err("daemon auth token is empty".to_string());
    }

    Ok(token)
}

fn token_path() -> Result<PathBuf, String> {
    let base =
        dirs::config_dir().ok_or_else(|| "unable to resolve config directory".to_string())?;
    Ok(base.join("skill").join("daemon").join("auth.token"))
}

// ── EXG model daemon proxies ──────────────────────────────────────────────────
//
// The webview cannot always reach the daemon over HTTP (macOS WKWebView
// restrictions, ATS, etc.).  These Tauri commands proxy the requests through
// native `ureq` calls so the webview never needs direct network access.
//
// All proxies are async + spawn_blocking to avoid blocking the Tauri main thread.

/// Ensure daemon is reachable, restarting it if necessary.
fn ensure_daemon_for_proxy() {
    let addr: std::net::SocketAddr = std::env::var("SKILL_DAEMON_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:18444".to_string())
        .parse()
        .unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], 18444)));
    if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
        return;
    }
    eprintln!("[proxy] daemon unreachable, attempting restart…");
    ensure_daemon_running();
}

/// Blocking GET helper used inside spawn_blocking.
pub(crate) fn daemon_get(path: &str) -> Result<serde_json::Value, String> {
    ensure_daemon_for_proxy();
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, path)
}

/// Blocking POST helper used inside spawn_blocking.
pub(crate) fn daemon_post(
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    ensure_daemon_for_proxy();
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(&base_url, &token, path, body)
}

async fn daemon_get_async(path: &'static str) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || daemon_get(path))
        .await
        .map_err(|e| e.to_string())?
}

async fn daemon_post_async(
    path: &'static str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || daemon_post(path, &body))
        .await
        .map_err(|e| e.to_string())?
}

// ── EXG model proxies ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_exg_catalog() -> Result<serde_json::Value, String> {
    daemon_get_async("/v1/models/exg-catalog").await
}

#[tauri::command]
pub async fn get_eeg_model_config() -> Result<serde_json::Value, String> {
    daemon_get_async("/v1/models/config").await
}

#[tauri::command]
pub async fn get_eeg_model_status() -> Result<serde_json::Value, String> {
    daemon_get_async("/v1/models/status").await
}

#[tauri::command]
pub async fn set_eeg_model_config(config: serde_json::Value) -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/models/config", config).await
}

#[tauri::command]
pub async fn trigger_weights_download() -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/models/trigger-weights-download", serde_json::json!({})).await
}

#[tauri::command]
pub async fn cancel_weights_download() -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/models/cancel-weights-download", serde_json::json!({})).await
}

#[tauri::command]
pub async fn estimate_reembed() -> Result<serde_json::Value, String> {
    daemon_get_async("/v1/models/estimate-reembed").await
}

#[tauri::command]
pub async fn trigger_reembed() -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/models/trigger-reembed", serde_json::json!({})).await
}

// ── LSL proxies ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn lsl_discover() -> Result<serde_json::Value, String> {
    daemon_get_async("/v1/lsl/discover").await
}

#[tauri::command]
pub async fn lsl_get_config() -> Result<serde_json::Value, String> {
    daemon_get_async("/v1/lsl/config").await
}

#[tauri::command]
pub async fn lsl_set_auto_connect(enabled: bool) -> Result<serde_json::Value, String> {
    daemon_post_async(
        "/v1/lsl/auto-connect",
        serde_json::json!({"enabled": enabled}),
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn lsl_pair_stream(
    source_id: String,
    name: String,
    stream_type: String,
    channels: u32,
    sample_rate: f64,
) -> Result<serde_json::Value, String> {
    daemon_post_async(
        "/v1/lsl/pair",
        serde_json::json!({
            "source_id": source_id, "name": name, "stream_type": stream_type,
            "channels": channels, "sample_rate": sample_rate
        }),
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn lsl_unpair_stream(source_id: String) -> Result<serde_json::Value, String> {
    daemon_post_async(
        "/v1/lsl/unpair",
        serde_json::json!({"source_id": source_id}),
    )
    .await
}

#[tauri::command]
pub async fn lsl_get_idle_timeout() -> Result<serde_json::Value, String> {
    daemon_get_async("/v1/lsl/idle-timeout").await
}

#[tauri::command]
pub async fn lsl_set_idle_timeout(secs: serde_json::Value) -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/lsl/idle-timeout", serde_json::json!({"secs": secs})).await
}

#[tauri::command]
pub async fn lsl_virtual_source_running() -> Result<serde_json::Value, String> {
    daemon_get_async("/v1/lsl/virtual-source/running").await
}

#[tauri::command]
pub async fn lsl_virtual_source_start() -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/lsl/virtual-source/start", serde_json::json!({})).await
}

/// Start the virtual EEG source with explicit signal configuration.
/// The body is forwarded verbatim to the daemon so it can honour whatever
/// fields it supports; unknown fields are silently ignored.
#[tauri::command(rename_all = "snake_case")]
#[allow(clippy::too_many_arguments)]
pub async fn lsl_virtual_source_start_configured(
    channels: u32,
    sample_rate: u32,
    template: String,
    quality: String,
    amplitude_uv: f64,
    noise_uv: f64,
    line_noise: String,
    dropout_prob: f64,
) -> Result<serde_json::Value, String> {
    daemon_post_async(
        "/v1/lsl/virtual-source/start",
        serde_json::json!({
            "channels":     channels,
            "sample_rate":  sample_rate,
            "template":     template,
            "quality":      quality,
            "amplitude_uv": amplitude_uv,
            "noise_uv":     noise_uv,
            "line_noise":   line_noise,
            "dropout_prob": dropout_prob,
        }),
    )
    .await
}

#[tauri::command]
pub async fn lsl_virtual_source_stop() -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/lsl/virtual-source/stop", serde_json::json!({})).await
}

#[cfg(test)]
fn daemon_cmds_test_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    #[derive(Clone)]
    struct ExpectedReq {
        method: &'static str,
        path: &'static str,
        response_json: String,
    }

    fn read_http_request(stream: &mut TcpStream) -> (String, String, String, String) {
        let mut buf = Vec::<u8>::new();
        let mut tmp = [0_u8; 4096];
        let mut header_end = None;

        while header_end.is_none() {
            let n = stream.read(&mut tmp).expect("read request bytes");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = Some(i + 4);
            }
        }

        let header_end = header_end.unwrap_or(buf.len());
        let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let mut lines = head.lines();
        let first = lines.next().unwrap_or("");
        let mut parts = first.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").to_string();

        let content_len = head
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                if k.eq_ignore_ascii_case("content-length") {
                    return v.trim().parse::<usize>().ok();
                }
                None
            })
            .unwrap_or(0);

        let mut body_bytes = if buf.len() > header_end {
            buf[header_end..].to_vec()
        } else {
            Vec::new()
        };
        while body_bytes.len() < content_len {
            let n = stream.read(&mut tmp).expect("read request body");
            if n == 0 {
                break;
            }
            body_bytes.extend_from_slice(&tmp[..n]);
        }

        (
            method,
            path,
            head,
            String::from_utf8_lossy(&body_bytes).to_string(),
        )
    }

    fn write_json_response(stream: &mut TcpStream, status: &str, body: &str) {
        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(resp.as_bytes()).expect("write response");
        let _ = stream.flush();
    }

    #[test]
    fn tauri_daemon_http_contract_smoke() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        std::env::set_var("SKILL_DAEMON_ADDR", addr.to_string());

        let token_path = token_path().expect("token path");
        if let Some(parent) = token_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&token_path, "test-token\n").unwrap();

        let status_json = serde_json::to_string(&StatusResponse::default()).unwrap();
        let expected = vec![
            ExpectedReq {
                method: "GET",
                path: "/v1/status",
                response_json: status_json,
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/history/sessions",
                response_json: "[]".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/settings/update-check-interval",
                response_json: "{\"value\":123}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/settings/update-check-interval",
                response_json: "{\"value\":123}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/activity/recent-windows",
                response_json: "[]".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/activity/recent-input",
                response_json: "[]".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/activity/input-buckets",
                response_json: "[]".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/hooks",
                response_json: "[]".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/llm/downloads",
                response_json: "[]".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/selection/active-model",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/control/scanner/wifi-config",
                response_json: "{\"wifi_shield_ip\":\"1.2.3.4\",\"galea_ip\":\"\"}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/lsl/config",
                response_json: "{\"auto_connect\":false,\"paired_streams\":[]}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/lsl/idle-timeout",
                response_json: "{\"secs\":77}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/analysis/day-metrics",
                response_json: "{\"ok\":true}".into(),
            },
        ];

        let server = std::thread::spawn(move || {
            let mut i = 0usize;
            while i < expected.len() {
                let (mut stream, _) = listener.accept().expect("accept");
                let (method, path, head, _body) = read_http_request(&mut stream);
                if method.is_empty() {
                    continue;
                }
                let e = &expected[i];
                assert_eq!(method, e.method);
                assert_eq!(path, e.path);
                assert!(
                    head.to_ascii_lowercase()
                        .contains("authorization: bearer test-token"),
                    "missing bearer header: {head}"
                );
                write_json_response(&mut stream, "200 OK", &e.response_json);
                i += 1;
            }
        });

        let _ = fetch_daemon_status().expect("status");
        let _ = fetch_history_sessions().expect("history");
        let _ = set_update_check_interval(123).expect("set interval");
        let _ = fetch_update_check_interval().expect("get interval");
        let _ = fetch_recent_active_windows(Some(5)).expect("recent windows");
        let _ = fetch_recent_input_activity(Some(5)).expect("recent input");
        let _ = fetch_input_buckets(Some(1), Some(2)).expect("input buckets");
        let _ = fetch_hooks().expect("hooks");
        let _ = llm_get_downloads().expect("downloads");
        llm_set_active_model("model.gguf".into()).expect("active model");
        let _ = scanner_set_wifi_config("1.2.3.4".into(), "".into()).expect("wifi cfg");
        let _ = fetch_lsl_config().expect("lsl cfg");
        let _ = get_lsl_idle_timeout().expect("lsl timeout");
        let _ = post_json_value_with_auth(
            "/v1/analysis/day-metrics",
            &serde_json::json!({"day":"20260101"}),
        )
        .expect("analysis post");

        server.join().unwrap();
    }

    #[test]
    fn tauri_daemon_http_contract_control_and_llm_routes() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        std::env::set_var("SKILL_DAEMON_ADDR", addr.to_string());

        let token_path = token_path().expect("token path");
        if let Some(parent) = token_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&token_path, "test-token\n").unwrap();

        let status_json = serde_json::to_string(&StatusResponse::default()).unwrap();
        let expected = vec![
            ExpectedReq {
                method: "GET",
                path: "/v1/ws-port",
                response_json: "{\"port\":18444}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/log/recent?since=7",
                response_json: "{\"next_seq\":9,\"lines\":[\"8\\tline\"]}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/control/retry-connect",
                response_json: status_json.clone(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/control/cancel-retry",
                response_json: status_json.clone(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/control/start-session",
                response_json: status_json.clone(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/control/cancel-session",
                response_json: status_json.clone(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/control/scanner/start",
                response_json: "{\"running\":true}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/control/scanner/stop",
                response_json: "{\"running\":false}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/control/scanner/state",
                response_json: "{\"running\":false}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/settings/notch-preset",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/settings/location-test",
                response_json: "{\"ok\":true}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/ui/accent-color",
                response_json: "{\"value\":\"blue\"}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/ui/accent-color",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/devices/set-preferred",
                response_json: "[]".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/devices/forget",
                response_json: "[]".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/skills/refresh-interval",
                response_json: "{\"value\":90}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/skills/sync-on-launch",
                response_json: "{\"value\":true}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/skills/disabled",
                response_json: "{\"value\":[\"x\"]}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/history/find-session",
                response_json: "{\"csv_path\":\"/tmp/a.csv\"}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/server/start",
                response_json: "{\"result\":\"starting\"}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/llm/server/status",
                response_json: "{\"running\":true}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/llm/server/logs",
                response_json: "[]".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/server/switch-model",
                response_json: "{\"result\":\"switching\"}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/server/switch-mmproj",
                response_json: "{\"result\":\"switching\"}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/server/stop",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/catalog/refresh",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/catalog/add-model",
                response_json: "{\"filename\":\"model.gguf\"}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/downloads/pause",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/selection/active-mmproj",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/selection/autoload-mmproj",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/chat-completions",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/abort-stream",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/llm/cancel-tool-call",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/v1/some/custom",
                response_json: "{}".into(),
            },
            ExpectedReq {
                method: "GET",
                path: "/v1/version",
                response_json: "{\"ok\":true}".into(),
            },
        ];

        let server = std::thread::spawn(move || {
            let mut i = 0usize;
            while i < expected.len() {
                let (mut stream, _) = listener.accept().expect("accept");
                let (method, path, head, _body) = read_http_request(&mut stream);
                if method.is_empty() {
                    continue;
                }
                let e = &expected[i];
                assert_eq!(method, e.method);
                assert_eq!(path, e.path);
                assert!(
                    head.to_ascii_lowercase()
                        .contains("authorization: bearer test-token"),
                    "missing bearer header: {head}"
                );
                write_json_response(&mut stream, "200 OK", &e.response_json);
                i += 1;
            }
        });

        assert_eq!(fetch_daemon_ws_port().unwrap(), 18444);
        let (next, lines) = fetch_daemon_log_recent(7).unwrap();
        assert_eq!(next, 9);
        assert_eq!(lines.len(), 1);
        let _ = retry_connect().unwrap();
        let _ = cancel_retry().unwrap();
        let _ = start_session_sync(Some("muse".into())).unwrap();
        let _ = cancel_session_sync().unwrap();
        let _ = scanner_start().unwrap();
        let _ = scanner_stop().unwrap();
        let _ = scanner_state().unwrap();
        set_notch_preset(None).unwrap();
        let _ = test_location().unwrap();
        assert_eq!(fetch_accent_color().unwrap(), "blue");
        set_accent_color("teal".into()).unwrap();
        let _ = set_preferred_device("d1".into()).unwrap();
        let _ = forget_device("d1".into()).unwrap();
        assert_eq!(fetch_skills_refresh_interval().unwrap(), 90);
        assert!(fetch_skills_sync_on_launch().unwrap());
        assert_eq!(get_disabled_skills().unwrap(), vec!["x".to_string()]);
        assert_eq!(
            find_history_session(123).unwrap().as_deref(),
            Some("/tmp/a.csv")
        );
        assert_eq!(llm_server_start().unwrap(), "starting");
        let _ = llm_server_status().unwrap();
        let _ = llm_server_logs().unwrap();
        assert_eq!(
            llm_server_switch_model("model.gguf".into()).unwrap(),
            "switching"
        );
        assert_eq!(
            llm_server_switch_mmproj("mmproj.gguf".into()).unwrap(),
            "switching"
        );
        llm_server_stop().unwrap();
        llm_refresh_catalog().unwrap();
        assert_eq!(
            llm_add_model(
                "a/b".into(),
                "model.gguf".into(),
                Some(1.0),
                None,
                Some(false)
            )
            .unwrap(),
            "model.gguf"
        );
        llm_download_action("/v1/llm/downloads/pause", "model.gguf".into()).unwrap();
        llm_set_active_mmproj("mmproj.gguf".into()).unwrap();
        llm_set_autoload_mmproj(true).unwrap();
        let _ = llm_chat_completions(vec![], serde_json::json!({})).unwrap();
        llm_abort_stream().unwrap();
        llm_cancel_tool_call("tc-1".into()).unwrap();
        post_json_with_auth_pub("/v1/some/custom", &serde_json::json!({"x":1})).unwrap();
        let _ = fetch_json_value_with_auth("/v1/version").unwrap();

        server.join().unwrap();
    }

    #[test]
    fn service_autoinstall_installs_when_not_installed() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));
        std::env::remove_var("SKILL_DAEMON_SERVICE_AUTOINSTALL");

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        std::env::set_var("SKILL_DAEMON_ADDR", addr.to_string());

        let expected = vec![
            ExpectedReq {
                method: "GET",
                path: "/service/status",
                response_json: "{\"status\":\"not_installed\"}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/service/install",
                response_json: "{\"ok\":true}".into(),
            },
        ];

        let server = std::thread::spawn(move || {
            let mut i = 0usize;
            while i < expected.len() {
                let (mut stream, _) = listener.accept().expect("accept");
                let (method, path, _head, _body) = read_http_request(&mut stream);
                if method.is_empty() {
                    continue;
                }
                let e = &expected[i];
                assert_eq!(method, e.method);
                assert_eq!(path, e.path);
                write_json_response(&mut stream, "200 OK", &e.response_json);
                i += 1;
            }
        });

        ensure_daemon_background_service();
        server.join().unwrap();
    }

    #[test]
    fn service_autoinstall_skips_when_running() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));
        std::env::remove_var("SKILL_DAEMON_SERVICE_AUTOINSTALL");

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        std::env::set_var("SKILL_DAEMON_ADDR", addr.to_string());

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let (method, path, _head, _body) = read_http_request(&mut stream);
            assert_eq!(method, "GET");
            assert_eq!(path, "/service/status");
            write_json_response(&mut stream, "200 OK", "{\"status\":\"running\"}");
        });

        ensure_daemon_background_service();
        server.join().unwrap();
    }

    #[test]
    fn service_autoinstall_installs_when_stopped() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));
        std::env::remove_var("SKILL_DAEMON_SERVICE_AUTOINSTALL");

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        std::env::set_var("SKILL_DAEMON_ADDR", addr.to_string());

        let expected = vec![
            ExpectedReq {
                method: "GET",
                path: "/service/status",
                response_json: "{\"status\":\"stopped\"}".into(),
            },
            ExpectedReq {
                method: "POST",
                path: "/service/install",
                response_json: "{\"ok\":true}".into(),
            },
        ];

        let server = std::thread::spawn(move || {
            let mut i = 0usize;
            while i < expected.len() {
                let (mut stream, _) = listener.accept().expect("accept");
                let (method, path, _head, _body) = read_http_request(&mut stream);
                if method.is_empty() {
                    continue;
                }
                let e = &expected[i];
                assert_eq!(method, e.method);
                assert_eq!(path, e.path);
                write_json_response(&mut stream, "200 OK", &e.response_json);
                i += 1;
            }
        });

        ensure_daemon_background_service();
        server.join().unwrap();
    }

    #[test]
    fn protocol_gate_reports_true_when_versions_match() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));

        let token_path = token_path().expect("token path");
        if let Some(parent) = token_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&token_path, "test-token\n").unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        std::env::set_var("SKILL_DAEMON_ADDR", addr.to_string());

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let (method, path, head, _body) = read_http_request(&mut stream);
            assert_eq!(method, "GET");
            assert_eq!(path, "/v1/version");
            assert!(head
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token"));
            write_json_response(
                &mut stream,
                "200 OK",
                &format!(
                    "{{\"daemon\":\"skill-daemon\",\"protocol_version\":{},\"daemon_version\":\"0.0.87\"}}",
                    PROTOCOL_VERSION
                ),
            );
        });

        assert!(wait_for_protocol_compatibility(Duration::from_secs(1)).unwrap());
        server.join().unwrap();
    }

    #[test]
    fn protocol_gate_reports_false_on_version_mismatch() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));

        let token_path = token_path().expect("token path");
        if let Some(parent) = token_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&token_path, "test-token\n").unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        std::env::set_var("SKILL_DAEMON_ADDR", addr.to_string());

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let (method, path, head, _body) = read_http_request(&mut stream);
            assert_eq!(method, "GET");
            assert_eq!(path, "/v1/version");
            assert!(head
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token"));
            write_json_response(
                &mut stream,
                "200 OK",
                &format!(
                    "{{\"daemon\":\"skill-daemon\",\"protocol_version\":{},\"daemon_version\":\"0.0.10\"}}",
                    PROTOCOL_VERSION + 1
                ),
            );
        });

        assert!(!wait_for_protocol_compatibility(Duration::from_secs(1)).unwrap());
        server.join().unwrap();
    }

    #[test]
    fn runtime_ready_attempts_rollback_after_restart_mismatch() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let mut waits = std::collections::VecDeque::from([
            Ok(false), // initial check: mismatch
            Ok(false), // after restart: still mismatch
            Ok(true),  // after rollback launch: compatible
        ]);

        let mut ensure_count = 0usize;
        let mut restart_count = 0usize;
        let mut rollback_launch_count = 0usize;
        let mut snapshot_count = 0usize;
        let mut service_count = 0usize;

        let compatible = ensure_daemon_runtime_ready_with_hooks(
            || ensure_count += 1,
            || waits.pop_front().unwrap_or(Err("no wait result".into())),
            || restart_count += 1,
            || true,
            || rollback_launch_count += 1,
            || snapshot_count += 1,
            || service_count += 1,
        );

        assert!(compatible);
        assert_eq!(ensure_count, 2, "initial ensure + post-restart ensure");
        assert_eq!(restart_count, 2, "restart path + rollback path");
        assert_eq!(
            rollback_launch_count, 1,
            "rollback should be launched exactly once"
        );
        assert_eq!(
            snapshot_count, 1,
            "compatible runtime should refresh rollback snapshot"
        );
        assert_eq!(
            service_count, 1,
            "background service repair should always run"
        );
    }

    #[test]
    fn runtime_ready_degraded_when_wait_errors_and_no_rollback() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let mut waits = std::collections::VecDeque::from([
            Err("unreachable".to_string()),
            Err("still unreachable".to_string()),
        ]);

        let mut ensure_count = 0usize;
        let mut restart_count = 0usize;
        let mut rollback_launch_count = 0usize;
        let mut snapshot_count = 0usize;
        let mut service_count = 0usize;

        let compatible = ensure_daemon_runtime_ready_with_hooks(
            || ensure_count += 1,
            || waits.pop_front().unwrap_or(Err("no wait result".into())),
            || restart_count += 1,
            || false,
            || rollback_launch_count += 1,
            || snapshot_count += 1,
            || service_count += 1,
        );

        assert!(!compatible);
        assert_eq!(ensure_count, 1, "no restart when initial wait errors");
        assert_eq!(restart_count, 0);
        assert_eq!(rollback_launch_count, 0);
        assert_eq!(snapshot_count, 0, "no snapshot refresh on degraded startup");
        assert_eq!(service_count, 1);
    }

    #[test]
    fn runtime_ready_happy_path_no_restart_or_rollback() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let mut waits = std::collections::VecDeque::from([Ok(true)]);

        let mut ensure_count = 0usize;
        let mut restart_count = 0usize;
        let mut rollback_launch_count = 0usize;
        let mut snapshot_count = 0usize;
        let mut service_count = 0usize;

        let compatible = ensure_daemon_runtime_ready_with_hooks(
            || ensure_count += 1,
            || waits.pop_front().unwrap_or(Err("no wait result".into())),
            || restart_count += 1,
            || true,
            || rollback_launch_count += 1,
            || snapshot_count += 1,
            || service_count += 1,
        );

        assert!(compatible);
        assert_eq!(ensure_count, 1);
        assert_eq!(restart_count, 0);
        assert_eq!(rollback_launch_count, 0);
        assert_eq!(snapshot_count, 1);
        assert_eq!(service_count, 1);
    }

    #[test]
    fn runtime_ready_recovers_after_single_restart_without_rollback() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let mut waits = std::collections::VecDeque::from([Ok(false), Ok(true)]);

        let mut ensure_count = 0usize;
        let mut restart_count = 0usize;
        let mut rollback_launch_count = 0usize;
        let mut snapshot_count = 0usize;
        let mut service_count = 0usize;

        let compatible = ensure_daemon_runtime_ready_with_hooks(
            || ensure_count += 1,
            || waits.pop_front().unwrap_or(Err("no wait result".into())),
            || restart_count += 1,
            || true,
            || rollback_launch_count += 1,
            || snapshot_count += 1,
            || service_count += 1,
        );

        assert!(compatible);
        assert_eq!(ensure_count, 2);
        assert_eq!(restart_count, 1);
        assert_eq!(rollback_launch_count, 0);
        assert_eq!(snapshot_count, 1);
        assert_eq!(service_count, 1);
    }

    #[test]
    fn service_autoinstall_disabled_by_env_is_noop() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();
        std::env::set_var("SKILL_DAEMON_SERVICE_AUTOINSTALL", "0");
        std::env::set_var("SKILL_DAEMON_ADDR", "127.0.0.1:9");

        ensure_daemon_background_service();

        std::env::remove_var("SKILL_DAEMON_SERVICE_AUTOINSTALL");
    }

    #[test]
    fn rollback_snapshot_copies_current_daemon_bin() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));

        let fake_bin = td.path().join(if cfg!(target_os = "windows") {
            "skill-daemon.exe"
        } else {
            "skill-daemon"
        });
        std::fs::write(&fake_bin, b"daemon-binary").unwrap();
        std::env::set_var("SKILL_DAEMON_BIN", &fake_bin);

        update_daemon_rollback_snapshot_best_effort();

        let rollback = daemon_rollback_bin_path().unwrap();
        assert!(rollback.exists(), "rollback snapshot should exist");

        let src = std::fs::read(&fake_bin).unwrap();
        let dst = std::fs::read(&rollback).unwrap();
        assert_eq!(src, dst);

        std::env::remove_var("SKILL_DAEMON_BIN");
    }

    #[test]
    fn runtime_ready_fails_when_rollback_also_incompatible() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let mut waits = std::collections::VecDeque::from([
            Ok(false), // initial mismatch
            Ok(false), // after restart mismatch
            Ok(false), // rollback also mismatch
        ]);

        let mut ensure_count = 0usize;
        let mut restart_count = 0usize;
        let mut rollback_launch_count = 0usize;
        let mut snapshot_count = 0usize;
        let mut service_count = 0usize;

        let compatible = ensure_daemon_runtime_ready_with_hooks(
            || ensure_count += 1,
            || waits.pop_front().unwrap_or(Err("no wait result".into())),
            || restart_count += 1,
            || true,
            || rollback_launch_count += 1,
            || snapshot_count += 1,
            || service_count += 1,
        );

        assert!(!compatible);
        assert_eq!(ensure_count, 2);
        assert_eq!(restart_count, 2);
        assert_eq!(rollback_launch_count, 1);
        assert_eq!(
            snapshot_count, 0,
            "no snapshot update on incompatible runtime"
        );
        assert_eq!(service_count, 1);
    }

    #[test]
    fn rollback_snapshot_noop_when_source_missing() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));

        let fake_missing = td.path().join(if cfg!(target_os = "windows") {
            "missing-daemon.exe"
        } else {
            "missing-daemon"
        });
        std::env::set_var("SKILL_DAEMON_BIN", &fake_missing);

        let rollback = daemon_rollback_bin_path().unwrap();
        if rollback.exists() {
            let _ = std::fs::remove_file(&rollback);
        }

        update_daemon_rollback_snapshot_best_effort();

        assert!(
            !rollback.exists(),
            "rollback snapshot should not be created when source binary is missing"
        );

        std::env::remove_var("SKILL_DAEMON_BIN");
    }

    #[test]
    fn runtime_ready_uses_rollback_when_initial_probe_errors() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let mut waits = std::collections::VecDeque::from([
            Err("initial probe failed".to_string()),
            Ok(true), // rollback probe succeeds
        ]);

        let mut ensure_count = 0usize;
        let mut restart_count = 0usize;
        let mut rollback_launch_count = 0usize;
        let mut snapshot_count = 0usize;
        let mut service_count = 0usize;

        let compatible = ensure_daemon_runtime_ready_with_hooks(
            || ensure_count += 1,
            || waits.pop_front().unwrap_or(Err("no wait result".into())),
            || restart_count += 1,
            || true,
            || rollback_launch_count += 1,
            || snapshot_count += 1,
            || service_count += 1,
        );

        assert!(compatible);
        assert_eq!(
            ensure_count, 1,
            "no normal restart ensure pass on probe error"
        );
        assert_eq!(
            restart_count, 1,
            "rollback branch performs one restart attempt"
        );
        assert_eq!(rollback_launch_count, 1);
        assert_eq!(snapshot_count, 1);
        assert_eq!(service_count, 1);
    }

    #[test]
    fn service_autoinstall_unknown_status_does_not_install() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));
        std::env::remove_var("SKILL_DAEMON_SERVICE_AUTOINSTALL");

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        std::env::set_var("SKILL_DAEMON_ADDR", addr.to_string());

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let (method, path, _head, _body) = read_http_request(&mut stream);
            assert_eq!(method, "GET");
            assert_eq!(path, "/service/status");
            write_json_response(&mut stream, "200 OK", "{\"status\":\"unknown\"}");
        });

        ensure_daemon_background_service();
        server.join().unwrap();
    }

    #[test]
    fn daemon_required_env_parsing() {
        std::env::remove_var("SKILL_DAEMON_REQUIRED");
        assert!(!daemon_required_env());

        std::env::set_var("SKILL_DAEMON_REQUIRED", "true");
        assert!(daemon_required_env());

        std::env::set_var("SKILL_DAEMON_REQUIRED", "1");
        assert!(daemon_required_env());

        std::env::set_var("SKILL_DAEMON_REQUIRED", "off");
        assert!(!daemon_required_env());

        std::env::remove_var("SKILL_DAEMON_REQUIRED");
    }

    #[test]
    fn runtime_ready_degraded_after_restart_error_without_rollback() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let mut waits = std::collections::VecDeque::from([
            Ok(false), // initial mismatch
            Err("restart probe failed".to_string()),
        ]);

        let mut ensure_count = 0usize;
        let mut restart_count = 0usize;
        let mut rollback_launch_count = 0usize;
        let mut snapshot_count = 0usize;
        let mut service_count = 0usize;

        let compatible = ensure_daemon_runtime_ready_with_hooks(
            || ensure_count += 1,
            || waits.pop_front().unwrap_or(Err("no wait result".into())),
            || restart_count += 1,
            || false,
            || rollback_launch_count += 1,
            || snapshot_count += 1,
            || service_count += 1,
        );

        assert!(!compatible);
        assert_eq!(ensure_count, 2);
        assert_eq!(restart_count, 1);
        assert_eq!(rollback_launch_count, 0);
        assert_eq!(snapshot_count, 0);
        assert_eq!(service_count, 1);
    }

    #[test]
    fn wait_for_protocol_compatibility_times_out_without_token() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));
        std::env::set_var("SKILL_DAEMON_ADDR", "127.0.0.1:9");

        let err = wait_for_protocol_compatibility(Duration::from_millis(1)).unwrap_err();
        assert!(
            err.contains("timed out")
                || err.contains("token")
                || err.contains("No such file")
                || err.contains("os error"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn service_autoinstall_env_parsing_variants() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        std::env::set_var("SKILL_DAEMON_SERVICE_AUTOINSTALL", "false");
        ensure_daemon_background_service();

        std::env::set_var("SKILL_DAEMON_SERVICE_AUTOINSTALL", "no");
        ensure_daemon_background_service();

        std::env::set_var("SKILL_DAEMON_SERVICE_AUTOINSTALL", "off");
        ensure_daemon_background_service();

        std::env::remove_var("SKILL_DAEMON_SERVICE_AUTOINSTALL");
    }

    #[test]
    fn async_proxy_helpers_are_used_for_v1_routes() {
        let src = include_str!("daemon_cmds.rs");
        assert!(src.contains("async fn daemon_get_async"));
        assert!(src.contains("async fn daemon_post_async"));

        // Guard against regressions back to duplicated per-route spawn_blocking wrappers.
        assert!(!src.contains("spawn_blocking(|| daemon_get(\"/v1/"));
        assert!(!src.contains("spawn_blocking(move || daemon_post(\"/v1/"));
    }
}

#[tauri::command]
pub async fn lsl_iroh_start() -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/lsl/iroh/start", serde_json::json!({})).await
}

#[tauri::command]
pub async fn lsl_iroh_stop() -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/lsl/iroh/stop", serde_json::json!({})).await
}

#[tauri::command]
pub async fn lsl_iroh_status() -> Result<serde_json::Value, String> {
    daemon_get_async("/v1/lsl/iroh/status").await
}

// ── Session control proxies ─────────────────────────────────────────────────

#[tauri::command]
pub async fn switch_session(target: String) -> Result<serde_json::Value, String> {
    daemon_post_async(
        "/v1/control/switch-session",
        serde_json::json!({"target": target}),
    )
    .await
}

#[tauri::command]
pub async fn cancel_session() -> Result<serde_json::Value, String> {
    daemon_post_async("/v1/control/cancel-session", serde_json::json!({})).await
}

#[cfg(test)]
mod async_contract_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    fn read_req(stream: &mut TcpStream) -> (String, String, String) {
        let mut buf = Vec::<u8>::new();
        let mut tmp = [0_u8; 4096];
        let mut header_end = None;
        while header_end.is_none() {
            let n = stream.read(&mut tmp).expect("read request bytes");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = Some(i + 4);
            }
        }
        let header_end = header_end.unwrap_or(buf.len());
        let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let mut parts = head.lines().next().unwrap_or("").split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").to_string();
        (method, path, head)
    }

    fn write_json(stream: &mut TcpStream, body: &str) {
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(resp.as_bytes()).expect("write response");
    }

    #[test]
    fn tauri_daemon_http_contract_async_proxy_routes() {
        let _guard = daemon_cmds_test_lock().lock().unwrap();

        let td = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", td.path());
        std::env::set_var("XDG_CONFIG_HOME", td.path().join(".config"));
        std::env::set_var("APPDATA", td.path().join("AppData/Roaming"));

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().unwrap();
        std::env::set_var("SKILL_DAEMON_ADDR", addr.to_string());

        let token_path = token_path().expect("token path");
        if let Some(parent) = token_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&token_path, "test-token\n").unwrap();

        let expected: Vec<(&str, &str, &str)> = vec![
            ("GET", "/v1/models/exg-catalog", "{}"),
            ("GET", "/v1/models/config", "{}"),
            ("GET", "/v1/models/status", "{}"),
            ("POST", "/v1/models/config", "{}"),
            ("POST", "/v1/models/trigger-weights-download", "{}"),
            ("POST", "/v1/models/cancel-weights-download", "{}"),
            ("GET", "/v1/models/estimate-reembed", "{}"),
            ("POST", "/v1/models/trigger-reembed", "{}"),
            ("GET", "/v1/lsl/discover", "{}"),
            ("GET", "/v1/lsl/config", "{}"),
            ("POST", "/v1/lsl/auto-connect", "{}"),
            ("POST", "/v1/lsl/pair", "{}"),
            ("POST", "/v1/lsl/unpair", "{}"),
            ("GET", "/v1/lsl/idle-timeout", "{\"secs\":12}"),
            ("POST", "/v1/lsl/idle-timeout", "{}"),
            (
                "GET",
                "/v1/lsl/virtual-source/running",
                "{\"running\":false}",
            ),
            ("POST", "/v1/lsl/virtual-source/start", "{}"),
            ("POST", "/v1/lsl/virtual-source/start", "{}"),
            ("POST", "/v1/lsl/virtual-source/stop", "{}"),
            ("POST", "/v1/lsl/iroh/start", "{}"),
            ("POST", "/v1/lsl/iroh/stop", "{}"),
            ("GET", "/v1/lsl/iroh/status", "{}"),
            ("POST", "/v1/control/switch-session", "{}"),
            ("POST", "/v1/control/cancel-session", "{}"),
        ];

        let server = std::thread::spawn(move || {
            let mut i = 0usize;
            while i < expected.len() {
                let (mut stream, _) = listener.accept().expect("accept");
                let (method, path, head) = read_req(&mut stream);
                // allow bare TCP readiness probes from ensure_daemon_for_proxy()
                if method.is_empty() {
                    continue;
                }
                let (m, p, body) = expected[i];
                assert_eq!(method, m);
                assert_eq!(path, p);
                assert!(
                    head.to_ascii_lowercase()
                        .contains("authorization: bearer test-token"),
                    "missing bearer header: {head}"
                );
                write_json(&mut stream, body);
                i += 1;
            }
        });

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = get_exg_catalog().await.unwrap();
            let _ = get_eeg_model_config().await.unwrap();
            let _ = get_eeg_model_status().await.unwrap();
            let _ = set_eeg_model_config(serde_json::json!({})).await.unwrap();
            let _ = trigger_weights_download().await.unwrap();
            let _ = cancel_weights_download().await.unwrap();
            let _ = estimate_reembed().await.unwrap();
            let _ = trigger_reembed().await.unwrap();
            let _ = lsl_discover().await.unwrap();
            let _ = lsl_get_config().await.unwrap();
            let _ = lsl_set_auto_connect(true).await.unwrap();
            let _ = lsl_pair_stream("s1".into(), "n".into(), "EEG".into(), 8, 256.0)
                .await
                .unwrap();
            let _ = lsl_unpair_stream("s1".into()).await.unwrap();
            let _ = lsl_get_idle_timeout().await.unwrap();
            let _ = lsl_set_idle_timeout(serde_json::json!(12)).await.unwrap();
            let _ = lsl_virtual_source_running().await.unwrap();
            let _ = lsl_virtual_source_start().await.unwrap();
            let _ = lsl_virtual_source_start_configured(
                8,
                256,
                "GoodQuality".into(),
                "good".into(),
                20.0,
                1.0,
                "50hz".into(),
                0.0,
            )
            .await
            .unwrap();
            let _ = lsl_virtual_source_stop().await.unwrap();
            let _ = lsl_iroh_start().await.unwrap();
            let _ = lsl_iroh_stop().await.unwrap();
            let _ = lsl_iroh_status().await.unwrap();
            let _ = switch_session("muse".into()).await.unwrap();
            let _ = cancel_session().await.unwrap();
        });

        server.join().unwrap();
    }
}
