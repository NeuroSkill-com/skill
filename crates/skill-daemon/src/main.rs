// Allow needless_return in cfg-gated code paths where return is required
// to exit early before the #[cfg(not(feature))] fallback block.
#![allow(clippy::needless_return)]

mod activity;
mod auth_middleware;
pub(crate) mod background;
pub(crate) mod cmd_dispatch;
pub(crate) mod embed;
mod handlers;
mod idle_reembed;
pub(crate) mod monitor;
pub(crate) mod reconnect;
mod routes;
mod scanner;
mod service_installer;
pub(crate) mod session;
pub(crate) mod session_runner;

// Re-export from skill-daemon-state for internal use via `crate::` paths
pub(crate) use skill_daemon_state::auth;
pub(crate) use skill_daemon_state::state;
pub(crate) use skill_daemon_state::text_embedder;
// tracker is accessed via skill_daemon_state::tracker where needed
// Note: util is a local module that re-exports from skill_daemon_state::util
// plus adds daemon-specific functions (spawn_session_for_target)
pub(crate) mod util;

use skill_daemon_state::util::load_or_create_token;
use skill_daemon_state::AppState;

use std::{net::SocketAddr, path::PathBuf};

use axum::{middleware, routing::get, Router};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use axum::http::Method;

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

    // Parse CLI flags
    let args: Vec<String> = std::env::args().collect();
    let cli_iroh_logs = args.iter().any(|a| a == "--iroh-logs");

    let skill_dir = skill_data_dir();
    let state = AppState::new(load_or_create_token()?, skill_dir.clone());

    // CLI flag overrides the persisted setting
    if cli_iroh_logs {
        state
            .iroh_logs_enabled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    init_tracing(state.app_log.clone(), state.iroh_logs_enabled.clone());

    // Spawn the remote-access iroh tunnel.  It proxies authenticated iroh
    // peers to this daemon's HTTP port, enabling phone pairing and remote EEG.
    {
        let api_port = handlers::daemon_addr().port();
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

    // Spawn daemon-authoritative background loops.
    reconnect::spawn_reconnect_loop(state.clone(), state.reconnect.clone());
    monitor::spawn_status_monitor(state.clone());
    background::spawn_all(state.clone());
    idle_reembed::spawn_idle_reembed_loop(state.clone());

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
        .route("/version", get(handlers::version))
        .route("/log/recent", get(handlers::get_log_recent))
        .route("/status", get(handlers::status).post(handlers::update_status))
        .route("/devices", get(handlers::devices).post(handlers::update_devices))
        .route(
            "/devices/set-preferred",
            axum::routing::post(handlers::set_preferred_device),
        )
        .route("/devices/pair", axum::routing::post(handlers::pair_device))
        .route("/devices/forget", axum::routing::post(handlers::forget_device))
        .route(
            "/control/retry-connect",
            axum::routing::post(handlers::control_retry_connect),
        )
        .route(
            "/control/cancel-retry",
            axum::routing::post(handlers::control_cancel_retry),
        )
        .route("/reconnect-state", get(handlers::get_reconnect_state))
        .route(
            "/control/enable-reconnect",
            axum::routing::post(handlers::enable_reconnect),
        )
        .route(
            "/control/disable-reconnect",
            axum::routing::post(handlers::disable_reconnect),
        )
        .route(
            "/control/start-session",
            axum::routing::post(handlers::control_start_session),
        )
        .route(
            "/control/switch-session",
            axum::routing::post(handlers::control_switch_session),
        )
        .route(
            "/control/cancel-session",
            axum::routing::post(handlers::control_cancel_session),
        )
        .route(
            "/control/scanner/start",
            axum::routing::post(handlers::control_scanner_start),
        )
        .route(
            "/control/scanner/stop",
            axum::routing::post(handlers::control_scanner_stop),
        )
        .route("/control/scanner/state", get(handlers::control_scanner_state))
        .route(
            "/control/scanner/wifi-config",
            axum::routing::post(handlers::control_scanner_wifi_config),
        )
        .route(
            "/control/scanner/cortex-config",
            axum::routing::post(handlers::control_scanner_cortex_config),
        )
        .route("/lsl/discover", get(handlers::lsl_discover))
        .route("/ws-port", get(handlers::ws_port))
        .route("/ws-clients", get(handlers::ws_clients))
        .route("/ws-request-log", get(handlers::ws_request_log))
        .route("/auth/tokens", get(handlers::list_tokens).post(handlers::create_token))
        .route("/auth/tokens/revoke", axum::routing::post(handlers::revoke_token))
        .route("/auth/tokens/delete", axum::routing::post(handlers::delete_token))
        .route(
            "/auth/default-token/refresh",
            axum::routing::post(handlers::refresh_default_token),
        )
        .route("/events", get(handlers::ws_events))
        .route("/events/push", axum::routing::post(handlers::push_event))
        .route("/cmd", axum::routing::post(handlers::cmd_tunnel))
        .merge(routes::labels::router())
        .merge(routes::history::router())
        .merge(routes::settings::router())
        .merge(routes::api::router())
        .merge(routes::analysis::router())
        .merge(routes::search::router())
        .merge(routes::iroh::router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::auth_middleware,
        ));

    // Root-level command tunnel for CLI HTTP mode (POST / with JSON body)
    let root_cmd = Router::new()
        .route("/", axum::routing::post(handlers::cmd_tunnel_root))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::auth_middleware,
        ));

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
    // Root-level routes that require authentication
    let authed_root = Router::new()
        .route("/service/install", axum::routing::post(handlers::service_install))
        .route("/service/uninstall", axum::routing::post(handlers::service_uninstall))
        .route("/service/status", get(handlers::service_status))
        // Aliases without /v1/ prefix — used by neuroloop's skill-llm.ts
        .route("/llm/status", get(handlers::llm_status_alias))
        .route("/v1/models", get(handlers::openai_models_alias))
        // Screenshot images (auth required — supports ?token= for <img> tags).
        .route("/screenshots/{filename}", get(handlers::serve_screenshot))
        .route(
            "/screenshots/{date}/{filename}",
            get(handlers::serve_screenshot_with_date),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware::auth_middleware,
        ));

    let app = Router::new()
        // Public health check endpoints (no auth required)
        .route("/healthz", get(handlers::healthz))
        .route("/readyz", get(handlers::readyz))
        .merge(authed_root)
        .nest("/v1", v1)
        .merge(root_cmd)
        .layer(cors)
        .with_state(state.clone());

    // ── Auto-start the LLM server if previously enabled ───────────────────────
    //
    // When the user starts the LLM server, `config.enabled` is persisted to
    // disk.  On daemon restart we honour that flag and auto-start the server
    // so the experience is seamless.  If no model is available the start is
    // silently skipped.
    #[cfg(feature = "llm")]
    {
        let llm_state = state.clone();
        tokio::spawn(async move {
            let cfg = llm_state.llm_config.lock().map(|g| g.clone()).unwrap_or_default();
            if cfg.enabled {
                info!("LLM server was enabled — auto-starting");
                let _ =
                    crate::routes::settings_llm_runtime::llm_server_start_impl(axum::extract::State(llm_state)).await;
            }
        });
    }

    let addr = handlers::daemon_addr();
    info!(%addr, "skill daemon listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown)
        .await?;

    info!("shutting down...");

    // Broadcast shutdown event so connected clients can react gracefully.
    let _ = shutdown_state.events_tx.send(skill_daemon_common::EventEnvelope {
        r#type: "DaemonShutdown".to_string(),
        ts_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        correlation_id: None,
        payload: serde_json::json!({}),
    });

    // 1. Stop the LLM server first — ggml-metal holds GPU resources that
    //    assert-fail if freed during process exit (atexit) instead of being
    //    released explicitly.
    #[cfg(feature = "llm")]
    {
        skill_llm::shutdown_cell(&shutdown_state.llm_state_cell);
        info!("LLM server stopped");
    }

    // 2. Drop the active BLE session so btleplug stops firing delegate
    //    callbacks *before* the broadcast channel is dropped.
    if let Ok(mut slot) = shutdown_state.session_handle.lock() {
        drop(slot.take());
    }

    // 3. Cancel any in-flight EXG weights download.
    shutdown_state
        .exg_download_cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);

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

