// SPDX-License-Identifier: GPL-3.0-only
//! Network request/response interception.
//!
//! Provides two layers of interception:
//!
//! 1. **Native (wry)** — navigation handler, download handler, new-window
//!    handler.  These are set at launch via [`InterceptConfig`] and
//!    forwarded to wry's builder methods.
//!
//! 2. **JS-level** — `fetch` / `XMLHttpRequest` monkey-patching injected
//!    as an initialization script.  Intercepted requests and responses are
//!    sent to Rust via the IPC channel and can be collected with the
//!    [`Command::GetInterceptedRequests`] command.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// ── Intercepted record types ─────────────────────────────────────────────────

/// An intercepted HTTP request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptedRequest {
    /// Auto-incrementing sequence number.
    pub seq: u64,
    /// HTTP method (GET, POST, ...).
    pub method: String,
    /// Full URL.
    pub url: String,
    /// Request headers as JSON string (from JS `Headers` object).
    pub headers: String,
    /// Request body (for POST/PUT), may be empty.
    pub body: String,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: f64,
}

/// An intercepted HTTP response (paired with a request by `seq`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptedResponse {
    /// Sequence number matching the originating request.
    pub seq: u64,
    /// HTTP status code.
    pub status: u16,
    /// Status text (e.g. "OK", "Not Found").
    pub status_text: String,
    /// Response headers as JSON string.
    pub headers: String,
    /// Response body (text or base64-encoded for binary).
    pub body: String,
    /// Whether the body is base64-encoded.
    pub body_base64: bool,
    /// Full URL (may differ from request due to redirects).
    pub url: String,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: f64,
}

/// A navigation event intercepted by wry's navigation handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationEvent {
    /// The URL being navigated to.
    pub url: String,
    /// Whether the navigation was allowed.
    pub allowed: bool,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: f64,
}

/// Collected network traffic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkLog {
    pub requests: Vec<InterceptedRequest>,
    pub responses: Vec<InterceptedResponse>,
    pub navigations: Vec<NavigationEvent>,
}

// ── Shared state ─────────────────────────────────────────────────────────────

/// Thread-safe store for intercepted network traffic.
#[derive(Debug, Clone, Default)]
pub struct InterceptStore {
    inner: Arc<Mutex<NetworkLog>>,
}

impl InterceptStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_request(&self, req: InterceptedRequest) {
        self.inner.lock().expect("lock poisoned").requests.push(req);
    }

    pub fn push_response(&self, resp: InterceptedResponse) {
        self.inner.lock().expect("lock poisoned").responses.push(resp);
    }

    pub fn push_navigation(&self, nav: NavigationEvent) {
        self.inner.lock().expect("lock poisoned").navigations.push(nav);
    }

    /// Take a snapshot of all collected traffic and optionally clear it.
    pub fn snapshot(&self, clear: bool) -> NetworkLog {
        let mut guard = self.inner.lock().expect("lock poisoned");
        if clear {
            std::mem::take(&mut *guard)
        } else {
            guard.clone()
        }
    }

    /// Clear all collected traffic.
    pub fn clear(&self) {
        let mut guard = self.inner.lock().expect("lock poisoned");
        *guard = NetworkLog::default();
    }
}

// ── JS initialization script ─────────────────────────────────────────────────

/// Generate the JS initialization script that monkey-patches `fetch` and
/// `XMLHttpRequest` to send intercepted requests/responses via IPC.
///
/// The IPC messages use the format:
///   `__net_req:<json>`  — intercepted request
///   `__net_res:<json>`  — intercepted response
pub fn interception_init_script() -> String {
    r#"
    (function() {
        if (window.__skillNetInterceptInstalled) return;
        window.__skillNetInterceptInstalled = true;
        var __seq = 0;

        /* ── fetch ──────────────────────────────────────────────────── */
        var __origFetch = window.fetch;
        window.fetch = function(input, init) {
            var seq = ++__seq;
            var method = (init && init.method) ? init.method : 'GET';
            var url = (typeof input === 'string') ? input : (input.url || String(input));
            var headers = '{}';
            try {
                if (init && init.headers) {
                    if (init.headers instanceof Headers) {
                        var h = {}; init.headers.forEach(function(v,k){ h[k]=v; }); headers = JSON.stringify(h);
                    } else {
                        headers = JSON.stringify(init.headers);
                    }
                }
            } catch(e) {}
            var body = (init && init.body) ? String(init.body) : '';
            var ts = Date.now();

            try {
                window.ipc.postMessage('__net_req:' + JSON.stringify({
                    seq: seq, method: method, url: url,
                    headers: headers, body: body, timestamp_ms: ts
                }));
            } catch(e) {}

            return __origFetch.apply(this, arguments).then(function(response) {
                var resClone = response.clone();
                resClone.text().then(function(bodyText) {
                    try {
                        window.ipc.postMessage('__net_res:' + JSON.stringify({
                            seq: seq, status: response.status,
                            status_text: response.statusText || '',
                            headers: JSON.stringify(Object.fromEntries(response.headers.entries())),
                            body: bodyText, body_base64: false,
                            url: response.url || url, timestamp_ms: Date.now()
                        }));
                    } catch(e) {}
                }).catch(function(){});
                return response;
            });
        };

        /* ── XMLHttpRequest ─────────────────────────────────────────── */
        var __origXHROpen = XMLHttpRequest.prototype.open;
        var __origXHRSend = XMLHttpRequest.prototype.send;
        var __origXHRSetHeader = XMLHttpRequest.prototype.setRequestHeader;

        XMLHttpRequest.prototype.open = function(method, url) {
            this.__skillSeq = ++__seq;
            this.__skillMethod = method;
            this.__skillUrl = url;
            this.__skillHeaders = {};
            return __origXHROpen.apply(this, arguments);
        };

        XMLHttpRequest.prototype.setRequestHeader = function(key, value) {
            if (this.__skillHeaders) this.__skillHeaders[key] = value;
            return __origXHRSetHeader.apply(this, arguments);
        };

        XMLHttpRequest.prototype.send = function(body) {
            var self = this;
            var seq = self.__skillSeq || 0;
            var ts = Date.now();

            try {
                window.ipc.postMessage('__net_req:' + JSON.stringify({
                    seq: seq, method: self.__skillMethod || 'GET',
                    url: self.__skillUrl || '',
                    headers: JSON.stringify(self.__skillHeaders || {}),
                    body: body ? String(body) : '',
                    timestamp_ms: ts
                }));
            } catch(e) {}

            self.addEventListener('load', function() {
                try {
                    var respHeaders = '{}';
                    try { respHeaders = self.getAllResponseHeaders(); } catch(e) {}
                    window.ipc.postMessage('__net_res:' + JSON.stringify({
                        seq: seq, status: self.status,
                        status_text: self.statusText || '',
                        headers: respHeaders,
                        body: self.responseText || '',
                        body_base64: false,
                        url: self.responseURL || self.__skillUrl || '',
                        timestamp_ms: Date.now()
                    }));
                } catch(e) {}
            });

            return __origXHRSend.apply(this, arguments);
        };
    })();
    "#.to_string()
}
