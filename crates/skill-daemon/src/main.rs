mod activity;
mod auth;
pub(crate) mod cmd_dispatch;
pub(crate) mod embed;
mod routes;
mod service_installer;
mod session_runner;
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
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use btleplug::{
    api::{Central as _, Manager as _, Peripheral as _, ScanFilter},
    platform::Manager as BtManager,
};
use rand::RngCore;
use skill_daemon_common::{
    ApiError, DeviceLogEntry, DiscoveredDeviceResponse, EventEnvelope, ForgetDeviceRequest, HealthResponse,
    LslDiscoveredStreamResponse, PairDeviceRequest, ScannerCortexConfigRequest, ScannerStateResponse,
    ScannerWifiConfigRequest, SessionControlRequest, SetPreferredDeviceRequest, StatusResponse, VersionResponse,
    WsClient, WsPortResponse, WsRequestLog, DAEMON_NAME, PROTOCOL_VERSION,
};
use tokio::sync::{broadcast, oneshot};
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

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
    let state = AppState::new(load_or_create_token()?, skill_dir);
    activity::start_workers(state.clone());

    let v1 = Router::new()
        .route("/version", get(version))
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
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    // Root-level command tunnel for CLI HTTP mode (POST / with JSON body)
    let root_cmd = Router::new()
        .route("/", axum::routing::post(cmd_tunnel_root))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/service/install", axum::routing::post(service_install))
        .route("/service/uninstall", axum::routing::post(service_uninstall))
        .route("/service/status", get(service_status))
        .nest("/v1", v1)
        .merge(root_cmd)
        .with_state(state);

    let addr = daemon_addr();
    info!(%addr, "skill daemon listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown)
        .await?;

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

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "skill_daemon=info,info".into()),
        )
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
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })),
    }
}

async fn service_uninstall() -> Json<serde_json::Value> {
    let bin = std::env::current_exe().unwrap_or_default();
    let installer = crate::service_installer::ServiceInstaller::new(bin);
    match installer.uninstall() {
        Ok(()) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })),
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

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let current = state.status.lock().map(|g| g.clone()).unwrap_or(StatusResponse {
        state: "disconnected".to_string(),
        device_name: None,
        sample_count: 0,
        battery: 0.0,
        device_error: None,
        target_name: None,
        retry_attempt: 0,
        retry_countdown_secs: 0,
        paired_devices: Vec::new(),
    });
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

    if let Ok(mut status) = state.status.lock() {
        if !status.paired_devices.iter().any(|d| d.id == req.id) {
            status.paired_devices.push(skill_daemon_common::PairedDeviceResponse {
                id: req.id,
                name: paired_name.unwrap_or_else(|| "Unknown".to_string()),
                last_seen: now_unix_secs(),
            });
        }
    }

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

    Json(out)
}

fn default_status(state: &str) -> StatusResponse {
    StatusResponse {
        state: state.to_string(),
        device_name: None,
        sample_count: 0,
        battery: 0.0,
        device_error: None,
        target_name: None,
        retry_attempt: 0,
        retry_countdown_secs: 0,
        paired_devices: Vec::new(),
    }
}

