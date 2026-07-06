// SPDX-License-Identifier: GPL-3.0-only
//! Core daemon routes — version/pairing/devices/control/auth/events.
//!
//! These map directly onto handlers in [`crate::handlers`]. Grouped here (like
//! the other `routes::*` modules) so `main.rs` just composes routers instead of
//! registering ~30 routes inline.

use axum::{
    routing::{get, post},
    Router,
};

use crate::handlers;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/version", get(handlers::version))
        .route("/log/recent", get(handlers::get_log_recent))
        .route("/pair/generate-code", post(handlers::pair_generate_code))
        .route("/pair/start", post(handlers::pair_start))
        .route("/pair/approve", post(handlers::pair_approve))
        .route("/status", get(handlers::status).post(handlers::update_status))
        .route("/devices", get(handlers::devices).post(handlers::update_devices))
        .route("/devices/set-preferred", post(handlers::set_preferred_device))
        .route("/devices/pair", post(handlers::pair_device))
        .route("/devices/forget", post(handlers::forget_device))
        .route("/control/retry-connect", post(handlers::control_retry_connect))
        .route("/control/cancel-retry", post(handlers::control_cancel_retry))
        .route("/reconnect-state", get(handlers::get_reconnect_state))
        .route("/control/enable-reconnect", post(handlers::enable_reconnect))
        .route("/control/disable-reconnect", post(handlers::disable_reconnect))
        .route("/control/start-session", post(handlers::control_start_session))
        .route("/control/switch-session", post(handlers::control_switch_session))
        .route("/control/cancel-session", post(handlers::control_cancel_session))
        .route("/control/scanner/start", post(handlers::control_scanner_start))
        .route("/control/scanner/stop", post(handlers::control_scanner_stop))
        .route("/control/scanner/state", get(handlers::control_scanner_state))
        .route(
            "/control/scanner/wifi-config",
            post(handlers::control_scanner_wifi_config),
        )
        .route(
            "/control/scanner/cortex-config",
            post(handlers::control_scanner_cortex_config),
        )
        .route("/lsl/discover", get(handlers::lsl_discover))
        .route("/ws-port", get(handlers::ws_port))
        .route("/ws-clients", get(handlers::ws_clients))
        .route("/ws-request-log", get(handlers::ws_request_log))
        .route("/auth/tokens", get(handlers::list_tokens).post(handlers::create_token))
        .route("/auth/tokens/revoke", post(handlers::revoke_token))
        .route("/auth/tokens/delete", post(handlers::delete_token))
        .route("/auth/default-token/refresh", post(handlers::refresh_default_token))
        .route("/events", get(handlers::ws_events))
        .route("/events/push", post(handlers::push_event))
        .route("/cmd", post(handlers::cmd_tunnel))
}
