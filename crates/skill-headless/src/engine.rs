// SPDX-License-Identifier: GPL-3.0-only
//! Core browser engine — spawns a hidden wry webview and processes commands.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use crossbeam_channel::{bounded, Sender};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{WebContext, WebView, WebViewBuilder};

// Platform-specific: allow event loop creation on non-main threads.
#[cfg(target_os = "linux")]
use tao::platform::unix::EventLoopBuilderExtUnix;
#[cfg(target_os = "windows")]
use tao::platform::windows::EventLoopBuilderExtWindows;
use tao::platform::run_return::EventLoopExtRunReturn;

#[cfg(target_os = "linux")]
use tao::platform::unix::WindowExtUnix;
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;

use crate::command::Command;
use crate::error::HeadlessError;
use crate::intercept::{
    self, InterceptStore, InterceptedRequest, InterceptedResponse, NavigationEvent,
};
use crate::response::Response;
use crate::session::Cookie;

/// Global flag: when `true`, `Browser::launch` returns `Err` immediately.
/// Set once at app startup via `Browser::set_unavailable()` when the host
/// application (e.g. Tauri) owns the main-thread event loop.
static HEADLESS_UNAVAILABLE: AtomicBool = AtomicBool::new(false);

/// Global cancellation flag for the current external fetch operation.
/// Checked during the page-load wait loop so users can interrupt slow pages.
static FETCH_CANCELLED: AtomicBool = AtomicBool::new(false);

/// External page renderer provided by the host application (e.g. Tauri).
///
/// When set, `external_fetch_page` uses this instead of launching a
/// standalone headless browser.  The function receives `(url, wait_ms)`
/// and must return the visible text content of the rendered page.
static EXTERNAL_RENDERER: std::sync::OnceLock<
    Box<dyn Fn(&str, u64) -> Result<String, String> + Send + Sync>,
> = std::sync::OnceLock::new();

// ── Configuration ────────────────────────────────────────────────────────────

/// Display mode for the browser session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// **Headless** — the window is truly invisible (`with_visible(false)`).
    /// No window, no taskbar entry, nothing is ever shown to the user.
    ///
    /// Because an invisible window has no real viewport, the webview's
    /// `window.innerWidth` / `innerHeight` would normally report 0x0.
    /// We inject an initialization script that overrides those properties
    /// (and `document.documentElement.clientWidth/Height`) with the
    /// configured dimensions.  `SetViewport` keeps them in sync.
    ///
    /// **Limitation:** CSS layout (`getBoundingClientRect`, `%` widths)
    /// still uses the native 0-width viewport.  Use `Headful` mode if
    /// you need pixel-accurate layout measurements.
    ///
    /// This is the default.
    #[default]
    Headless,

    /// **Headful** — the window is shown on-screen at its normal position.
    /// Useful for debugging, demos, or interactive automation where the
    /// user needs to see what the browser is doing.  CSS layout uses real
    /// pixel dimensions.
    Headful,
}

/// Browser configuration.
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Initial viewport width.
    pub width: u32,
    /// Initial viewport height.
    pub height: u32,
    /// Display mode — [`Mode::Headless`] (default) or [`Mode::Headful`].
    pub mode: Mode,
    /// Custom user-agent string. `None` = system default.
    pub user_agent: Option<String>,
    /// Data directory for persistent storage / cache. `None` = ephemeral.
    pub data_dir: Option<std::path::PathBuf>,
    /// Command response timeout (default 30 s).
    pub timeout: Duration,
    /// Whether to enable browser dev tools (F12).
    pub devtools: bool,
    /// Initial URL to load (default about:blank).
    pub initial_url: String,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            mode: Mode::Headless,
            user_agent: None,
            data_dir: None,
            timeout: Duration::from_secs(30),
            devtools: false,
            initial_url: "about:blank".into(),
        }
    }
}

// ── Internal types ───────────────────────────────────────────────────────────

/// A command envelope sent to the event-loop thread.
struct Envelope {
    command: Command,
    reply: Sender<Response>,
}

/// Custom user event for the tao event loop.
enum UserEvent {
    /// A new command arrived.
    Command(Envelope),
}

// ── Browser handle ───────────────────────────────────────────────────────────

/// Handle to a running headless browser session.
///
/// Cheap to clone — all clones share the same underlying session.
#[derive(Clone)]
pub struct Browser {
    proxy: EventLoopProxy<UserEvent>,
    timeout: Duration,
    closed: Arc<AtomicBool>,
}