async fn control_retry_connect(State(state): State<AppState>) -> Json<StatusResponse> {
    let mut out = default_status("connecting");

    // Cancel any existing session before retrying.
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }

    // Check if we have a preferred/target device to reconnect to.
    let target = state.status.lock().ok().and_then(|s| s.target_name.clone());

    if let Ok(mut status) = state.status.lock() {
        status.state = "connecting".to_string();
        status.retry_attempt = 0;
        status.retry_countdown_secs = 0;
        status.device_error = None;
        out = status.clone();
    }

    // Spawn session runner if target is openbci or usb device.
    let is_openbci = target
        .as_deref()
        .map(|t| t == "openbci" || t.starts_with("usb:") || t.starts_with("cgx:"))
        .unwrap_or(false);
    if is_openbci {
        let handle = session_runner::spawn_openbci_session(state.clone());
        if let Ok(mut slot) = state.session_handle.lock() {
            *slot = Some(handle);
        }
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
    if let Some(ref t) = target {
        if let Some(port) = t.strip_prefix("usb:") {
            let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
            let mut settings = skill_settings::load_settings(&skill_dir);
            settings.openbci.serial_port = port.to_string();
            let path = skill_settings::settings_path(&skill_dir);
            if let Ok(json) = serde_json::to_string_pretty(&settings) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    if let Ok(mut status) = state.status.lock() {
        status.state = "connecting".to_string();
        status.target_name = req.target;
        status.device_error = None;
        out = status.clone();
    }

    // Determine if this should launch an OpenBCI session.
    // Accept "openbci" directly, or any "usb:" / "cgx:" device ID.
    let is_openbci = target
        .as_deref()
        .map(|t| t == "openbci" || t.starts_with("usb:") || t.starts_with("cgx:"))
        .unwrap_or(false);

    if is_openbci {
        // Cancel any existing session before starting a new one.
        if let Ok(mut slot) = state.session_handle.lock() {
            if let Some(handle) = slot.take() {
                let _ = handle.cancel_tx.send(());
            }
        }
        let handle = session_runner::spawn_openbci_session(state.clone());
        if let Ok(mut slot) = state.session_handle.lock() {
            *slot = Some(handle);
        }
    }

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

    if let Ok(mut status) = state.status.lock() {
        status.state = "connecting".to_string();
        status.target_name = req.target;
        status.device_error = None;
        out = status.clone();
    }

    let is_openbci = target
        .as_deref()
        .map(|t| t == "openbci" || t.starts_with("usb:") || t.starts_with("cgx:"))
        .unwrap_or(false);
    if is_openbci {
        let handle = session_runner::spawn_openbci_session(state.clone());
        if let Ok(mut slot) = state.session_handle.lock() {
            *slot = Some(handle);
        }
    }

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
                        error!(%skipped, "websocket client lagged behind event stream");
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

async fn detect_ble_devices() -> Vec<DiscoveredDeviceResponse> {
    let Ok(manager) = BtManager::new().await else {
        return Vec::new();
    };
    let Ok(adapters) = manager.adapters().await else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for adapter in adapters {
        let _ = adapter.start_scan(ScanFilter::default()).await;
        tokio::time::sleep(Duration::from_millis(800)).await;

        let Ok(peripherals) = adapter.peripherals().await else {
            continue;
        };

        for p in peripherals {
            let id = format!("ble:{}", p.id());
            let mut name = p.id().to_string();
            let mut rssi = 0i16;

            if let Ok(Some(props)) = p.properties().await {
                if let Some(local_name) = props.local_name {
                    name = local_name;
                }
                if let Some(rv) = props.rssi {
                    rssi = rv;
                }
            }

            out.push(DiscoveredDeviceResponse {
                id,
                name,
                last_seen: now_unix_secs(),
                last_rssi: rssi,
                is_paired: false,
                is_preferred: false,
                transport: "ble".to_string(),
            });
        }
    }

    out
}

async fn cortex_probe_headsets(
    client: &skill_devices::emotiv::client::CortexClient,
) -> Result<Vec<skill_devices::emotiv::types::HeadsetInfo>, String> {
    let (mut rx, handle) = client.connect().await.map_err(|e| e.to_string())?;

    use skill_devices::emotiv::types::CortexEvent;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

    loop {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await;
        match ev {
            Ok(Some(CortexEvent::Authorized)) => break,
            Ok(Some(CortexEvent::Error(e))) => return Err(e),
            Ok(None) => return Err("Channel closed before authorized".into()),
            Err(_) => return Err("Timed out waiting for authorization".into()),
            _ => continue,
        }
    }

    handle.query_headsets().await.map_err(|e| e.to_string())?;

    loop {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await;
        match ev {
            Ok(Some(CortexEvent::HeadsetsQueried(list))) => return Ok(list),
            Ok(Some(CortexEvent::Error(e))) => return Err(e),
            Ok(None) => return Err("Channel closed before headset query".into()),
            Err(_) => return Err("Timed out waiting for headset query".into()),
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

async fn run_usb_scanner_task(state: AppState, mut stop_rx: oneshot::Receiver<()>) {
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

                let ble_discovered = detect_ble_devices().await;

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
                cortex_tick = cortex_tick.wrapping_add(1);

                let mut discovered = usb_discovered;
                discovered.extend(ble_discovered);
                discovered.extend(cortex_discovered);
                discovered.extend(wifi_discovered);
                let discovered_count = discovered.len();

                if let Ok(mut guard) = state.devices.lock() {
                    let old: HashMap<String, DiscoveredDeviceResponse> =
                        guard.iter().map(|d| (d.id.clone(), d.clone())).collect();

                    let keep_other: Vec<DiscoveredDeviceResponse> = guard
                        .iter()
                        .filter(|d| {
                            !d.id.starts_with("usb:")
                                && !d.id.starts_with("cgx:")
                                && !d.id.starts_with("ble:")
                                && !d.id.starts_with("cortex:")
                                && !d.id.starts_with("wifi:")
                                && !d.id.starts_with("galea:")
                        })
                        .cloned()
                        .collect();

                    let current_ids: HashSet<String> =
                        discovered.iter().map(|d| d.id.clone()).collect();

                    let mut merged: Vec<DiscoveredDeviceResponse> = keep_other;
                    for mut d in discovered {
                        if let Some(prev) = old.get(&d.id) {
                            d.is_paired = prev.is_paired;
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
                            && !d.id.starts_with("galea:"))
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
