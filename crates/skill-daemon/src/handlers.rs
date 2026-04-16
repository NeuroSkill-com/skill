use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use base64::Engine as _;
use skill_daemon_common::{
    DiscoveredDeviceResponse, EventEnvelope, ForgetDeviceRequest, HealthResponse, LslDiscoveredStreamResponse,
    PairDeviceRequest, ScannerCortexConfigRequest, ScannerStateResponse, ScannerWifiConfigRequest,
    SessionControlRequest, SetPreferredDeviceRequest, StatusResponse, VersionResponse, WsClient, WsPortResponse,
    WsRequestLog, DAEMON_NAME, PROTOCOL_VERSION,
};
use std::net::SocketAddr;
use tokio::sync::{broadcast, oneshot};
use tracing::error;

use crate::state::AppState;
use crate::util::{
    add_client, default_status, is_paired_target, now_unix_ms, now_unix_secs, persist_paired_devices,
    preferred_peer_target, push_device_log, remove_client, resolve_target_fields, spawn_session_for_target,
    target_requires_pairing, token_path, write_string_atomic,
};

// ── Health / version ───────────────────────────────────────────────────────

pub(crate) async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

pub(crate) async fn readyz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let ready = state.ready.load(std::sync::atomic::Ordering::Relaxed);
    let test_mode = state.test_mode.load(std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({
        "ok": ready,
        "ready": ready,
        "test_mode": test_mode,
    }))
}

/// Serve a screenshot image by bare filename (e.g., `20260413081553.webp`).
/// Infers the date subdirectory from the first 8 characters of the filename.
pub(crate) async fn serve_screenshot(
    state: axum::extract::State<crate::state::AppState>,
    axum::extract::Path(filename): axum::extract::Path<String>,
) -> axum::response::Response {
    let date_prefix = if filename.len() >= 8 { &filename[..8] } else { "" };
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let path = skill_dir.join("screenshots").join(date_prefix).join(&filename);
    serve_file(path).await
}

/// Serve a screenshot image with explicit date path (e.g., `20260413/20260413081553.webp`).
pub(crate) async fn serve_screenshot_with_date(
    state: axum::extract::State<crate::state::AppState>,
    axum::extract::Path((date, filename)): axum::extract::Path<(String, String)>,
) -> axum::response::Response {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let path = skill_dir.join("screenshots").join(&date).join(&filename);
    serve_file(path).await
}

async fn serve_file(path: std::path::PathBuf) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mime = if path.extension().and_then(|e| e.to_str()) == Some("webp") {
                "image/webp"
            } else if path.extension().and_then(|e| e.to_str()) == Some("png") {
                "image/png"
            } else {
                "application/octet-stream"
            };
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, "public, max-age=86400"),
                ],
                bytes,
            )
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// Alias for `/v1/llm/server/status` — neuroloop's `skill-llm.ts` probes `/llm/status`.
pub(crate) async fn llm_status_alias(state: axum::extract::State<crate::state::AppState>) -> Json<serde_json::Value> {
    crate::routes::settings_llm_runtime::llm_server_status_impl(state).await
}

/// OpenAI-compatible `/v1/models` — returns the active model or empty list.
pub(crate) async fn openai_models_alias(
    state: axum::extract::State<crate::state::AppState>,
) -> Json<serde_json::Value> {
    let status = crate::routes::settings_llm_runtime::llm_server_status_impl(axum::extract::State(state.0.clone()))
        .await
        .0;
    let mut data = vec![];
    if let Some(model) = status.get("model_name").and_then(|v| v.as_str()) {
        data.push(serde_json::json!({ "id": model, "object": "model", "owned_by": "skill" }));
    }
    Json(serde_json::json!({ "object": "list", "data": data }))
}