impl Browser {
    /// Launch a new headless browser session on a background thread.
    ///
    /// This spawns a dedicated OS thread that owns the tao event loop and
    /// the wry webview.  The returned `Browser` handle can be used from
    /// **any** thread to send commands.
    ///
    /// # Platform notes
    ///
    /// - **Linux**: requires a running display server (X11 or Wayland).
    ///   In CI, wrap with `xvfb-run`.
    /// - **macOS**: uses WKWebView.  Must *not* be called from the main
    ///   thread if another NSApplication run loop is active.
    /// - **Windows**: uses WebView2 (Edge Chromium).
    /// Mark the headless browser as unavailable for this process.
    ///
    /// Call this once at startup when another GUI framework (Tauri, Cocoa,
    /// etc.) owns the main-thread event loop.  On macOS, tao panics if a
    /// second event loop is created on a non-main thread — this flag
    /// prevents `Browser::launch` from even trying.
    ///
    /// This is a no-op on Linux/Windows where `with_any_thread(true)` is
    /// available.
    pub fn set_unavailable() {
        HEADLESS_UNAVAILABLE.store(true, Ordering::Relaxed);
    }

    /// Whether the headless browser has been marked as unavailable.
    pub fn is_unavailable() -> bool {
        HEADLESS_UNAVAILABLE.load(Ordering::Relaxed)
    }

    /// Register an external page renderer.
    ///
    /// The host application (e.g. Tauri) can provide a function that
    /// renders a URL using its own webview infrastructure and returns
    /// the visible text.  This is used as a fallback when the standalone
    /// headless browser is unavailable (macOS inside Tauri).
    ///
    /// The function signature is `(url: &str, wait_ms: u64) -> Result<String, String>`.
    pub fn set_external_renderer(
        f: impl Fn(&str, u64) -> Result<String, String> + Send + Sync + 'static,
    ) {
        let _ = EXTERNAL_RENDERER.set(Box::new(f));
    }

    /// Whether an external renderer is registered.
    pub fn has_external_renderer() -> bool {
        EXTERNAL_RENDERER.get().is_some()
    }

    pub fn launch(config: BrowserConfig) -> Result<Self, HeadlessError> {
        // On macOS inside a Tauri app, tao panics when a second event loop
        // is created on a background thread.  The app must call
        // `Browser::set_unavailable()` at startup to prevent this.
        if HEADLESS_UNAVAILABLE.load(Ordering::Relaxed) {
            return Err(HeadlessError::InitFailed(
                "headless browser unavailable: main event loop is owned by the host application".into()
            ));
        }

        let timeout = config.timeout;
        let closed = Arc::new(AtomicBool::new(false));
        let closed2 = closed.clone();

        // Channel to receive the proxy handle from the event-loop thread.
        let (proxy_tx, proxy_rx) = bounded::<Result<EventLoopProxy<UserEvent>, String>>(1);

        std::thread::Builder::new()
            .name("skill-headless-evloop".into())
            .spawn(move || {
                if let Err(e) = run_event_loop(config, proxy_tx, closed2) {
                    eprintln!("[skill-headless] event loop error: {e}");
                }
            })
            .map_err(|e| HeadlessError::InitFailed(e.to_string()))?;

        let proxy = proxy_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| HeadlessError::InitFailed("event loop did not start in time".into()))?
            .map_err(HeadlessError::InitFailed)?;

        Ok(Self {
            proxy,
            timeout,
            closed,
        })
    }

    /// Send a command and wait for the response (blocking).
    pub fn send(&self, command: Command) -> Result<Response, HeadlessError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(HeadlessError::SessionClosed);
        }

        let is_close = matches!(command, Command::Close);
        let (reply_tx, reply_rx) = bounded(1);

        self.proxy
            .send_event(UserEvent::Command(Envelope {
                command,
                reply: reply_tx,
            }))
            .map_err(|_| HeadlessError::ChannelClosed)?;

        let resp = reply_rx.recv_timeout(self.timeout)?;

        if is_close {
            self.closed.store(true, Ordering::Relaxed);
        }

        Ok(resp)
    }

    /// Send a command without waiting for a response (fire-and-forget).
    pub fn send_async(&self, command: Command) -> Result<(), HeadlessError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(HeadlessError::SessionClosed);
        }

        let (reply_tx, _reply_rx) = bounded(1);
        self.proxy
            .send_event(UserEvent::Command(Envelope {
                command,
                reply: reply_tx,
            }))
            .map_err(|_| HeadlessError::ChannelClosed)?;
        Ok(())
    }

    /// Whether this session has been closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

// ── Event loop (runs on dedicated thread) ────────────────────────────────────

