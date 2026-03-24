// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import { pickFamilyTarget, pickLlmTarget } from "$lib/onboarding-logic";
import type { LlmModelEntry } from "$lib/onboarding-logic";

function mkEntry(overrides: Partial<LlmModelEntry> = {}): LlmModelEntry {
  return {
    family_id: "test",
    family_name: "Test Model",
    quant: "Q4_K_M",
    is_mmproj: false,
    recommended: false,
    state: "available",
    size_gb: 2.0,
    ...overrides,
  };
}

describe("pickFamilyTarget", () => {
  it("returns null for empty entries", () => {
    expect(pickFamilyTarget([], "test", /test/)).toBeNull();
  });

  it("prefers Q4_K_M quantization", () => {
    const entries = [
      mkEntry({ quant: "Q8_0" }),
      mkEntry({ quant: "Q4_K_M" }),
      mkEntry({ quant: "Q4_0" }),
    ];
    expect(pickFamilyTarget(entries, "test", /test/)?.quant).toBe("Q4_K_M");
  });

  it("falls back to Q8_0 if no Q4_K_M", () => {
    const entries = [mkEntry({ quant: "Q8_0" }), mkEntry({ quant: "Q2_K" })];
    expect(pickFamilyTarget(entries, "test", /test/)?.quant).toBe("Q8_0");
  });

  it("matches by regex when family_id differs", () => {
    const entries = [mkEntry({ family_id: "other", family_name: "Qwen3.5 4B" })];
    expect(pickFamilyTarget(entries, "qwen35-4b", /qwen3\.5\s*4b/i)).not.toBeNull();
  });

  it("excludes mmproj entries", () => {
    const entries = [mkEntry({ is_mmproj: true, quant: "Q4_K_M" })];
    expect(pickFamilyTarget(entries, "test", /test/)).toBeNull();
  });
});

describe("pickLlmTarget", () => {
  it("returns null for empty catalog", () => {
    expect(pickLlmTarget([])).toBeNull();
  });

  it("prefers already-downloaded model", () => {
    const entries = [
      mkEntry({ family_id: "qwen35-4b", state: "available" }),
      mkEntry({ family_id: "other", state: "downloaded", quant: "Q2_K" }),
    ];
    expect(pickLlmTarget(entries)?.state).toBe("downloaded");
  });

  it("picks qwen35-4b when nothing downloaded", () => {
    const entries = [
      mkEntry({ family_id: "qwen35-4b", family_name: "Qwen3.5 4B", quant: "Q4_K_M" }),
      mkEntry({ family_id: "lfm25-vl-1.6b", family_name: "LFM2.5-VL 1.6B", quant: "Q8_0" }),
    ];
    expect(pickLlmTarget(entries)?.family_id).toBe("qwen35-4b");
  });

  it("falls back to recommended smallest model", () => {
    const entries = [
      mkEntry({ family_id: "big", recommended: true, size_gb: 8 }),
      mkEntry({ family_id: "small", recommended: true, size_gb: 1 }),
    ];
    expect(pickLlmTarget(entries)?.family_id).toBe("small");
  });
});
