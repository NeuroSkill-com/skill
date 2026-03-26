// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! WebSocket broadcast server + Bonjour/mDNS service registration.
//!
//! # Usage (from `lib.rs` setup)
//!
//! ```ignore
//! let (broadcaster, serve_handle) = ws_server::bind();
//! ws_server::register_mdns(&app_name, serve_handle.port);
//! let app_handle = app.handle().clone();
//! tauri::async_runtime::spawn(async move {
//!     serve_handle.serve(app_handle).await;
//! });
//! app.manage(broadcaster);
//! ```
//!
//! From any event handler, broadcast with:
//!
//! ```ignore
//! app.state::<WsBroadcaster>().send("status", &payload);
//! ```
//!
//! # Broadcast events
//!
//! | Event            | Rate      | Description                                        |
//! |------------------|-----------|----------------------------------------------------|
//! | `eeg-bands`        | ~4 Hz     | Derived scores, band powers, HR, head pose (60+ fields) |
//! | `status`           | ~1 Hz     | Device heartbeat: battery, sample counts, state    |
//! | `label-created`    | on-demand | Fired when any client creates a label              |
//! | `dnd-eligibility`  | ~4 Hz     | DND pipeline state: focus_score, smoothed_score, threshold, elapsed_secs, duration_secs, dnd_active |
//! | `dnd-state-changed`| on-demand | Fired when DND is activated or deactivated (payload: bool) |
//!
//! **Note:** Raw EEG samples (256 Hz), PPG (64 Hz), IMU (50 Hz), and
//! spectrogram slices are *not* broadcast over the WebSocket API — their
//! high frequency would overwhelm the connection. Those streams are only
//! available via the internal Tauri IPC event bus for the native UI.
//!
//! # Wire format (outbound — server → client)
//!
//! Every broadcast message is a UTF-8 JSON object:
//! ```json
//! { "event": "eeg-bands", "payload": { … } }
//! ```
//! This mirrors the Tauri event bus so the same TypeScript types work on both
//! sides.
//!
//! # Wire format (inbound — client → server)
//!
//! Clients can send JSON commands:
//! ```json
//! { "command": "calibrate" }
//! { "command": "label", "text": "eyes closed meditation" }
//! { "command": "search", "start_utc": 1700000000, "end_utc": 1700000300 }
//! ```
//!
//! The server responds on the same connection with:
//! ```json
//! { "command": "calibrate", "ok": true }
//! { "command": "label", "ok": true, "label_id": 42 }
//! { "command": "search", "ok": true, "result": { … } }
//! { "command": "…", "ok": false, "error": "description" }
//! ```
//!
//! Command handler implementations live in [`super::ws_commands`].
//!
//! ## Available commands
//!
//! | command       | parameters                                              | description |
//! |---------------|---------------------------------------------------------|-------------|
//! | `status`          | _(none)_                                                | Return full system status snapshot |
//! | `notify`          | `title` (string, required); `body` (string, optional)   | Show a native OS notification |
//! | `calibrate`       | _(none)_                                                | Open the calibration window (no auto-start) |
//! | `run_calibration` | `id` (string, optional profile UUID)                    | Open calibration window and start the profile immediately |
//! | `timer`           | _(none)_                                                | Open the focus-timer window and auto-start the work phase |
//! | `say`             | `text` (string, required), `voice` (string, optional)   | Speak text via on-device TTS (fire-and-forget; responds immediately) |
//! | `dnd`             | _(none)_                                                | DND automation status: config, timer progress, app-active, OS-active |
//! | `dnd_set`         | `enabled` (bool, required)                              | Force-enable or disable DND immediately, bypassing the EEG threshold |
//! | `label`           | `text` (string, required); `context` (string, optional) | Submit a label for the current EEG window |
//! | `search_labels`        | `query` (string); `k`, `ef` (optional u64); `mode`: `"text"\|"context"\|"both"` (default `"text"`) | Search labels by free text |
//! | `interactive_search`   | `query` (string); `k_text`, `k_eeg`, `k_labels`, `reach_minutes` (optional u64) | Cross-modal 4-layer graph search (query→labels→EEG→found labels); returns `nodes`, `edges`, `dot` |
//! | `search`               | `start_utc`, `end_utc` (u64); `k`, `ef` (optional u64) | Search EEG embeddings in a time range |
//! | `compare`         | `a_start_utc`, `a_end_utc`, `b_start_utc`, `b_end_utc` (u64) | Compare band-power metrics of two time ranges |
//! | `sessions`        | _(none)_                                                | List all embedding sessions (contiguous recording ranges) |
//! | `sleep`           | `start_utc`, `end_utc` (u64)                            | Classify sleep stages and return a hypnogram |
//! | `umap`            | `a_start_utc`, `a_end_utc`, `b_start_utc`, `b_end_utc` (u64) | Enqueue a 3D UMAP projection job; returns `job_id` |
//! | `umap_poll`       | `job_id` (u64)                                          | Poll a queued UMAP job for results |
//! | `health_sync`     | HealthKit sync payload (`sleep?`, `workouts?`, …)       | Upsert Apple HealthKit data from the iOS companion app |
//! | `health_query`    | `type`, `start_utc?`, `end_utc?`, `limit?`              | Query stored HealthKit samples by type |
//! | `health_summary`  | `start_utc?`, `end_utc?`                                | Aggregate HealthKit counts for a time range |
//! | `health_metric_types` | _(none)_                                            | List all distinct HealthKit metric types stored |
//! | `calendar_events` | `start_utc`, `end_utc` (i64)                           | Fetch OS calendar events overlapping the range (EventKit / iCal) |
//! | `calendar_status` | _(none)_                                                | Return calendar access status (`authorized`/`denied`/…) and platform |
//! | `calendar_request_permission` | _(none)_                               | Request calendar access (macOS: shows system dialog; no-op elsewhere) |
//!
//! ## Examples
//!
//! ### Python (websockets)
//!
//! ```python
//! import asyncio, json, websockets
//!
//! async def main():
//!     async with websockets.connect("ws://localhost:<port>") as ws:
//!
//!         # Open calibration window
//!         await ws.send(json.dumps({"command": "calibrate"}))
//!         print(await ws.recv())
//!
//!         # Submit a label
//!         await ws.send(json.dumps({
//!             "command": "label",
//!             "text": "eyes closed resting state"
//!         }))
//!         print(await ws.recv())
//!
//!         # Search embeddings from the last 5 minutes
//!         import time
//!         now = int(time.time())
//!         await ws.send(json.dumps({
//!             "command": "search",
//!             "start_utc": now - 300,
//!             "end_utc": now,
//!             "k": 5
//!         }))
//!         result = json.loads(await ws.recv())
//!         print(json.dumps(result, indent=2))
//!
//!         # Listen for live events
//!         async for msg in ws:
//!             data = json.loads(msg)
//!             print(data["event"], data.get("payload", {}).keys())
//!
//! asyncio.run(main())
//! ```
//!
//! ### Node.js (ws)
//!
//! ```js
//! const WebSocket = require("ws");
//! const ws = new WebSocket("ws://localhost:<port>");
//!
//! ws.on("open", () => {
//!   // Open calibration window
//!   ws.send(JSON.stringify({ command: "calibrate" }));
//!
//!   // Submit a label
//!   ws.send(JSON.stringify({ command: "label", text: "focused reading" }));
//!
//!   // Search last 5 minutes
//!   const now = Math.floor(Date.now() / 1000);
//!   ws.send(JSON.stringify({
//!     command: "search",
//!     start_utc: now - 300,
//!     end_utc: now,
//!     k: 10
//!   }));
//! });
//!
//! ws.on("message", (data) => {
//!   const msg = JSON.parse(data);
//!   if (msg.command) {
//!     // Response to a command
//!     console.log("command response:", msg);
//!   } else if (msg.event) {
//!     // Broadcast event
//!     console.log("event:", msg.event);
//!   }
//! });
//! ```
//!
//! ### curl (one-shot via HTTP REST — no persistent connection needed)
//!
//! Every WebSocket command is also reachable as a plain HTTP call under `/v1/`.
//! No WebSocket library required; works from `curl`, Python `requests`,
//! Jupyter notebooks, shell scripts, etc.
//!
//! ```bash
//! PORT=8375
//!
//! # System status
//! curl http://localhost:$PORT/v1/status | jq .
//!
//! # Submit a label
//! curl -X POST http://localhost:$PORT/v1/label \
//!      -H 'Content-Type: application/json' \
//!      -d '{"text":"meditation session","context":"morning"}'
//!
//! # Search last 10 minutes
//! NOW=$(date +%s)
//! curl -X POST http://localhost:$PORT/v1/search \
//!      -H 'Content-Type: application/json' \
//!      -d "{\"start_utc\":$((NOW-600)),\"end_utc\":$NOW,\"k\":5}" | jq .
//!
//! # Speak text
//! curl -X POST http://localhost:$PORT/v1/say \
//!      -H 'Content-Type: application/json' \
//!      -d '{"text":"Focus session complete."}'
//!
//! # Enable DND
//! curl -X POST http://localhost:$PORT/v1/dnd \
//!      -H 'Content-Type: application/json' \
//!      -d '{"enabled":true}'
//! ```
//!
//! ### curl (one-shot via websocat)
//!
//! ```bash
//! # Install: brew install websocat
//!
//! # Open calibration window
//! echo '{"command":"calibrate"}' | websocat ws://localhost:<port>
//!
//! # Submit a label
//! echo '{"command":"label","text":"meditation session"}' | websocat ws://localhost:<port>
//!
//! # Search last 10 minutes
//! echo '{"command":"search","start_utc":'$(($(date +%s)-600))',"end_utc":'$(date +%s)'}' \
//!   | websocat ws://localhost:<port>
//! ```
//!
//! # mDNS service
//!
//! Service type : `_{app_name}._tcp.local.` (e.g. `_skill._tcp.local.`)  
//! Instance name: app name in lowercase  
//! TXT records  : `version=1`, `format=json`
//!
//! Clients can discover the port with any DNS-SD browser
//! (e.g. `dns-sd -B _skill._tcp`, `avahi-browse _skill._tcp`).

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex as StdMutex};

