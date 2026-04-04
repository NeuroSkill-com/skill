// SPDX-License-Identifier: GPL-3.0-only
// Shared Playwright mock for Tauri invoke + daemon HTTP fetch.
//
// Usage in spec files:
//   import { buildDaemonMockScript } from "./helpers/daemon-mock";
//   await page.addInitScript({ content: buildDaemonMockScript(commandMap) });

export interface CommandMap {
  [cmd: string]: unknown;
}

/**
 * Build a script that mocks both:
 * 1. window.__TAURI_INTERNALS__.invoke — for native Tauri commands
 * 2. window.fetch — intercepts daemon HTTP calls (/v1/*) and routes
 *    them through the same command map
 *
 * The command map keys are Tauri command names (snake_case).
 * For daemon HTTP calls, the URL path is reverse-mapped to the command name.
 */
export function buildDaemonMockScript(commands: CommandMap): string {
  const cmdJson = JSON.stringify(commands);

  return `
    // ── Command data ──
    const __CMD_DATA__ = ${cmdJson};

    // ── Reverse map: daemon URL path → command name ──
    const __PATH_TO_CMD__ = {
      "/v1/status": "get_status",
      "/v1/settings/gpu-stats": "get_gpu_stats",
      "/v1/settings/llm-config": "get_llm_config",
      "/v1/settings/ws-config": "get_ws_config",
      "/v1/ui/main-window-auto-fit": "get_main_window_auto_fit",
      "/v1/ui/daily-goal": "get_daily_goal",
      "/v1/ui/goal-notified-date": "get_goal_notified_date",
      "/v1/ws-port": "get_ws_port",
      "/v1/ws-clients": "get_ws_clients",
      "/v1/ws-request-log": "get_ws_request_log",
      "/v1/llm/server/status": "get_llm_server_status",
      "/v1/llm/server/logs": "get_llm_logs",
      "/v1/llm/catalog": "get_llm_catalog",
      "/v1/llm/downloads": "get_llm_downloads",
      "/v1/llm/chat/last-session": "get_last_chat_session",
      "/v1/llm/chat/load-session": "load_chat_session",
      "/v1/llm/chat/new-session": "new_chat_session",
      "/v1/llm/chat/sessions": "list_chat_sessions",
      "/v1/llm/chat/rename": "rename_chat_session",
      "/v1/llm/chat/save-message": "save_chat_message",
      "/v1/llm/chat/save-tool-calls": "save_chat_tool_calls",
      "/v1/llm/chat/session-params": "get_session_params",
      "/v1/llm/chat/set-session-params": "set_session_params",
      "/v1/models/status": "get_eeg_model_status",
      "/v1/models/config": "get_eeg_model_config",
      "/v1/models/estimate-reembed": "estimate_reembed",
      "/v1/models/exg-catalog": "get_exg_catalog",
      "/v1/history/sessions": "list_sessions",
      "/v1/history/stats": "get_history_stats",
      "/v1/history/find-session": "find_session_for_timestamp",
      "/v1/history/daily-recording-mins": "get_daily_recording_mins",
      "/v1/analysis/metrics": "get_session_metrics",
      "/v1/analysis/timeseries": "get_session_timeseries",
      "/v1/analysis/sleep": "get_sleep_stages",
      "/v1/analysis/location": "get_session_location",
      "/v1/analysis/embedding-count": "get_session_embedding_count",
      "/v1/analysis/umap": "compute_umap_compare",
      "/v1/labels": "get_recent_labels",
      "/v1/search/eeg": "search_labels_by_text",
      "/v1/hooks": "get_hooks",
      "/v1/hooks/statuses": "get_hook_statuses",
      "/v1/hooks/log": "get_hook_log",
      "/v1/hooks/log-count": "get_hook_log_count",
      "/v1/settings/dnd/config": "get_dnd_config",
      "/v1/settings/dnd/active": "get_dnd_active",
      "/v1/settings/dnd/status": "get_dnd_status",
      "/v1/settings/dnd/focus-modes": "list_focus_modes",
      "/v1/settings/sleep-config": "get_sleep_config",
      "/v1/settings/neutts-config": "get_neutts_config",
      "/v1/settings/tts-preload": "get_tts_preload",
      "/v1/settings/screenshot/config": "get_screenshot_config",
      "/v1/settings/screenshot/metrics": "get_screenshot_metrics",
      "/v1/settings/screenshot/around": "get_screenshots_around",
      "/v1/settings/screenshot/search-text": "search_screenshots_by_text",
      "/v1/settings/screenshot/dir": "get_screenshots_dir",
      "/v1/settings/screenshot/ocr-ready": "check_ocr_models_ready",
      "/v1/activity/latest-bands": "get_latest_bands",
      "/v1/control/retry-connect": "retry_connect",
      "/v1/control/cancel-retry": "cancel_retry",
      "/v1/devices/forget": "forget_device",
      "/v1/devices/set-preferred": "set_preferred_device",
    };

    // ── Tauri invoke mock ──
    window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
    window.__TAURI_INTERNALS__.metadata = {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
      windows: [{ label: "main" }],
      webviews: [{ label: "main", windowLabel: "main" }],
    };

    window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
      if (cmd === "get_daemon_bootstrap") {
        return Promise.resolve({
          port: 18444,
          token: "test-token",
          compatible_protocol: true,
          daemon_version: "0.0.1",
          protocol_version: 1,
        });
      }
      if (cmd === "plugin:event|listen") return Promise.resolve(0);
      if (cmd === "plugin:event|unlisten") return Promise.resolve();
      if (cmd in __CMD_DATA__) return Promise.resolve(__CMD_DATA__[cmd]);
      return Promise.resolve(null);
    };

    // ── Fetch mock (intercepts daemon HTTP) ──
    const __origFetch__ = window.fetch;
    window.fetch = function(url, opts) {
      const urlStr = typeof url === "string" ? url : url.toString();
      if (urlStr.includes("127.0.0.1") && urlStr.includes("/v1/")) {
        const path = "/" + urlStr.split("/v1/").pop();
        const fullPath = "/v1/" + urlStr.split("/v1/").pop();
        const cmd = __PATH_TO_CMD__[fullPath];
        const data = cmd && cmd in __CMD_DATA__ ? __CMD_DATA__[cmd] : null;
        return Promise.resolve(new Response(JSON.stringify(data), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }));
      }
      return __origFetch__.call(window, url, opts);
    };
  `;
}
