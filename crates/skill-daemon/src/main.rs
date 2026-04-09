// Allow needless_return in cfg-gated code paths where return is required
// to exit early before the #[cfg(not(feature))] fallback block.
#![allow(clippy::needless_return)]

mod activity;
mod auth;
pub(crate) mod cmd_dispatch;
pub(crate) mod embed;
mod routes;
mod service_installer;
pub(crate) mod session;
pub(crate) mod session_runner;
mod state;
mod tracker;

use state::AppState;

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use btleplug::{
    api::{Central as _, CentralEvent, Manager as _, Peripheral as _, ScanFilter},
    platform::Manager as BtManager,
};
use futures::StreamExt;
use rand::RngCore;
use skill_daemon_common::{
    ApiError, DeviceLogEntry, DiscoveredDeviceResponse, EventEnvelope, ForgetDeviceRequest, HealthResponse,
    LslDiscoveredStreamResponse, PairDeviceRequest, ScannerCortexConfigRequest, ScannerStateResponse,
    ScannerWifiConfigRequest, SessionControlRequest, SetPreferredDeviceRequest, StatusResponse, VersionResponse,
    WsClient, WsPortResponse, WsRequestLog, DAEMON_NAME, PROTOCOL_VERSION,
};
use tokio::sync::{broadcast, oneshot};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Write PID file for process management
    let pid_path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("skill")
        .join("daemon")
        .join("daemon.pid");
    if let Some(parent) = pid_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&pid_path, std::process::id().to_string());

    // Graceful shutdown on SIGTERM/SIGINT
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        info!("received shutdown signal");
    };

    let skill_dir = skill_data_dir();
    let state = AppState::new(load_or_create_token()?, skill_dir.clone());
    init_tracing(state.app_log.clone());

    // Spawn the remote-access iroh tunnel.  It proxies authenticated iroh
    // peers to this daemon's HTTP port, enabling phone pairing and remote EEG.
    {
        let api_port = daemon_addr().port();
        let (eeg_tx, _eeg_rx) = skill_iroh::event_channel();
        if let Ok(mut g) = state.iroh_device_tx.lock() {
            *g = Some(eeg_tx);
        }
        skill_iroh::spawn(
            skill_dir.clone(),
            api_port,
            state.iroh_auth.clone(),
            state.iroh_runtime.clone(),
            state.iroh_peer_map.clone(),
            state.iroh_device_tx.clone(),
        );
    }

    // Restore paired devices from paired_devices.json (fast path) or fall back
    // to settings.json (written by older builds / Tauri side).
    {
        let paired_path = skill_dir.join(skill_constants::PAIRED_DEVICES_FILE);
        let paired: Vec<skill_settings::PairedDevice> = if paired_path.exists() {
            skill_data::util::load_json_or_default(&paired_path)
        } else {
            skill_settings::load_settings(&skill_dir).paired
        };
        if let Ok(mut status) = state.status.lock() {
            status.paired_devices = paired
                .into_iter()
                .map(|p| skill_daemon_common::PairedDeviceResponse {
                    id: p.id,
                    name: p.name,
                    last_seen: p.last_seen,
                })
                .collect();
        }
    }

    activity::start_workers(state.clone());

    // Probe HF cache for the currently configured model weights so the UI
    // shows the correct state immediately on first load.
    {
        let st = state.exg_model_status.clone();
        let sd = skill_dir.clone();
        std::thread::spawn(move || {
            let config = skill_eeg::eeg_model_config::load_model_config(&sd);
            if let Some((path, backend)) = routes::settings::probe_weights_for_config(&config) {
                if let Ok(mut status) = st.lock() {
                    status.weights_found = true;
                    status.weights_path = Some(path);
                    status.active_model_backend = Some(backend);
                }
            }
        });
    }

    // Load HNSW label indices from disk (background thread).
    {
        let label_idx = state.label_index.clone();
        let sd = skill_dir.clone();
        std::thread::spawn(move || {
            label_idx.load(&sd);
            info!("label HNSW indices loaded");
        });
    }

    let v1 = Router::new()
        .route("/version", get(version))
        .route("/log/recent", get(get_log_recent))
        .route("/status", get(status).post(update_status))
        .route("/devices", get(devices).post(update_devices))
        .route("/devices/set-preferred", axum::routing::post(set_preferred_device))
        .route("/devices/pair", axum::routing::post(pair_device))
        .route("/devices/forget", axum::routing::post(forget_device))
        .route("/control/retry-connect", axum::routing::post(control_retry_connect))
        .route("/control/cancel-retry", axum::routing::post(control_cancel_retry))
        .route("/control/start-session", axum::routing::post(control_start_session))
        .route("/control/switch-session", axum::routing::post(control_switch_session))
        .route("/control/cancel-session", axum::routing::post(control_cancel_session))
        .route("/control/scanner/start", axum::routing::post(control_scanner_start))
        .route("/control/scanner/stop", axum::routing::post(control_scanner_stop))
        .route("/control/scanner/state", get(control_scanner_state))
        .route(
            "/control/scanner/wifi-config",
            axum::routing::post(control_scanner_wifi_config),
        )
        .route(
            "/control/scanner/cortex-config",
            axum::routing::post(control_scanner_cortex_config),
        )
        .route("/lsl/discover", get(lsl_discover))
        .route("/ws-port", get(ws_port))
        .route("/ws-clients", get(ws_clients))
        .route("/ws-request-log", get(ws_request_log))
        .route("/auth/tokens", get(list_tokens).post(create_token))
        .route("/auth/tokens/revoke", axum::routing::post(revoke_token))
        .route("/auth/tokens/delete", axum::routing::post(delete_token))
        .route(
            "/auth/default-token/refresh",
            axum::routing::post(refresh_default_token),
        )
        .route("/events", get(ws_events))
        .route("/events/push", axum::routing::post(push_event))
        .route("/cmd", axum::routing::post(cmd_tunnel))
        .merge(routes::labels::router())
        .merge(routes::history::router())
        .merge(routes::settings::router())
        .merge(routes::api::router())
        .merge(routes::analysis::router())
        .merge(routes::search::router())
        .merge(routes::iroh::router())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    // Root-level command tunnel for CLI HTTP mode (POST / with JSON body)
    let root_cmd = Router::new()
        .route("/", axum::routing::post(cmd_tunnel_root))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    // ── CORS — allow Tauri webview (and any local tool) to reach the daemon ──
    //
    // WKWebView on macOS treats `fetch()` from the Tauri devUrl / custom
    // protocol as cross-origin when the target is `http://127.0.0.1:<port>`.
    // Without CORS headers the browser strips the `Authorization` header and
    // the request fails the auth middleware.  A permissive CORS layer fixes
    // this: we already authenticate every request via bearer tokens so the
    // origin check adds no security value.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
        .expose_headers(Any);

    let shutdown_state = state.clone();
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/service/install", axum::routing::post(service_install))
        .route("/service/uninstall", axum::routing::post(service_uninstall))
        .route("/service/status", get(service_status))
        .nest("/v1", v1)
        .merge(root_cmd)
        .layer(cors)
        .with_state(state);

    let addr = daemon_addr();
    info!(%addr, "skill daemon listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown)
        .await?;

    // Cancel any in-flight EXG weights download.
    shutdown_state
        .exg_download_cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);

    // Drop the active BLE session so btleplug stops firing delegate callbacks
    // before the event channel is torn down (prevents spurious
    // "Error sending notification event: send failed because receiver is gone").
    if let Ok(mut slot) = shutdown_state.session_handle.lock() {
        drop(slot.take());
    }

    // Clean up PID file
    let _ = std::fs::remove_file(&pid_path);
    info!("daemon shut down cleanly");
    Ok(())
}