use axum::http::{HeaderName, Method};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

#[cfg(feature = "llm")]
use crate::llm::LlmStateCell;

use mdns_sd::{ServiceDaemon, ServiceInfo};
use serde::Serialize;
use tauri::AppHandle;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use crate::constants::{
    MDNS_HOST_SUFFIX, MDNS_SERVICE_SUFFIX, MDNS_TXT_FORMAT, MDNS_TXT_VERSION,
    WS_BROADCAST_CAPACITY, WS_DEFAULT_PORT, WS_HOST,
};

// ── Client & request tracking ─────────────────────────────────────────────────

/// Maximum number of request log entries kept in memory.
const MAX_REQUEST_LOG: usize = skill_constants::WS_MAX_REQUEST_LOG;

/// A connected WebSocket client tracked for the API status panel.
#[derive(Clone, Serialize)]
pub struct WsClient {
    pub peer: String,
    pub connected_at: u64, // unix seconds
}

/// A single request log entry.
#[derive(Clone, Serialize)]
pub struct WsRequestLog {
    pub timestamp: u64, // unix seconds
    pub peer: String,
    pub command: String,
    pub ok: bool,
}

/// Shared tracking state for connected WS clients and the request log.
/// Wrapped in `Arc<StdMutex<…>>` so both the accept loop and Tauri commands
/// can access it.
pub struct WsTracker {
    pub clients: Vec<WsClient>,
    pub requests: Vec<WsRequestLog>,
    pub port: u16,
}

