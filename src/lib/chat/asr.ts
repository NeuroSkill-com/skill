// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Shared voice-input (ASR + VAD) types and helpers for the chat window.
 *
 * The daemon owns the engine: `asr_start` / `asr_stop` / `asr_status` /
 * `asr_set_ptt` Tauri commands drive it, and lifecycle + transcript events
 * arrive on the `/v1/events` WebSocket as `"asr"` events (see `onDaemonEvent`).
 *
 * `settings.asr` (the persisted defaults) is owned by the daemon: `fetchAsrDefaults`
 * reads it via `get_asr_settings` and `saveAsrDefaults` writes it via
 * `set_asr_settings` (both hit settings.json, the source of truth). A
 * `localStorage` copy is kept only as an instant-first-paint cache —
 * `loadAsrDefaults` reads it synchronously on mount, then the async daemon fetch
 * overwrites it. `asr_start` is still called with the resolved mode, so the
 * daemon stays the source of truth at runtime too.
 */

import { invoke } from "@tauri-apps/api/core";

export type AsrTrigger = "continuous" | "push_to_talk";
export type AsrRouting = "voice_loop" | "transcribe_only";

/** Per-session voice-mode selection passed to `asr_start`. */
export interface AsrMode {
  trigger: AsrTrigger;
  routing: AsrRouting;
  language: string;
}

/** Chat-side `settings.asr` defaults (mirrors the daemon `AsrConfig`). */
export interface AsrDefaults {
  enabled: boolean;
  default_trigger: AsrTrigger;
  default_routing: AsrRouting;
  language: string;
  /** ASR engine backend (e.g. "whisper"). */
  engine: string;
  /** HuggingFace repo id of the speech-recognition model. */
  model: string;
}

export const ASR_DEFAULTS_FALLBACK: AsrDefaults = {
  enabled: true,
  default_trigger: "continuous",
  default_routing: "voice_loop",
  language: "en",
  engine: "whisper",
  model: "openai/whisper-base.en",
};

export const ASR_DEFAULTS_KEY = "chat.asrDefaults";

/** Normalise a partial/untrusted object into a full `AsrDefaults`. */
function coerceAsrDefaults(parsed: Partial<AsrDefaults> | null | undefined): AsrDefaults {
  return {
    enabled: parsed?.enabled ?? ASR_DEFAULTS_FALLBACK.enabled,
    default_trigger: parsed?.default_trigger ?? ASR_DEFAULTS_FALLBACK.default_trigger,
    default_routing: parsed?.default_routing ?? ASR_DEFAULTS_FALLBACK.default_routing,
    language: parsed?.language ?? ASR_DEFAULTS_FALLBACK.language,
    engine: parsed?.engine ?? ASR_DEFAULTS_FALLBACK.engine,
    model: parsed?.model ?? ASR_DEFAULTS_FALLBACK.model,
  };
}

/**
 * Synchronously read the cached ASR defaults for instant first paint.
 *
 * This is only the `localStorage` cache — the daemon (settings.json) is the
 * source of truth. Callers should follow up with `fetchAsrDefaults()` on mount
 * to reconcile against the daemon.
 */
export function loadAsrDefaults(): AsrDefaults {
  try {
    const raw = localStorage.getItem(ASR_DEFAULTS_KEY);
    if (!raw) return { ...ASR_DEFAULTS_FALLBACK };
    return coerceAsrDefaults(JSON.parse(raw) as Partial<AsrDefaults>);
  } catch {
    return { ...ASR_DEFAULTS_FALLBACK };
  }
}

/** Daemon `get_asr_settings` response shape. */
interface GetAsrSettingsResponse {
  ok: boolean;
  asr?: Partial<AsrDefaults>;
}

/**
 * Load the authoritative ASR defaults from the daemon (settings.json).
 *
 * Falls back to the built-in defaults on any error. On success the result is
 * also written to the `localStorage` cache so the next first paint is correct.
 */
export async function fetchAsrDefaults(): Promise<AsrDefaults> {
  try {
    const res = await invoke<GetAsrSettingsResponse>("get_asr_settings");
    const defaults = coerceAsrDefaults(res?.asr);
    cacheAsrDefaults(defaults);
    return defaults;
  } catch {
    return { ...ASR_DEFAULTS_FALLBACK };
  }
}

/** Write the ASR defaults to the `localStorage` first-paint cache. */
function cacheAsrDefaults(defaults: AsrDefaults): void {
  try {
    localStorage.setItem(ASR_DEFAULTS_KEY, JSON.stringify(defaults));
  } catch {}
}

/**
 * Persist the ASR defaults to the daemon (settings.json) and the local cache.
 *
 * Best-effort: the daemon write is fire-and-forget (errors are swallowed) and
 * the cache write ignores storage errors.
 */
export function saveAsrDefaults(defaults: AsrDefaults): void {
  cacheAsrDefaults(defaults);
  invoke("set_asr_settings", { config: defaults }).catch(() => {});
}

// ── Engine event payload (daemon `"asr"` WebSocket events) ────────────────────

export type AsrEventKind =
  | "loading"
  | "download"
  | "listening"
  | "speech_start"
  | "speech_end"
  | "transcript"
  | "error"
  | "stopped"
  | "assistant";

/** Payload of a daemon `"asr"` event (`DaemonEvent.payload`). */
export interface AsrEventPayload {
  kind: AsrEventKind;
  // transcript / assistant
  text?: string;
  is_final?: boolean;
  // error
  message?: string;
  // assistant
  spoken?: boolean;
  // download progress
  label?: string;
  downloaded?: number;
  total?: number;
}

/** Visual listening state derived from the event stream. */
export type AsrPhase = "idle" | "loading" | "listening" | "speaking";

/** Status returned by the `asr_status` command. */
export interface AsrStatus {
  running: boolean;
  available: boolean;
  trigger: AsrTrigger | null;
  routing: AsrRouting | null;
  language: string | null;
  ptt_active: boolean;
}
