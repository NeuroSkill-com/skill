mod activity;
mod routes;
mod service_installer;
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
        .route("/events", get(ws_events))
        .route("/events/push", axum::routing::post(push_event))
        .merge(routes::labels::router())
        .merge(routes::history::router())
        .merge(routes::settings::router())
        .merge(routes::api::router())
        .merge(routes::analysis::router())
        .merge(routes::search::router())
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/service/install", axum::routing::post(service_install))
        .route("/service/uninstall", axum::routing::post(service_uninstall))
        .route("/service/status", get(service_status))
        .nest("/v1", v1)
        .with_state(state);

    let addr = daemon_addr();
    info!(%addr, "skill daemon listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}

fn skill_data_dir() -> PathBuf {
    std::env::var("SKILL_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".skill"))
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
    let mut out = default_status("scanning");

    if let Ok(mut status) = state.status.lock() {
        status.state = "scanning".to_string();
        status.retry_attempt = 0;
        status.retry_countdown_secs = 0;
        out = status.clone();
    }

    Json(out)
}

async fn control_cancel_retry(State(state): State<AppState>) -> Json<StatusResponse> {
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

    if let Ok(mut status) = state.status.lock() {
        status.state = "connecting".to_string();
        status.target_name = req.target;
        status.device_error = None;
        out = status.clone();
    }

    Json(out)
}

async fn control_switch_session(
    State(state): State<AppState>,
    Json(req): Json<SessionControlRequest>,
) -> Json<StatusResponse> {
    let mut out = default_status("connecting");

    if let Ok(mut status) = state.status.lock() {
        status.state = "connecting".to_string();
        status.target_name = req.target;
        status.device_error = None;
        out = status.clone();
    }

    Json(out)
}

async fn control_cancel_session(State(state): State<AppState>) -> Json<StatusResponse> {
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
            handle_ws(socket, state.events_tx.subscribe()).await;
            remove_client(&state, &peer);
        }
    })
}

async fn push_event(State(state): State<AppState>, Json(envelope): Json<EventEnvelope>) -> Json<serde_json::Value> {
    let _ = state.events_tx.send(envelope);
    Json(serde_json::json!({ "ok": true }))
}

async fn handle_ws(mut socket: WebSocket, mut rx: broadcast::Receiver<EventEnvelope>) {
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

    loop {
        match rx.recv().await {
            Ok(event) => {
                let payload = match serde_json::to_string(&event) {
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

    if is_authorized(headers, state.auth_token.as_str()) {
        record_request(&state, peer, command, true);
        return next.run(request).await;
    }

    record_request(&state, peer, command, false);

    let body = Json(ApiError {
        code: "unauthorized",
        message: "missing or invalid bearer token".to_string(),
    });

    (StatusCode::UNAUTHORIZED, body).into_response()
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
                let vid_match = usb.vid == 0x0403 && usb.pid == 0x6015;
                let product_match = usb
                    .product
                    .as_deref()
                    .map(|p| {
                        let pl = p.to_lowercase();
                        pl.contains("ft231x") || pl.contains("openbci") || pl.contains("ftdi")
                    })
                    .unwrap_or(false);
                vid_match || product_match
            }
            _ => lower.contains("ttyusb") || lower.contains("usbserial"),
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
                let ports = tokio::task::spawn_blocking(detect_openbci_serial_ports)
                    .await
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

                let cgx_ports = tokio::task::spawn_blocking(detect_cgx_serial_ports)
                    .await
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

fn is_authorized(headers: HeaderMap, expected_token: &str) -> bool {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return false;
    };

    let Ok(value) = value.to_str() else {
        return false;
    };

    let Some(token) = value.strip_prefix("Bearer ") else {
        return false;
    };

    token == expected_token
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

    std::fs::write(&token_path, format!("{token}\n"))?;
    tighten_file_permissions(&token_path)?;

    info!(path = %token_path.display(), "created daemon auth token");
    Ok(token)
}

fn token_path() -> anyhow::Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("unable to resolve config directory"))?;
    Ok(base.join("skill").join("daemon").join("auth.token"))
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
fn tighten_file_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
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
fn tighten_dir_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
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
    use super::is_authorized;
    use axum::http::{header, HeaderMap, HeaderValue};

    #[test]
    fn bearer_header_is_validated() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer test-token"));

        assert!(is_authorized(headers, "test-token"));
    }
}