impl WsTracker {
    fn new(port: u16) -> Self {
        Self {
            clients: Vec::new(),
            requests: Vec::new(),
            port,
        }
    }

    pub fn add_client(&mut self, peer: &str) {
        let now = crate::unix_secs();
        self.clients.push(WsClient {
            peer: peer.to_owned(),
            connected_at: now,
        });
    }

    pub fn remove_client(&mut self, peer: &str) {
        self.clients.retain(|c| c.peer != peer);
    }

    pub fn log_request(&mut self, peer: &str, command: &str, ok: bool) {
        let now = crate::unix_secs();
        self.requests.push(WsRequestLog {
            timestamp: now,
            peer: peer.to_owned(),
            command: command.to_owned(),
            ok,
        });
        if self.requests.len() > MAX_REQUEST_LOG {
            self.requests
                .drain(0..(self.requests.len() - MAX_REQUEST_LOG));
        }
    }
}

/// Type alias for the managed tracker state.
pub type SharedTracker = Arc<StdMutex<WsTracker>>;

// ── WsBroadcaster ─────────────────────────────────────────────────────────────

/// A `Send + Sync` handle for broadcasting JSON messages to every connected
/// WebSocket client.  Store as Tauri managed state; clone is cheap
/// (just clones the inner `broadcast::Sender`).
#[derive(Clone)]
pub struct WsBroadcaster {
    tx: broadcast::Sender<String>,
    pub tracker: SharedTracker,
}

