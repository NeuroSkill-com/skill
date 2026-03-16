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

use crate::command::Command;
use crate::error::HeadlessError;
use crate::response::Response;
use crate::session::Cookie;

// ── Configuration ────────────────────────────────────────────────────────────

/// Browser configuration.
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Initial viewport width.
    pub width: u32,
    /// Initial viewport height.
    pub height: u32,
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
    /// Whether the window should be visible (default false = headless).
    pub visible: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            user_agent: None,
            data_dir: None,
            timeout: Duration::from_secs(30),
            devtools: false,
            initial_url: "about:blank".into(),
            visible: false,
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
    pub fn launch(config: BrowserConfig) -> Result<Self, HeadlessError> {
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
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // Send the proxy handle back to the launching thread.
    let _ = proxy_tx.send(Ok(proxy));

    let window = WindowBuilder::new()
        .with_title("skill-headless")
        .with_inner_size(LogicalSize::new(config.width, config.height))
        .with_visible(config.visible)
        .build(&event_loop)
        .map_err(|e| HeadlessError::InitFailed(e.to_string()))?;

    // IPC callback state: we use this to receive JS evaluation results.
    let ipc_response: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let ipc_notify: Arc<(Mutex<bool>, std::sync::Condvar)> =
        Arc::new((Mutex::new(false), std::sync::Condvar::new()));

    let ipc_resp_clone = ipc_response.clone();
    let ipc_notify_clone = ipc_notify.clone();

    // Optional persistent web context for data directory / cache.
    let mut web_context = config
        .data_dir
        .as_ref()
        .map(|dir| WebContext::new(Some(dir.clone())));

    let mut builder = if let Some(ref mut ctx) = web_context {
        WebViewBuilder::with_web_context(ctx)
    } else {
        WebViewBuilder::new()
    };

    builder = builder
        .with_url(&config.initial_url)
        .with_ipc_handler(move |msg| {
            let body = msg.body().to_string();
            *ipc_resp_clone.lock().unwrap() = Some(body);
            let (lock, cvar) = &*ipc_notify_clone;
            *lock.lock().unwrap() = true;
            cvar.notify_all();
        })
        .with_devtools(config.devtools);

    if let Some(ref ua) = config.user_agent {
        builder = builder.with_user_agent(ua);
    }

    let webview = builder
        .build(&window)
        .map_err(|e| HeadlessError::InitFailed(e.to_string()))?;

    // We need to keep webview alive for the duration of the event loop.
    // Wrap in Option so we can destroy it on Close.
    let webview: Arc<Mutex<Option<WebView>>> = Arc::new(Mutex::new(Some(webview)));

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Keep web_context alive for the entire event loop lifetime.
        let _ = &web_context;

        match event {
            Event::UserEvent(UserEvent::Command(envelope)) => {
                let Envelope { command, reply } = envelope;
                let resp = {
                    let wv_guard = webview.lock().unwrap();
                    if let Some(ref wv) = *wv_guard {
                        execute_command(wv, &window, &command, &ipc_response, &ipc_notify)
                    } else {
                        Response::Error("webview destroyed".into())
                    }
                };

                // Handle Close — destroy the webview and exit.
                if matches!(command, Command::Close) {
                    *webview.lock().unwrap() = None;
                    *control_flow = ControlFlow::Exit;
                    closed.store(true, Ordering::Relaxed);
                }

                let _ = reply.send(resp);
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
}

// ── Command dispatch ─────────────────────────────────────────────────────────

fn execute_command(
    wv: &WebView,
    window: &Window,
    command: &Command,
    ipc_response: &Arc<Mutex<Option<String>>>,
    ipc_notify: &Arc<(Mutex<bool>, std::sync::Condvar)>,
) -> Response {
    match command {
        // ── Page ─────────────────────────────────────────────────────────
        Command::Navigate { url } => match wv.load_url(url) {
            Ok(_) => Response::Ok,
            Err(e) => Response::Error(format!("navigate: {e}")),
        },

        Command::Reload { ignore_cache } => {
            let script = if *ignore_cache {
                "location.reload(true)"
            } else {
                "location.reload()"
            };
            eval_js_fire(wv, script)
        }

        Command::GoBack => eval_js_fire(wv, "history.back()"),
        Command::GoForward => eval_js_fire(wv, "history.forward()"),
        Command::StopLoading => eval_js_fire(wv, "window.stop()"),

        Command::GetUrl => eval_js_sync(wv, "return location.href;", ipc_response, ipc_notify),
        Command::GetTitle => {
            eval_js_sync(wv, "return document.title;", ipc_response, ipc_notify)
        }

        Command::GetContent => eval_js_sync(
            wv,
            "return document.documentElement.outerHTML;",
            ipc_response,
            ipc_notify,
        ),

        Command::Screenshot => {
            Response::Error(
                "screenshot not natively supported by wry; \
                 inject html2canvas.js first, then use EvalJs to capture a data URL"
                    .into(),
            )
        }

        Command::PrintToPdf => {
            Response::Error("PDF printing not supported by wry backend".into())
        }

        // ── Runtime ──────────────────────────────────────────────────────
        Command::EvalJs { script } => eval_js_sync(wv, script, ipc_response, ipc_notify),

        Command::EvalJsNoReturn { script } => eval_js_fire(wv, script),

        Command::CallFunction { function, args } => {
            let args_str = args.join(", ");
            let script = format!("return {function}({args_str});");
            eval_js_sync(wv, &script, ipc_response, ipc_notify)
        }

        // ── DOM ──────────────────────────────────────────────────────────
        Command::InjectCss { css } => {
            let escaped = css.replace('\\', "\\\\").replace('`', "\\`");
            let script = format!(
                r#"(() => {{ const s = document.createElement('style'); s.textContent = `{escaped}`; document.head.appendChild(s); }})();"#
            );
            eval_js_fire(wv, &script)
        }

        Command::InjectScriptUrl { url } => {
            let escaped = url.replace('\\', "\\\\").replace('\'', "\\'");
            let script = format!(
                r#"(() => {{ const s = document.createElement('script'); s.src = '{escaped}'; document.head.appendChild(s); }})();"#
            );
            eval_js_fire(wv, &script)
        }

        Command::InjectScriptContent { content } => {
            let escaped = content.replace('\\', "\\\\").replace('`', "\\`");
            let script = format!(
                r#"(() => {{ const s = document.createElement('script'); s.textContent = `{escaped}`; document.head.appendChild(s); }})();"#
            );
            eval_js_fire(wv, &script)
        }

        Command::QuerySelector { selector } => {
            let sel = js_escape(selector);
            let script = format!(
                r#"return JSON.stringify(Array.from(document.querySelectorAll('{sel}')).map(e => e.outerHTML));"#
            );
            eval_js_sync(wv, &script, ipc_response, ipc_notify)
        }

        Command::QuerySelectorText { selector } => {
            let sel = js_escape(selector);
            let script = format!(
                r#"return JSON.stringify(Array.from(document.querySelectorAll('{sel}')).map(e => e.textContent || ''));"#
            );
            eval_js_sync(wv, &script, ipc_response, ipc_notify)
        }

        Command::GetAttribute {
            selector,
            attribute,
        } => {
            let sel = js_escape(selector);
            let attr = js_escape(attribute);
            let script = format!(
                r#"{{ const el = document.querySelector('{sel}'); return el ? (el.getAttribute('{attr}') || '') : ''; }}"#
            );
            eval_js_sync(wv, &script, ipc_response, ipc_notify)
        }

        Command::Click { selector } => {
            let sel = js_escape(selector);
            let script = format!(
                r#"{{ const el = document.querySelector('{sel}'); if (el) {{ el.click(); return 'ok'; }} return 'not_found'; }}"#
            );
            eval_js_sync(wv, &script, ipc_response, ipc_notify)
        }

        Command::TypeText { selector, text } => {
            let txt = js_escape(text);
            let script = if let Some(sel) = selector {
                let s = js_escape(sel);
                format!(
                    r#"{{ const el = document.querySelector('{s}'); if (el) {{ el.focus(); }} document.execCommand('insertText', false, '{txt}'); }}"#
                )
            } else {
                format!(r#"document.execCommand('insertText', false, '{txt}')"#)
            };
            eval_js_fire(wv, &script)
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
            eval_js_fire(wv, &script)
        }

        Command::ScrollBy { x, y } => eval_js_fire(wv, &format!("window.scrollBy({x}, {y})")),

        Command::ScrollTo { x, y } => eval_js_fire(wv, &format!("window.scrollTo({x}, {y})")),

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
            eval_js_fire(wv, &format!("document.cookie = '{cookie_str}'"))
        }

        Command::GetCookies { domain: _ } => {
            eval_js_sync(
                wv,
                "return document.cookie;",
                ipc_response,
                ipc_notify,
            )
        }

        Command::DeleteCookies { name, domain } => {
            let n = js_escape(name);
            let d = domain.as_deref().map(js_escape).unwrap_or_default();
            let domain_part = if d.is_empty() {
                String::new()
            } else {
                format!("; domain={d}")
            };
            eval_js_fire(
                wv,
                &format!(
                    "document.cookie = '{n}=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/{domain_part}'"
                ),
            )
        }

        Command::ClearCookies => {
            let script = r#"
                document.cookie.split(';').forEach(c => {
                    const name = c.split('=')[0].trim();
                    document.cookie = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/';
                });
            "#;
            eval_js_fire(wv, script)
        }

        // ── localStorage ─────────────────────────────────────────────────
        Command::GetLocalStorage { key } => {
            let k = js_escape(key);
            eval_js_sync(
                wv,
                &format!("return localStorage.getItem('{k}');"),
                ipc_response,
                ipc_notify,
            )
        }

        Command::SetLocalStorage { key, value } => {
            let k = js_escape(key);
            let v = js_escape(value);
            eval_js_fire(wv, &format!("localStorage.setItem('{k}', '{v}')"))
        }

        Command::RemoveLocalStorage { key } => {
            let k = js_escape(key);
            eval_js_fire(wv, &format!("localStorage.removeItem('{k}')"))
        }

        Command::ClearLocalStorage => eval_js_fire(wv, "localStorage.clear()"),

        // ── sessionStorage ───────────────────────────────────────────────
        Command::GetSessionStorage { key } => {
            let k = js_escape(key);
            eval_js_sync(
                wv,
                &format!("return sessionStorage.getItem('{k}');"),
                ipc_response,
                ipc_notify,
            )
        }

        Command::SetSessionStorage { key, value } => {
            let k = js_escape(key);
            let v = js_escape(value);
            eval_js_fire(wv, &format!("sessionStorage.setItem('{k}', '{v}')"))
        }

        // ── Emulation ────────────────────────────────────────────────────
        Command::SetUserAgent { user_agent: _ } => {
            Response::Error("user-agent can only be set at launch via BrowserConfig".into())
        }

        Command::SetViewport { width, height } => {
            window.set_inner_size(LogicalSize::new(*width, *height));
            Response::Ok
        }

        Command::SetJsEnabled { enabled: _ } => {
            Response::Error("toggling JS at runtime is not supported by wry".into())
        }

        // ── Cache ────────────────────────────────────────────────────────
        Command::ClearCache => {
            let script = r#"
                (async () => {
                    if ('caches' in window) {
                        const names = await caches.keys();
                        await Promise.all(names.map(n => caches.delete(n)));
                    }
                })()
            "#;
            // This is async — fire and forget since Cache API clear is best-effort.
            eval_js_fire(wv, script)
        }

        Command::ClearBrowsingData => {
            let script = r#"
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
                })()
            "#;
            eval_js_fire(wv, script)
        }

        // ── Waiting ──────────────────────────────────────────────────────
        Command::WaitForSelector {
            selector,
            timeout_ms,
        } => {
            let sel = js_escape(selector);
            let script = format!(
                r#"
                const deadline = Date.now() + {timeout_ms};
                async function __poll() {{
                    while (Date.now() < deadline) {{
                        if (document.querySelector('{sel}')) return 'found';
                        await new Promise(r => setTimeout(r, 100));
                    }}
                    return 'timeout';
                }}
                return await __poll();
                "#
            );
            eval_js_sync(wv, &script, ipc_response, ipc_notify)
        }

        Command::WaitForNavigation { timeout_ms } => {
            let script = format!(
                r#"
                return await new Promise((resolve) => {{
                    const timer = setTimeout(() => resolve('timeout'), {timeout_ms});
                    window.addEventListener('load', () => {{
                        clearTimeout(timer);
                        resolve('loaded');
                    }}, {{ once: true }});
                }});
                "#
            );
            eval_js_sync(wv, &script, ipc_response, ipc_notify)
        }

        Command::Close => Response::Ok,
    }
}