fn skill_data_dir() -> PathBuf {
    std::env::var("SKILL_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| skill_settings::default_skill_dir())
}

fn init_tracing(app_log: std::sync::Arc<std::sync::Mutex<(u64, std::collections::VecDeque<String>)>>) {
    use std::io::Write;
    use tracing_subscriber::fmt::MakeWriter;

    /// A writer that writes each byte to both stderr and the shared ring buffer.
    /// `tracing_subscriber::fmt` calls `make_writer()` once per log event and
    /// then calls `write` / `flush` on the returned writer.
    #[derive(Clone)]
    struct TeeWriter {
        log: std::sync::Arc<std::sync::Mutex<(u64, std::collections::VecDeque<String>)>>,
        // Accumulate bytes for the current log line so we can store it whole.
        buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    }

    impl Write for TeeWriter {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            // Forward to real stderr.
            std::io::stderr().write_all(data)?;
            // Buffer locally for line assembly.
            if let Ok(mut b) = self.buf.lock() {
                b.extend_from_slice(data);
                // tracing-subscriber calls `write_all` once per event with the
                // complete formatted line (including the trailing '\n') and
                // never calls `flush()`.  Commit to the ring buffer as soon as
                // we see a newline so log lines are never lost.
                if data.contains(&b'\n') {
                    let line = String::from_utf8_lossy(&b).trim_end_matches('\n').to_string();
                    b.clear();
                    if !line.is_empty() {
                        if let Ok(mut guard) = self.log.lock() {
                            const CAP: usize = 512;
                            let (seq, buf) = &mut *guard;
                            if buf.len() >= CAP {
                                buf.pop_front();
                            }
                            buf.push_back(format!("{}\t{}", seq, line));
                            *seq += 1;
                        }
                    }
                }
            }
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            std::io::stderr().flush()?;
            // Flush any partial line that didn't end with '\n'.
            let line = if let Ok(mut b) = self.buf.lock() {
                let s = String::from_utf8_lossy(&b).trim_end_matches('\n').to_string();
                b.clear();
                s
            } else {
                return Ok(());
            };
            if line.is_empty() {
                return Ok(());
            }
            if let Ok(mut guard) = self.log.lock() {
                const CAP: usize = 512;
                let (seq, buf) = &mut *guard;
                if buf.len() >= CAP {
                    buf.pop_front();
                }
                buf.push_back(format!("{}\t{}", seq, line));
                *seq += 1;
            }
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for TeeWriter {
        type Writer = TeeWriter;
        fn make_writer(&'a self) -> TeeWriter {
            TeeWriter {
                log: self.log.clone(),
                buf: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }
    }

    let writer = TeeWriter {
        log: app_log,
        buf: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "skill_daemon=info,info".into()),
        )
        .with_writer(writer)
        .with_target(false)
        .compact()
        .init();
}

fn daemon_addr() -> SocketAddr {
    std::env::var("SKILL_DAEMON_ADDR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 18444)))
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn readyz() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn service_install() -> Json<serde_json::Value> {
    let bin = std::env::current_exe().unwrap_or_default();
    let installer = crate::service_installer::ServiceInstaller::new(bin);
    match installer.install() {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
}

async fn service_uninstall() -> Json<serde_json::Value> {
    let bin = std::env::current_exe().unwrap_or_default();
    let installer = crate::service_installer::ServiceInstaller::new(bin);
    match installer.uninstall() {
        Ok(()) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
}

async fn service_status() -> Json<serde_json::Value> {
    let bin = std::env::current_exe().unwrap_or_default();
    let installer = crate::service_installer::ServiceInstaller::new(bin);
    let status = installer.status();
    Json(serde_json::json!({ "status": status }))
}

async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        daemon: DAEMON_NAME.to_string(),
        protocol_version: PROTOCOL_VERSION,
        daemon_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// `GET /v1/log/recent?since=<seq>`
///
/// Returns daemon log lines (from the tracing ring buffer) whose sequence
/// number is >= `since`.  The Tauri dev process polls this endpoint every
/// second and pipes new lines to its own stderr so all daemon output appears
/// in the same terminal as the rest of `npm run tauri dev` output.
async fn get_log_recent(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let since: u64 = params.get("since").and_then(|v| v.parse().ok()).unwrap_or(0);
    let (next_seq, lines) = if let Ok(guard) = state.app_log.lock() {
        let next = guard.0;
        let lines: Vec<String> = guard
            .1
            .iter()
            .filter_map(|entry| {
                // Format: "<seq>\t<text>"
                let tab = entry.find('\t')?;
                let seq: u64 = entry[..tab].parse().ok()?;
                if seq >= since {
                    Some(entry[tab + 1..].to_string())
                } else {
                    None
                }
            })
            .collect();
        (next, lines)
    } else {
        (0, vec![])
    };
    Json(serde_json::json!({ "next_seq": next_seq, "lines": lines }))
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let current = state.status.lock().map(|g| g.clone()).unwrap_or_default();
    Json(current)
}

async fn update_status(State(state): State<AppState>, Json(next): Json<StatusResponse>) -> Json<StatusResponse> {
    if let Ok(mut guard) = state.status.lock() {
        *guard = next.clone();
    }
    Json(next)
}

async fn devices(State(state): State<AppState>) -> Json<Vec<DiscoveredDeviceResponse>> {
    let current = state.devices.lock().map(|g| g.clone()).unwrap_or_default();
    Json(current)
}

async fn update_devices(
    State(state): State<AppState>,
    Json(next): Json<Vec<DiscoveredDeviceResponse>>,
) -> Json<Vec<DiscoveredDeviceResponse>> {
    if let Ok(mut guard) = state.devices.lock() {
        *guard = next.clone();
    }
    Json(next)
}

async fn set_preferred_device(
    State(state): State<AppState>,
    Json(req): Json<SetPreferredDeviceRequest>,
) -> Json<Vec<DiscoveredDeviceResponse>> {
    let mut out = Vec::new();
    if let Ok(mut guard) = state.devices.lock() {
        for d in guard.iter_mut() {
            d.is_preferred = !req.id.is_empty() && d.id == req.id;
        }
        out = guard.clone();
    }
    Json(out)
}

async fn pair_device(
    State(state): State<AppState>,
    Json(req): Json<PairDeviceRequest>,
) -> Json<Vec<DiscoveredDeviceResponse>> {
    let mut out = Vec::new();
    let mut paired_name: Option<String> = None;
    if let Ok(mut guard) = state.devices.lock() {
        if let Some(d) = guard.iter_mut().find(|d| d.id == req.id) {
            d.is_paired = true;
            paired_name = Some(d.name.clone());
        }
        out = guard.clone();
    }

    let name = paired_name.unwrap_or_else(|| "Unknown".to_string());

    if let Ok(mut status) = state.status.lock() {
        if !status.paired_devices.iter().any(|d| d.id == req.id) {
            status.paired_devices.push(skill_daemon_common::PairedDeviceResponse {
                id: req.id.clone(),
                name: name.clone(),
                last_seen: now_unix_secs(),
            });
        }
    }

    // Persist to settings.json so paired devices survive daemon restarts.
    persist_paired_devices(&state);

    Json(out)
}

async fn forget_device(
    State(state): State<AppState>,
    Json(req): Json<ForgetDeviceRequest>,
) -> Json<Vec<DiscoveredDeviceResponse>> {
    let mut out = Vec::new();
    if let Ok(mut guard) = state.devices.lock() {
        if let Some(d) = guard.iter_mut().find(|d| d.id == req.id) {
            d.is_paired = false;
        }
        out = guard.clone();
    }

    if let Ok(mut status) = state.status.lock() {
        status.paired_devices.retain(|d| d.id != req.id);
    }

    // Persist removal to settings.json.
    persist_paired_devices(&state);

    Json(out)
}

/// Persist the current `status.paired_devices` list to disk.
///
/// Writes two files:
/// * `paired_devices.json` — lightweight fast-path read on daemon startup
/// * `settings.json` — kept in sync for Tauri and backward compatibility
///
/// Non-fatal: logs a warning on failure but never panics.
fn persist_paired_devices(state: &AppState) {
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

/// Spawn the appropriate session runner for the given target device.
/// Cancels any existing session first.
fn spawn_session_for_target(state: &AppState, target: Option<&str>) {
    // Cancel any existing session.
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }

    let Some(t) = target else { return };

    // All devices route through the generic adapter session runner.
    let handle = session::spawn_device_session(state.clone(), t.to_string());

    if let Some(h) = handle {
        if let Ok(mut slot) = state.session_handle.lock() {
            *slot = Some(h);
        }
    }
}

fn default_status(state: &str) -> StatusResponse {
    StatusResponse {
        state: state.to_string(),
        ..Default::default()
    }
}

/// Resolve canonical target fields for status/UI from a requested target.
/// Returns `(target_id, target_display_name)`.
fn resolve_target_fields(state: &AppState, target: Option<&str>) -> (Option<String>, Option<String>) {
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

fn target_requires_pairing(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.contains(':') || lower == "neurosky" || lower.starts_with("muse")
}

fn is_paired_target(state: &AppState, target: &str) -> bool {
    state
        .status
        .lock()
        .ok()
        .map(|s| s.paired_devices.iter().any(|d| d.id == target || d.name == target))
        .unwrap_or(false)
}

async fn control_retry_connect(State(state): State<AppState>) -> Json<StatusResponse> {
    let mut out = default_status("connecting");

    // Cancel any existing session before retrying.
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }

    // Snapshot paired order + previous target from status.
    let (status_target, paired_order): (Option<String>, Vec<String>) = state
        .status
        .lock()
        .ok()
        .map(|s| {
            (
                s.target_id.clone().or_else(|| s.target_name.clone()),
                s.paired_devices.iter().map(|d| d.id.clone()).collect(),
            )
        })
        .unwrap_or_default();

    // Preferred target from discovered list (paired-only).
    let preferred_target = state
        .devices
        .lock()
        .ok()
        .and_then(|d| d.iter().find(|x| x.is_preferred && x.is_paired).map(|x| x.id.clone()));

    // Availability snapshot from currently discovered paired devices.
    let available: std::collections::HashSet<String> = state
        .devices
        .lock()
        .ok()
        .map(|d| d.iter().filter(|x| x.is_paired).map(|x| x.id.clone()).collect())
        .unwrap_or_default();

    // Base preference: previous target -> preferred -> first paired.
    let mut target = status_target
        .or(preferred_target)
        .or_else(|| paired_order.first().cloned());

    // Never auto-connect an unpaired target.
    if let Some(current) = target.as_ref() {
        if !is_paired_target(&state, current) {
            push_device_log(
                &state,
                "session",
                &format!("retry-connect dropped unpaired target={current}"),
            );
            target = None;
        }
    }

    // If default target is unavailable, fall back to next paired+available.
    if let Some(current) = target.as_ref() {
        if !available.contains(current) {
            if let Some(next) = paired_order.iter().find(|id| available.contains(*id)) {
                push_device_log(
                    &state,
                    "session",
                    &format!("retry-connect fallback: {current} unavailable, using {next}"),
                );
                target = Some(next.clone());
            }
        }
    } else if let Some(next) = paired_order.iter().find(|id| available.contains(*id)) {
        push_device_log(
            &state,
            "session",
            &format!("retry-connect auto-selected paired available target={next}"),
        );
        target = Some(next.clone());
    }

    if let Some(ref t) = target {
        push_device_log(&state, "session", &format!("retry-connect target={t}"));
    } else {
        push_device_log(&state, "session", "retry-connect skipped: no target device");
    }

    let resolved_target = target
        .as_deref()
        .map(|t| (t.to_string(), resolve_target_fields(&state, Some(t))));

    if let Ok(mut status) = state.status.lock() {
        status.retry_attempt = 0;
        status.retry_countdown_secs = 0;
        status.device_error = None;
        if let Some((t, (target_id, target_display_name))) = resolved_target.clone() {
            status.state = "connecting".to_string();
            status.target_name = Some(t);
            status.target_id = target_id;
            status.target_display_name = target_display_name;
        } else {
            status.state = "disconnected".to_string();
            status.target_id = None;
            status.target_display_name = None;
            status.device_error = Some("No target device selected. Set a default device and try again.".to_string());
        }
        out = status.clone();
    }

    if let Some(t) = target {
        spawn_session_for_target(&state, Some(&t));
    }

    Json(out)
}

async fn control_cancel_retry(State(state): State<AppState>) -> Json<StatusResponse> {
    // Cancel any running session.
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }
    // Clear the BLE scan pause so the background listener resumes immediately.
    // Without this, cancelling a mid-connection attempt would leave the
    // listener parked and BLE discovery would stop until the next connect.
    state.ble_scan_paused.store(false, std::sync::atomic::Ordering::Relaxed);

    let mut out = default_status("disconnected");

    if let Ok(mut status) = state.status.lock() {
        status.state = "disconnected".to_string();
        status.device_error = None;
        status.retry_attempt = 0;
        status.retry_countdown_secs = 0;
        out = status.clone();
    }

    Json(out)
}

async fn control_start_session(
    State(state): State<AppState>,
    Json(req): Json<SessionControlRequest>,
) -> Json<StatusResponse> {
    let mut out = default_status("connecting");

    let target = req.target.clone();

    // If the target is a device ID from the scanner (e.g. "usb:COM3",
    // "usb:/dev/ttyUSB0"), extract the serial port and store it in the
    // OpenBCI config so the session runner can use it.
    //
    // Crucially, also promote the board type to Cyton when the persisted
    // value is BLE-only (Ganglion).  Without this, create_and_start_board
    // would attempt a BLE scan even though the user plugged in a USB dongle.
    if let Some(ref t) = target {
        if let Some(port) = t.strip_prefix("usb:") {
            let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
            let mut settings = skill_settings::load_settings(&skill_dir);
            settings.openbci.serial_port = port.to_string();
            // A usb: target always means a serial board (Cyton or CytonDaisy).
            // Preserve the user's choice if it is already a serial board;
            // otherwise reset to Cyton so we never attempt a BLE / WiFi /
            // UDP connection when a dongle is plugged in.
            if !settings.openbci.board.is_serial() {
                settings.openbci.board = skill_settings::OpenBciBoard::Cyton;
            }
            let path = skill_settings::settings_path(&skill_dir);
            if let Ok(json) = serde_json::to_string_pretty(&settings) {
                let _ = std::fs::write(path, json);
            }
        }
        if target_requires_pairing(t) && !is_paired_target(&state, t) {
            if let Ok(mut status) = state.status.lock() {
                status.state = "disconnected".to_string();
                status.device_error = Some("Target device is not paired. Pair it first in Settings → Devices.".into());
                out = status.clone();
            }
            return Json(out);
        }
    }

    let (target_id, target_display_name) = resolve_target_fields(&state, target.as_deref());
    if let Ok(mut status) = state.status.lock() {
        status.state = "connecting".to_string();
        status.target_name = req.target;
        status.target_id = target_id;
        status.target_display_name = target_display_name;
        status.device_error = None;
        out = status.clone();
    }

    spawn_session_for_target(&state, target.as_deref());

    Json(out)
}

async fn control_switch_session(
    State(state): State<AppState>,
    Json(req): Json<SessionControlRequest>,
) -> Json<StatusResponse> {
    // Cancel any existing session before switching.
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }

    let mut out = default_status("connecting");
    let target = req.target.clone();

    if let Some(ref t) = target {
        if target_requires_pairing(t) && !is_paired_target(&state, t) {
            if let Ok(mut status) = state.status.lock() {
                status.state = "disconnected".to_string();
                status.device_error = Some("Target device is not paired. Pair it first in Settings → Devices.".into());
                out = status.clone();
            }
            return Json(out);
        }
    }

    let (target_id, target_display_name) = resolve_target_fields(&state, target.as_deref());
    if let Ok(mut status) = state.status.lock() {
        status.state = "connecting".to_string();
        status.target_name = req.target;
        status.target_id = target_id;
        status.target_display_name = target_display_name;
        status.device_error = None;
        out = status.clone();
    }

    spawn_session_for_target(&state, target.as_deref());

    Json(out)
}

async fn control_cancel_session(State(state): State<AppState>) -> Json<StatusResponse> {
    // Cancel any running session task.
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }

    let mut out = default_status("disconnected");

    if let Ok(mut status) = state.status.lock() {
        status.state = "disconnected".to_string();
        status.target_name = None;
        status.target_id = None;
        status.target_display_name = None;
        status.device_error = None;
        out = status.clone();
    }

    Json(out)
}

async fn control_scanner_start(State(state): State<AppState>) -> Json<ScannerStateResponse> {
    let already_running = state.scanner_running.lock().map(|g| *g).unwrap_or(false);
    if already_running {
        push_device_log(&state, "scanner", "start requested but scanner already running");
        return Json(ScannerStateResponse { running: true });
    }

    let (tx, rx) = oneshot::channel();
    if let Ok(mut slot) = state.scanner_stop_tx.lock() {
        *slot = Some(tx);
    }
    if let Ok(mut running) = state.scanner_running.lock() {
        *running = true;
    }
    // Clear any stale pause flag left over from a connection attempt that was
    // interrupted while the scanner was stopped.  Without this, the freshly
    // spawned BLE listener task would stall waiting for the flag to clear.
    state.ble_scan_paused.store(false, std::sync::atomic::Ordering::Relaxed);

    push_device_log(&state, "scanner", "scanner started");

    let state2 = state.clone();
    tokio::spawn(async move {
        run_usb_scanner_task(state2.clone(), rx).await;
        if let Ok(mut running) = state2.scanner_running.lock() {
            *running = false;
        }
        if let Ok(mut slot) = state2.scanner_stop_tx.lock() {
            *slot = None;
        }
    });

    Json(ScannerStateResponse { running: true })
}

async fn control_scanner_stop(State(state): State<AppState>) -> Json<ScannerStateResponse> {
    if let Ok(mut slot) = state.scanner_stop_tx.lock() {
        if let Some(tx) = slot.take() {
            let _ = tx.send(());
        }
    }
    if let Ok(mut running) = state.scanner_running.lock() {
        *running = false;
    }
    push_device_log(&state, "scanner", "scanner stopped");
    Json(ScannerStateResponse { running: false })
}

async fn control_scanner_state(State(state): State<AppState>) -> Json<ScannerStateResponse> {
    let running = state.scanner_running.lock().map(|g| *g).unwrap_or(false);
    Json(ScannerStateResponse { running })
}

async fn control_scanner_wifi_config(
    State(state): State<AppState>,
    Json(cfg): Json<ScannerWifiConfigRequest>,
) -> Json<ScannerWifiConfigRequest> {
    if let Ok(mut guard) = state.scanner_wifi_config.lock() {
        *guard = cfg.clone();
    }
    Json(cfg)
}

async fn control_scanner_cortex_config(
    State(state): State<AppState>,
    Json(cfg): Json<ScannerCortexConfigRequest>,
) -> Json<ScannerCortexConfigRequest> {
    if let Ok(mut guard) = state.scanner_cortex_config.lock() {
        *guard = cfg.clone();
    }
    Json(cfg)
}

async fn lsl_discover() -> Json<Vec<LslDiscoveredStreamResponse>> {
    let streams = tokio::task::spawn_blocking(|| skill_lsl::discover_streams(3.0))
        .await
        .unwrap_or_default();

    let out = streams
        .into_iter()
        .map(|s| LslDiscoveredStreamResponse {
            name: s.name,
            stream_type: s.stream_type,
            channels: s.channel_count,
            sample_rate: s.sample_rate,
            source_id: s.source_id,
            hostname: s.hostname,
        })
        .collect();

    Json(out)
}

async fn ws_port() -> Json<WsPortResponse> {
    Json(WsPortResponse {
        port: daemon_addr().port(),
    })
}

async fn ws_clients(State(state): State<AppState>) -> Json<Vec<WsClient>> {
    let clients = state.tracker.lock().map(|g| g.clients.clone()).unwrap_or_default();
    Json(clients)
}

async fn ws_request_log(State(state): State<AppState>) -> Json<Vec<WsRequestLog>> {
    let requests = state.tracker.lock().map(|g| g.requests.clone()).unwrap_or_default();
    Json(requests)
}

async fn ws_events(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let peer = addr.to_string();
    add_client(&state, &peer);

    ws.on_upgrade(move |socket| {
        let peer = peer.clone();
        let state = state.clone();
        async move {
            let rx = state.events_tx.subscribe();
            handle_ws(socket, rx, state.clone()).await;
            remove_client(&state, &peer);
        }
    })
}

// ── Token management ────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct CreateTokenRequest {
    name: String,
    acl: auth::TokenAcl,
    expiry: auth::TokenExpiry,
}

#[derive(serde::Deserialize)]
struct TokenIdRequest {
    id: String,
}

async fn list_tokens(State(state): State<AppState>) -> impl IntoResponse {
    let store = match state.token_store.lock() {
        Ok(store) => store.clone(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "ok": false, "error": "token store unavailable" })),
            )
                .into_response();
        }
    };
    (StatusCode::OK, Json(store.list_redacted())).into_response()
}

