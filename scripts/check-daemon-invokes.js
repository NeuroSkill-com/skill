#!/usr/bin/env node

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const ROOT = path.resolve("src/lib");
const EXTRA_FILES = [
  path.resolve("src/routes/+page.svelte"),
  path.resolve("src/routes/onboarding/+page.svelte"),
  path.resolve("src/routes/api/+page.svelte"),
  path.resolve("src/routes/chat/+page.svelte"),
  path.resolve("src/routes/calibration/+page.svelte"),
];

const DAEMON_OWNED_COMMANDS = new Set([
  // skills
  "get_skills_refresh_interval",
  "set_skills_refresh_interval",
  "get_skills_sync_on_launch",
  "set_skills_sync_on_launch",
  "get_skills_last_sync",
  "sync_skills_now",
  "list_skills",
  "get_skills_license",
  "set_disabled_skills",

  // lsl settings + session
  "lsl_set_idle_timeout",
  "lsl_set_auto_connect",
  "lsl_discover",
  "lsl_connect",
  "lsl_switch_session",
  "lsl_start_secondary",
  "lsl_cancel_secondary",
  "lsl_pair_stream",
  "lsl_unpair_stream",
  "lsl_iroh_start",
  "lsl_iroh_stop",
  "lsl_iroh_status",
  "lsl_virtual_source_running",
  "lsl_virtual_source_start",
  "lsl_virtual_source_start_configured",
  "lsl_virtual_source_stop",
  "lsl_start_virtual_source",
  "lsl_stop_virtual_source",
  "lsl_get_config",
  "lsl_get_idle_timeout",
  "list_secondary_sessions",

  // session control
  "start_session",
  "switch_session",
  "cancel_session",

  // devices
  "get_devices",
  "get_openbci_config",
  "set_openbci_config",
  "get_device_api_config",
  "set_device_api_config",
  "get_scanner_config",
  "set_scanner_config",
  "get_device_log",
  "list_serial_ports",
  "forget_device",
  "set_preferred_device",
  "pair_device",
  "get_cortex_ws_state",
  "get_status",
  "retry_connect",
  "cancel_retry",
  "get_ws_port",

  // settings
  "get_gpu_stats",
  "get_storage_format",
  "set_storage_format",
  "get_ws_config",
  "set_ws_config",
  "get_api_token",
  "set_api_token",
  "get_hf_endpoint",
  "set_hf_endpoint",
  "get_active_window_tracking",
  "set_active_window_tracking",
  "get_active_window",
  "get_input_activity_tracking",
  "set_input_activity_tracking",
  "get_last_input_activity",
  "get_main_window_auto_fit",
  "set_main_window_auto_fit",
  "get_location_enabled",
  "set_location_enabled",
  "get_inference_device",
  "set_inference_device",

  // llm config + web cache
  "get_llm_config",
  "set_llm_config",
  "web_cache_stats",
  "web_cache_list",
  "web_cache_clear",
  "web_cache_remove_domain",
  "web_cache_remove_entry",

  // lsl
  "lsl_set_idle_timeout",
  "lsl_set_auto_connect",
  "lsl_discover",
  "lsl_connect",
  "lsl_switch_session",
  "lsl_start_secondary",
  "lsl_cancel_secondary",
  "lsl_pair_stream",
  "lsl_unpair_stream",
  "lsl_iroh_start",
  "lsl_iroh_status",
  "lsl_iroh_stop",
  "lsl_virtual_source_running",
  "lsl_virtual_source_start",
  "lsl_virtual_source_start_configured",
  "lsl_virtual_source_stop",
  "lsl_start_virtual_source",
  "lsl_stop_virtual_source",
  "lsl_get_config",
  "lsl_get_idle_timeout",
  "list_secondary_sessions",

  // session control
  "start_session",
  "switch_session",
  "cancel_session",

  // chat persistence
  "get_last_chat_session",

  // previously typed-client migrated, now also in daemonInvoke proxy
  "cancel_retry",
  "cancel_tool_call",
  "forget_device",
  "get_cortex_ws_state",
  "get_gpu_stats",
  "get_last_chat_session",
  "get_llm_config",
  "get_main_window_auto_fit",
  "get_session_params",
  "get_status",
  "get_ws_config",
  "get_ws_port",
  "list_secondary_sessions",
  "load_chat_session",
  "lsl_cancel_secondary",
  "new_chat_session",
  "rename_chat_session",
  "retry_connect",
  "save_chat_message",
  "save_chat_tool_calls",
  "set_llm_config",
  "set_preferred_device",
  "set_session_params",

  // EEG streaming (moved to daemon WS)
  "subscribe_eeg",
  "subscribe_ppg",
  "subscribe_imu",
  "get_latest_bands",

  // search / compare jobs + IPC streaming
  "chat_completions_ipc",
  "stream_search_embeddings",
  "enqueue_umap_compare",
  "poll_job",
  "interactive_search",
  "regenerate_interactive_svg",
  "regenerate_interactive_dot",
  "save_dot_file",
  "save_svg_file",

  // auth tokens
  "list_auth_tokens",
  "create_auth_token",
  "revoke_auth_token",
  "delete_auth_token",
  "refresh_default_token",

  // iroh
  "get_iroh_info",
  "iroh_phone_invite",
  "list_iroh_totp",
  "create_iroh_totp",
  "get_iroh_scope_groups",
  "list_iroh_clients",
  "register_iroh_client",

  // dnd / full disk access
  "open_full_disk_access",

  // bulk-migrated via daemonInvoke proxy
  "abort_llm_stream",
  "cancel_llm_download",
  "cancel_weights_download",
  "check_ocr_models_ready",
  "compute_umap_compare",
  "delete_label",
  "delete_llm_model",
  "delete_session",
  "download_llm_model",
  "download_ocr_models",
  "estimate_reembed",
  "estimate_screenshot_reembed",
  "find_session_for_timestamp",
  "get_csv_metrics",
  "get_daily_goal",
  "get_day_metrics_batch",
  "get_daily_recording_mins",
  "get_dnd_active",
  "get_dnd_config",
  "get_dnd_status",
  "get_eeg_model_config",
  "get_eeg_model_status",
  "get_embedding_model",
  "get_embedding_overlap",
  "get_daemon_watchdog",
  "get_exg_catalog",
  "get_exg_inference_device",
  "get_filter_config",
  "get_goal_notified_date",
  "get_history_stats",
  "get_hook_log",
  "get_hook_log_count",
  "get_hook_statuses",
  "get_hooks",
  "get_label_embedding_status",
  "get_llm_catalog",
  "get_llm_downloads",
  "get_llm_logs",
  "get_llm_server_status",
  "get_model_hardware_fit",
  "get_neutts_config",
  "get_reembed_config",
  "get_recent_labels",
  "get_screenshot_config",
  "get_screenshot_metrics",
  "get_screenshots_around",
  "get_screenshots_dir",
  "get_session_embedding_count",
  "get_session_location",
  "get_session_metrics",
  "get_session_timeseries",
  "get_sleep_config",
  "get_sleep_stages",
  "get_stale_label_count",
  "get_tts_preload",
  "get_umap_config",
  "get_ws_clients",
  "get_ws_request_log",
  "list_all_sessions",
  "list_embedding_models",
  "list_embedding_sessions",
  "list_focus_modes",
  "list_local_session_days",
  "list_sessions",
  "list_sessions_for_local_day",
  "pause_llm_download",
  "query_annotations",
  "rebuild_screenshot_embeddings",
  "reembed_all_labels",
  "reembed_labels",
  "refresh_llm_catalog",
  "resume_llm_download",
  "list_search_devices",
  "rebuild_label_index",
  "search_corpus_stats",
  "search_labels_by_text",
  "search_screenshots_by_text",
  "set_daemon_watchdog",
  "set_daily_goal",
  "set_dnd_config",
  "set_eeg_model_config",
  "set_embedding_model",
  "set_embedding_overlap",
  "set_exg_inference_device",
  "set_filter_config",
  "set_goal_notified_date",
  "set_hooks",
  "set_reembed_config",
  "set_neutts_config",
  "set_screenshot_config",
  "set_sleep_config",
  "set_tts_preload",
  "set_umap_config",
  "start_llm_server",
  "stop_llm_server",
  "submit_label",
  "suggest_hook_distances",
  "suggest_hook_keywords",
  "switch_llm_mmproj",
  "switch_llm_model",
  "test_dnd",
  "trigger_reembed",
  "trigger_weights_download",
  "update_label",
  "load_chat_session",
  "list_chat_sessions",
  "rename_chat_session",
  "delete_chat_session",
  "archive_chat_session",
  "unarchive_chat_session",
  "list_archived_chat_sessions",
  "save_chat_message",
  "get_session_params",
  "set_session_params",
  "new_chat_session",
  "save_chat_tool_calls",
  "cancel_tool_call",
]);

