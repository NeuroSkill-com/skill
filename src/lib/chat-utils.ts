// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Pure utility functions for the chat page — tool-call parsing,
 * danger detection, and assistant output parsing.
 *
 * Extracted from `routes/chat/+page.svelte` to reduce file size and
 * allow independent unit testing.
 */

// ── Tool-call stripping ─────────────────────────────────────────────────────

/** Known built-in tool names — must stay in sync with KNOWN_TOOL_NAMES in tools.rs */
const KNOWN_TOOLS = new Set([
  "date",
  "location",
  "web_search",
  "web_fetch",
  "bash",
  "read_file",
  "write_file",
  "edit_file",
  "search_output",
]);

function isToolCallObject(v: Record<string, unknown>): boolean {
  if (("name" in v || "tool" in v || "tool_calls" in v) && ("parameters" in v || "arguments" in v || "tool_calls" in v))
    return true;
  const keys = Object.keys(v);
  return (
    keys.length > 0 &&
    keys.some((k) => KNOWN_TOOLS.has(k)) &&
    Object.values(v).every((val) => typeof val === "object" && val !== null)
  );
}

function looksLikeToolCallJsonPrefix(s: string): boolean {
  const trimmed = s.trimStart();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return false;

  const probe = trimmed.slice(0, 320).toLowerCase();
  const isDictStyle = [...KNOWN_TOOLS].some((name) => probe.includes(`"${name}":`) || probe.includes(`"${name}": `));
  if (isDictStyle) return true;

  const mentionsToolName =
    probe.includes('"name"') ||
    probe.includes('"tool"') ||
    probe.includes('"tool_calls"') ||
    probe.includes('"function"');
  const mentionsArgs = probe.includes('"parameter') || probe.includes('"argument') || probe.includes("<think>");

  return mentionsToolName && mentionsArgs;
}

function isToolCallArray(v: unknown): boolean {
  if (!Array.isArray(v)) return false;
  return v.some(
    (item) =>
      typeof item === "object" &&
      item !== null &&
      !Array.isArray(item) &&
      isToolCallObject(item as Record<string, unknown>),
  );
}

/**
 * Strip tool-call JSON code fences that leaked into rawAcc.
 *
 * The streaming sanitizer on the Rust side holds back tool-call JSON, but
 * it can only recognise a fence as a tool call once it has seen BOTH a
 * "tool"/"name" key AND a "parameters"/"arguments" key.  Tokens emitted
 * before that threshold are already in rawAcc and need to be cleaned here.
 */
