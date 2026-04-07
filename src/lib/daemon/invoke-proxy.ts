// SPDX-License-Identifier: GPL-3.0-only
//
// Transitional drop-in replacement for Tauri invoke() that routes
// daemon-owned commands to HTTP endpoints.  Same call signature as invoke().
//
// End-state: replace daemonInvoke("cmd", args) with typed client calls
// at each call site, then delete this file.

import { daemonGet, daemonPost } from "./http";

// biome-ignore lint/suspicious/noExplicitAny: generic proxy
type AnyArgs = Record<string, any>;

// ── Route table ────────────────────────────────────────────────────────────
// Maps Tauri command names → daemon HTTP endpoints.
// Aliases (multiple commands → same endpoint) are collapsed with comments.

const G = "GET" as const;
const P = "POST" as const;

const ROUTES: Record<string, [typeof G | typeof P, string]> = {
  // Control
  retry_connect: [P, "/v1/control/retry-connect"],
  cancel_retry: [P, "/v1/control/cancel-retry"],
  start_session: [P, "/v1/control/start-session"],
  switch_session: [P, "/v1/control/switch-session"],
  cancel_session: [P, "/v1/control/cancel-session"],
  lsl_cancel_secondary: [P, "/v1/control/cancel-session"],

  // Devices
  forget_device: [P, "/v1/devices/forget"],
  set_preferred_device: [P, "/v1/devices/set-preferred"],

  // Status (aliases: get_cortex_ws_state, list_secondary_sessions)
  get_status: [G, "/v1/status"],
  get_cortex_ws_state: [G, "/v1/status"],
  list_secondary_sessions: [G, "/v1/status"],

  // LLM server
  get_llm_server_status: [G, "/v1/llm/server/status"],
  get_model_hardware_fit: [G, "/v1/llm/server/status"], // alias
  start_llm_server: [P, "/v1/llm/server/start"],
  stop_llm_server: [P, "/v1/llm/server/stop"],
  get_llm_logs: [G, "/v1/llm/server/logs"],
  abort_llm_stream: [P, "/v1/llm/abort-stream"],
  switch_llm_model: [P, "/v1/llm/server/switch-model"],
  switch_llm_mmproj: [P, "/v1/llm/server/switch-mmproj"],

  // LLM config
  get_llm_config: [G, "/v1/settings/llm-config"],
  set_llm_config: [P, "/v1/settings/llm-config"],

  // LLM catalog
  get_llm_catalog: [G, "/v1/llm/catalog"],
  refresh_llm_catalog: [P, "/v1/llm/catalog/refresh"],

  // LLM downloads
  get_llm_downloads: [G, "/v1/llm/downloads"],
  download_llm_model: [P, "/v1/llm/download/start"],
  cancel_llm_download: [P, "/v1/llm/download/cancel"],
  pause_llm_download: [P, "/v1/llm/download/pause"],
  resume_llm_download: [P, "/v1/llm/download/resume"],
  delete_llm_model: [P, "/v1/llm/download/delete"],

  // Chat persistence
  get_last_chat_session: [P, "/v1/llm/chat/last-session"],
  load_chat_session: [P, "/v1/llm/chat/load-session"],
  new_chat_session: [P, "/v1/llm/chat/new-session"],
  rename_chat_session: [P, "/v1/llm/chat/rename"],
  save_chat_message: [P, "/v1/llm/chat/save-message"],
  save_chat_tool_calls: [P, "/v1/llm/chat/save-tool-calls"],
  cancel_tool_call: [P, "/v1/llm/cancel-tool-call"],
  get_session_params: [P, "/v1/llm/chat/session-params"],
  set_session_params: [P, "/v1/llm/chat/set-session-params"],

  // History (aliases: list_all_sessions, list_embedding_sessions)
  list_sessions: [G, "/v1/history/sessions"],
  list_all_sessions: [G, "/v1/history/sessions"],
  list_embedding_sessions: [G, "/v1/history/sessions"],
  list_sessions_for_local_day: [P, "/v1/history/sessions"],
  list_local_session_days: [P, "/v1/history/sessions"],
  delete_session: [P, "/v1/history/sessions/delete"],
  get_history_stats: [G, "/v1/history/stats"],
  get_daily_recording_mins: [P, "/v1/history/daily-recording-mins"],
  find_session_for_timestamp: [P, "/v1/history/find-session"],

  // Analysis
  get_session_metrics: [P, "/v1/analysis/metrics"],
  get_session_timeseries: [P, "/v1/analysis/timeseries"],
  get_sleep_stages: [P, "/v1/analysis/sleep"],
  get_csv_metrics: [P, "/v1/analysis/csv-metrics"],
  get_session_location: [P, "/v1/analysis/location"],
  get_session_embedding_count: [P, "/v1/analysis/embedding-count"],
  compute_umap_compare: [P, "/v1/analysis/umap"],

  // Labels
  submit_label: [P, "/v1/labels"],
  get_recent_labels: [G, "/v1/labels"],
  query_annotations: [P, "/v1/labels"],
  update_label: [P, "/v1/labels"],
  delete_label: [P, "/v1/labels"],

  // Search
  search_labels_by_text: [P, "/v1/search/eeg"],
  interactive_search: [P, "/v1/search/eeg"],
  regenerate_interactive_svg: [P, "/v1/search/eeg"],
  regenerate_interactive_dot: [P, "/v1/search/eeg"],
  save_dot_file: [P, "/v1/search/eeg"],
  save_svg_file: [P, "/v1/search/eeg"],

  // Models / EEG (aliases: get_embedding_model, list_embedding_models, etc.)
  get_eeg_model_config: [G, "/v1/models/config"],
  get_embedding_model: [G, "/v1/models/config"], // alias
  set_eeg_model_config: [P, "/v1/models/config"],
  set_embedding_model: [P, "/v1/models/config"], // alias
  get_eeg_model_status: [G, "/v1/models/status"],
  get_exg_catalog: [G, "/v1/models/exg-catalog"],
  list_embedding_models: [G, "/v1/models/exg-catalog"], // alias
  trigger_reembed: [P, "/v1/models/trigger-reembed"],
  reembed_all_labels: [P, "/v1/models/trigger-reembed"], // alias
  trigger_weights_download: [P, "/v1/models/trigger-weights-download"],
  cancel_weights_download: [P, "/v1/models/cancel-weights-download"],
  estimate_reembed: [G, "/v1/models/estimate-reembed"],
  get_stale_label_count: [G, "/v1/models/estimate-reembed"], // alias
  estimate_screenshot_reembed: [G, "/v1/models/estimate-reembed"], // alias
  rebuild_screenshot_embeddings: [P, "/v1/models/rebuild-index"],

  // Filter / overlap / inference
  get_filter_config: [G, "/v1/settings/filter-config"],
  set_filter_config: [P, "/v1/settings/filter-config"],
  get_embedding_overlap: [G, "/v1/settings/embedding-overlap"],
  set_embedding_overlap: [P, "/v1/settings/embedding-overlap"],
  get_exg_inference_device: [G, "/v1/settings/exg-inference-device"],
  set_exg_inference_device: [P, "/v1/settings/exg-inference-device"],

  // Screenshots
  get_screenshot_config: [G, "/v1/settings/screenshot/config"],
  set_screenshot_config: [P, "/v1/settings/screenshot/config"],
  get_screenshot_metrics: [G, "/v1/settings/screenshot/metrics"],
  get_screenshots_around: [P, "/v1/settings/screenshot/around"],
  search_screenshots_by_text: [P, "/v1/settings/screenshot/search-text"],
  check_ocr_models_ready: [G, "/v1/settings/screenshot/ocr-ready"],
  download_ocr_models: [P, "/v1/settings/screenshot/download-ocr"],
  get_screenshots_dir: [G, "/v1/settings/screenshot/dir"],

  // Hooks
  get_hooks: [G, "/v1/hooks"],
  set_hooks: [P, "/v1/hooks"],
  get_hook_statuses: [G, "/v1/hooks/statuses"],
  get_hook_log: [P, "/v1/hooks/log"],
  get_hook_log_count: [G, "/v1/hooks/log-count"],
  suggest_hook_keywords: [P, "/v1/hooks/suggest-keywords"],
  suggest_hook_distances: [P, "/v1/hooks/suggest-distances"],

  // LSL
  lsl_discover: [G, "/v1/lsl/discover"],
  lsl_get_config: [G, "/v1/lsl/config"],
  lsl_set_auto_connect: [P, "/v1/lsl/auto-connect"],
  lsl_pair_stream: [P, "/v1/lsl/pair"],
  lsl_unpair_stream: [P, "/v1/lsl/unpair"],
  lsl_get_idle_timeout: [G, "/v1/lsl/idle-timeout"],
  lsl_set_idle_timeout: [P, "/v1/lsl/idle-timeout"],
  lsl_virtual_source_running: [G, "/v1/lsl/virtual-source/running"],
  lsl_virtual_source_start: [P, "/v1/lsl/virtual-source/start"],
  lsl_virtual_source_stop: [P, "/v1/lsl/virtual-source/stop"],
  lsl_iroh_start: [P, "/v1/lsl/iroh/start"],
  lsl_iroh_stop: [P, "/v1/lsl/iroh/stop"],
  lsl_iroh_status: [G, "/v1/lsl/iroh/status"],

  // DnD / Goals
  get_dnd_config: [G, "/v1/settings/dnd/config"],
  set_dnd_config: [P, "/v1/settings/dnd/config"],
  get_dnd_active: [G, "/v1/settings/dnd/active"],
  get_dnd_status: [G, "/v1/settings/dnd/status"],
  test_dnd: [P, "/v1/settings/dnd/test"],
  list_focus_modes: [G, "/v1/settings/dnd/focus-modes"],
  get_daily_goal: [G, "/v1/ui/daily-goal"],
  set_daily_goal: [P, "/v1/ui/daily-goal"],
  get_goal_notified_date: [G, "/v1/ui/goal-notified-date"],
  set_goal_notified_date: [P, "/v1/ui/goal-notified-date"],

  // Sleep / TTS / UMAP
  get_sleep_config: [G, "/v1/settings/sleep-config"],
  set_sleep_config: [P, "/v1/settings/sleep-config"],
  get_neutts_config: [G, "/v1/settings/neutts-config"],
  set_neutts_config: [P, "/v1/settings/neutts-config"],
  get_tts_preload: [G, "/v1/settings/tts-preload"],
  set_tts_preload: [P, "/v1/settings/tts-preload"],
  get_umap_config: [G, "/v1/settings/umap-config"],
  set_umap_config: [P, "/v1/settings/umap-config"],

  // Auth tokens
  list_auth_tokens: [G, "/v1/auth/tokens"],
  create_auth_token: [P, "/v1/auth/tokens"],
  revoke_auth_token: [P, "/v1/auth/tokens/revoke"],
  delete_auth_token: [P, "/v1/auth/tokens/delete"],
  refresh_default_token: [P, "/v1/auth/default-token/refresh"],

  // Misc
  get_gpu_stats: [G, "/v1/settings/gpu-stats"],
  get_main_window_auto_fit: [G, "/v1/ui/main-window-auto-fit"],
  get_ws_config: [G, "/v1/settings/ws-config"],
  get_ws_port: [G, "/v1/ws-port"],
  get_ws_clients: [G, "/v1/ws-clients"],
  get_ws_request_log: [G, "/v1/ws-request-log"],
};

