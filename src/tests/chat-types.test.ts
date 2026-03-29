// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import {
  type Attachment,
  buildUserContent,
  DEFAULT_TOOL_CONFIG,
  estimateTokens,
  type StoredMessage,
  SYSTEM_PROMPT_PRESETS,
  storedToMessage,
  THINKING_LEVELS,
  TOOL_THINKING_LEVELS,
} from "$lib/chat-types";

describe("storedToMessage", () => {
  it("converts basic stored message", () => {
    const sm: StoredMessage = {
      id: 1,
      session_id: 1,
      role: "assistant",
      content: "Hello world",
      thinking: null,
      created_at: 1700000000,
      tool_calls: [],
    };
    const counter = { value: 0 };
    const msg = storedToMessage(sm, counter);
    expect(msg.role).toBe("assistant");
    expect(msg.content).toBe("Hello world");
    expect(msg.thinking).toBeUndefined();
    expect(msg.pending).toBe(false);
    expect(msg.id).toBe(1);
    expect(counter.value).toBe(1);
  });

  it("preserves thinking text", () => {
    const sm: StoredMessage = {
      id: 2,
      session_id: 1,
      role: "assistant",
      content: "The answer is 4.",
      thinking: "Let me think... 2+2=4",
      created_at: 1700000000,
      tool_calls: [],
    };
    const counter = { value: 0 };
    const msg = storedToMessage(sm, counter);
    expect(msg.thinking).toBe("Let me think... 2+2=4");
    expect(msg.thinkOpen).toBe(false);
  });

  it("converts tool calls", () => {
    const sm: StoredMessage = {
      id: 3,
      session_id: 1,
      role: "assistant",
      content: "",
      thinking: null,
      created_at: 1700000000,
      tool_calls: [
        {
          id: 1,
          message_id: 3,
          tool: "date",
          status: "done",
          detail: "2026-03-28",
          tool_call_id: "call_0",
          args: {},
          result: { ok: true },
          created_at: 1700000000,
        },
      ],
    };
    const counter = { value: 0 };
    const msg = storedToMessage(sm, counter);
    expect(msg.toolUses).toHaveLength(1);
    expect(msg.toolUses?.[0].tool).toBe("date");
    expect(msg.toolUses?.[0].status).toBe("done");
    expect(msg.toolUses?.[0].expanded).toBe(false);
  });

  it("increments counter across calls", () => {
    const counter = { value: 10 };
    const sm: StoredMessage = {
      id: 1,
      session_id: 1,
      role: "user",
      content: "hi",
      thinking: null,
      created_at: 0,
      tool_calls: [],
    };
    storedToMessage(sm, counter);
    storedToMessage(sm, counter);
    expect(counter.value).toBe(12);
  });
});

describe("buildUserContent", () => {
  it("returns plain string when no images", () => {
    expect(buildUserContent("hello", [])).toBe("hello");
  });

  it("returns parts array with images", () => {
    const imgs: Attachment[] = [{ dataUrl: "data:image/png;base64,abc", mimeType: "image/png", name: "test.png" }];
    const result = buildUserContent("describe this", imgs);
    expect(Array.isArray(result)).toBe(true);
    const parts = result as Array<{ type: string }>;
    expect(parts).toHaveLength(2);
    expect(parts[0].type).toBe("text");
    expect(parts[1].type).toBe("image_url");
  });

  it("omits text part when empty", () => {
    const imgs: Attachment[] = [{ dataUrl: "data:image/png;base64,abc", mimeType: "image/png", name: "test.png" }];
    const result = buildUserContent("  ", imgs);
    const parts = result as Array<{ type: string }>;
    expect(parts).toHaveLength(1);
    expect(parts[0].type).toBe("image_url");
  });
});

describe("estimateTokens", () => {
  it("estimates ~1 token per 4 chars plus overhead", () => {
    expect(estimateTokens("")).toBe(1);
    expect(estimateTokens("abcd")).toBe(2);
    expect(estimateTokens("a".repeat(100))).toBe(26);
  });
});

describe("constants", () => {
  it("THINKING_LEVELS has expected entries", () => {
    expect(THINKING_LEVELS).toHaveLength(4);
    expect(THINKING_LEVELS[0].key).toBe("minimal");
    expect(THINKING_LEVELS[3].budget).toBeNull();
  });

  it("TOOL_THINKING_LEVELS starts with chat", () => {
    expect(TOOL_THINKING_LEVELS[0].key).toBe("chat");
  });

  it("DEFAULT_TOOL_CONFIG has tools enabled", () => {
    expect(DEFAULT_TOOL_CONFIG.enabled).toBe(true);
    expect(DEFAULT_TOOL_CONFIG.date).toBe(true);
    expect(DEFAULT_TOOL_CONFIG.bash).toBe(false);
  });

  it("SYSTEM_PROMPT_PRESETS has entries with icons", () => {
    expect(SYSTEM_PROMPT_PRESETS.length).toBeGreaterThan(0);
    for (const p of SYSTEM_PROMPT_PRESETS) {
      expect(p.key).toBeTruthy();
      expect(p.icon).toBeTruthy();
      expect(p.prompt).toBeTruthy();
    }
  });
});