async fn create_token(State(state): State<AppState>, Json(req): Json<CreateTokenRequest>) -> impl IntoResponse {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let Ok(mut store) = state.token_store.lock() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": "token store unavailable" })),
        )
            .into_response();
    };

    let token = match store.create(req.name, req.acl, req.expiry) {
        Ok(token) => token,
        Err(auth::TokenStoreError::MaxTokensReached) => {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("maximum active token count reached ({})", auth::TokenStore::MAX_TOKENS)
                })),
            )
                .into_response();
        }
    };

    let _ = store.save(&skill_dir);
    (StatusCode::OK, Json(token)).into_response() // Full token returned (secret visible) on creation only
}

async fn revoke_token(State(state): State<AppState>, Json(req): Json<TokenIdRequest>) -> impl IntoResponse {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let Ok(mut store) = state.token_store.lock() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": "token store unavailable" })),
        )
            .into_response();
    };

    let ok = store.revoke(&req.id);
    let _ = store.save(&skill_dir);
    (StatusCode::OK, Json(serde_json::json!({ "ok": ok }))).into_response()
}

async fn delete_token(State(state): State<AppState>, Json(req): Json<TokenIdRequest>) -> impl IntoResponse {
    if req.id == "default" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "cannot delete the default token — use refresh instead" })),
        )
            .into_response();
    }
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let Ok(mut store) = state.token_store.lock() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": "token store unavailable" })),
        )
            .into_response();
    };

    let ok = store.delete(&req.id);
    let _ = store.save(&skill_dir);
    (StatusCode::OK, Json(serde_json::json!({ "ok": ok }))).into_response()
}