pub(crate) async fn service_install() -> Json<serde_json::Value> {
    let bin = std::env::current_exe().unwrap_or_default();
    let installer = crate::service_installer::ServiceInstaller::new(bin);
    match installer.install() {
        Ok(_) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
}

pub(crate) async fn service_uninstall() -> Json<serde_json::Value> {
    let bin = std::env::current_exe().unwrap_or_default();
    let installer = crate::service_installer::ServiceInstaller::new(bin);
    match installer.uninstall() {
        Ok(()) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
}

pub(crate) async fn service_status() -> Json<serde_json::Value> {
    let bin = std::env::current_exe().unwrap_or_default();
    let installer = crate::service_installer::ServiceInstaller::new(bin);
    let status = installer.status();
    Json(serde_json::json!({ "status": status }))
}

pub(crate) async fn version() -> Json<VersionResponse> {
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
pub(crate) async fn get_log_recent(
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

// ── Status / devices ───────────────────────────────────────────────────────

pub(crate) async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let mut current = state.status.lock().map(|g| g.clone()).unwrap_or_default();

    let peer_activity = skill_iroh::peer_activity_snapshot();
    current.iroh_connected_peers = peer_activity.iter().filter(|p| p.tunnel_connected).count();
    current.iroh_tunnel_online = current.iroh_connected_peers > 0;
    current.iroh_remote_device_connected = peer_activity.iter().any(|p| p.remote_device_connected);
    current.iroh_streaming_active = peer_activity.iter().any(|p| p.streaming_active);
    current.iroh_eeg_streaming_active = peer_activity.iter().any(|p| p.eeg_streaming_active);

    // Surface live iroh tunnel peer presence even before a streaming session is
    // fully connected, so UI can show "client online / waiting for stream".
    if let Some(peer) = peer_activity
        .iter()
        .find(|p| p.tunnel_connected)
        .map(|p| p.peer_id.clone())
    {
        if current.iroh_client_name.is_none() {
            if let Ok(auth) = state.iroh_auth.lock() {
                current.iroh_client_name = auth.client_name_for_endpoint(&peer);
            }
        }
    } else if current.state != "connected" {
        // Clear stale tunnel-only identity when no peer is currently online.
        current.iroh_client_name = None;
        if current.device_kind == "iroh-remote" {
            current.phone_info = None;
        }
    }

    Json(current)
}

pub(crate) async fn update_status(
    State(state): State<AppState>,
    Json(next): Json<StatusResponse>,
) -> Json<StatusResponse> {
    if let Ok(mut guard) = state.status.lock() {
        *guard = next.clone();
    }
    Json(next)
}

pub(crate) async fn devices(State(state): State<AppState>) -> Json<Vec<DiscoveredDeviceResponse>> {
    let current = state.devices.lock().map(|g| g.clone()).unwrap_or_default();
    Json(current)
}

pub(crate) async fn update_devices(
    State(state): State<AppState>,
    Json(next): Json<Vec<DiscoveredDeviceResponse>>,
) -> Json<Vec<DiscoveredDeviceResponse>> {
    if let Ok(mut guard) = state.devices.lock() {
        *guard = next.clone();
    }
    Json(next)
}

pub(crate) async fn set_preferred_device(
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

pub(crate) async fn pair_device(
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

pub(crate) async fn forget_device(
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

// ── Session control ────────────────────────────────────────────────────────

pub(crate) async fn control_retry_connect(State(state): State<AppState>) -> Json<StatusResponse> {
    let mut out = default_status("connecting");

    // Snapshot status-derived preference + paired order.
    let (status_target, status_state, status_error, paired_order): (
        Option<String>,
        String,
        Option<String>,
        Vec<String>,
    ) = state
        .status
        .lock()
        .ok()
        .map(|s| {
            (
                s.target_id.clone().or_else(|| s.target_name.clone()),
                s.state.clone(),
                s.device_error.clone(),
                s.paired_devices.iter().map(|d| d.id.clone()).collect(),
            )
        })
        .unwrap_or_else(|| (None, "disconnected".to_string(), None, Vec::new()));

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

    // Peer activity-based preference:
    //   1) peer with remote BLE device connected + actively streaming
    //   2) peer with remote BLE device connected
    //   3) any live peer tunnel
    //   4) recent peer cache fallback
    let peer_activity = skill_iroh::peer_activity_snapshot();
    let recent_peer_ids = skill_iroh::cached_peer_ids_recent(5);
    let preferred_peer = preferred_peer_target(&peer_activity, &recent_peer_ids);
    let live_peer_target = peer_activity
        .iter()
        .find(|p| p.tunnel_connected)
        .map(|p| format!("peer:{}", p.peer_id));
    let recent_peer_target = recent_peer_ids.into_iter().next().map(|peer| format!("peer:{peer}"));

    // Default preference stays local-first to avoid breaking existing BLE flows
    // unless a remote BLE device is already connected on iOS.
    let mut target = status_target
        .clone()
        .or(preferred_target)
        .or_else(|| paired_order.first().cloned());

    // If iOS already has a remote BLE device connected, prioritize that peer.
    if let Some(peer) = preferred_peer.clone().filter(|p| {
        peer_activity
            .iter()
            .any(|a| format!("peer:{}", a.peer_id) == *p && a.remote_device_connected)
    }) {
        target = Some(peer);
    }

    // If current target is a stale peer (not live/recent), fall back to local defaults.
    if let Some(current) = target.as_ref() {
        if current.starts_with("peer:") {
            let peer_is_live =
                live_peer_target.as_ref() == Some(current) || recent_peer_target.as_ref() == Some(current);
            if !peer_is_live {
                target = paired_order.first().cloned();
            }
        }
    }

    // Never auto-connect an unpaired local target.
    if let Some(current) = target.as_ref() {
        if !current.starts_with("peer:") && !is_paired_target(&state, current) {
            push_device_log(
                &state,
                "session",
                &format!("retry-connect dropped unpaired target={current}"),
            );
            target = None;
        }
    }

    // If default local target is unavailable, fall back to next paired+available.
    if let Some(current) = target.as_ref() {
        if !current.starts_with("peer:") && !available.contains(current) {
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

    // If local reconnect is failing, allow live iroh stream to take over.
    if status_error.is_some() {
        if let Some(peer) = preferred_peer.or(live_peer_target).or(recent_peer_target) {
            target = Some(peer);
        }
    }

    if let Some(ref t) = target {
        // Avoid duplicate cancellation/restart loops for the same target.
        let same_target_active =
            (status_state == "connecting" || status_state == "connected") && (status_target.as_ref() == Some(t));
        if same_target_active {
            push_device_log(
                &state,
                "session",
                &format!("retry-connect noop: already active target={t}"),
            );
            if let Ok(status) = state.status.lock() {
                return Json(status.clone());
            }
        }
        push_device_log(&state, "session", &format!("retry-connect target={t}"));
    } else {
        push_device_log(&state, "session", "retry-connect skipped: no target device");
    }

    // Cancel any existing session only when we actually need to switch/retry.
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
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

pub(crate) async fn control_cancel_retry(State(state): State<AppState>) -> Json<StatusResponse> {
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

    // Also disable reconnect.
    if let Ok(mut rc) = state.reconnect.lock() {
        rc.pending = false;
        rc.attempt = 0;
        rc.countdown = 0;
    }
    state.broadcast("reconnect-state", crate::reconnect::ReconnectState::default());

    Json(out)
}

pub(crate) async fn get_reconnect_state(State(state): State<AppState>) -> Json<crate::reconnect::ReconnectState> {
    let rc = state.reconnect.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Json(rc)
}

pub(crate) async fn enable_reconnect(State(state): State<AppState>) -> Json<serde_json::Value> {
    if let Ok(mut rc) = state.reconnect.lock() {
        rc.pending = true;
    }
    if let Ok(rc) = state.reconnect.lock() {
        state.broadcast("reconnect-state", &*rc);
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn disable_reconnect(State(state): State<AppState>) -> Json<serde_json::Value> {
    if let Ok(mut rc) = state.reconnect.lock() {
        rc.pending = false;
        rc.attempt = 0;
        rc.countdown = 0;
    }
    if let Ok(mut s) = state.status.lock() {
        s.retry_attempt = 0;
        s.retry_countdown_secs = 0;
    }
    state.broadcast("reconnect-state", crate::reconnect::ReconnectState::default());
    if let Ok(s) = state.status.lock() {
        state.broadcast("status", &*s);
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn control_start_session(
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

pub(crate) async fn control_switch_session(
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

pub(crate) async fn control_cancel_session(State(state): State<AppState>) -> Json<StatusResponse> {
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

// ── Scanner control ────────────────────────────────────────────────────────

pub(crate) async fn control_scanner_start(State(state): State<AppState>) -> Json<ScannerStateResponse> {
    Json(start_scanner_inner(&state))
}

/// Start the device scanner if not already running.  Called from the HTTP
/// handler and from `background::spawn_auto_scanner` at daemon boot.
pub fn start_scanner_inner(state: &AppState) -> ScannerStateResponse {
    let already_running = state.scanner_running.lock().map(|g| *g).unwrap_or(false);
    if already_running {
        push_device_log(state, "scanner", "start requested but scanner already running");
        return ScannerStateResponse { running: true };
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

    push_device_log(state, "scanner", "scanner started");

    let state2 = state.clone();
    tokio::spawn(async move {
        crate::scanner::run_usb_scanner_task(state2.clone(), rx).await;
        if let Ok(mut running) = state2.scanner_running.lock() {
            *running = false;
        }
        if let Ok(mut slot) = state2.scanner_stop_tx.lock() {
            *slot = None;
        }
    });

    ScannerStateResponse { running: true }
}

pub(crate) async fn control_scanner_stop(State(state): State<AppState>) -> Json<ScannerStateResponse> {
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

pub(crate) async fn control_scanner_state(State(state): State<AppState>) -> Json<ScannerStateResponse> {
    let running = state.scanner_running.lock().map(|g| *g).unwrap_or(false);
    Json(ScannerStateResponse { running })
}

pub(crate) async fn control_scanner_wifi_config(
    State(state): State<AppState>,
    Json(cfg): Json<ScannerWifiConfigRequest>,
) -> Json<ScannerWifiConfigRequest> {
    if let Ok(mut guard) = state.scanner_wifi_config.lock() {
        *guard = cfg.clone();
    }
    Json(cfg)
}

pub(crate) async fn control_scanner_cortex_config(
    State(state): State<AppState>,
    Json(cfg): Json<ScannerCortexConfigRequest>,
) -> Json<ScannerCortexConfigRequest> {
    if let Ok(mut guard) = state.scanner_cortex_config.lock() {
        *guard = cfg.clone();
    }
    Json(cfg)
}

// ── LSL ────────────────────────────────────────────────────────────────────

pub(crate) async fn lsl_discover() -> Json<Vec<LslDiscoveredStreamResponse>> {
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

// ── WebSocket ──────────────────────────────────────────────────────────────

pub(crate) fn daemon_addr() -> std::net::SocketAddr {
    std::env::var("SKILL_DAEMON_ADDR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::net::SocketAddr::from(([127, 0, 0, 1], 18444)))
}

pub(crate) async fn ws_port() -> Json<WsPortResponse> {
    Json(WsPortResponse {
        port: daemon_addr().port(),
    })
}

pub(crate) async fn ws_clients(State(state): State<AppState>) -> Json<Vec<WsClient>> {
    let clients = state.tracker.lock().map(|g| g.clients.clone()).unwrap_or_default();
    Json(clients)
}

pub(crate) async fn ws_request_log(State(state): State<AppState>) -> Json<Vec<WsRequestLog>> {
    let requests = state.tracker.lock().map(|g| g.requests.clone()).unwrap_or_default();
    Json(requests)
}

pub(crate) async fn ws_events(
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

// ── Token management ───────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub(crate) struct CreateTokenRequest {
    name: String,
    acl: crate::auth::TokenAcl,
    expiry: crate::auth::TokenExpiry,
}

#[derive(serde::Deserialize)]
pub(crate) struct TokenIdRequest {
    id: String,
}

pub(crate) async fn list_tokens(State(state): State<AppState>) -> impl IntoResponse {
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

pub(crate) async fn create_token(
    State(state): State<AppState>,
    Json(req): Json<CreateTokenRequest>,
) -> impl IntoResponse {
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
        Err(crate::auth::TokenStoreError::MaxTokensReached) => {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("maximum active token count reached ({})", crate::auth::TokenStore::MAX_TOKENS)
                })),
            )
                .into_response();
        }
    };

    let _ = store.save(&skill_dir);
    (StatusCode::OK, Json(token)).into_response() // Full token returned (secret visible) on creation only
}

pub(crate) async fn revoke_token(State(state): State<AppState>, Json(req): Json<TokenIdRequest>) -> impl IntoResponse {
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

pub(crate) async fn delete_token(State(state): State<AppState>, Json(req): Json<TokenIdRequest>) -> impl IntoResponse {
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

pub(crate) async fn refresh_default_token(State(state): State<AppState>) -> impl IntoResponse {
    use rand::Rng;
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

// ── Events / command tunnel ────────────────────────────────────────────────

pub(crate) async fn push_event(
    State(state): State<AppState>,
    Json(envelope): Json<EventEnvelope>,
) -> Json<serde_json::Value> {
    let _ = state.events_tx.send(envelope);
    Json(serde_json::json!({ "ok": true }))
}

/// Universal command tunnel — accepts CLI JSON commands via `POST /v1/cmd`.
pub(crate) async fn cmd_tunnel(
    State(state): State<AppState>,
    Json(msg): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(crate::cmd_dispatch::dispatch(state, msg).await)
}

/// Root-level command tunnel — accepts CLI JSON commands via `POST /`.
pub(crate) async fn cmd_tunnel_root(
    State(state): State<AppState>,
    Json(msg): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    Json(crate::cmd_dispatch::dispatch(state, msg).await)
}

pub(crate) async fn handle_ws(mut socket: WebSocket, mut rx: broadcast::Receiver<EventEnvelope>, state: AppState) {
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
                                // Spawned as a separate task so that ARM 2 of the select!
                                // loop (stream_rx.recv()) can drain the mpsc channel and
                                // forward delta tokens to the socket concurrently with
                                // inference.  Without the spawn the select! loop is blocked
                                // for the entire generation, stream_rx is never polled, and
                                // blocking_send deadlocks once the 64-slot buffer is full.
                                #[cfg(feature = "llm")]
                                {
                                    let mut tx = stream_tx.clone();
                                    let state2 = state.clone();
                                    tokio::spawn(async move {
                                        crate::cmd_dispatch::dispatch_llm_chat_streaming(
                                            state2, cmd, &mut tx,
                                        ).await;
                                    });
                                }
                                #[cfg(not(feature = "llm"))]
                                {
                                    let response = crate::cmd_dispatch::dispatch(state.clone(), cmd).await;
                                    if let Ok(resp_str) = serde_json::to_string(&response) {
                                        if socket.send(Message::Text(resp_str.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            } else if !cmd_name.is_empty() {
                                let response = crate::cmd_dispatch::dispatch(state.clone(), cmd).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use std::time::Duration;
    use tempfile::TempDir;

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
}