fn run_event_loop(
    config: BrowserConfig,
    proxy_tx: Sender<Result<EventLoopProxy<UserEvent>, String>>,
    closed: Arc<AtomicBool>,
) -> Result<(), HeadlessError> {
    let mut builder = EventLoopBuilder::<UserEvent>::with_user_event();

    // Allow creating the event loop on a non-main thread.
    #[cfg(target_os = "linux")]
    builder.with_any_thread(true);
    #[cfg(target_os = "windows")]
    builder.with_any_thread(true);

    let mut event_loop = builder.build();
    let proxy = event_loop.create_proxy();

    // Send the proxy handle back to the launching thread.
    let _ = proxy_tx.send(Ok(proxy));

    // Build the window.
    //
    // **Headless** — the window is truly invisible (`with_visible(false)`).
    // No window, no taskbar entry, nothing shown to the user.  Because
    // WebKitGTK reports 0x0 for `innerWidth`/`innerHeight` on invisible
    // windows, we inject a JS initialization script that overrides those
    // properties with the configured dimensions.
    //
    // **Headful** — the window is shown on-screen at its normal position.
    let window = WindowBuilder::new()
        .with_title("skill-headless")
        .with_inner_size(LogicalSize::new(config.width, config.height))
        .with_visible(config.mode == Mode::Headful)
        .build(&event_loop)
        .map_err(|e| HeadlessError::InitFailed(e.to_string()))?;

    // Optional persistent web context for data directory / cache.
    let mut web_context = config
        .data_dir
        .as_ref()
        .map(|dir| WebContext::new(Some(dir.clone())));

    // IPC handler: used by async JS operations that need to return results
    // after Promises resolve.  The JS side calls `window.ipc.postMessage(id:result)`.
    let pending_ipc: Arc<Mutex<std::collections::HashMap<String, Sender<Response>>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));
    let pending_ipc_clone = pending_ipc.clone();

    // Network interception state.
    let intercept_store = InterceptStore::new();
    let intercept_store_ipc = intercept_store.clone();
    let blocked_urls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let blocked_urls_nav = blocked_urls.clone();
    let intercept_store_nav = intercept_store.clone();

    let mut wv_builder = if let Some(ref mut ctx) = web_context {
        WebViewBuilder::new_with_web_context(ctx)
    } else {
        WebViewBuilder::new()
    };

    // Register a custom protocol so we always have a valid http-like origin.
    // On WebKitGTK, wry's IPC handler builds an http::Request using the
    // webview URI — non-http schemes like about: or data: cause a panic.
    // The `skill` protocol serves a blank page and gives us a valid origin
    // for localStorage/sessionStorage/cookies.
    wv_builder = wv_builder.with_custom_protocol("skill".into(), |_id, _req| {
        http::Response::builder()
            .header("Content-Type", "text/html; charset=utf-8")
            .body(std::borrow::Cow::Borrowed(
                b"<!DOCTYPE html><html><head><title></title></head><body></body></html>"
                    as &[u8],
            ))
            .expect("static HTTP response")
    });

    // Use our custom protocol as the initial URL if the user specified about:blank.
    let effective_url = if config.initial_url == "about:blank" {
        "skill://localhost/".to_string()
    } else {
        config.initial_url.clone()
    };

    // In headless mode the window is invisible, so the webview reports
    // 0x0 for innerWidth/innerHeight.  We override those properties (and
    // related APIs) with the configured dimensions so page layout,
    // media queries, and user scripts see the expected viewport.
    if config.mode == Mode::Headless {
        let w = config.width;
        let h = config.height;
        let init_js = format!(
            r#"
            (function() {{
                var __vw = {w}, __vh = {h};
                Object.defineProperty(window, 'innerWidth',  {{ get: function() {{ return __vw; }}, configurable: true }});
                Object.defineProperty(window, 'innerHeight', {{ get: function() {{ return __vh; }}, configurable: true }});
                Object.defineProperty(window, 'outerWidth',  {{ get: function() {{ return __vw; }}, configurable: true }});
                Object.defineProperty(window, 'outerHeight', {{ get: function() {{ return __vh; }}, configurable: true }});
                Object.defineProperty(document.documentElement, 'clientWidth',  {{ get: function() {{ return __vw; }}, configurable: true }});
                Object.defineProperty(document.documentElement, 'clientHeight', {{ get: function() {{ return __vh; }}, configurable: true }});
                window.__skillSetViewport = function(w, h) {{ __vw = w; __vh = h; }};
            }})();
            "#
        );
        wv_builder = wv_builder.with_initialization_script(&init_js);
    }

    wv_builder = wv_builder
        .with_url(&effective_url)
        .with_devtools(config.devtools)
        .with_navigation_handler({
            move |url: String| {
                let patterns = blocked_urls_nav.lock().expect("lock poisoned");
                let blocked = patterns.iter().any(|p| url.contains(p.as_str()));
                let ts = js_timestamp();
                intercept_store_nav.push_navigation(NavigationEvent {
                    url: url.clone(),
                    allowed: !blocked,
                    timestamp_ms: ts,
                });
                !blocked // return true to allow, false to block
            }
        })
        .with_ipc_handler(move |msg| {
            let body = msg.body().to_string();

            // ── Network interception messages ────────────────────────
            if let Some(json) = body.strip_prefix("__net_req:") {
                if let Ok(req) = serde_json::from_str::<InterceptedRequest>(json) {
                    intercept_store_ipc.push_request(req);
                }
                return;
            }
            if let Some(json) = body.strip_prefix("__net_res:") {
                if let Ok(resp) = serde_json::from_str::<InterceptedResponse>(json) {
                    intercept_store_ipc.push_response(resp);
                }
                return;
            }

            // ── Async IPC replies (existing) ─────────────────────────
            // Expected format: "ipc_id:result_text"
            if let Some((id, result)) = body.split_once(':') {
                let mut pending = pending_ipc_clone.lock().expect("lock poisoned");
                if let Some(reply) = pending.remove(id) {
                    let _ = reply.send(Response::Text(result.to_string()));
                }
            }
        });

    if let Some(ref ua) = config.user_agent {
        wv_builder = wv_builder.with_user_agent(ua);
    }

    // On Linux, use build_gtk with the inner vbox for Wayland + X11 support.
    // GtkApplicationWindow is a GtkBin that already contains a GtkBox,
    // so we must add the webview to that inner box, not the window itself.
    #[cfg(target_os = "linux")]
    let webview = {
        use gtk::prelude::*;
        let vbox = window
            .gtk_window()
            .children()
            .into_iter()
            .next()
            .and_then(|w| w.downcast::<gtk::Box>().ok())
            .expect("tao window should contain a GtkBox");
        wv_builder
            .build_gtk(&vbox)
            .map_err(|e| HeadlessError::InitFailed(e.to_string()))?
    };

    #[cfg(not(target_os = "linux"))]
    let webview = wv_builder
        .build(&window)
        .map_err(|e| HeadlessError::InitFailed(e.to_string()))?;

    // We need to keep webview alive for the duration of the event loop.
    // Wrap in Option so we can destroy it on Close.
    let webview: Arc<Mutex<Option<WebView>>> = Arc::new(Mutex::new(Some(webview)));

    event_loop.run_return(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Keep web_context alive for the entire event loop lifetime.
        let _ = &web_context;

        match event {
            Event::UserEvent(UserEvent::Command(envelope)) => {
                let Envelope { command, reply } = envelope;

                let wv_guard = webview.lock().expect("lock poisoned");
                if let Some(ref wv) = *wv_guard {
                    execute_command(
                        wv, &window, &command, reply.clone(), &pending_ipc,
                        config.mode, &intercept_store, &blocked_urls,
                    );
                } else {
                    let _ = reply.send(Response::Error("webview destroyed".into()));
                }
                drop(wv_guard);

                // Handle Close — destroy the webview and exit.
                if matches!(command, Command::Close) {
                    *webview.lock().expect("lock poisoned") = None;
                    *control_flow = ControlFlow::Exit;
                    closed.store(true, Ordering::Relaxed);
                }
            }

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
                closed.store(true, Ordering::Relaxed);
            }

            _ => {}
        }
    });

    Ok(())
}