// ── Channel commands ───────────────────────────────────────────────────────
// Commands that originally used Tauri IPC Channels for streaming.
// We call the daemon HTTP endpoint and synthesize onmessage events.

const CHANNEL_ROUTES: Record<string, string> = {
  chat_completions_ipc: "/v1/llm/chat-completions",
  stream_search_embeddings: "/v1/search/eeg",
};

function handleChannelCommand(cmd: string, args: AnyArgs): Promise<void> {
  const path = CHANNEL_ROUTES[cmd];
  const { channel, onProgress, ...rest } = args;
  const ch = channel ?? onProgress;
  const emit = (msg: AnyArgs) => {
    if (ch && typeof ch.onmessage === "function") ch.onmessage(msg);
  };
  return daemonPost<AnyArgs>(path, rest)
    .then((result) => {
      if (cmd === "chat_completions_ipc") {
        const content = result?.content ?? "";
        if (content) emit({ type: "delta", content });
        emit({
          type: "done",
          finish_reason: result?.finish_reason ?? "stop",
          prompt_tokens: result?.prompt_tokens ?? 0,
          completion_tokens: result?.completion_tokens ?? 0,
          n_ctx: result?.n_ctx ?? 0,
        });
      } else {
        emit({ kind: "started", query_count: 0, searched_days: [] });
        if (Array.isArray(result?.results)) {
          for (const entry of result.results) {
            emit({ kind: "result", entry, done_count: 0 });
          }
        }
        emit({ kind: "done", total: result?.results?.length ?? 0 });
      }
    })
    .catch((e: unknown) => {
      emit(
        cmd === "chat_completions_ipc" ? { type: "error", message: String(e) } : { kind: "error", error: String(e) },
      );
    });
}

