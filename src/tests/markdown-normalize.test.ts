import { describe, expect, it } from "vitest";
import { marked } from "marked";

import { normalizeMarkdown } from "$lib/markdown-normalize";

describe("markdown normalization", () => {
  it("trims space after opening strong delimiter", () => {
    expect(normalizeMarkdown("** Current Time:**")).toBe("**Current Time:**");
  });

  it("trims space before closing strong delimiter", () => {
    expect(normalizeMarkdown("**Current Time: **")).toBe("**Current Time:**");
  });

  it("forces bold html when closing strong is punctuation-adjacent", () => {
    expect(normalizeMarkdown("**Current Time:**4:55 AM")).toBe("<strong>Current Time:</strong>4:55 AM");
  });

  it("forces italic html when closing em is punctuation-adjacent", () => {
    expect(normalizeMarkdown("*Note:*value")).toBe("<em>Note:</em>value");
  });

  it("preserves normal markdown when flanking rules are already satisfied", () => {
    expect(normalizeMarkdown("**Current Time:** 4:55 AM")).toBe("**Current Time:** 4:55 AM");
  });

  it("does not rewrite inline code spans", () => {
    expect(normalizeMarkdown("Use `**Current Time:**4:55` exactly.")).toBe("Use `**Current Time:**4:55` exactly.");
  });

  it("does not rewrite fenced code blocks", () => {
    const raw = "```md\n**Current Time:**4:55\n```";
    expect(normalizeMarkdown(raw)).toBe(raw);
  });

  it("strips a leading orphaned json fence preamble", () => {
    const raw = [
      "```json",
      "{",
      '"daboth pieces of information:',
      "",
      "**Time Information:**",
      "I'll call the date tool.",
    ].join("\n");

    expect(normalizeMarkdown(raw)).toBe([
      "**Time Information:**",
      "I'll call the date tool.",
    ].join("\n"));
  });

  it("preserves closed json fences", () => {
    const raw = [
      "```json",
      '{"time":"4:55"}',
      "```",
    ].join("\n");

    expect(normalizeMarkdown(raw)).toBe(raw);
  });

  it("renders repaired strong markdown through marked", () => {
    const html = marked.parse(normalizeMarkdown("**Current Time:**4:55 AM"), { breaks: true, gfm: true }) as string;
    expect(html).toContain("<strong>Current Time:</strong>4:55 AM");
  });
});