fn init_tracing(
    app_log: std::sync::Arc<std::sync::Mutex<(u64, std::collections::VecDeque<String>)>>,
    iroh_logs_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
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

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::Layer;

    // Targets that belong to the iroh networking stack.
    const IROH_PREFIXES: &[&str] = &[
        "iroh",
        "quinn",
        "endpoint",
        "magicsock",
        "derp",
        "relay",
        "netcheck",
        "portmapper",
        "discovery",
    ];

    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "skill_daemon=info,info".into());

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_target(false)
        .compact();

    let iroh_flag = iroh_logs_enabled;
    let iroh_filter = tracing_subscriber::filter::filter_fn(move |metadata| {
        // If iroh logs are enabled, allow everything through.
        if iroh_flag.load(std::sync::atomic::Ordering::Relaxed) {
            return true;
        }
        // Suppress iroh-related targets at WARN and below (i.e. WARN, INFO, DEBUG, TRACE).
        // ERROR level always passes through.
        if metadata.level() > &tracing::Level::ERROR {
            let target = metadata.target();
            for prefix in IROH_PREFIXES {
                if target.starts_with(prefix) {
                    return false;
                }
            }
        }
        true
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer.with_filter(iroh_filter))
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request, StatusCode};
    use axum::routing::get;
    use futures::{SinkExt, StreamExt};
    use tempfile::TempDir;
    use tokio_tungstenite::connect_async;
    use tower::ServiceExt;

    fn test_app(state: AppState) -> Router {
        let v1 = Router::new()
            .route("/version", get(handlers::version))
            .route("/events", get(handlers::ws_events))
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware::auth_middleware,
            ));

        Router::new()
            .route("/healthz", get(handlers::healthz))
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

        let _ = state.events_tx.send(skill_daemon_common::EventEnvelope {
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

        let _ = state.events_tx.send(skill_daemon_common::EventEnvelope {
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
}