impl WsBroadcaster {
    /// Serialise `payload` into `{"event":…,"payload":…}` and fan it out to
    /// all connected clients.  Non-blocking; silently drops the message if no
    /// clients are subscribed.
    pub fn send<P: Serialize>(&self, event: &str, payload: &P) {
        let json = match serde_json::to_string(&serde_json::json!({
            "event":   event,
            "payload": payload,
        })) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[ws] serialize error for '{event}': {e}");
                return;
            }
        };
        // Err(SendError) just means no receivers are subscribed — not a problem.
        let _ = self.tx.send(json);
    }
}

// ── Bind + ServeHandle ────────────────────────────────────────────────────────

/// Returned by [`bind`]; contains everything needed to start the accept loop.
pub struct ServeHandle {
    listener: std::net::TcpListener,
    tx: broadcast::Sender<String>,
    tracker: SharedTracker,
    /// The OS-assigned TCP port the server is bound to.
    pub port: u16,
    /// Dynamic LLM state cell — routes are always mounted, content swapped at runtime.
    #[cfg(feature = "llm")]
    llm_cell: Option<LlmStateCell>,
}

impl ServeHandle {
    /// Attach the LLM state cell so that `/v1/*` and `/llm/*` routes are
    /// always mounted.  The cell content is swapped by start/stop commands
    /// without touching the router.  Call before [`serve`].
    #[cfg(feature = "llm")]
    pub fn set_llm(&mut self, cell: LlmStateCell) {
        self.llm_cell = Some(cell);
    }

    /// Start the combined HTTP + WebSocket + (optional) LLM server.
    /// Spawn this with `tauri::async_runtime::spawn`.  Never returns.
    pub async fn serve(self, app: AppHandle) {
        self.serve_with_mode(app, false).await;
    }

    /// Start server in regular (`readonly=false`) or restricted read-only mode.
    pub async fn serve_with_mode(self, app: AppHandle, readonly: bool) {
        let listener =
            TcpListener::from_std(self.listener).expect("[ws] TcpListener::from_std failed");
        let state = crate::api::SharedState {
            app,
            tx: self.tx,
            tracker: self.tracker,
            readonly,
        };

        // Build the main WS/REST router, then merge the LLM router only for
        // non-readonly servers.
        #[cfg(feature = "llm")]
        let router = {
            let base = crate::api::router(state);
            if readonly {
                base
            } else {
                let cell = self.llm_cell.unwrap_or_else(crate::llm::new_state_cell);
                eprintln!("[llm] mounting /v1/* and /llm/* routes on the shared HTTP port");
                base.merge(crate::llm::router(cell))
            }
        };

        #[cfg(not(feature = "llm"))]
        let router = crate::api::router(state);

        // CORS — allow the Tauri WebView (null/tauri://) and any localhost
        // origin to call the REST + LLM endpoints from JS.
        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(|origin, _| {
                let s = origin.as_bytes();
                s == b"null"                                     // Tauri WebView
                    || s.starts_with(b"tauri://")               // tauri:// scheme
                    || s.starts_with(b"http://localhost")        // Vite dev server
                    || s.starts_with(b"http://127.0.0.1") // loopback
            }))
            .allow_methods(AllowMethods::list([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
                Method::HEAD,
            ]))
            .allow_headers(AllowHeaders::list([
                HeaderName::from_static("content-type"),
                HeaderName::from_static("authorization"),
                HeaderName::from_static("x-requested-with"),
            ]))
            .allow_credentials(false);

        let router = router.layer(cors);