// ── Command dispatch ─────────────────────────────────────────────────────────

/// Execute a command on the event-loop thread.
///
/// For commands that need a JS return value, we use
/// `evaluate_script_with_callback` which calls back on the webview thread
/// without blocking the event loop.  The callback sends the response
/// through the `reply` channel.
fn execute_command(
    wv: &WebView,
    window: &Window,
    command: &Command,
    reply: Sender<Response>,
    pending_ipc: &Arc<Mutex<std::collections::HashMap<String, Sender<Response>>>>,
    mode: Mode,
    intercept_store: &InterceptStore,
    blocked_urls: &Arc<Mutex<Vec<String>>>,
) {
    match command {
        // ── Page ─────────────────────────────────────────────────────────
        Command::Navigate { url } => {
            let resp = match wv.load_url(url) {
                Ok(_) => Response::Ok,
                Err(e) => Response::Error(format!("navigate: {e}")),
            };
            let _ = reply.send(resp);
        }

        Command::Reload { ignore_cache } => {
            let script = if *ignore_cache {
                "location.reload(true)"
            } else {
                "location.reload()"
            };
            eval_fire(wv, script, reply);
        }

        Command::GoBack => eval_fire(wv, "history.back()", reply),
        Command::GoForward => eval_fire(wv, "history.forward()", reply),
        Command::StopLoading => eval_fire(wv, "window.stop()", reply),

        Command::GetUrl => eval_with_cb(wv, "location.href", reply),
        Command::GetTitle => eval_with_cb(wv, "document.title", reply),

        Command::GetContent => {
            eval_with_cb(wv, "document.documentElement.outerHTML", reply)
        }

        Command::Screenshot => {
            // Capture the visible viewport as a PNG using a canvas-based
            // DOM walker.  We iterate over all visible elements and paint
            // their computed backgrounds, borders, and text onto a 2D canvas.
            //
            // This is a lightweight "mini html2canvas" that handles the most
            // common cases (solid backgrounds, text, images, borders).
            // For pixel-perfect fidelity, inject the full html2canvas library
            // and use EvalJs instead.
            eval_async_ipc(wv, pending_ipc, reply, r#"
                (async () => {
                    const W = window.innerWidth  || document.documentElement.clientWidth  || 800;
                    const H = window.innerHeight || document.documentElement.clientHeight || 600;
                    const canvas = document.createElement('canvas');
                    canvas.width  = W;
                    canvas.height = H;
                    const ctx = canvas.getContext('2d');

                    /* Paint document background first */
                    const docBg = getComputedStyle(document.documentElement).backgroundColor;
                    const bodyBg = document.body ? getComputedStyle(document.body).backgroundColor : 'rgba(0,0,0,0)';
                    ctx.fillStyle = 'white';
                    ctx.fillRect(0, 0, W, H);
                    if (docBg && docBg !== 'rgba(0, 0, 0, 0)') { ctx.fillStyle = docBg; ctx.fillRect(0, 0, W, H); }
                    if (bodyBg && bodyBg !== 'rgba(0, 0, 0, 0)') { ctx.fillStyle = bodyBg; ctx.fillRect(0, 0, W, H); }

                    /* Walk all elements in DOM order */
                    const walker = document.createTreeWalker(
                        document.body || document.documentElement,
                        NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT,
                        null
                    );
                    let node;
                    while ((node = walker.nextNode())) {
                        if (node.nodeType === Node.ELEMENT_NODE) {
                            const style = getComputedStyle(node);
                            if (style.display === 'none' || style.visibility === 'hidden') continue;
                            const rect = node.getBoundingClientRect();
                            if (rect.width === 0 || rect.height === 0) continue;

                            /* Background */
                            const bg = style.backgroundColor;
                            if (bg && bg !== 'rgba(0, 0, 0, 0)') {
                                ctx.fillStyle = bg;
                                ctx.fillRect(rect.left, rect.top, rect.width, rect.height);
                            }

                            /* Border (simple solid) */
                            const bw = parseFloat(style.borderTopWidth) || 0;
                            if (bw > 0 && style.borderTopStyle !== 'none') {
                                ctx.strokeStyle = style.borderTopColor || '#000';
                                ctx.lineWidth = bw;
                                ctx.strokeRect(rect.left, rect.top, rect.width, rect.height);
                            }

                            /* Images */
                            if (node.tagName === 'IMG' && node.complete && node.naturalWidth > 0) {
                                try { ctx.drawImage(node, rect.left, rect.top, rect.width, rect.height); } catch(e) {}
                            }

                            /* Canvas elements */
                            if (node.tagName === 'CANVAS') {
                                try { ctx.drawImage(node, rect.left, rect.top, rect.width, rect.height); } catch(e) {}
                            }
                        } else if (node.nodeType === Node.TEXT_NODE) {
                            const text = node.textContent.trim();
                            if (!text) continue;
                            const parent = node.parentElement;
                            if (!parent) continue;
                            const style = getComputedStyle(parent);
                            if (style.display === 'none' || style.visibility === 'hidden') continue;

                            const range = document.createRange();
                            range.selectNodeContents(node);
                            const rects = range.getClientRects();
                            ctx.fillStyle = style.color || '#000';
                            ctx.font = style.fontStyle + ' ' + style.fontWeight + ' ' + style.fontSize + ' ' + style.fontFamily;
                            ctx.textBaseline = 'top';
                            for (const r of rects) {
                                ctx.fillText(text, r.left, r.top);
                            }
                        }
                    }

                    return canvas.toDataURL('image/png');
                })()
            "#);
        }

        Command::PrintToPdf => {
            let _ = reply.send(Response::Error(
                "PDF printing not supported by wry backend".into(),
            ));
        }

        // ── Runtime ──────────────────────────────────────────────────────
        Command::EvalJs { script } => eval_with_cb(wv, script, reply),

        Command::EvalJsNoReturn { script } => eval_fire(wv, script, reply),

        Command::CallFunction { function, args } => {
            let args_str = args.join(", ");
            let script = format!("{function}({args_str})");
            eval_with_cb(wv, &script, reply);
        }

        // ── DOM ──────────────────────────────────────────────────────────
        Command::InjectCss { css } => {
            let escaped = css.replace('\\', "\\\\").replace('`', "\\`");
            let script = format!(
                r#"(() => {{ const s = document.createElement('style'); s.textContent = `{escaped}`; document.head.appendChild(s); }})()"#
            );
            eval_fire(wv, &script, reply);
        }

        Command::InjectScriptUrl { url } => {
            let escaped = url.replace('\\', "\\\\").replace('\'', "\\'");
            let script = format!(
                r#"(() => {{ const s = document.createElement('script'); s.src = '{escaped}'; document.head.appendChild(s); }})()"#
            );
            eval_fire(wv, &script, reply);
        }

        Command::InjectScriptContent { content } => {
            let escaped = content.replace('\\', "\\\\").replace('`', "\\`");
            let script = format!(
                r#"(() => {{ const s = document.createElement('script'); s.textContent = `{escaped}`; document.head.appendChild(s); }})()"#
            );
            eval_fire(wv, &script, reply);
        }

        Command::QuerySelector { selector } => {
            let sel = js_escape(selector);
            let script = format!(
                r#"JSON.stringify(Array.from(document.querySelectorAll('{sel}')).map(e => e.outerHTML))"#
            );
            eval_with_cb(wv, &script, reply);
        }

        Command::QuerySelectorText { selector } => {
            let sel = js_escape(selector);
            let script = format!(
                r#"JSON.stringify(Array.from(document.querySelectorAll('{sel}')).map(e => e.textContent || ''))"#
            );
            eval_with_cb(wv, &script, reply);
        }

        Command::GetAttribute {
            selector,
            attribute,
        } => {
            let sel = js_escape(selector);
            let attr = js_escape(attribute);
            let script = format!(
                r#"(() => {{ const el = document.querySelector('{sel}'); return el ? (el.getAttribute('{attr}') || '') : ''; }})()"#
            );
            eval_with_cb(wv, &script, reply);
        }

        Command::Click { selector } => {
            let sel = js_escape(selector);
            let script = format!(
                r#"(() => {{ const el = document.querySelector('{sel}'); if (el) {{ el.click(); return 'ok'; }} return 'not_found'; }})()"#
            );
            eval_with_cb(wv, &script, reply);
        }

        Command::TypeText { selector, text } => {
            let txt = js_escape(text);
            let script = if let Some(sel) = selector {
                let s = js_escape(sel);
                format!(
                    r#"(() => {{ const el = document.querySelector('{s}'); if (el) {{ el.focus(); }} document.execCommand('insertText', false, '{txt}'); }})()"#
                )
            } else {
                format!(r#"document.execCommand('insertText', false, '{txt}')"#)
            };
            eval_fire(wv, &script, reply);
        }

        Command::SetValue { selector, value } => {
            let sel = js_escape(selector);
            let val = js_escape(value);
            let script = format!(
                r#"(() => {{
                    const el = document.querySelector('{sel}');
                    if (el) {{
                        el.value = '{val}';
                        el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                        el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    }}
                }})()"#
            );
            eval_fire(wv, &script, reply);
        }

        Command::ScrollBy { x, y } => {
            eval_fire(wv, &format!("window.scrollBy({x}, {y})"), reply)
        }

        Command::ScrollTo { x, y } => {
            eval_fire(wv, &format!("window.scrollTo({x}, {y})"), reply)
        }

        // ── Cookies ──────────────────────────────────────────────────────
        Command::SetCookie { cookie } => {
            let Cookie {
                name,
                value,
                domain,
                path,
                expires,
                http_only: _,
                secure,
                same_site,
            } = cookie;
            let mut parts = vec![format!("{}={}", js_escape(name), js_escape(value))];
            if !domain.is_empty() {
                parts.push(format!("domain={}", js_escape(domain)));
            }
            if !path.is_empty() {
                parts.push(format!("path={}", js_escape(path)));
            } else {
                parts.push("path=/".into());
            }
            if *expires > 0.0 {
                parts.push(format!("expires={expires}"));
            }
            if *secure {
                parts.push("secure".into());
            }
            parts.push(format!("samesite={}", same_site.as_str()));
            let cookie_str = parts.join("; ");
            eval_fire(wv, &format!("document.cookie = '{cookie_str}'"), reply);
        }

        Command::GetCookies { domain: _ } => {
            eval_with_cb(wv, "document.cookie", reply);
        }

        Command::DeleteCookies { name, domain } => {
            let n = js_escape(name);
            let d = domain.as_deref().map(js_escape).unwrap_or_default();
            let domain_part = if d.is_empty() {
                String::new()
            } else {
                format!("; domain={d}")
            };
            eval_fire(
                wv,
                &format!(
                    "document.cookie = '{n}=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/{domain_part}'"
                ),
                reply,
            );
        }

        Command::ClearCookies => {
            let script = r#"
                document.cookie.split(';').forEach(c => {
                    const name = c.split('=')[0].trim();
                    document.cookie = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
                })
            "#;
            eval_fire(wv, script, reply);
        }

        // ── localStorage ─────────────────────────────────────────────────
        Command::GetLocalStorage { key } => {
            let k = js_escape(key);
            eval_with_cb(wv, &format!("localStorage.getItem('{k}')"), reply);
        }

        Command::SetLocalStorage { key, value } => {
            let k = js_escape(key);
            let v = js_escape(value);
            eval_fire(wv, &format!("localStorage.setItem('{k}', '{v}')"), reply);
        }

        Command::RemoveLocalStorage { key } => {
            let k = js_escape(key);
            eval_fire(wv, &format!("localStorage.removeItem('{k}')"), reply);
        }

        Command::ClearLocalStorage => eval_fire(wv, "localStorage.clear()", reply),

        // ── sessionStorage ───────────────────────────────────────────────
        Command::GetSessionStorage { key } => {
            let k = js_escape(key);
            eval_with_cb(wv, &format!("sessionStorage.getItem('{k}')"), reply);
        }

        Command::SetSessionStorage { key, value } => {
            let k = js_escape(key);
            let v = js_escape(value);
            eval_fire(
                wv,
                &format!("sessionStorage.setItem('{k}', '{v}')"),
                reply,
            );
        }

        // ── Emulation ────────────────────────────────────────────────────
        Command::SetUserAgent { user_agent: _ } => {
            let _ = reply.send(Response::Error(
                "user-agent can only be set at launch via BrowserConfig".into(),
            ));
        }

        Command::SetViewport { width, height } => {
            window.set_inner_size(LogicalSize::new(*width, *height));
            // In headless mode the window is invisible, so the native
            // innerWidth/innerHeight stay 0.  Update the JS overrides.
            if mode == Mode::Headless {
                let _ = wv.evaluate_script(
                    &format!("if(window.__skillSetViewport) window.__skillSetViewport({width},{height});"),
                );
            }
            let _ = reply.send(Response::Ok);
        }

        Command::SetJsEnabled { enabled: _ } => {
            let _ = reply.send(Response::Error(
                "toggling JS at runtime is not supported by wry".into(),
            ));
        }

        // ── Cache ────────────────────────────────────────────────────────
        Command::ClearCache => {
            eval_async_ipc(wv, pending_ipc, reply, r#"
                (async () => {
                    if ('caches' in window) {
                        const names = await caches.keys();
                        await Promise.all(names.map(n => caches.delete(n)));
                    }
                    return 'ok';
                })()
            "#);
        }

        Command::ClearBrowsingData => {
            eval_async_ipc(wv, pending_ipc, reply, r#"
                (async () => {
                    localStorage.clear();
                    sessionStorage.clear();
                    document.cookie.split(';').forEach(c => {
                        const name = c.split('=')[0].trim();
                        document.cookie = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
                    });
                    if ('caches' in window) {
                        const names = await caches.keys();
                        await Promise.all(names.map(n => caches.delete(n)));
                    }
                    return 'ok';
                })()
            "#);
        }

        // ── Waiting ──────────────────────────────────────────────────────
        Command::WaitForSelector {
            selector,
            timeout_ms,
        } => {
            let sel = js_escape(selector);
            eval_async_ipc(wv, pending_ipc, reply, &format!(
                r#"
                (async () => {{
                    const deadline = Date.now() + {timeout_ms};
                    while (Date.now() < deadline) {{
                        if (document.querySelector('{sel}')) return 'found';
                        await new Promise(r => setTimeout(r, 100));
                    }}
                    return 'timeout';
                }})()
                "#
            ));
        }

        Command::WaitForNavigation { timeout_ms } => {
            eval_async_ipc(wv, pending_ipc, reply, &format!(
                r#"
                new Promise((resolve) => {{
                    const timer = setTimeout(() => resolve('timeout'), {timeout_ms});
                    window.addEventListener('load', () => {{
                        clearTimeout(timer);
                        resolve('loaded');
                    }}, {{ once: true }});
                }})
                "#
            ));
        }

        // ── Network Interception ────────────────────────────────────────
        Command::EnableInterception => {
            let script = intercept::interception_init_script();
            let resp = match wv.evaluate_script(&script) {
                Ok(_) => Response::Ok,
                Err(e) => Response::Error(format!("enable interception: {e}")),
            };
            let _ = reply.send(resp);
        }

        Command::DisableInterception => {
            // Restore original fetch/XHR by reloading the flag.
            // A full restore would require storing originals, but clearing
            // the flag prevents future re-injection.
            let _ = wv.evaluate_script("window.__skillNetInterceptInstalled = false;");
            let _ = reply.send(Response::Ok);
        }

        Command::GetInterceptedRequests { clear } => {
            let log = intercept_store.snapshot(*clear);
            let _ = reply.send(Response::Network(log));
        }

        Command::SetBlockedUrls { patterns } => {
            *blocked_urls.lock().expect("lock poisoned") = patterns.clone();
            let _ = reply.send(Response::Ok);
        }

        Command::ClearBlockedUrls => {
            blocked_urls.lock().expect("lock poisoned").clear();
            let _ = reply.send(Response::Ok);
        }

        Command::Close => {
            let _ = reply.send(Response::Ok);
        }
    }
}

// ── JS helpers ───────────────────────────────────────────────────────────────

/// Counter for generating unique IPC message IDs.
static IPC_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Evaluate an async JS expression (returns a Promise) using the IPC channel.
///
/// Wraps the expression so its resolved value is sent back via
/// `window.ipc.postMessage("id:result")`.  The reply channel is stored in
/// `pending_ipc` and matched when the IPC handler fires.
fn eval_async_ipc(
    wv: &WebView,
    pending_ipc: &Arc<Mutex<std::collections::HashMap<String, Sender<Response>>>>,
    reply: Sender<Response>,
    script: &str,
) {
    let id = IPC_COUNTER.fetch_add(1, Ordering::Relaxed);
    let id_str = format!("__ipc_{id}");

    // Register the pending reply.
    pending_ipc.lock().expect("lock poisoned").insert(id_str.clone(), reply.clone());

    let wrapped = format!(
        r#"
        Promise.resolve({script}).then(__r => {{
            window.ipc.postMessage('{id_str}:' + String(__r ?? ''));
        }}).catch(__e => {{
            window.ipc.postMessage('{id_str}:__error__:' + __e.message);
        }});
        "#
    );

    if let Err(e) = wv.evaluate_script(&wrapped) {
        // Remove pending entry and send error immediately.
        pending_ipc.lock().expect("lock poisoned").remove(&id_str);
        let _ = reply.send(Response::Error(format!("eval failed: {e}")));
    }
}

/// Evaluate JS and get the result via `evaluate_script_with_callback`.
///
/// This does NOT block the event loop.  The callback fires asynchronously
/// on the webview thread and sends the result through the reply channel.
fn eval_with_cb(wv: &WebView, script: &str, reply: Sender<Response>) {
    let reply_err = reply.clone();
    match wv.evaluate_script_with_callback(script, move |result| {
        // The callback receives the JS result as a String.
        // wry returns the raw JS value stringified — strings come with quotes.
        let cleaned = unquote_js_string(&result);
        let _ = reply.send(Response::Text(cleaned));
    }) {
        Ok(_) => {} // response will come via callback
        Err(e) => {
            let _ = reply_err.send(Response::Error(format!("eval failed: {e}")));
        }
    }
}

/// Evaluate JS fire-and-forget (no callback, immediate response).
fn eval_fire(wv: &WebView, script: &str, reply: Sender<Response>) {
    let resp = match wv.evaluate_script(script) {
        Ok(_) => Response::Ok,
        Err(e) => Response::Error(format!("eval failed: {e}")),
    };
    let _ = reply.send(resp);
}

/// Strip surrounding quotes from a JS callback result.
///
/// `evaluate_script_with_callback` returns strings as `"value"` (with literal
/// quotes).  `null` and `undefined` come as-is.
fn unquote_js_string(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed == "null" || trimmed == "undefined" {
        return trimmed.to_string();
    }
    // If wrapped in double quotes, parse as JSON string to handle escapes.
    if trimmed.starts_with('"') && trimmed.ends_with('"') {
        if let Ok(parsed) = serde_json::from_str::<String>(trimmed) {
            return parsed;
        }
    }
    trimmed.to_string()
}

/// Escape a string for safe embedding in a JS single-quoted string literal.
fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Current time as milliseconds since the Unix epoch (like `Date.now()` in JS).
fn js_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0
}

// ── External renderer access ─────────────────────────────────────────────────

/// Render a URL using the registered external renderer (if any).
///
/// Returns `None` if no external renderer is registered.
/// Returns `Some(Ok(text))` on success or `Some(Err(msg))` on failure.
pub fn external_fetch_page(url: &str, wait_ms: u64) -> Option<Result<String, String>> {
    FETCH_CANCELLED.store(false, Ordering::Relaxed);
    EXTERNAL_RENDERER.get().map(|f| f(url, wait_ms))
}

/// Signal the current external fetch to abort.
///
/// Called from the UI when the user cancels a tool call.  The external
/// renderer checks this flag during its page-load wait loop.
pub fn cancel_current_fetch() {
    FETCH_CANCELLED.store(true, Ordering::Relaxed);
}

/// Check whether the current fetch has been cancelled.
pub fn is_fetch_cancelled() -> bool {
    FETCH_CANCELLED.load(Ordering::Relaxed)
}