// ── Job queue ──────────────────────────────────────────────────────────────
// enqueue_umap_compare + poll_job: the daemon runs UMAP synchronously,
// so we fire-and-forget into an in-memory result map.

let _nextJobId = 0;
const _jobResults = new Map<number, AnyArgs>();

function handleEnqueue(args: AnyArgs): AnyArgs {
  const jobId = ++_nextJobId;
  const t0 = Date.now();
  daemonPost("/v1/analysis/umap", args).then(
    (result) => _jobResults.set(jobId, { status: "complete", job_id: jobId, result, elapsed_ms: Date.now() - t0 }),
    (err) =>
      _jobResults.set(jobId, { status: "error", job_id: jobId, error: String(err), elapsed_ms: Date.now() - t0 }),
  );
  return { job_id: jobId, estimated_ready_utc: Date.now() + 15000, queue_position: 0, estimated_secs: 15 };
}

function handlePoll(args: AnyArgs): AnyArgs {
  const jobId = args?.jobId ?? args?.job_id ?? 0;
  const cached = _jobResults.get(jobId);
  if (cached) {
    _jobResults.delete(jobId);
    return cached;
  }
  return { status: "running", job_id: jobId, queue_position: 0, estimated_secs: 5 };
}

// ── Main entry point ───────────────────────────────────────────────────────

export async function daemonInvoke<T>(cmd: string, args?: AnyArgs): Promise<T> {
  if (cmd in CHANNEL_ROUTES) {
    await handleChannelCommand(cmd, args ?? {});
    return undefined as T;
  }
  if (cmd === "enqueue_umap_compare") return handleEnqueue(args ?? {}) as T;
  if (cmd === "poll_job") return handlePoll(args ?? {}) as T;

  const route = ROUTES[cmd];
  if (route) {
    try {
      return route[0] === "GET" ? await daemonGet<T>(route[1]) : await daemonPost<T>(route[1], args ?? {});
    } catch (_daemonErr) {
      // Daemon HTTP failed — try Tauri invoke as fallback.
      // If Tauri invoke also fails, throw the Tauri error (more specific).
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<T>(cmd, args);
    }
  }

  // Unknown command — always use Tauri invoke
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}
