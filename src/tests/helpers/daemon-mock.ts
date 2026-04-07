// SPDX-License-Identifier: GPL-3.0-only
// Shared Playwright mock for Tauri invoke + daemon HTTP fetch + daemon WebSocket
// + Tauri cross-window event system.
//
// Usage in spec files:
//   import { buildDaemonMockScript } from "./helpers/daemon-mock";
//   await page.addInitScript({ content: buildDaemonMockScript(commandMap) });
//
// Test helpers exposed on window after page.goto():
//
//   window.__skillEmitEvent__(type, payload)
//     → injects a daemon WebSocket event (same path as real daemon WS frames)
//
//   window.__skillFireTauriEvent__(event, payload)
//     → fires a Tauri app-level event (same path as cross-window emit/listen)
//       Use this to test dashboard behaviour driven by virtual-device-status,
//       virtual-eeg-sample, virtual-eeg-bands, etc.

export interface CommandMap {
  [cmd: string]: unknown;
}

export function buildDaemonMockScript(commands: CommandMap): string {
  const cmdJson = JSON.stringify(commands);

  return `
    // ── Command data ──────────────────────────────────────────────────────
    const __CMD_DATA__ = ${cmdJson};

    // ── Reverse map: daemon URL path → command name ───────────────────────
    const __PATH_TO_CMD__ = {
      "/v1/status":                          "get_status",
      "/v1/settings/gpu-stats":              "get_gpu_stats",
      "/v1/settings/llm-config":             "get_llm_config",
      "/v1/settings/ws-config":              "get_ws_config",
      "/v1/ui/main-window-auto-fit":         "get_main_window_auto_fit",
      "/v1/ui/daily-goal":                   "get_daily_goal",
      "/v1/ui/goal-notified-date":           "get_goal_notified_date",
      "/v1/ws-port":                         "get_ws_port",
      "/v1/ws-clients":                      "get_ws_clients",
      "/v1/ws-request-log":                  "get_ws_request_log",
      "/v1/llm/server/status":               "get_llm_server_status",
      "/v1/llm/server/logs":                 "get_llm_logs",
      "/v1/llm/catalog":                     "get_llm_catalog",
      "/v1/llm/downloads":                   "get_llm_downloads",
      "/v1/llm/chat/last-session":           "get_last_chat_session",
      "/v1/llm/chat/load-session":           "load_chat_session",
      "/v1/llm/chat/new-session":            "new_chat_session",
      "/v1/llm/chat/sessions":               "list_chat_sessions",
      "/v1/llm/chat/rename":                 "rename_chat_session",
      "/v1/llm/chat/save-message":           "save_chat_message",
      "/v1/llm/chat/save-tool-calls":        "save_chat_tool_calls",
      "/v1/llm/chat/session-params":         "get_session_params",
      "/v1/llm/chat/set-session-params":     "set_session_params",
      "/v1/models/status":                   "get_eeg_model_status",
      "/v1/models/config":                   "get_eeg_model_config",
      "/v1/models/estimate-reembed":         "estimate_reembed",
      "/v1/models/exg-catalog":              "get_exg_catalog",
      "/v1/history/sessions":                "list_sessions",
      "/v1/history/stats":                   "get_history_stats",
      "/v1/history/find-session":            "find_session_for_timestamp",
      "/v1/history/daily-recording-mins":    "get_daily_recording_mins",
      "/v1/analysis/metrics":                "get_session_metrics",
      "/v1/analysis/timeseries":             "get_session_timeseries",
      "/v1/analysis/sleep":                  "get_sleep_stages",
      "/v1/analysis/location":               "get_session_location",
      "/v1/analysis/embedding-count":        "get_session_embedding_count",
      "/v1/analysis/umap":                   "compute_umap_compare",
      "/v1/labels":                          "get_recent_labels",
      "/v1/search/eeg":                      "search_labels_by_text",
      "/v1/hooks":                           "get_hooks",
      "/v1/hooks/statuses":                  "get_hook_statuses",
      "/v1/hooks/log":                       "get_hook_log",
      "/v1/hooks/log-count":                 "get_hook_log_count",
      "/v1/settings/dnd/config":             "get_dnd_config",
      "/v1/settings/dnd/active":             "get_dnd_active",
      "/v1/settings/dnd/status":             "get_dnd_status",
      "/v1/settings/dnd/focus-modes":        "list_focus_modes",
      "/v1/settings/sleep-config":           "get_sleep_config",
      "/v1/settings/neutts-config":          "get_neutts_config",
      "/v1/settings/tts-preload":            "get_tts_preload",
      "/v1/settings/screenshot/config":      "get_screenshot_config",
      "/v1/settings/screenshot/metrics":     "get_screenshot_metrics",
      "/v1/settings/screenshot/around":      "get_screenshots_around",
      "/v1/settings/screenshot/search-text": "search_screenshots_by_text",
      "/v1/settings/screenshot/dir":         "get_screenshots_dir",
      "/v1/settings/screenshot/ocr-ready":   "check_ocr_models_ready",
      "/v1/activity/latest-bands":           "get_latest_bands",
      "/v1/control/retry-connect":           "retry_connect",
      "/v1/control/cancel-retry":            "cancel_retry",
      "/v1/control/start-session":           "start_session",
      "/v1/control/cancel-session":          "cancel_session",
      "/v1/devices/forget":                  "forget_device",
      "/v1/devices/set-preferred":           "set_preferred_device",
      "/v1/settings/openbci-config":         "get_openbci_config",
      "/v1/settings/scanner-config":         "get_scanner_config",
      "/v1/settings/device-api-config":      "get_device_api_config",
      "/v1/settings/device-log":             "get_device_log",
      "/v1/device/serial-ports":             "list_serial_ports",
      "/v1/devices":                         "get_devices",
      "/v1/lsl/virtual-source/running":      "lsl_virtual_source_running",
      "/v1/lsl/virtual-source/start":        "lsl_virtual_source_start",
      "/v1/lsl/virtual-source/stop":         "lsl_virtual_source_stop",
    };

    // ── Tauri event system mock ───────────────────────────────────────────
    //
    // Implements transformCallback / runCallback so that listen() and emit()
    // from @tauri-apps/api/event work correctly within a single page context.
    //
    // This mirrors what the real Tauri runtime provides.  Tests can call
    //   window.__skillFireTauriEvent__(event, payload)
    // to simulate a Tauri event arriving from another window (e.g. the
    // virtual-devices window firing "virtual-device-status").

    (function() {
      window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};

      // Callback registry: id → fn
      const _cbs = new Map();
      let _cbId = 1;

      window.__TAURI_INTERNALS__.transformCallback = function(fn, once) {
        const id = _cbId++;
        _cbs.set(id, function(data) {
          if (once) _cbs.delete(id);
          if (fn) fn(data);
        });
        return id;
      };

      window.__TAURI_INTERNALS__.runCallback = function(id, data) {
        const fn = _cbs.get(id);
        if (fn) fn(data);
      };

      window.__TAURI_INTERNALS__.unregisterCallback = function(id) {
        _cbs.delete(id);
      };

      window.__TAURI_INTERNALS__.callbacks = _cbs;

      // Event listener registry: eventName → [callbackId, ...]
      const _listeners = {};

      function tauriEmit(eventName, payload) {
        const ids = _listeners[eventName] || [];
        for (var i = 0; i < ids.length; i++) {
          window.__TAURI_INTERNALS__.runCallback(ids[i], {
            event: eventName,
            id: ids[i],
            payload: payload,
          });
        }
      }

      window.__TAURI_INTERNALS__.metadata = {
        currentWindow:  { label: "main" },
        currentWebview: { label: "main", windowLabel: "main" },
        windows:   [{ label: "main" }],
        webviews:  [{ label: "main", windowLabel: "main" }],
      };

      // ── Tauri invoke mock ───────────────────────────────────────────────
      window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
        // ── Event plugin ──────────────────────────────────────────────────
        if (cmd === "plugin:event|listen") {
          var ev = (args || {}).event;
          var hId = (args || {}).handler;
          if (ev) {
            if (!_listeners[ev]) _listeners[ev] = [];
            _listeners[ev].push(hId);
          }
          // Return the handler ID as the event listener ID so unlisten works.
          return Promise.resolve(hId);
        }
        if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") {
          tauriEmit((args || {}).event, (args || {}).payload);
          return Promise.resolve(null);
        }
        if (cmd === "plugin:event|unlisten") {
          var ev2 = (args || {}).event;
          var evId = (args || {}).eventId;
          if (ev2 && _listeners[ev2]) {
            _listeners[ev2] = _listeners[ev2].filter(function(h) { return h !== evId; });
          }
          return Promise.resolve(null);
        }

        // ── Bootstrap ─────────────────────────────────────────────────────
        if (cmd === "get_daemon_bootstrap") {
          return Promise.resolve({
            port: 18444, token: "test-token",
            compatible_protocol: true, daemon_version: "0.0.1", protocol_version: 1,
          });
        }

        // ── Command data ──────────────────────────────────────────────────
        if (cmd in __CMD_DATA__) return Promise.resolve(__CMD_DATA__[cmd]);
        return Promise.resolve(null);
      };

      // ── Test helpers ────────────────────────────────────────────────────

      // Fire a Tauri app-level event — simulates emit() from another window.
      // Use this to test cross-window communication (e.g. virtual-device-status).
      window.__skillFireTauriEvent__ = function(eventName, payload) {
        tauriEmit(eventName, payload);
      };
    })();

    // ── WebSocket mock ────────────────────────────────────────────────────
    //
    // Intercepts the daemon event-stream WebSocket (/v1/events).
    // window.__skillEmitEvent__(type, payload) pushes a daemon WS event into
    // the page's ws.ts handler pipeline — the same path as real daemon events.

    (function() {
      const _OrigWS = window.WebSocket;

      function MockWs(url) {
        this.url        = url;
        this.readyState = 0;
        this.onopen     = null;
        this.onmessage  = null;
        this.onerror    = null;
        this.onclose    = null;
        this._listeners = {};
        window.__SKILL_MOCK_WS__ = this;
        var self = this;
        setTimeout(function() {
          self.readyState = 1;
          var ev = { type: 'open' };
          if (self.onopen) self.onopen(ev);
          (self._listeners['open'] || []).forEach(function(h) { h(ev); });
        }, 20);
      }
      MockWs.prototype.send  = function() {};
      MockWs.prototype.close = function() { this.readyState = 3; };
      MockWs.prototype.addEventListener = function(type, handler) {
        if (!this._listeners[type]) this._listeners[type] = [];
        this._listeners[type].push(handler);
      };
      MockWs.prototype.removeEventListener = function(type, handler) {
        if (!this._listeners[type]) return;
        this._listeners[type] = this._listeners[type].filter(function(h) { return h !== handler; });
      };
      MockWs.prototype.__pushEvent = function(type, payload) {
        var data = JSON.stringify({ type: type, ts_unix_ms: Date.now(), payload: payload });
        var ev   = new MessageEvent('message', { data: data });
        if (this.onmessage) this.onmessage(ev);
        (this._listeners['message'] || []).forEach(function(h) { h(ev); });
      };

      window.WebSocket = function MockWebSocketCtor(url, protocols) {
        if (typeof url === 'string' && url.indexOf('/v1/events') !== -1) {
          return new MockWs(url);
        }
        return new _OrigWS(url, protocols);
      };
      window.WebSocket.CONNECTING = 0;
      window.WebSocket.OPEN       = 1;
      window.WebSocket.CLOSING    = 2;
      window.WebSocket.CLOSED     = 3;

      // Push a daemon WebSocket event — same path as real daemon WS frames.
      window.__skillEmitEvent__ = function(type, payload) {
        if (window.__SKILL_MOCK_WS__) {
          window.__SKILL_MOCK_WS__.__pushEvent(type, payload);
        }
      };
    })();

    // ── Fetch mock (daemon HTTP) ──────────────────────────────────────────
    const __origFetch__ = window.fetch;
    window.fetch = function(url, opts) {
      const urlStr = typeof url === "string" ? url : url.toString();
      if (urlStr.includes("127.0.0.1") && urlStr.includes("/v1/")) {
        const fullPath = "/v1/" + urlStr.split("/v1/").pop().split("?")[0];
        const cmd  = __PATH_TO_CMD__[fullPath];
        const data = (cmd && cmd in __CMD_DATA__) ? __CMD_DATA__[cmd] : null;
        return Promise.resolve(new Response(JSON.stringify(data), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }));
      }
      return __origFetch__.call(window, url, opts);
    };
  `;
}
