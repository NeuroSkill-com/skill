// SPDX-License-Identifier: GPL-3.0-only

use serde::Serialize;
use skill_daemon_common::{
    DiscoveredDeviceResponse, ForgetDeviceRequest, PairedDeviceResponse, ScannerStateResponse,
    ScannerWifiConfigRequest, SessionControlRequest, SetPreferredDeviceRequest, StatusResponse,
    VersionResponse, WsPortResponse, PROTOCOL_VERSION,
};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
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

/// Ensure the daemon process is running.  If it's not reachable, attempt to
/// spawn it.  Called once during `setup_app`.
pub(crate) fn ensure_daemon_running() {
    let base_url = daemon_base_url();
    // Quick health check — if the daemon is already up, nothing to do.
    let reachable = std::net::TcpStream::connect_timeout(
        &std::env::var("SKILL_DAEMON_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:18444".to_string())
            .parse()
            .unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], 18444))),
        Duration::from_millis(300),
    )
    .is_ok();
    if reachable {
        eprintln!("[daemon] already running at {base_url}");
        return;
    }

    // Try to spawn the daemon binary.
    let bin = std::env::var("SKILL_DAEMON_BIN").unwrap_or_else(|_| {
        // In production, the daemon binary is next to the app binary.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join("skill-daemon");
                if candidate.exists() {
                    return candidate.display().to_string();
                }
            }
        }
        "skill-daemon".to_string()
    });

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
    let bin = std::env::var("SKILL_DAEMON_BIN").unwrap_or_else(|_| "skill-daemon".to_string());
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

pub(crate) fn start_session(target: Option<String>) -> Result<StatusResponse, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    post_json_with_auth_response(
        &base_url,
        &token,
        "/v1/control/start-session",
        &SessionControlRequest { target },
    )
}

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

pub(crate) fn llm_get_catalog() -> Result<crate::llm::catalog::LlmCatalog, String> {
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

pub(crate) fn fetch_skills_refresh_interval() -> Result<u64, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value =
        fetch_json_with_auth(&base_url, &token, "/v1/skills/refresh-interval")?;
    Ok(v.get("value")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0))
}

pub(crate) fn fetch_skills_sync_on_launch() -> Result<bool, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value =
        fetch_json_with_auth(&base_url, &token, "/v1/skills/sync-on-launch")?;
    Ok(v.get("value")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
}

pub(crate) fn get_disabled_skills() -> Result<Vec<String>, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    let v: serde_json::Value = fetch_json_with_auth(&base_url, &token, "/v1/skills/disabled")?;
    Ok(serde_json::from_value(v.get("value").cloned().unwrap_or_default()).unwrap_or_default())
}

pub(crate) fn fetch_lsl_config() -> Result<serde_json::Value, String> {
    let base_url = daemon_base_url();
    let token = load_daemon_token()?;
    fetch_json_with_auth(&base_url, &token, "/v1/lsl/config")
}

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

struct MirrorState {
    last_sent_at: Instant,
    last_payload: String,
}

fn mirror_status_state() -> &'static Mutex<MirrorState> {
    static STATE: OnceLock<Mutex<MirrorState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(MirrorState {
            last_sent_at: Instant::now() - Duration::from_secs(10),
            last_payload: String::new(),
        })
    })
}

fn mirror_devices_state() -> &'static Mutex<MirrorState> {
    static STATE: OnceLock<Mutex<MirrorState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(MirrorState {
            last_sent_at: Instant::now() - Duration::from_secs(10),
            last_payload: String::new(),
        })
    })
}

pub(crate) fn mirror_status_to_daemon(local: &crate::DeviceStatus) {
    let status = StatusResponse {
        state: local.state.clone(),
        device_name: local.device_name.clone(),
        sample_count: local.sample_count,
        battery: local.battery,
        device_error: local.device_error.clone(),
        target_name: local.target_name.clone(),
        retry_attempt: local.retry_attempt,
        retry_countdown_secs: local.retry_countdown_secs,
        paired_devices: local
            .paired_devices
            .iter()
            .map(|d| PairedDeviceResponse {
                id: d.id.clone(),
                name: d.name.clone(),
                last_seen: d.last_seen,
            })
            .collect(),
    };

    let Ok(payload) = serde_json::to_string(&status) else {
        return;
    };

    if let Ok(mut guard) = mirror_status_state().lock() {
        let elapsed = guard.last_sent_at.elapsed();
        if elapsed < Duration::from_millis(500) {
            return;
        }
        if guard.last_payload == payload && elapsed < Duration::from_secs(5) {
            return;
        }
        guard.last_payload = payload;
        guard.last_sent_at = Instant::now();
    }

    let base_url = daemon_base_url();
    let Ok(token) = load_daemon_token() else {
        return;
    };

    let _ = post_json_with_auth::<StatusResponse>(&base_url, &token, "/v1/status", &status);
}

pub(crate) fn mirror_devices_to_daemon(local: &[crate::DiscoveredDevice]) {
    let devices: Vec<DiscoveredDeviceResponse> = local
        .iter()
        .map(|d| DiscoveredDeviceResponse {
            id: d.id.clone(),
            name: d.name.clone(),
            last_seen: d.last_seen,
            last_rssi: d.last_rssi,
            is_paired: d.is_paired,
            is_preferred: d.is_preferred,
            transport: serde_json::to_value(d.transport)
                .ok()
                .and_then(|v| v.as_str().map(std::string::ToString::to_string))
                .unwrap_or_else(|| "ble".to_string()),
        })
        .collect();

    let Ok(payload) = serde_json::to_string(&devices) else {
        return;
    };

    if let Ok(mut guard) = mirror_devices_state().lock() {
        let elapsed = guard.last_sent_at.elapsed();
        if elapsed < Duration::from_millis(500) {
            return;
        }
        if guard.last_payload == payload && elapsed < Duration::from_secs(5) {
            return;
        }
        guard.last_payload = payload;
        guard.last_sent_at = Instant::now();
    }

    let base_url = daemon_base_url();
    let Ok(token) = load_daemon_token() else {
        return;
    };

    let _ = post_json_with_auth::<Vec<DiscoveredDeviceResponse>>(
        &base_url,
        &token,
        "/v1/devices",
        &devices,
    );
}

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

#[allow(dead_code)]
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