// ── JS helpers ───────────────────────────────────────────────────────────────

/// Evaluate JS and send the result back via IPC.  Blocks until the IPC
/// callback fires or a timeout (5 s) elapses.
fn eval_js_sync(
    wv: &WebView,
    script: &str,
    ipc_response: &Arc<Mutex<Option<String>>>,
    ipc_notify: &Arc<(Mutex<bool>, std::sync::Condvar)>,
) -> Response {
    // Clear previous IPC state.
    *ipc_response.lock().unwrap() = None;
    {
        let (lock, _) = &**ipc_notify;
        *lock.lock().unwrap() = false;
    }

    // Wrap the user script so the result is sent via IPC.
    // The user script can use `return` to provide a value.
    let wrapped = format!(
        r#"
        (async () => {{
            try {{
                const __result = await (async () => {{ {script} }})();
                window.ipc.postMessage(String(__result ?? ''));
            }} catch(e) {{
                window.ipc.postMessage('__error__:' + e.message);
            }}
        }})();
        "#
    );

    if let Err(e) = wv.evaluate_script(&wrapped) {
        return Response::Error(format!("eval failed: {e}"));
    }

    // Wait for IPC callback.
    let (lock, cvar) = &**ipc_notify;
    let result = cvar
        .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(5), |ready| {
            !*ready
        })
        .unwrap();

    if result.1.timed_out() {
        return Response::Error("JS eval timed out (no IPC response in 5 s)".into());
    }

    let msg = ipc_response.lock().unwrap().take().unwrap_or_default();

    if let Some(err) = msg.strip_prefix("__error__:") {
        Response::Error(err.to_string())
    } else {
        Response::Text(msg)
    }
}

/// Evaluate JS fire-and-forget (no IPC round-trip).
fn eval_js_fire(wv: &WebView, script: &str) -> Response {
    match wv.evaluate_script(script) {
        Ok(_) => Response::Ok,
        Err(e) => Response::Error(format!("eval failed: {e}")),
    }
}

/// Escape a string for safe embedding in a JS single-quoted string literal.
fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
