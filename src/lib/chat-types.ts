// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Shared type definitions for the chat feature.
 *
 * Extracted from `routes/chat/+page.svelte` to reduce file size and
 * allow reuse across chat sub-components.
 */

// ── Core types ──────────────────────────────────────────────────────────────

export type Role = "user" | "assistant" | "system";
export type ServerStatus = "stopped" | "loading" | "running";

export interface ToolUseEvent {
  tool: string;
  status: string;           // "calling" | "done" | "error" | "approval_required" | "cancelled"
  detail?: string;
  toolCallId?: string;
  args?: any;               // structured arguments from tool_execution_start
  result?: any;             // structured result from tool_execution_end
  expanded?: boolean;       // UI toggle
}

export interface Attachment { dataUrl: string; mimeType: string; name: string; }

export interface UsageInfo {
  prompt_tokens:     number;
  completion_tokens: number;
  total_tokens:      number;
  n_ctx:             number;
}

export interface Message {
  id:           number;
  role:         Role;
  content:      string;
  /** Assistant text emitted before a <think> block, shown as its own bubble */
  leadIn?:      string;
  /** Images attached to a user message */
  attachments?: Attachment[];
  /** Chain-of-thought text between <think>…</think> (stripped from content) */
  thinking?:    string;
  /** Whether the thinking block is expanded in the UI */
  thinkOpen?:   boolean;
  /** True while we're streaming tokens in */
  pending?:     boolean;
  /** ms taken for first token */
  ttft?:        number;
  /** ms for full response */
  elapsed?:     number;
  /** Token usage from the final SSE chunk */
  usage?:       UsageInfo;
  /** Tool calls made during this response */
  toolUses?:    ToolUseEvent[];
}

// ── Server status payload ───────────────────────────────────────────────────

export interface ServerStatusPayload {
  status: ServerStatus;
  model_name: string;
  model?: string;
  supports_vision?: boolean;
}

// ── IPC streaming types (mirror Rust ChatChunk) ─────────────────────────────

export interface ChatChunkDelta         { type: "delta"; content: string; }
export interface ChatChunkToolUse       { type: "tool_use"; tool: string; status: string; detail?: string; }
export interface ChatChunkToolExecStart { type: "tool_execution_start"; tool_call_id: string; tool_name: string; args: any; }
export interface ChatChunkToolExecEnd   { type: "tool_execution_end";   tool_call_id: string; tool_name: string; result: any; is_error: boolean; }
export interface ChatChunkToolCancelled { type: "tool_cancelled"; tool_call_id: string; tool_name: string; }
export interface ChatChunkDone {
  type:              "done";
  finish_reason:     string;
  prompt_tokens:     number;
  completion_tokens: number;
  n_ctx:             number;
}
export interface ChatChunkError { type: "error"; message: string; }
export type ChatChunk =
  | ChatChunkDelta | ChatChunkToolUse | ChatChunkToolExecStart
  | ChatChunkToolExecEnd | ChatChunkToolCancelled | ChatChunkDone | ChatChunkError;

// ── Thinking budget ─────────────────────────────────────────────────────────

export type ThinkingLevel = "minimal" | "normal" | "extended" | "unlimited";

export const THINKING_LEVELS: { labelKey: string; key: ThinkingLevel; budget: number | null }[] = [
  { labelKey: "chat.think.minimal",   key: "minimal",   budget: 512   },
  { labelKey: "chat.think.normal",    key: "normal",    budget: 2048  },
  { labelKey: "chat.think.extended",  key: "extended",  budget: 8192  },
  { labelKey: "chat.think.unlimited", key: "unlimited", budget: null  },
];

// ── Tool configuration ──────────────────────────────────────────────────────

export type ToolExecutionMode = "sequential" | "parallel";

export interface ToolConfig {
  enabled: boolean;
  date: boolean; location: boolean; web_search: boolean; web_fetch: boolean;
  bash: boolean; read_file: boolean; write_file: boolean; edit_file: boolean;
  execution_mode: ToolExecutionMode;
  max_rounds: number;
  max_calls_per_round: number;
}

export const DEFAULT_TOOL_CONFIG: ToolConfig = {
  enabled: true,
  date: true, location: true, web_search: true, web_fetch: true,
  bash: false, read_file: false, write_file: false, edit_file: false,
  execution_mode: "parallel", max_rounds: 3, max_calls_per_round: 4,
};

