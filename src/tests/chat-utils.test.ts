// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
import { describe, it, expect } from "vitest";
import {
  stripToolCallFences,
  cleanAssistantLeadIn,
  cleanLeadInForDisplay,
  detectToolDanger,
  parseAssistantOutput,
} from "$lib/chat-utils";

// ── stripToolCallFences ─────────────────────────────────────────────────────

describe("stripToolCallFences", () => {
  it("removes a fenced tool-call JSON block", () => {
    const input = 'Hello\n```json\n{"name":"bash","parameters":{"command":"ls"}}\n```\nWorld';
    const result = stripToolCallFences(input);
    expect(result).toContain("Hello");
    expect(result).toContain("World");
    expect(result).not.toContain("bash");
  });

  it("keeps non-tool-call fenced blocks", () => {
    const input = '```json\n{"key":"value"}\n```';
    const result = stripToolCallFences(input);
    expect(result).toContain("key");
  });

  it("removes [TOOL_CALL]...[/TOOL_CALL] blocks", () => {
    const input = "Before [TOOL_CALL]{...}[/TOOL_CALL] After";
    const result = stripToolCallFences(input);
    expect(result).toContain("Before");
    expect(result).toContain("After");
    expect(result).not.toContain("TOOL_CALL");
  });

  it("removes incomplete [TOOL_C prefix at end of stream", () => {
    const input = "Some text [TOOL_CALL]{partial";
    const result = stripToolCallFences(input);
    expect(result).not.toContain("TOOL_C");
  });

  it("returns empty string for empty input", () => {
    expect(stripToolCallFences("")).toBe("");
  });

  it("passes through plain text unchanged", () => {
    const plain = "Hello, how can I help you today?";
    expect(stripToolCallFences(plain)).toBe(plain);
  });

  it("removes array-style tool calls", () => {
    const input = '```json\n[{"name":"date","parameters":{}}]\n```';
    const result = stripToolCallFences(input);
    expect(result).not.toContain("date");
  });

  it("removes dict-style multi-tool calls", () => {
    const input = '```json\n{"date":{},"location":{}}\n```';
    const result = stripToolCallFences(input);
    expect(result).not.toContain("date");
    expect(result).not.toContain("location");
  });
});

// ── cleanAssistantLeadIn ────────────────────────────────────────────────────

describe("cleanAssistantLeadIn", () => {
  it("strips code fence markers", () => {
    const input = "```python\nprint('hello')\n```";
    const result = cleanAssistantLeadIn(input);
    expect(result).not.toContain("```");
  });

  it("filters lines that are just 'json' or 'copy'", () => {
    const input = "result:\njson\nactual content";
    const result = cleanAssistantLeadIn(input);
    expect(result).not.toMatch(/^\s*json\s*$/m);
    expect(result).toContain("actual content");
  });

  it("trims whitespace", () => {
    expect(cleanAssistantLeadIn("  hello  ")).toBe("hello");
  });

  it("returns empty for empty input", () => {
    expect(cleanAssistantLeadIn("")).toBe("");
  });
});

// ── cleanLeadInForDisplay ───────────────────────────────────────────────────

describe("cleanLeadInForDisplay", () => {
  it("returns empty for whitespace-only input", () => {
    expect(cleanLeadInForDisplay("   ", false)).toBe("");
  });

  it("strips tool fences even without active tool uses", () => {
    const input = '```json\n{"name":"bash","parameters":{}}\n```\nHello';
    const result = cleanLeadInForDisplay(input, false);
    expect(result).toContain("Hello");
    expect(result).not.toContain("bash");
  });

  it("aggressively strips incomplete fences when tools active", () => {
    const input = "Some text\n```python\nprint('incomplete";
    const result = cleanLeadInForDisplay(input, true);
    expect(result).not.toContain("```");
  });
});

// ── detectToolDanger ────────────────────────────────────────────────────────

describe("detectToolDanger", () => {
  it("flags dangerous bash rm command", () => {
    const result = detectToolDanger({ tool: "bash", args: { command: "rm -rf /tmp/foo" } });
    expect(result).toBe("chat.tools.dangerBash");
  });

  it("flags sudo commands", () => {
    const result = detectToolDanger({ tool: "bash", args: { command: "sudo apt install foo" } });
    expect(result).toBe("chat.tools.dangerBash");
  });

  it("returns null for safe bash commands", () => {
    const result = detectToolDanger({ tool: "bash", args: { command: "ls -la" } });
    expect(result).toBeNull();
  });

  it("flags write_file to /etc/", () => {
    const result = detectToolDanger({ tool: "write_file", args: { path: "/etc/hosts" } });
    expect(result).toBe("chat.tools.dangerPath");
  });

  it("returns null for write_file to safe path", () => {
    const result = detectToolDanger({ tool: "write_file", args: { path: "/home/user/test.txt" } });
    expect(result).toBeNull();
  });

  it("returns null for non-dangerous tools", () => {
    expect(detectToolDanger({ tool: "date" })).toBeNull();
    expect(detectToolDanger({ tool: "location" })).toBeNull();
    expect(detectToolDanger({ tool: "web_search", args: { query: "rust programming" } })).toBeNull();
  });

  it("returns null when bash has no command arg", () => {
    expect(detectToolDanger({ tool: "bash" })).toBeNull();
    expect(detectToolDanger({ tool: "bash", args: {} })).toBeNull();
  });
});

// ── parseAssistantOutput ────────────────────────────────────────────────────

describe("parseAssistantOutput", () => {
  it("returns content only when no <think> tags", () => {
    const result = parseAssistantOutput("Hello, world!");
    expect(result.content).toBe("Hello, world!");
    expect(result.thinking).toBe("");
    expect(result.leadIn).toBe("");
  });

  it("extracts thinking from <think> block", () => {
    const input = "<think>Let me reason about this.</think>The answer is 42.";
    const result = parseAssistantOutput(input);
    expect(result.thinking).toBe("Let me reason about this.");
    expect(result.content).toContain("42");
  });

  it("merges multiple think blocks", () => {
    const input = "<think>First thought.</think>Interim text.<think>Second thought.</think>Final answer.";
    const result = parseAssistantOutput(input);
    expect(result.thinking).toContain("First thought.");
    expect(result.thinking).toContain("Second thought.");
    expect(result.content).toContain("Final answer");
  });

  it("handles unclosed think tag (still streaming)", () => {
    const input = "<think>Still thinking about this...";
    const result = parseAssistantOutput(input);
    expect(result.thinking).toContain("Still thinking");
    expect(result.content).toBe("");
  });

  it("returns empty for empty input", () => {
    const result = parseAssistantOutput("");
    expect(result.content).toBe("");
    expect(result.thinking).toBe("");
    expect(result.leadIn).toBe("");
  });
});
