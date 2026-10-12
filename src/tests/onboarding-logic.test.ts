// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import type { LlmModelEntry } from "$lib/settings/onboarding-logic";
import { llmCardTitle, pickFamilyTarget, pickLlmTarget } from "$lib/settings/onboarding-logic";

function mkEntry(overrides: Partial<LlmModelEntry> = {}): LlmModelEntry {
  return {
    family_id: "test",
    family_name: "Test Model",
    filename: `${overrides.family_id ?? "test"}-${overrides.quant ?? "Q4_K_M"}.gguf`,
    quant: "Q4_K_M",
    is_mmproj: false,
    recommended: false,
    state: "not_downloaded",
    size_gb: 2.0,
    progress: 0,
    ...overrides,
  };
}

describe("pickFamilyTarget", () => {
  it("returns null for empty entries", () => {
    expect(pickFamilyTarget([], "test", /test/)).toBeNull();
  });

  it("prefers Q4_K_M quantization", () => {
    const entries = [mkEntry({ quant: "Q8_0" }), mkEntry({ quant: "Q4_K_M" }), mkEntry({ quant: "Q4_0" })];
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
      mkEntry({ family_id: "qwen35-4b", state: "not_downloaded" }),
      mkEntry({ family_id: "other", state: "downloaded", quant: "Q2_K" }),
    ];
    expect(pickLlmTarget(entries)?.state).toBe("downloaded");
  });

  it("bootstraps Qwen3.5 0.8B when nothing is downloaded", () => {
    const entries = [
      mkEntry({ family_id: "qwen35-4b", family_name: "Qwen3.5 4B", quant: "Q4_K_M", size_gb: 2.6 }),
      mkEntry({ family_id: "qwen35-0.8b", family_name: "Qwen3.5 0.8B", quant: "Q8_0", size_gb: 0.9 }),
      mkEntry({ family_id: "qwen35-0.8b", family_name: "Qwen3.5 0.8B", quant: "Q4_K_M", size_gb: 0.58 }),
    ];
    const picked = pickLlmTarget(entries);
    expect(picked?.family_id).toBe("qwen35-0.8b");
    expect(picked?.quant).toBe("Q4_K_M");
  });

  it("falls back to Qwen3 0.6B when the 0.8B family is absent", () => {
    const entries = [mkEntry({ family_id: "qwen3-0.6b", family_name: "Qwen3 0.6B", quant: "Q4_K_M" })];
    expect(pickLlmTarget(entries)?.family_id).toBe("qwen3-0.6b");
  });

  // The regression behind "Start downloads Qwen3 0.6B even though I picked
  // another model through the CLI": the active selection was never consulted.
  it("honours an already-selected active model over its own default", () => {
    const chosen = mkEntry({
      family_id: "qwen35-27b",
      family_name: "Qwen3.5 27B",
      filename: "Qwen3.5-27B-Q4_K_M.gguf",
    });
    const entries = [mkEntry({ family_id: "qwen35-0.8b", family_name: "Qwen3.5 0.8B", quant: "Q4_K_M" }), chosen];
    expect(pickLlmTarget(entries, "Qwen3.5-27B-Q4_K_M.gguf")?.family_id).toBe("qwen35-27b");
  });

  it("ignores an active model that is an mmproj or absent from the catalog", () => {
    const entries = [mkEntry({ family_id: "qwen35-0.8b", family_name: "Qwen3.5 0.8B", quant: "Q4_K_M" })];
    expect(pickLlmTarget(entries, "not-in-catalog.gguf")?.family_id).toBe("qwen35-0.8b");
  });

  it("falls back to recommended smallest model", () => {
    const entries = [
      mkEntry({ family_id: "big", recommended: true, size_gb: 8 }),
      mkEntry({ family_id: "small", recommended: true, size_gb: 1 }),
    ];
    expect(pickLlmTarget(entries)?.family_id).toBe("small");
  });
});

// The card used to be titled from a static i18n string naming a model we do not
// download ("Qwen3.5 4B (Q4_K_M)") while the quant badge beside it showed the
// real pick.
describe("llmCardTitle", () => {
  it("names the model that will actually be downloaded", () => {
    const e = mkEntry({ family_name: "Qwen3.5 0.8B (Unsloth)", quant: "Q4_K_M" });
    expect(llmCardTitle(e, "fallback")).toBe("Qwen3.5 0.8B (Unsloth) (Q4_K_M)");
  });

  it("uses the fallback when there is no target yet", () => {
    expect(llmCardTitle(null, "Qwen3.5 0.8B (Q4_K_M)")).toBe("Qwen3.5 0.8B (Q4_K_M)");
  });
});