        eprintln!(
            "[http/ws] listening on :{}",
            listener.local_addr().map_or(0, |a| a.port())
        );
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .expect("[http/ws] server error");
    }
}

/// Bind synchronously with an explicit host and preferred port.
///
/// `host` is typically `"127.0.0.1"` (loopback-only) or `"0.0.0.0"` (all
/// interfaces / LAN-accessible).  If `port` is already in use the server
/// falls back to an OS-assigned port on the same host.
pub fn bind_with(host: impl AsRef<str>, port: u16) -> (WsBroadcaster, ServeHandle) {
    let host = host.as_ref();
    let std_listener = std::net::TcpListener::bind((host, port))
        .or_else(|_| {
            eprintln!("[ws] {host}:{port} in use, falling back to OS-assigned port");
            std::net::TcpListener::bind((host, 0u16))
        })
        .expect("[ws] failed to bind WebSocket listener");
    std_listener
        .set_nonblocking(true)
        .expect("[ws] set_nonblocking failed");

    let port = std_listener
        .local_addr()
        .expect("[ws] no local addr")
        .port();

    let (tx, _) = broadcast::channel::<String>(WS_BROADCAST_CAPACITY);

    let tracker = Arc::new(StdMutex::new(WsTracker::new(port)));
    let broadcaster = WsBroadcaster {
        tx: tx.clone(),
        tracker: tracker.clone(),
    };
    let serve_handle = ServeHandle {
        listener: std_listener,
        tx,
        port,
        tracker,
        #[cfg(feature = "llm")]
        llm_cell: None,
    };

    (broadcaster, serve_handle)
}

/// Convenience wrapper using the compiled-in defaults.
#[allow(dead_code)]
pub fn bind() -> (WsBroadcaster, ServeHandle) {
    bind_with(WS_HOST, WS_DEFAULT_PORT)
}

// ── mDNS / Bonjour ────────────────────────────────────────────────────────────

/// Register a `_{app_name}._tcp.local.` DNS-SD service so LAN clients can
/// discover the WebSocket port without prior configuration.
///
/// The service type and host name are both derived from `app_name` so they
/// update automatically when the application is renamed.
///
/// The mDNS daemon is leaked intentionally: it runs in its own background
/// thread and must outlive the call site for as long as the process runs.
pub fn register_mdns(_app_name: &str, port: u16) {
    let ip = local_ip();
    let app_name = "skill"; // set skill instead of NeuroSkill for Bonjour discovery

    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[mdns] daemon start failed: {e}");
            return;
        }
    };

    // "_skill._tcp.local." — type changes with the app name automatically.
    let service_type = format!("_{}{}", app_name, MDNS_SERVICE_SUFFIX);
    // "skill.local." — must end with ".local." per RFC 6762.
    let host = format!("{}{}", app_name, MDNS_HOST_SUFFIX);

    let mut props = std::collections::HashMap::new();
    props.insert("version".to_owned(), MDNS_TXT_VERSION.to_owned());
    props.insert("format".to_owned(), MDNS_TXT_FORMAT.to_owned());

    let info = match ServiceInfo::new(&service_type, app_name, &host, ip, port, props) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[mdns] ServiceInfo error: {e}");
            return;
        }
    };

    match daemon.register(info) {
        Ok(_) => eprintln!("[mdns] {app_name}{MDNS_SERVICE_SUFFIX} → {ip}:{port}"),
        Err(e) => eprintln!("[mdns] register failed: {e}"),
    }

    // Leak the daemon so its background thread keeps running for the process
    // lifetime.  Dropping it would unregister the mDNS service.
    Box::leak(Box::new(daemon));
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Determine the local outbound IP address without sending any data.
///
/// Connecting a UDP socket sets the kernel's routing decision; reading back
/// `local_addr()` reveals which interface (and therefore which IP) would be
/// used for outbound traffic.  No packet is ever transmitted.
///
/// Returns [`IpAddr`], which implements [`mdns_sd::AsIpAddrs`].
fn local_ip() -> IpAddr {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0")
        .expect("[ws] UDP bind for local IP detection failed");
    // connect() on UDP only records the remote address in the kernel — safe.
    let _ = sock.connect("8.8.8.8:80");
    match sock.local_addr() {
        Ok(addr) => addr.ip(),
        _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}