// ── Stored-message type (mirrors Rust StoredMessage) ────────────────────────

export interface StoredToolCallRow {
  id:           number;
  message_id:   number;
  tool:         string;
  status:       string;
  detail?:      string | null;
  tool_call_id?: string | null;
  args?:        any;
  result?:      any;
  created_at:   number;
}

export interface StoredMessage {
  id:         number;
  session_id: number;
  role:       string;
  content:    string;
  thinking:   string | null;
  created_at: number;
  tool_calls: StoredToolCallRow[];
}

export interface ChatSessionResponse {
  session_id: number;
  messages:   StoredMessage[];
}

// ── System prompt presets ───────────────────────────────────────────────────

export const SYSTEM_PROMPT_DEFAULT = "You are a helpful assistant.";
export const SYSTEM_PROMPT_KEY     = "chat.systemPrompt";

export const SYSTEM_PROMPT_PRESETS: { key: string; icon: string; prompt: string }[] = [
  {
    key:    "default",
    icon:   "🤖",
    prompt: "You are a helpful assistant.",
  },
  {
    key:    "coach",
    icon:   "🧘",
    prompt:
      "You are a neurofeedback coach specialising in relaxation and stress reduction. " +
      "Give practical, evidence-based advice grounded in the user's live EEG data. " +
      "Be encouraging, clear, and concise.",
  },
  {
    key:    "focus",
    icon:   "🎯",
    prompt:
      "You are a focus and cognitive-performance coach. " +
      "Help the user optimise their attention, working memory, and mental endurance " +
      "using insights from their EEG readings. Offer actionable protocols.",
  },
  {
    key:    "educator",
    icon:   "📚",
    prompt:
      "You are a neuroscience educator. " +
      "Explain brain-wave patterns and EEG metrics in plain, accessible language. " +
      "Use analogies freely and avoid unnecessary jargon.",
  },
  {
    key:    "sleep",
    icon:   "😴",
    prompt:
      "You are a sleep and recovery specialist. " +
      "Interpret the user's EEG data to provide personalised guidance on improving " +
      "sleep quality, recovery, and circadian regulation.",
  },
  {
    key:    "mindfulness",
    icon:   "🌿",
    prompt:
      "You are a mindfulness and meditation guide. " +
      "Use the user's real-time brainwave data to suggest meditation techniques, " +
      "breathing exercises, and awareness practices tailored to their current state.",
  },
];

// ── EEG band snapshot — re-exported from the canonical BandChart module ─────
// The full BandPowers / BandSnapshot interfaces live in BandChart.svelte.
// Re-export here so chat-related code doesn't need to import from a UI component.
export type { BandPowers, BandSnapshot } from "$lib/BandChart.svelte";

// ── Conversion helpers ──────────────────────────────────────────────────────

export function storedToMessage(sm: StoredMessage, idCounter: { value: number }): Message {
  const msg: Message = {
    id:        ++idCounter.value,
    role:      sm.role as Role,
    content:   sm.content,
    thinking:  sm.thinking ?? undefined,
    thinkOpen: false,
    pending:   false,
  };
  if (sm.tool_calls && sm.tool_calls.length > 0) {
    msg.toolUses = sm.tool_calls.map((tc: StoredToolCallRow): ToolUseEvent => ({
      tool:       tc.tool,
      status:     tc.status,
      detail:     tc.detail ?? undefined,
      toolCallId: tc.tool_call_id ?? undefined,
      args:       tc.args ?? undefined,
      result:     tc.result ?? undefined,
      expanded:   false,
    }));
  }
  return msg;
}

/**
 * Build the content field for a user message (plain string or parts array).
 * If images are present, returns a multi-part content array for the API.
 */
export function buildUserContent(text: string, imgs: Attachment[]): string | any[] {
  if (imgs.length === 0) return text;
  const parts: any[] = [];
  if (text.trim()) parts.push({ type: "text", text });
  for (const img of imgs) {
    parts.push({ type: "image_url", image_url: { url: img.dataUrl } });
  }
  return parts;
}

/**
 * Rough token estimate: ~4 chars per token, ~1 token overhead.
 */
export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4) + 1;
}