const INVOKE_RE = /(?<!daemon)invoke(?:<[^>]+>)?\(\s*["'`]([^"'`]+)["'`]/g;

function lineForIndex(content, idx) {
  return content.slice(0, idx).split("\n").length;
}

async function listFiles(dir) {
  const out = [];
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...(await listFiles(full)));
    } else if (entry.isFile() && entry.name.endsWith(".svelte")) {
      out.push(full);
    }
  }
  return out;
}

const offenders = [];
const targetFiles = [...(await listFiles(ROOT)), ...EXTRA_FILES];
for (const file of targetFiles) {
  const text = await readFile(file, "utf8");
  INVOKE_RE.lastIndex = 0;
  for (let m = INVOKE_RE.exec(text); m; m = INVOKE_RE.exec(text)) {
    const cmd = m[1];
    if (!DAEMON_OWNED_COMMANDS.has(cmd)) continue;
    offenders.push({
      file: path.relative(process.cwd(), file),
      line: lineForIndex(text, m.index),
      cmd,
    });
  }
}

if (offenders.length > 0) {
  console.error("❌ Direct invoke(...) detected for daemon-owned commands. Use $lib/daemon/client wrappers instead.\n");
  for (const o of offenders) {
    console.error(`- ${o.file}:${o.line} -> ${o.cmd}`);
  }
  process.exit(1);
}

console.log("✅ No direct invoke(...) calls for daemon-owned commands in guarded Svelte files");
