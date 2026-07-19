// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * TTS engine selection helpers for the LLM/TTS settings UI.
 *
 * The daemon owns the selection: `get_tts_engine` reads it and `set_tts_engine`
 * writes it (applies it live to the `skill-tts` router *and* persists it to
 * settings.json — the source of truth). A `localStorage` copy is kept only as an
 * instant-first-paint cache.
 */

import { invoke } from "@tauri-apps/api/core";

/** Selected TTS engine plus engine-specific overrides (mirrors daemon settings). */
export interface TtsEngineConfig {
  /** "kitten" | "neutts" | "qwen3-tts" | "orpheus" | "kyutai-tts". */
  engine: string;
  /** HuggingFace repo override (empty = engine default). */
  model: string;
  /** Voice/speaker override (empty = engine default). */
  voice: string;
  /** Preset voices for the active engine (from daemon; optional cache). */
  voices?: string[];
}

export const TTS_ENGINE_FALLBACK: TtsEngineConfig = {
  engine: "kitten",
  model: "",
  voice: "",
};

export const TTS_ENGINE_KEY = "llm.ttsEngine";

function coerce(parsed: Partial<TtsEngineConfig> | null | undefined): TtsEngineConfig {
  const rawVoices = parsed?.voices;
  const voices = Array.isArray(rawVoices) ? rawVoices.filter((v): v is string => typeof v === "string") : undefined;
  return {
    engine: parsed?.engine ?? TTS_ENGINE_FALLBACK.engine,
    model: parsed?.model ?? TTS_ENGINE_FALLBACK.model,
    voice: parsed?.voice ?? TTS_ENGINE_FALLBACK.voice,
    voices: voices?.length ? voices : undefined,
  };
}

/** Synchronously read the cached selection for instant first paint. */
export function loadTtsEngine(): TtsEngineConfig {
  try {
    const raw = localStorage.getItem(TTS_ENGINE_KEY);
    if (!raw) return { ...TTS_ENGINE_FALLBACK };
    return coerce(JSON.parse(raw) as Partial<TtsEngineConfig>);
  } catch {
    return { ...TTS_ENGINE_FALLBACK };
  }
}

function cache(cfg: TtsEngineConfig): void {
  try {
    localStorage.setItem(TTS_ENGINE_KEY, JSON.stringify(cfg));
  } catch {}
}

/** Load the authoritative selection from the daemon (settings.json). */
export async function fetchTtsEngine(): Promise<TtsEngineConfig> {
  try {
    const res = await invoke<Partial<TtsEngineConfig>>("get_tts_engine");
    const cfg = coerce(res);
    cache(cfg);
    return cfg;
  } catch {
    return { ...TTS_ENGINE_FALLBACK };
  }
}

/** Persist the selection to the daemon (applies live) and the local cache. */
export function saveTtsEngine(cfg: TtsEngineConfig): void {
  cache(cfg);
  invoke("set_tts_engine", { config: cfg }).catch(() => {});
}