async fn refresh_default_token(State(state): State<AppState>) -> impl IntoResponse {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let new_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

    let path = match token_path() {
        Ok(path) => path,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "ok": false, "error": format!("path error: {e}") })),
            )
                .into_response();
        }
    };

    if let Err(e) = write_string_atomic(&path, &format!("{new_token}\n")) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": format!("write error: {e}") })),
        )
            .into_response();
    }

    match state.auth_token.lock() {
        Ok(mut guard) => {
            *guard = new_token.clone();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "ok": false, "error": "auth token state unavailable" })),
            )
                .into_response();
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({ "ok": true, "token": new_token })),
    )
        .into_response()
}

async fn push_event(State(state): State<AppState>, Json(envelope): Json<EventEnvelope>) -> Json<serde_json::Value> {
    let _ = state.events_tx.send(envelope);
    Json(serde_json::json!({ "ok": true }))
}

/// Universal command tunnel — accepts CLI JSON commands via `POST /v1/cmd`.
async fn cmd_tunnel(State(state): State<AppState>, Json(msg): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(cmd_dispatch::dispatch(state, msg).await)
}

/// Root-level command tunnel — accepts CLI JSON commands via `POST /`.
async fn cmd_tunnel_root(State(state): State<AppState>, Json(msg): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(cmd_dispatch::dispatch(state, msg).await)
}