export function stripToolCallFences(raw: string): string {
  // 1. Complete fenced blocks
  let s = raw.replace(/```(?:json)?\n([\s\S]*?)\n?```/g, (match, body: string) => {
    const trimmedBody = body.trim();
    try {
      const v = JSON.parse(trimmedBody);
      if (typeof v === "object" && v !== null && !Array.isArray(v) && isToolCallObject(v)) return "";
      if (isToolCallArray(v)) return "";
    } catch {
      /* not JSON — keep */
    }
    if (looksLikeToolCallJsonPrefix(trimmedBody)) return "";
    return match;
  });

  // 2a. Incomplete fence immediately before a <think> tag.
  s = s.replace(/```(?:json)?\n([\s\S]*?)(?=\n*<think>)/g, (match, body: string) =>
    looksLikeToolCallJsonPrefix(body) ? "" : match,
  );

  // 2b. Incomplete fence at end of string (still streaming).
  s = s.replace(/```(?:json)?\n([\s\S]*)$/g, (match, body: string) => (looksLikeToolCallJsonPrefix(body) ? "" : match));

  // 3. Bare inline tool-call JSON (not fenced)
  s = s.replace(/(?:^|\n)\s*[[{][\s\S]*$/gm, (match) => {
    if (looksLikeToolCallJsonPrefix(match.trim())) return "";
    return match;
  });

  // 4. Complete [TOOL_CALL]…[/TOOL_CALL] blocks
  s = s.replace(/\[TOOL_CALL\][\s\S]*?\[\/TOOL_CALL\]/g, "");

  // 5. Incomplete [TOOL_CALL] at end of string (mid-stream)
  s = s.replace(/\[TOOL_C[\s\S]*$/g, "");

  return s;
}

// ── Lead-in cleaning ────────────────────────────────────────────────────────

export function cleanAssistantLeadIn(raw: string): string {
  return raw
    .replace(/```[a-z]*\s*/gi, "")
    .split("\n")
    .filter((line) => !/^\s*(json|copy)\s*$/i.test(line))
    .join("\n")
    .trim();
}

/**
 * Clean lead-in text for display.  When tool calls are active, aggressively
 * strip ALL incomplete code fences and any JSON-like fragments.
 */
export function cleanLeadInForDisplay(raw: string, hasToolUses: boolean): string {
  if (!raw.trim()) return "";

  let s = stripToolCallFences(raw);

  if (hasToolUses) {
    s = s.replace(/```[a-z]*\n[\s\S]*$/gi, "");
    s = s.replace(/\n\s*[[{][\s\S]*$/g, "");
  }

  return s.trim();
}

// ── Danger detection ────────────────────────────────────────────────────────

/** Bash patterns that indicate a potentially dangerous command (mirrored from Rust). */
const DANGEROUS_BASH_PATTERNS = [
  "rm ",
  "rm\t",
  "rmdir",
  "shred",
  "mkfs",
  "dd if=",
  "dd of=",
  "sudo ",
  "su -",
  "su\t",
  "> /dev/",
  "chmod",
  "chown",
  "kill ",
  "killall",
  "pkill",
  "shutdown",
  "reboot",
  "halt",
  "poweroff",
  "systemctl stop",
  "systemctl disable",
  ":(){ :|:& };:",
  "/etc/",
  "/boot/",
  "/usr/",
  "/var/",
  "/sys/",
  "/proc/",
];

/** Sensitive path prefixes (mirrored from Rust). */
const SENSITIVE_PATH_PREFIXES = [
  "/etc/",
  "/boot/",
  "/usr/",
  "/var/",
  "/sys/",
  "/proc/",
  "/bin/",
  "/sbin/",
  "/lib/",
  "/opt/",
];

/**
 * Check if a tool call looks dangerous based on its name and arguments.
 * Returns a danger reason i18n key, or null if safe.
 *
 * Accepts any object with `tool` and optional `args` fields.
 */
export function detectToolDanger(tu: { tool: string; args?: Record<string, unknown> }): string | null {
  if (tu.tool === "bash" && typeof tu.args?.command === "string") {
    const cmd = tu.args.command.toLowerCase();
    for (const pat of DANGEROUS_BASH_PATTERNS) {
      if (cmd.includes(pat)) {
        return `chat.tools.dangerBash`;
      }
    }
  }
  if (["write_file", "edit_file", "read_file"].includes(tu.tool) && typeof tu.args?.path === "string") {
    const path = tu.args.path;
    for (const prefix of SENSITIVE_PATH_PREFIXES) {
      if (path.startsWith(prefix)) {
        return `chat.tools.dangerPath`;
      }
    }
  }
  return null;
}

// ── Assistant output parsing ────────────────────────────────────────────────

/**
 * Split raw assistant output into three display zones:
 * - `leadIn`  = inter-think text segments (all except the last)
 * - `thinking` = merged <think>…</think> blocks
 * - `content` = the final answer (last non-empty segment)
 */
export function parseAssistantOutput(raw: string): {
  leadIn: string;
  thinking: string;
  content: string;
} {
  const s = stripToolCallFences(raw);

  if (!s.includes("<think>")) return { leadIn: "", thinking: "", content: s };

  const thinkingParts: string[] = [];
  let withoutThink = s.replace(/<think>([\s\S]*?)<\/think>/g, (_: string, inner: string) => {
    thinkingParts.push(inner.trim());
    return "\x00";
  });

  // Handle an unclosed <think> at the end (still streaming)
  const openIdx = withoutThink.indexOf("<think>");
  if (openIdx !== -1) {
    thinkingParts.push(withoutThink.slice(openIdx + 7).trim());
    withoutThink = withoutThink.slice(0, openIdx);
  }

  const thinking = thinkingParts.join("\n\n");

  const parts = withoutThink
    .split("\x00")
    .map((p: string) => p.trim())
    .filter(Boolean);

  if (parts.length === 0) return { leadIn: "", thinking, content: "" };

  const content = parts[parts.length - 1];
  const leadIn = parts.slice(0, -1).map(cleanAssistantLeadIn).filter(Boolean).join("\n\n");

  return { leadIn, thinking, content };
}
