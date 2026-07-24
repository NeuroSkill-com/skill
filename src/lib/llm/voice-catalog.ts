// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Shared TTS / ASR engine catalogs from the daemon (`/v1/tts/engines`, `/v1/asr/engines`).
 * UI components should use these instead of hardcoding engine chips so new
 * rlx-models backends appear automatically.
 */

import { invoke } from "@tauri-apps/api/core";
import { daemonGet } from "$lib/daemon/http";

export interface TtsEngineInfo {
  id: string;
  label: string;
  models: string[];
  default_model: string;
  default_voice: string;
  voices: string[];
  experimental: boolean;
  downloadable: boolean;
  needs_bundle: boolean;
  /** False when this build cannot run the engine (e.g. Windows / feature off). */
  available?: boolean;
}

export interface AsrEngineInfo {
  id: string;
  label: string;
  models: string[];
  default_model: string;
  experimental: boolean;
  downloadable: boolean;
  /** False when ASR is unavailable in this build (e.g. Windows). */
  available?: boolean;
}

/** Offline fallback when the daemon is unreachable (mirrors skill-tts catalog). */
export const TTS_ENGINE_FALLBACK_LIST: TtsEngineInfo[] = [
  { id: "kitten", label: "KittenTTS", models: [], default_model: "", default_voice: "", voices: [], experimental: false, downloadable: true, needs_bundle: false },
  { id: "neutts", label: "NeuTTS", models: [], default_model: "", default_voice: "", voices: [], experimental: false, downloadable: true, needs_bundle: false },
  { id: "rlx-tts", label: "RLX-TTS", models: [], default_model: "", default_voice: "", voices: [], experimental: false, downloadable: true, needs_bundle: false },
  { id: "qwen3-tts", label: "Qwen3-TTS", models: ["Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice"], default_model: "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice", default_voice: "vivian", voices: ["vivian", "serena", "uncle_fu", "dylan", "eric", "ryan", "aiden", "ono_anna", "sohee"], experimental: false, downloadable: true, needs_bundle: false },
  { id: "orpheus", label: "Orpheus", models: [], default_model: "", default_voice: "tara", voices: ["tara", "leah", "jess", "leo", "dan", "mia", "zac", "zoe"], experimental: true, downloadable: true, needs_bundle: false },
  { id: "kyutai-tts", label: "Kyutai-TTS", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "inflect-nano", label: "Inflect-Nano", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: false, needs_bundle: true },
  { id: "styletts2", label: "StyleTTS2", models: [], default_model: "", default_voice: "", voices: [], experimental: false, downloadable: true, needs_bundle: false },
  { id: "piper", label: "Piper", models: [], default_model: "", default_voice: "", voices: [], experimental: false, downloadable: true, needs_bundle: false },
  { id: "chatterbox", label: "Chatterbox", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "f5tts", label: "F5-TTS", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "luxtts", label: "LuxTTS", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "moss-nano", label: "MOSS-Nano", models: [], default_model: "", default_voice: "", voices: [], experimental: false, downloadable: true, needs_bundle: false },
  { id: "soprano", label: "Soprano", models: [], default_model: "", default_voice: "", voices: [], experimental: false, downloadable: true, needs_bundle: false },
  { id: "supertonic", label: "Supertonic", models: [], default_model: "", default_voice: "", voices: [], experimental: false, downloadable: true, needs_bundle: false },
  { id: "sesame", label: "Sesame", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "zonos", label: "Zonos", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "gepard", label: "Gepard", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "metavoice", label: "MetaVoice", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "pocket-tts", label: "Pocket-TTS", models: [], default_model: "", default_voice: "", voices: [], experimental: false, downloadable: true, needs_bundle: false },
  { id: "parlertts", label: "Parler-TTS", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "miotts", label: "MioTTS", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "miratts", label: "MiraTTS", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
  { id: "melotts", label: "MeloTTS", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: false, needs_bundle: true },
  { id: "voxtral-tts", label: "Voxtral-TTS", models: [], default_model: "", default_voice: "", voices: [], experimental: true, downloadable: true, needs_bundle: false },
];

export const ASR_ENGINE_FALLBACK_LIST: AsrEngineInfo[] = [
  {
    id: "whisper",
    label: "Whisper",
    models: [
      "openai/whisper-tiny.en",
      "openai/whisper-base.en",
      "openai/whisper-small.en",
      "openai/whisper-small",
      "openai/whisper-medium",
      "openai/whisper-large-v3",
    ],
    default_model: "openai/whisper-base.en",
    experimental: false,
    downloadable: true,
  },
  {
    id: "qwen3-asr",
    label: "Qwen3-ASR",
    models: ["Qwen/Qwen3-ASR-0.6B", "Qwen/Qwen3-ASR-1.7B"],
    default_model: "Qwen/Qwen3-ASR-0.6B",
    experimental: false,
    downloadable: true,
  },
  {
    id: "voxtral",
    label: "Voxtral",
    models: ["mistralai/Voxtral-Mini-3B-2507"],
    default_model: "mistralai/Voxtral-Mini-3B-2507",
    experimental: true,
    downloadable: true,
  },
  {
    id: "funasr",
    label: "FunASR",
    models: ["FunAudioLLM/SenseVoiceSmall", "funasr/paraformer-zh"],
    default_model: "FunAudioLLM/SenseVoiceSmall",
    experimental: false,
    downloadable: true,
  },
  {
    id: "nemotron-asr",
    label: "Nemotron-ASR",
    models: ["nvidia/nemotron-3.5-asr-streaming-0.6b"],
    default_model: "nvidia/nemotron-3.5-asr-streaming-0.6b",
    experimental: true,
    downloadable: true,
  },
  {
    id: "rlx-asr",
    label: "RLX-ASR",
    models: ["eugenehp/rlx-asr"],
    default_model: "eugenehp/rlx-asr",
    experimental: true,
    downloadable: true,
  },
];

function coerceTts(raw: unknown): TtsEngineInfo[] {
  if (!Array.isArray(raw)) return [...TTS_ENGINE_FALLBACK_LIST];
  const out: TtsEngineInfo[] = [];
  for (const item of raw) {
    if (!item || typeof item !== "object") continue;
    const o = item as Record<string, unknown>;
    const id = typeof o.id === "string" ? o.id : "";
    if (!id) continue;
    const fb = TTS_ENGINE_FALLBACK_LIST.find((e) => e.id === id);
    out.push({
      id,
      label: typeof o.label === "string" ? o.label : (fb?.label ?? id),
      models: Array.isArray(o.models) ? o.models.filter((m): m is string => typeof m === "string") : (fb?.models ?? []),
      default_model: typeof o.default_model === "string" ? o.default_model : (fb?.default_model ?? ""),
      default_voice: typeof o.default_voice === "string" ? o.default_voice : (fb?.default_voice ?? ""),
      voices: Array.isArray(o.voices) ? o.voices.filter((m): m is string => typeof m === "string") : (fb?.voices ?? []),
      experimental: Boolean(o.experimental ?? fb?.experimental),
      downloadable: Boolean(o.downloadable ?? fb?.downloadable),
      needs_bundle: Boolean(o.needs_bundle ?? fb?.needs_bundle),
      available: o.available === undefined ? true : Boolean(o.available),
    });
  }
  return out.length ? out : [...TTS_ENGINE_FALLBACK_LIST];
}

function coerceAsr(raw: unknown): AsrEngineInfo[] {
  if (!Array.isArray(raw)) return [...ASR_ENGINE_FALLBACK_LIST];
  const out: AsrEngineInfo[] = [];
  for (const item of raw) {
    if (!item || typeof item !== "object") continue;
    const o = item as Record<string, unknown>;
    const id = typeof o.id === "string" ? o.id : "";
    if (!id) continue;
    const fb = ASR_ENGINE_FALLBACK_LIST.find((e) => e.id === id);
    out.push({
      id,
      label: typeof o.label === "string" ? o.label : (fb?.label ?? id),
      models: Array.isArray(o.models) ? o.models.filter((m): m is string => typeof m === "string") : (fb?.models ?? []),
      default_model: typeof o.default_model === "string" ? o.default_model : (fb?.default_model ?? ""),
      experimental: Boolean(o.experimental ?? fb?.experimental),
      downloadable: Boolean(o.downloadable ?? fb?.downloadable),
      available: o.available === undefined ? true : Boolean(o.available),
    });
  }
  return out.length ? out : [...ASR_ENGINE_FALLBACK_LIST];
}

/** Fetch TTS engines from the daemon (Tauri IPC → HTTP, with offline fallback). */
export async function fetchTtsEngines(): Promise<TtsEngineInfo[]> {
  try {
    const res = await invoke<{ engines?: unknown }>("get_tts_engines");
    return coerceTts(res?.engines);
  } catch {
    try {
      const res = await daemonGet<{ engines?: unknown }>("/v1/tts/engines");
      return coerceTts(res?.engines);
    } catch {
      return [...TTS_ENGINE_FALLBACK_LIST];
    }
  }
}

/** Fetch ASR engines from the daemon. */
export async function fetchAsrEngines(): Promise<AsrEngineInfo[]> {
  try {
    const res = await invoke<{ engines?: unknown }>("get_asr_engines");
    return coerceAsr(res?.engines);
  } catch {
    try {
      const res = await daemonGet<{ engines?: unknown }>("/v1/asr/engines");
      return coerceAsr(res?.engines);
    } catch {
      return [...ASR_ENGINE_FALLBACK_LIST];
    }
  }
}

/** i18n key for a TTS engine chip; falls back to English label in `t()`. */
export function ttsEngineLabelKey(id: string): string {
  return `chat.tts.engine.${id}`;
}

/** i18n key for an ASR engine chip. */
export function asrEngineLabelKey(id: string): string {
  return `chat.voice.engine.${id}`;
}