async fn handle_ws(mut socket: WebSocket, mut rx: broadcast::Receiver<EventEnvelope>, state: AppState) {
    let connected = EventEnvelope {
        r#type: "DaemonStarted".to_string(),
        ts_unix_ms: now_unix_ms(),
        correlation_id: None,
        payload: serde_json::json!({ "message": "connected" }),
    };

    match serde_json::to_string(&connected) {
        Ok(payload) => {
            if socket.send(Message::Text(payload.into())).await.is_err() {
                return;
            }
        }
        Err(err) => {
            error!(%err, "failed to serialize initial websocket event");
            return;
        }
    }

    // Channel for streaming messages back to the WS client.
    // Used by LLM chat streaming to send incremental deltas.
    let (_stream_tx, mut stream_rx) = tokio::sync::mpsc::channel::<String>(64);
    #[cfg(feature = "llm")]
    let stream_tx = _stream_tx;

    loop {
        tokio::select! {
            // Broadcast events → send to client
            event = rx.recv() => {
                match event {
                    Ok(ev) => {
                        let payload = match serde_json::to_string(&ev) {
                            Ok(v) => v,
                            Err(err) => {
                                error!(%err, "failed to serialize websocket event");
                                continue;
                            }
                        };
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::debug!(%skipped, "websocket client lagged behind event stream");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Streaming messages (from LLM chat) → send to client
            Some(msg_str) = stream_rx.recv() => {
                if socket.send(Message::Text(msg_str.into())).await.is_err() {
                    break;
                }
            }
            // Incoming messages from client → dispatch as commands
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let text_str: &str = &text;
                        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(text_str) {
                            let cmd_name = cmd.get("command")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("");

                            if cmd_name == "llm_chat" {
                                // LLM chat uses streaming: send deltas incrementally.
                                #[cfg(feature = "llm")]
                                {
                                    let mut tx = stream_tx.clone();
                                    cmd_dispatch::dispatch_llm_chat_streaming(
                                        state.clone(), cmd, &mut tx,
                                    ).await;
                                }
                                #[cfg(not(feature = "llm"))]
                                {
                                    let response = cmd_dispatch::dispatch(state.clone(), cmd).await;
                                    if let Ok(resp_str) = serde_json::to_string(&response) {
                                        if socket.send(Message::Text(resp_str.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            } else if !cmd_name.is_empty() {
                                let response = cmd_dispatch::dispatch(state.clone(), cmd).await;
                                if let Ok(resp_str) = serde_json::to_string(&response) {
                                    if socket.send(Message::Text(resp_str.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    // CORS preflight requests never carry credentials — let them through so
    // the CorsLayer (applied as an outer layer) can respond with the proper
    // `Access-Control-Allow-*` headers.
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }

    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let command = request.uri().path().to_string();

    match auth_decision(&headers, &request, &state) {
        AuthDecision::Allowed => {
            record_request(&state, peer, command, true);
            next.run(request).await
        }
        AuthDecision::MissingOrInvalid => {
            record_request(&state, peer, command, false);
            let body = Json(ApiError {
                code: "unauthorized",
                message: "missing or invalid bearer token".to_string(),
            });
            (StatusCode::UNAUTHORIZED, body).into_response()
        }
        AuthDecision::Forbidden => {
            record_request(&state, peer, command, false);
            let body = Json(ApiError {
                code: "forbidden",
                message: "token does not have permission for this endpoint".to_string(),
            });
            (StatusCode::FORBIDDEN, body).into_response()
        }
    }
}

fn detect_openbci_serial_ports() -> Vec<(String, String)> {
    let Ok(ports) = serialport::available_ports() else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for port in ports {
        let name = port.port_name.clone();
        let lower = name.to_lowercase();

        let is_openbci = match &port.port_type {
            serialport::SerialPortType::UsbPort(usb) => {
                // FTDI chips used across OpenBCI dongle revisions:
                //   0x6015 = FT231X  (current Cyton dongle)
                //   0x6001 = FT232R  (older Cyton/Ganglion dongles)
                //   0x6014 = FT232H  (rare but seen in some kits)
                let vid_match = usb.vid == 0x0403 && matches!(usb.pid, 0x6015 | 0x6001 | 0x6014);

                let product_match = usb
                    .product
                    .as_deref()
                    .map(|p| {
                        let pl = p.to_lowercase();
                        pl.contains("ft231x") || pl.contains("ft232") || pl.contains("openbci") || pl.contains("ftdi")
                    })
                    .unwrap_or(false);

                let manufacturer_match = usb
                    .manufacturer
                    .as_deref()
                    .map(|m| {
                        let ml = m.to_lowercase();
                        ml.contains("ftdi") || ml.contains("openbci")
                    })
                    .unwrap_or(false);

                vid_match || product_match || manufacturer_match
            }
            // Linux/macOS path-based fallback
            #[cfg(not(target_os = "windows"))]
            _ => lower.contains("ttyusb") || lower.contains("usbserial"),
            // Windows: FTDI dongles appear as generic COM ports when the
            // driver supplies no USB metadata.  Accept any COM port that
            // the system reports as PnP (non-built-in).  This is broader
            // than the USB branch above, but on Windows the fallback arm
            // only fires when `serialport` classifies the port as
            // `Unknown` — built-in COM0/COM1 are typically `PciPort`.
            #[cfg(target_os = "windows")]
            serialport::SerialPortType::Unknown => {
                // Heuristic: COM3 and above are almost always USB/PnP
                // adapters; COM1/COM2 are legacy motherboard UARTs.
                let port_num = lower
                    .strip_prefix("com")
                    .and_then(|n| n.parse::<u32>().ok())
                    .unwrap_or(0);
                port_num >= 3
            }
            #[cfg(target_os = "windows")]
            _ => false,
        };

        if is_openbci {
            let display = format!("OpenBCI ({name})");
            results.push((name, display));
        }
    }

    results
}

fn detect_cgx_serial_ports() -> Vec<(String, String)> {
    ::cognionics::prelude::enumerate_devices()
        .into_iter()
        .map(|d| {
            let display = if d.description.is_empty() {
                format!("CGX ({})", d.port)
            } else {
                format!("CGX {} ({})", d.description, d.port)
            };
            (d.port, display)
        })
        .collect()
}

fn detect_brainbit_devices() -> Vec<DiscoveredDeviceResponse> {
    use brainbit::prelude::*;
    let Ok(scanner) = Scanner::new(&[SensorFamily::LEBrainBit]) else {
        return Vec::new();
    };
    if scanner.start().is_err() {
        return Vec::new();
    }
    std::thread::sleep(std::time::Duration::from_secs(3));
    let _ = scanner.stop();
    let devices = scanner.devices().unwrap_or_default();
    devices
        .into_iter()
        .map(|d| {
            let name = d.name_str();
            let addr = d.address_str();
            let id = format!("brainbit:{addr}");
            let display = if name.is_empty() {
                format!("BrainBit ({addr})")
            } else {
                format!("BrainBit {name}")
            };
            DiscoveredDeviceResponse {
                id,
                name: display,
                last_seen: now_unix_secs(),
                last_rssi: 0,
                is_paired: false,
                is_preferred: false,
                transport: "ble".to_string(),
            }
        })
        .collect()
}

fn detect_brainmaster_devices() -> Vec<DiscoveredDeviceResponse> {
    let ports = brainmaster::device::BrainMasterDevice::scan().unwrap_or_default();
    ports
        .into_iter()
        .map(|port| DiscoveredDeviceResponse {
            id: format!("brainmaster:{port}"),
            name: format!("BrainMaster ({port})"),
            last_seen: now_unix_secs(),
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "usb_serial".to_string(),
        })
        .collect()
}

fn detect_gtec_devices() -> Vec<DiscoveredDeviceResponse> {
    let serials = gtec::device::UnicornDevice::scan(false).unwrap_or_default();
    serials
        .into_iter()
        .map(|serial| DiscoveredDeviceResponse {
            id: format!("gtec:{serial}"),
            name: format!("g.tec Unicorn ({serial})"),
            last_seen: now_unix_secs(),
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "ble".to_string(),
        })
        .collect()
}

fn detect_neurofield_devices() -> Vec<DiscoveredDeviceResponse> {
    let mut out = Vec::new();
    let online = neurofield::q21_api::Q21Api::get_online_pcan_interfaces();
    for bus in online {
        let bus_name = format!("{bus:?}");
        // Try to connect briefly to get device info.
        match neurofield::q21_api::Q21Api::new(bus) {
            Ok(mut api) => {
                let serial = api.eeg_device_serial();
                let dev_type = api.eeg_device_type();
                let name = format!("NeuroField Q21 ({dev_type:?} #{serial})");
                let id = format!("neurofield:{bus_name}:{serial}");
                api.release();
                out.push(DiscoveredDeviceResponse {
                    id,
                    name,
                    last_seen: now_unix_secs(),
                    last_rssi: 0,
                    is_paired: false,
                    is_preferred: false,
                    transport: "usb_serial".to_string(),
                });
            }
            Err(_) => {
                // PCAN interface online but no Q21 connected — report as available bus.
                out.push(DiscoveredDeviceResponse {
                    id: format!("neurofield:{bus_name}"),
                    name: format!("NeuroField PCAN ({bus_name})"),
                    last_seen: now_unix_secs(),
                    last_rssi: 0,
                    is_paired: false,
                    is_preferred: false,
                    transport: "usb_serial".to_string(),
                });
            }
        }
    }
    out
}

/// Return `true` when a BLE advertising name looks like a supported EEG/neurofeedback device.
fn is_known_eeg_ble_name(name: &str) -> bool {
    let n = name.to_lowercase();
    // Muse family (Muse 1/2/S, Muse-S Athena, Muse Monitor)
    n.starts_with("muse")
        // OpenBCI Ganglion
        || n.starts_with("ganglion")
        || n.starts_with("simblee")
        // OpenBCI Cyton
        || n.starts_with("openbci")
        || n.starts_with("cyton")
        // Neurable MW75
        || n.contains("mw75")
        || n.contains("neurable")
        // Hermes
        || n.starts_with("hermes")
        // Emotiv EPOC/Insight/Flex/MN8
        || n.starts_with("emotiv")
        || n.starts_with("epoc")
        || n.starts_with("insight")
        || n.starts_with("mn8")
        // Idun / Guardian
        || n.starts_with("idun")
        || n.starts_with("ige")
        || n.starts_with("guardian")
        // Mendi fNIRS
        || n.starts_with("mendi")
        // CGX / Cognionics
        || n.contains("cognionics")
        || n.contains("cgx")
        || n.starts_with("quick-")
        || n.starts_with("aim-")
        || n.starts_with("patch")
        // AttentivU
        || n.starts_with("atu")
        || n.starts_with("attentivu")
        // BrainBit
        || n.contains("brainbit")
        // g.tec Unicorn
        || n.contains("unicorn")
        || n.starts_with("un-")
        // NeuroField
        || n.contains("neurofield")
        || n.contains("q21")
        // NeuroSky
        || n.contains("neurosky")
        || n.contains("mindwave")
        // Neurosity Crown / Notion
        || n.contains("neurosity")
        || n.contains("crown")
        || n.contains("notion")
}

/// Read the current BLE device cache and return only devices whose names
/// match a known EEG/neurofeedback headset.  Entries not seen within the
/// last 60 seconds are suppressed (but kept in the cache for name recall).
fn read_ble_cache(state: &AppState) -> Vec<DiscoveredDeviceResponse> {
    let now = now_unix_secs();
    let Ok(cache) = state.ble_device_cache.lock() else {
        return Vec::new();
    };
    cache
        .iter()
        .filter_map(|(id, (name_opt, rssi, last_seen))| {
            // Must have a recognised EEG device name.
            let name = name_opt.as_deref()?;
            if !is_known_eeg_ble_name(name) {
                return None;
            }
            // Suppress stale entries (> 120 s since last advertisement).
            // 120 s covers the worst-case connection attempt window:
            // 600 ms pause + 5 s scan + 10 s connect + 15 s discover + margin.
            if now.saturating_sub(*last_seen) > 120 {
                return None;
            }
            Some(DiscoveredDeviceResponse {
                id: id.clone(),
                name: name.to_string(),
                last_seen: *last_seen,
                last_rssi: *rssi,
                is_paired: false,
                is_preferred: false,
                transport: "ble".to_string(),
            })
        })
        .collect()
}

/// Persistent, event-driven BLE scanner.
///
/// Creates the platform BLE manager **once** and subscribes to the adapter
/// event stream.  Each `DeviceDiscovered` / `DeviceUpdated` event is used to
/// update `state.ble_device_cache` with the peripheral's `local_name` and
/// RSSI.  This is far more reliable than the previous approach of tearing
/// down and re-creating the manager every 5 s with an 800 ms poll window,
/// which frequently caused CoreBluetooth to return `None` for `local_name`
/// (making the Muse look like an anonymous UUID and then being filtered out).
async fn run_ble_listener_task(state: AppState) {
    loop {
        // Stop when the outer scanner has been turned off.
        if !state.scanner_running.lock().map(|g| *g).unwrap_or(false) {
            return;
        }

        let Ok(manager) = BtManager::new().await else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        let Ok(adapters) = manager.adapters().await else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        let Some(adapter) = adapters.into_iter().next() else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        let Ok(mut events) = adapter.events().await else {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        // Start a continuous scan with no service-UUID filter so we see all
        // advertising packets (including Muse, which uses proprietary UUIDs).
        let _ = adapter.start_scan(ScanFilter::default()).await;

        // Process events until the stream ends or the scanner is stopped.
        loop {
            // When a BLE device is actively connecting, stop our scan so only
            // one CBCentralManager.scanForPeripherals() is active at a time.
            // On macOS, two concurrent scans suppress peripheral.connect()
            // delegate callbacks, causing connections to hang.
            if state.ble_scan_paused.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = adapter.stop_scan().await;
                while state.ble_scan_paused.load(std::sync::atomic::Ordering::Relaxed) {
                    if !state.scanner_running.lock().map(|g| *g).unwrap_or(false) {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                // Reconnection attempt finished — resume scan.
                let _ = adapter.start_scan(ScanFilter::default()).await;
            }

            // Short timeout so ble_scan_paused and scanner_running are
            // checked frequently even when no advertisements are arriving.
            let maybe_event = tokio::time::timeout(Duration::from_millis(300), events.next()).await;

            if !state.scanner_running.lock().map(|g| *g).unwrap_or(false) {
                return;
            }

            match maybe_event {
                // Adapter stream ended — break to outer loop to restart.
                Ok(None) => break,
                // Timeout — just re-check scanner_running and continue.
                Err(_) => continue,
                Ok(Some(CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id))) => {
                    if let Ok(peripheral) = adapter.peripheral(&id).await {
                        let mut name: Option<String> = None;
                        let mut rssi = 0i16;
                        if let Ok(Some(props)) = peripheral.properties().await {
                            name = props.local_name;
                            if let Some(rv) = props.rssi {
                                rssi = rv;
                            }
                        }
                        let key = format!("ble:{}", id);
                        if let Ok(mut cache) = state.ble_device_cache.lock() {
                            let entry = cache.entry(key).or_insert((None, 0i16, 0u64));
                            // Never overwrite a known name with None.
                            if name.is_some() {
                                entry.0 = name;
                            }
                            if rssi != 0 {
                                entry.1 = rssi;
                            }
                            entry.2 = now_unix_secs();
                        }
                    }
                }
                Ok(Some(_)) => {} // StateUpdate, ManufacturerData, etc. — ignored
            }
        }

        // Stream ended; brief pause before restarting.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn cortex_probe_headsets(
    client: &skill_devices::emotiv::client::CortexClient,
) -> anyhow::Result<Vec<skill_devices::emotiv::types::HeadsetInfo>> {
    let (mut rx, handle) = client.connect().await.map_err(|e| anyhow::anyhow!("{e}"))?;

    use skill_devices::emotiv::types::CortexEvent;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

    loop {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await;
        match ev {
            Ok(Some(CortexEvent::Authorized)) => break,
            Ok(Some(CortexEvent::Error(e))) => anyhow::bail!("{e}"),
            Ok(None) => anyhow::bail!("Channel closed before authorized"),
            Err(_) => anyhow::bail!("Timed out waiting for authorization"),
            _ => continue,
        }
    }

    handle.query_headsets().await.map_err(|e| anyhow::anyhow!("{e}"))?;

    loop {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await;
        match ev {
            Ok(Some(CortexEvent::HeadsetsQueried(list))) => return Ok(list),
            Ok(Some(CortexEvent::Error(e))) => anyhow::bail!("{e}"),
            Ok(None) => anyhow::bail!("Channel closed before headset query"),
            Err(_) => anyhow::bail!("Timed out waiting for headset query"),
            _ => continue,
        }
    }
}

async fn detect_cortex_devices(state: &AppState) -> Vec<DiscoveredDeviceResponse> {
    use skill_devices::emotiv::prelude::*;

    let (cfg_id, cfg_secret) = state
        .scanner_cortex_config
        .lock()
        .map(|g| (g.emotiv_client_id.clone(), g.emotiv_client_secret.clone()))
        .unwrap_or_else(|_| (String::new(), String::new()));

    let client_id = if cfg_id.trim().is_empty() {
        std::env::var("EMOTIV_CLIENT_ID").unwrap_or_default()
    } else {
        cfg_id
    };
    let client_secret = if cfg_secret.trim().is_empty() {
        std::env::var("EMOTIV_CLIENT_SECRET").unwrap_or_default()
    } else {
        cfg_secret
    };
    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        return Vec::new();
    }

    let config = CortexClientConfig {
        client_id,
        client_secret,
        auto_create_session: false,
        ..Default::default()
    };

    let client = CortexClient::new(config);
    let result = tokio::time::timeout(Duration::from_secs(12), cortex_probe_headsets(&client)).await;

    let Ok(Ok(headsets)) = result else {
        return Vec::new();
    };

    if headsets.is_empty() {
        return vec![DiscoveredDeviceResponse {
            id: "cortex:emotiv".to_string(),
            name: "Emotiv (Cortex)".to_string(),
            last_seen: now_unix_secs(),
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "cortex".to_string(),
        }];
    }

    headsets
        .into_iter()
        .map(|hs| DiscoveredDeviceResponse {
            id: format!("cortex:{}", hs.id),
            name: hs.id,
            last_seen: now_unix_secs(),
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "cortex".to_string(),
        })
        .collect()
}

fn detect_wifi_devices(cfg: &ScannerWifiConfigRequest) -> Vec<DiscoveredDeviceResponse> {
    let mut out = Vec::new();
    let now = now_unix_secs();

    let shield = cfg.wifi_shield_ip.trim();
    if !shield.is_empty() {
        out.push(DiscoveredDeviceResponse {
            id: format!("wifi:{shield}"),
            name: format!("OpenBCI WiFi Shield ({shield})"),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "wifi".to_string(),
        });
    }

    let galea = cfg.galea_ip.trim();
    if !galea.is_empty() {
        out.push(DiscoveredDeviceResponse {
            id: format!("galea:{galea}"),
            name: format!("Galea ({galea})"),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "wifi".to_string(),
        });
    }

    out
}

fn detect_manual_device_hints(state: &AppState) -> Vec<DiscoveredDeviceResponse> {
    let now = now_unix_secs();
    let mut out = vec![
        DiscoveredDeviceResponse {
            id: "neurosky".to_string(),
            name: "NeuroSky MindWave (serial)".to_string(),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "usb_serial".to_string(),
        },
        DiscoveredDeviceResponse {
            id: "brainvision:127.0.0.1:51244".to_string(),
            name: "BrainVision RDA (127.0.0.1:51244)".to_string(),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "wifi".to_string(),
        },
    ];

    let settings_device_id = state
        .skill_dir
        .lock()
        .ok()
        .map(|d| skill_settings::load_settings(&d).device_api.neurosity_device_id)
        .filter(|s| !s.trim().is_empty());

    let neurosity_device_id = settings_device_id
        .or_else(|| {
            std::env::var("SKILL_NEUROSITY_DEVICE_ID")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| {
            std::env::var("NEUROSITY_DEVICE_ID")
                .ok()
                .filter(|s| !s.trim().is_empty())
        });

    if let Some(device_id) = neurosity_device_id {
        out.push(DiscoveredDeviceResponse {
            id: format!("neurosity:{device_id}"),
            name: format!("Neurosity Crown/Notion ({device_id})"),
            last_seen: now,
            last_rssi: 0,
            is_paired: false,
            is_preferred: false,
            transport: "wifi".to_string(),
        });
    }

    out
}

async fn run_usb_scanner_task(state: AppState, mut stop_rx: oneshot::Receiver<()>) {
    // Spawn the persistent event-driven BLE listener.  It runs in a separate
    // task so the 5-second scanner tick is never blocked by BLE I/O, and the
    // CoreBluetooth/BlueZ adapter stays alive between ticks.
    tokio::spawn(run_ble_listener_task(state.clone()));

    let mut tick = tokio::time::interval(Duration::from_secs(5));
    let mut cortex_tick = 0u64;

    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            _ = tick.tick() => {
                // Timeout serial port enumeration — on Windows the FTDI
                // driver can occasionally stall `serialport::available_ports()`
                // for 10+ seconds when a dongle is mid-reset.  Without a
                // timeout this blocks the entire scanner tick.
                let ports = tokio::time::timeout(
                    Duration::from_secs(3),
                    tokio::task::spawn_blocking(detect_openbci_serial_ports),
                )
                .await
                .ok()
                .and_then(std::result::Result::ok)
                .unwrap_or_default();

                let mut usb_discovered: Vec<DiscoveredDeviceResponse> = ports.into_iter().map(|(port, display)| {
                    DiscoveredDeviceResponse {
                        id: format!("usb:{port}"),
                        name: display,
                        last_seen: now_unix_secs(),
                        last_rssi: 0,
                        is_paired: false,
                        is_preferred: false,
                        transport: "usb_serial".to_string(),
                    }
                }).collect();

                let cgx_ports = tokio::time::timeout(
                    Duration::from_secs(3),
                    tokio::task::spawn_blocking(detect_cgx_serial_ports),
                )
                .await
                .ok()
                .and_then(std::result::Result::ok)
                .unwrap_or_default();

                let cgx_discovered: Vec<DiscoveredDeviceResponse> = cgx_ports.into_iter().map(|(port, display)| {
                    DiscoveredDeviceResponse {
                        id: format!("cgx:{port}"),
                        name: display,
                        last_seen: now_unix_secs(),
                        last_rssi: 0,
                        is_paired: false,
                        is_preferred: false,
                        transport: "usb_serial".to_string(),
                    }
                }).collect();

                usb_discovered.extend(cgx_discovered);

                let ble_discovered = read_ble_cache(&state);

                let cortex_discovered = if cortex_tick.is_multiple_of(2) {
                    detect_cortex_devices(&state).await
                } else {
                    Vec::new()
                };

                let wifi_cfg = state
                    .scanner_wifi_config
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or(ScannerWifiConfigRequest {
                        wifi_shield_ip: String::new(),
                        galea_ip: String::new(),
                    });
                let wifi_discovered = detect_wifi_devices(&wifi_cfg);

                // NeuroField Q21 (PCAN-USB) — probe every other tick to avoid
                // holding the CAN bus open continuously.
                let neurofield_discovered = if cortex_tick.is_multiple_of(2) {
                    tokio::task::spawn_blocking(detect_neurofield_devices)
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                cortex_tick = cortex_tick.wrapping_add(1);

                let mut discovered = usb_discovered;
                discovered.extend(ble_discovered);
                discovered.extend(cortex_discovered);
                discovered.extend(wifi_discovered);
                discovered.extend(neurofield_discovered);

                // BrainBit (BLE via NeuroSDK2) — probe every other tick.
                let brainbit_discovered = if cortex_tick.is_multiple_of(2) {
                    tokio::task::spawn_blocking(detect_brainbit_devices)
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                discovered.extend(brainbit_discovered);

                // g.tec Unicorn (BLE) — probe every other tick.
                let gtec_discovered = if cortex_tick.is_multiple_of(2) {
                    tokio::task::spawn_blocking(detect_gtec_devices)
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                discovered.extend(gtec_discovered);

                // BrainMaster (USB serial)
                let brainmaster_discovered = tokio::time::timeout(
                    Duration::from_secs(3),
                    tokio::task::spawn_blocking(detect_brainmaster_devices),
                )
                .await
                .ok()
                .and_then(std::result::Result::ok)
                .unwrap_or_default();
                discovered.extend(brainmaster_discovered);

                // Manual-connect device hints (always visible in scanner list).
                discovered.extend(detect_manual_device_hints(&state));

                let discovered_count = discovered.len();

                if let Ok(mut guard) = state.devices.lock() {
                    let old: HashMap<String, DiscoveredDeviceResponse> =
                        guard.iter().map(|d| (d.id.clone(), d.clone())).collect();

                    // Build a set of paired device IDs from the authoritative
                    // status list.  This ensures devices are correctly marked
                    // as paired even on the very first scan tick after a daemon
                    // restart (when `old` is empty and carries no is_paired state).
                    let paired_ids: HashSet<String> = state
                        .status
                        .lock()
                        .map(|s| s.paired_devices.iter().map(|p| p.id.clone()).collect())
                        .unwrap_or_default();

                    let keep_other: Vec<DiscoveredDeviceResponse> = guard
                        .iter()
                        .filter(|d| {
                            !d.id.starts_with("usb:")
                                && !d.id.starts_with("cgx:")
                                && !d.id.starts_with("ble:")
                                && !d.id.starts_with("cortex:")
                                && !d.id.starts_with("wifi:")
                                && !d.id.starts_with("galea:")
                                && !d.id.starts_with("neurofield:")
                                && !d.id.starts_with("brainbit:")
                                && !d.id.starts_with("gtec:")
                                && !d.id.starts_with("brainmaster:")
                                && !d.id.starts_with("neurosky")
                                && !d.id.starts_with("neurosity:")
                                && !d.id.starts_with("brainvision:")
                                && !d.id.starts_with("rda:")
                        })
                        .cloned()
                        .collect();

                    let current_ids: HashSet<String> =
                        discovered.iter().map(|d| d.id.clone()).collect();

                    let mut merged: Vec<DiscoveredDeviceResponse> = keep_other;
                    for mut d in discovered {
                        // paired_ids (from settings) takes precedence so that
                        // devices remain marked as paired after a daemon restart.
                        d.is_paired = paired_ids.contains(&d.id);
                        if let Some(prev) = old.get(&d.id) {
                            if !d.is_paired {
                                d.is_paired = prev.is_paired;
                            }
                            d.is_preferred = prev.is_preferred;
                        }
                        merged.push(d);
                    }

                    merged.retain(|d| {
                        (!d.id.starts_with("usb:")
                            && !d.id.starts_with("cgx:")
                            && !d.id.starts_with("ble:")
                            && !d.id.starts_with("cortex:")
                            && !d.id.starts_with("wifi:")
                            && !d.id.starts_with("galea:")
                            && !d.id.starts_with("neurofield:")
                            && !d.id.starts_with("brainbit:")
                            && !d.id.starts_with("gtec:")
                            && !d.id.starts_with("brainmaster:")
                            && !d.id.starts_with("neurosky")
                            && !d.id.starts_with("neurosity:")
                            && !d.id.starts_with("brainvision:")
                            && !d.id.starts_with("rda:"))
                            || current_ids.contains(&d.id)
                    });
                    *guard = merged;
                }

                push_device_log(
                    &state,
                    "scanner",
                    &format!("scan tick discovered {} devices", discovered_count),
                );
            }
        }
    }
}

fn add_client(state: &AppState, peer: &str) {
    if let Ok(mut guard) = state.tracker.lock() {
        guard.clients.push(WsClient {
            peer: peer.to_string(),
            connected_at: now_unix_secs(),
        });
    }
}

fn remove_client(state: &AppState, peer: &str) {
    if let Ok(mut guard) = state.tracker.lock() {
        if let Some(idx) = guard.clients.iter().position(|c| c.peer == peer) {
            guard.clients.remove(idx);
        }
    }
}

fn record_request(state: &AppState, peer: String, command: String, ok: bool) {
    if let Ok(mut guard) = state.tracker.lock() {
        guard.add_request(peer, command, ok, now_unix_secs());
    }
}

fn push_device_log(state: &AppState, tag: &str, msg: &str) {
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

fn extract_bearer_token(headers: &HeaderMap, request: &axum::extract::Request) -> Option<String> {
    // 1. Authorization: Bearer <token> header
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        if let Ok(value) = value.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    // 2. ?token=<token> query parameter (for WebSocket — browsers can't set headers)
    if let Some(query) = request.uri().query() {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("token=") {
                let decoded = urlencoding::decode(val).unwrap_or_default();
                return Some(decoded.into_owned());
            }
        }
    }

    None
}

enum AuthDecision {
    Allowed,
    MissingOrInvalid,
    Forbidden,
}

fn auth_decision(headers: &HeaderMap, request: &axum::extract::Request, state: &AppState) -> AuthDecision {
    let Some(token) = extract_bearer_token(headers, request) else {
        return AuthDecision::MissingOrInvalid;
    };

    // Check in-memory default token first (fast path)
    if let Ok(current) = state.auth_token.lock() {
        if token == *current {
            return AuthDecision::Allowed;
        }
    }

    // Check on-disk default token (handles refresh without restart)
    if let Ok(path) = token_path() {
        if let Ok(file_token) = std::fs::read_to_string(path) {
            if token == file_token.trim() {
                return AuthDecision::Allowed;
            }
        }
    }

    // Check multi-token store and distinguish invalid token vs ACL denied.
    let method = request.method().as_str();
    let path = request.uri().path();
    if let Ok(mut store) = state.token_store.lock() {
        if store.authorize(&token, method, path) {
            return AuthDecision::Allowed;
        }
        if store.validate(&token).is_some() {
            return AuthDecision::Forbidden;
        }
        return AuthDecision::MissingOrInvalid;
    }

    AuthDecision::MissingOrInvalid
}

fn load_or_create_token() -> anyhow::Result<String> {
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

fn token_path() -> anyhow::Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("unable to resolve config directory"))?;
    Ok(base.join("skill").join("daemon").join("auth.token"))
}

/// Write JSON to `path` atomically: write to a sibling `.tmp` file then
/// rename into place.  A crash mid-write leaves the original file intact.
fn write_json_atomic(path: &Path, json: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

fn write_string_atomic(path: &Path, content: &str) -> anyhow::Result<()> {
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
fn tighten_file_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn tighten_file_permissions(path: &Path) -> anyhow::Result<()> {
    restrict_windows_acl(path)
}

#[cfg(unix)]
fn tighten_dir_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn tighten_dir_permissions(path: &Path) -> anyhow::Result<()> {
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

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use axum::routing::get;
    use futures::{SinkExt, StreamExt};
    use tempfile::TempDir;
    use tokio_tungstenite::connect_async;
    use tower::ServiceExt;

    #[test]
    fn ble_filter_accepts_known_eeg_devices() {
        // Muse family
        assert!(is_known_eeg_ble_name("Muse-AB12"));
        assert!(is_known_eeg_ble_name("MuseS-F921")); // Athena
        assert!(is_known_eeg_ble_name("Muse S-1234"));
        assert!(is_known_eeg_ble_name("Muse-2-XY99"));
        assert!(is_known_eeg_ble_name("MUSE-AB12")); // case-insensitive

        // Other EEG families
        assert!(is_known_eeg_ble_name("Ganglion-1234"));
        assert!(is_known_eeg_ble_name("MW75-Neuro"));
        assert!(is_known_eeg_ble_name("Hermes-001"));
        assert!(is_known_eeg_ble_name("Mendi-XY"));
        assert!(is_known_eeg_ble_name("IGE-Guardian"));
        assert!(is_known_eeg_ble_name("BrainBit-EEG"));
        assert!(is_known_eeg_ble_name("Unicorn-EEG"));
    }

    #[test]
    fn ble_filter_rejects_unrelated_devices() {
        assert!(!is_known_eeg_ble_name("JBL Flip 5"));
        assert!(!is_known_eeg_ble_name("Apple Watch"));
        assert!(!is_known_eeg_ble_name("iPhone 15"));
        assert!(!is_known_eeg_ble_name("AirPods Pro"));
        // Anonymous UUID-only names (empty string)
        assert!(!is_known_eeg_ble_name(""));
        // Random UUID-style names that BLE devices sometimes advertise
        assert!(!is_known_eeg_ble_name("8282ba24-1ffa-8bd5-659a-4b02f6783927"));
    }

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
    fn scanner_wifi_detects_wifi_transports() {
        let cfg = ScannerWifiConfigRequest {
            wifi_shield_ip: "192.168.4.1".to_string(),
            galea_ip: "10.0.0.42".to_string(),
        };
        let devices = detect_wifi_devices(&cfg);
        assert_eq!(devices.len(), 2);
        assert!(devices
            .iter()
            .any(|d| d.id == "wifi:192.168.4.1" && d.transport == "wifi"));
        assert!(devices
            .iter()
            .any(|d| d.id == "galea:10.0.0.42" && d.transport == "wifi"));
    }

    #[test]
    fn manual_hints_include_usb_serial_and_wifi() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("test".to_string(), td.path().to_path_buf());
        let hints = detect_manual_device_hints(&state);

        assert!(hints.iter().any(|d| d.id == "neurosky" && d.transport == "usb_serial"));
        assert!(hints
            .iter()
            .any(|d| d.id == "brainvision:127.0.0.1:51244" && d.transport == "wifi"));
    }

    #[test]
    fn ble_cache_filters_stale_and_unknown_devices() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("test".to_string(), td.path().to_path_buf());
        let now = now_unix_secs();

        {
            let mut cache = state.ble_device_cache.lock().unwrap();
            cache.insert("ble:muse".to_string(), (Some("Muse-AB12".to_string()), -48, now));
            cache.insert(
                "ble:stale".to_string(),
                (Some("Muse-OLD".to_string()), -70, now.saturating_sub(121)),
            );
            cache.insert("ble:jbl".to_string(), (Some("JBL Flip 5".to_string()), -30, now));
            cache.insert("ble:noname".to_string(), (None, -40, now));
        }

        let found = read_ble_cache(&state);
        assert_eq!(found.len(), 1, "only fresh known EEG BLE devices should remain");
        assert_eq!(found[0].id, "ble:muse");
        assert_eq!(found[0].transport, "ble");
    }

    #[test]
    fn ble_cache_large_scan_is_fast() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("test".to_string(), td.path().to_path_buf());
        let now = now_unix_secs();

        {
            let mut cache = state.ble_device_cache.lock().unwrap();
            for i in 0..10_000 {
                let id = format!("ble:{i}");
                let name = if i % 2 == 0 {
                    Some(format!("Muse-{i:04}"))
                } else {
                    Some(format!("Speaker-{i:04}"))
                };
                cache.insert(id, (name, -50, now));
            }
        }

        let t0 = std::time::Instant::now();
        let found = read_ble_cache(&state);
        let elapsed = t0.elapsed();

        assert_eq!(found.len(), 5_000);
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "BLE cache filter too slow: {elapsed:?}"
        );
    }

    #[test]
    fn auth_decision_missing_invalid_and_query_bearer() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("default-token".to_string(), td.path().to_path_buf());
        let headers = HeaderMap::new();

        let req_missing = Request::builder().uri("/v1/status").body(Body::empty()).unwrap();
        assert!(matches!(
            auth_decision(&headers, &req_missing, &state),
            AuthDecision::MissingOrInvalid
        ));

        let req_query = Request::builder()
            .uri("/v1/status?token=default-token")
            .body(Body::empty())
            .unwrap();
        assert!(matches!(
            auth_decision(&headers, &req_query, &state),
            AuthDecision::Allowed
        ));

        let mut bad_headers = HeaderMap::new();
        bad_headers.insert(header::AUTHORIZATION, "Bearer totally-wrong".parse().unwrap());
        let req_bad = Request::builder().uri("/v1/status").body(Body::empty()).unwrap();
        assert!(matches!(
            auth_decision(&bad_headers, &req_bad, &state),
            AuthDecision::MissingOrInvalid
        ));
    }

    #[test]
    fn auth_decision_forbidden_when_acl_denies_endpoint() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("default-token".to_string(), td.path().to_path_buf());

        let stream_secret = {
            let mut store = state.token_store.lock().unwrap();
            let tok = store
                .create(
                    "stream".to_string(),
                    crate::auth::TokenAcl::Stream,
                    crate::auth::TokenExpiry::Never,
                )
                .expect("create stream token");
            tok.token
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {stream_secret}").parse().unwrap(),
        );

        // Stream ACL allows read status.
        let req_get = Request::builder()
            .method("GET")
            .uri("/v1/status")
            .body(Body::empty())
            .unwrap();
        assert!(matches!(
            auth_decision(&headers, &req_get, &state),
            AuthDecision::Allowed
        ));

        // But forbids control mutation.
        let req_post = Request::builder()
            .method("POST")
            .uri("/v1/control/start-session")
            .body(Body::empty())
            .unwrap();
        assert!(matches!(
            auth_decision(&headers, &req_post, &state),
            AuthDecision::Forbidden
        ));
    }

    fn test_app(state: AppState) -> Router {
        let v1 = Router::new()
            .route("/version", get(version))
            .route("/events", get(ws_events))
            .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

        Router::new()
            .route("/healthz", get(healthz))
            .nest("/v1", v1)
            .with_state(state)
    }

    async fn spawn_test_server(
        app: Router,
    ) -> (
        SocketAddr,
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await;
        });
        (addr, tx, handle)
    }

    fn wait_for_client_count(state: &AppState, want: usize) -> bool {
        for _ in 0..200 {
            let got = state.tracker.lock().map(|g| g.clients.len()).unwrap_or(0);
            if got == want {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }

    #[tokio::test]
    async fn router_e2e_healthz_public_and_version_protected() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("test-token".to_string(), td.path().to_path_buf());
        let app = test_app(state.clone());

        let res = app
            .clone()
            .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .clone()
            .oneshot(Request::builder().uri("/v1/version").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let mut req = Request::builder().uri("/v1/version").body(Body::empty()).unwrap();
        req.headers_mut()
            .insert(header::AUTHORIZATION, "Bearer test-token".parse().unwrap());
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn router_e2e_forbidden_acl_and_ws_auth_gate() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("default-token".to_string(), td.path().to_path_buf());
        let app = test_app(state.clone());

        let stream_secret = {
            let mut store = state.token_store.lock().unwrap();
            let tok = store
                .create(
                    "stream".to_string(),
                    crate::auth::TokenAcl::Stream,
                    crate::auth::TokenExpiry::Never,
                )
                .expect("create stream token");
            tok.token
        };

        // ACL denial on non-read method.
        let mut req = Request::builder()
            .method("POST")
            .uri("/v1/version")
            .body(Body::empty())
            .unwrap();
        req.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {stream_secret}").parse().unwrap(),
        );
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);

        // WS endpoint with missing token should be unauthorized.
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // WS endpoint with valid token passes auth layer; without upgrade headers
        // handler will return 400 (expected in this in-process HTTP test).
        let mut req = Request::builder()
            .uri("/v1/events?token=default-token")
            .body(Body::empty())
            .unwrap();
        req.headers_mut().insert(header::CONNECTION, "upgrade".parse().unwrap());
        req.headers_mut().insert(header::UPGRADE, "websocket".parse().unwrap());
        req.headers_mut().insert("sec-websocket-version", "13".parse().unwrap());
        req.headers_mut()
            .insert("sec-websocket-key", "dGVzdA==".parse().unwrap());
        let res = app.clone().oneshot(req).await.unwrap();
        assert_ne!(res.status(), StatusCode::UNAUTHORIZED);

        // Unauthorized body shape contract.
        let res = app
            .clone()
            .oneshot(Request::builder().uri("/v1/version").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(v["code"], "unauthorized");
    }

    #[tokio::test]
    async fn router_e2e_forbidden_body_shape() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("default-token".to_string(), td.path().to_path_buf());
        let app = test_app(state.clone());

        let stream_secret = {
            let mut store = state.token_store.lock().unwrap();
            let tok = store
                .create(
                    "stream".to_string(),
                    crate::auth::TokenAcl::Stream,
                    crate::auth::TokenExpiry::Never,
                )
                .expect("create stream token");
            tok.token
        };

        let mut req = Request::builder()
            .method("POST")
            .uri("/v1/version")
            .body(Body::empty())
            .unwrap();
        req.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {stream_secret}").parse().unwrap(),
        );

        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(v["code"], "forbidden");
    }

    #[test]
    fn extract_bearer_token_header_and_query() {
        let req = Request::builder()
            .uri("/v1/events?token=query-token")
            .body(Body::empty())
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer header-token".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers, &req).as_deref(), Some("header-token"));

        let req = Request::builder()
            .uri("/v1/events?token=query-token")
            .body(Body::empty())
            .unwrap();
        let headers = HeaderMap::new();
        assert_eq!(extract_bearer_token(&headers, &req).as_deref(), Some("query-token"));

        let req = Request::builder()
            .uri("/v1/events?token=abc%2Bdef%3D")
            .body(Body::empty())
            .unwrap();
        let headers = HeaderMap::new();
        assert_eq!(extract_bearer_token(&headers, &req).as_deref(), Some("abc+def="));
    }

    #[test]
    fn extract_bearer_token_rejects_malformed_auth_header() {
        let req = Request::builder().uri("/v1/version").body(Body::empty()).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Basic abc123".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers, &req), None);

        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers, &req), None);

        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "bearer token".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers, &req), None);
    }

    #[test]
    fn detect_wifi_devices_empty_config_returns_none() {
        let cfg = ScannerWifiConfigRequest {
            wifi_shield_ip: String::new(),
            galea_ip: String::new(),
        };
        let found = detect_wifi_devices(&cfg);
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn auth_middleware_allows_options_without_token() {
        async fn ok() -> &'static str {
            "ok"
        }

        let td = TempDir::new().unwrap();
        let state = AppState::new("token".to_string(), td.path().to_path_buf());
        let app = Router::new()
            .route("/v1/echo", get(ok).post(ok))
            .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
            .with_state(state);

        let res = app
            .oneshot(
                Request::builder()
                    .method(axum::http::Method::OPTIONS)
                    .uri("/v1/echo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn scanner_start_stop_race_is_safe() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".to_string(), td.path().to_path_buf());

        let start = control_scanner_start(State(state.clone())).await.0;
        assert!(start.running);
        assert!(state.scanner_running.lock().map(|g| *g).unwrap_or(false));

        // Start again should be idempotent.
        let start2 = control_scanner_start(State(state.clone())).await.0;
        assert!(start2.running);

        let stop = control_scanner_stop(State(state.clone())).await.0;
        assert!(!stop.running);

        // Give background task a brief chance to observe stop signal.
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(!state.scanner_running.lock().map(|g| *g).unwrap_or(true));
        assert!(state.scanner_stop_tx.lock().map(|g| g.is_none()).unwrap_or(true));
    }

    #[tokio::test]
    async fn scanner_stop_without_start_is_safe() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".to_string(), td.path().to_path_buf());

        let stop = control_scanner_stop(State(state.clone())).await.0;
        assert!(!stop.running);
        assert!(!state.scanner_running.lock().map(|g| *g).unwrap_or(true));
    }

    #[tokio::test]
    async fn ws_receives_broadcast_and_tracks_clients() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("ws-token".to_string(), td.path().to_path_buf());
        let app = test_app(state.clone());
        let (addr, shutdown_tx, handle) = spawn_test_server(app).await;

        let url = format!("ws://{addr}/v1/events?token=ws-token");
        let (mut ws, _resp) = connect_async(url).await.expect("ws connect");
        assert!(
            wait_for_client_count(&state, 1),
            "client should be tracked after ws connect"
        );

        let _ = state.events_tx.send(EventEnvelope {
            r#type: "Ping".to_string(),
            ts_unix_ms: 1,
            correlation_id: None,
            payload: serde_json::json!({"ok": true}),
        });

        let mut saw_ping = false;
        for _ in 0..8 {
            let msg = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
                .await
                .expect("ws receive timeout")
                .expect("ws closed")
                .expect("ws error");
            if let tokio_tungstenite::tungstenite::Message::Text(txt) = msg {
                let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
                if v["type"] == "Ping" {
                    assert_eq!(v["payload"]["ok"], true);
                    saw_ping = true;
                    break;
                }
            }
        }
        assert!(saw_ping, "did not observe Ping event on websocket stream");

        let _ = ws.close(None).await;

        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }

    #[tokio::test]
    async fn ws_non_text_frame_does_not_break_stream() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("ws-token".to_string(), td.path().to_path_buf());
        let app = test_app(state.clone());
        let (addr, shutdown_tx, handle) = spawn_test_server(app).await;

        let url = format!("ws://{addr}/v1/events?token=ws-token");
        let (mut ws, _resp) = connect_async(url).await.expect("ws connect");

        // Send a binary frame; server should ignore unsupported frame types,
        // not drop the socket.
        let _ = ws
            .send(tokio_tungstenite::tungstenite::Message::Binary(vec![0, 1, 2].into()))
            .await;

        let _ = state.events_tx.send(EventEnvelope {
            r#type: "AfterBinary".to_string(),
            ts_unix_ms: 2,
            correlation_id: None,
            payload: serde_json::json!({"ok": true}),
        });

        let mut saw = false;
        for _ in 0..8 {
            let msg = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
                .await
                .expect("ws receive timeout")
                .expect("ws closed")
                .expect("ws error");
            if let tokio_tungstenite::tungstenite::Message::Text(txt) = msg {
                let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
                if v["type"] == "AfterBinary" {
                    saw = true;
                    break;
                }
            }
        }
        assert!(saw, "stream should continue after non-text frame");

        let _ = ws.close(None).await;
        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }

    #[test]
    fn ws_client_tracker_add_remove_is_consistent() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("ws-token".to_string(), td.path().to_path_buf());

        add_client(&state, "127.0.0.1:10001");
        add_client(&state, "127.0.0.1:10002");
        let n = state.tracker.lock().map(|g| g.clients.len()).unwrap_or(0);
        assert_eq!(n, 2);

        remove_client(&state, "127.0.0.1:10001");
        let n = state.tracker.lock().map(|g| g.clients.len()).unwrap_or(0);
        assert_eq!(n, 1);

        // Removing unknown peer is a no-op.
        remove_client(&state, "127.0.0.1:99999");
        let n = state.tracker.lock().map(|g| g.clients.len()).unwrap_or(0);
        assert_eq!(n, 1);

        remove_client(&state, "127.0.0.1:10002");
        let n = state.tracker.lock().map(|g| g.clients.len()).unwrap_or(0);
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn scanner_config_roundtrip_wifi_and_cortex() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".to_string(), td.path().to_path_buf());

        let wifi = ScannerWifiConfigRequest {
            wifi_shield_ip: "192.168.4.1".into(),
            galea_ip: "10.0.0.10".into(),
        };
        let out_wifi = control_scanner_wifi_config(State(state.clone()), Json(wifi.clone()))
            .await
            .0;
        assert_eq!(out_wifi.wifi_shield_ip, wifi.wifi_shield_ip);
        assert_eq!(out_wifi.galea_ip, wifi.galea_ip);
        let stored_wifi = state.scanner_wifi_config.lock().unwrap().clone();
        assert_eq!(stored_wifi.wifi_shield_ip, wifi.wifi_shield_ip);
        assert_eq!(stored_wifi.galea_ip, wifi.galea_ip);

        let cortex = ScannerCortexConfigRequest {
            emotiv_client_id: "client-id".into(),
            emotiv_client_secret: "client-secret".into(),
        };
        let out_cortex = control_scanner_cortex_config(State(state.clone()), Json(cortex.clone()))
            .await
            .0;
        assert_eq!(out_cortex.emotiv_client_id, cortex.emotiv_client_id);
        assert_eq!(out_cortex.emotiv_client_secret, cortex.emotiv_client_secret);
        let stored_cortex = state.scanner_cortex_config.lock().unwrap().clone();
        assert_eq!(stored_cortex.emotiv_client_id, cortex.emotiv_client_id);
        assert_eq!(stored_cortex.emotiv_client_secret, cortex.emotiv_client_secret);
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
}
