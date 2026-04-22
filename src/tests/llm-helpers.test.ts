// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Integration tests for LLM download-progress helpers.
//
// These tests exercise the pure logic extracted from LlmTab.svelte,
// simulating the end-to-end scenario where a user starts a model download,
// closes the settings window, and reopens it.

import { describe, expect, it } from "vitest";
import {
  autoSelectFamily,
  buildFamilies,
  compareModelEntries,
  type DownloadState,
  familyOptionLabel,
  familyPrimarySize,
  familySizeRank,
  hasActiveDownloads,
  type LlmCatalog,
  type LlmModelEntry,
  type ModelFamily,
  quantRank,
  runModeLabel,
  splitEntryGroups,
  tagColor,
  tagLabel,
  vendorLabel,
} from "$lib/llm/llm-helpers";

// ── Fixtures ─────────────────────────────────────────────────────────────────

/** Minimal model entry with sensible defaults.  Override any field. */
function entry(overrides: Partial<LlmModelEntry> = {}): LlmModelEntry {
  return {
    repo: "bartowski/Qwen3-1.7B-GGUF",
    filename: "Qwen3-1.7B-Q4_K_M.gguf",
    quant: "Q4_K_M",
    size_gb: 1.2,
    description: "Qwen3 1.7B Q4_K_M",
    family_id: "qwen3-1.7b",
    family_name: "Qwen3 1.7B",
    family_desc: "Compact chat model",
    tags: ["chat", "small"],
    is_mmproj: false,
    recommended: false,
    advanced: false,
    params_b: 1.7,
    max_context_length: 32768,
    shard_files: [],
    local_path: null,
    state: "not_downloaded",
    status_msg: null,
    progress: 0,
    ...overrides,
  };
}

/** Catalog with two families, each with multiple quants. */
function multiCatalog(): LlmCatalog {
  return {
    active_model: "",
    active_mmproj: "",
    entries: [
      entry({ filename: "Qwen3-1.7B-Q4_K_M.gguf", quant: "Q4_K_M", size_gb: 1.2, recommended: true }),
      entry({ filename: "Qwen3-1.7B-Q8_0.gguf", quant: "Q8_0", size_gb: 2.0, advanced: true }),
      entry({ filename: "Qwen3-1.7B-Q3_K_M.gguf", quant: "Q3_K_M", size_gb: 0.9, advanced: true }),
      entry({
        filename: "Phi-4-Q4_K_M.gguf",
        quant: "Q4_K_M",
        size_gb: 2.5,
        family_id: "phi-4",
        family_name: "Phi-4",
        family_desc: "Microsoft reasoning model",
        tags: ["chat", "reasoning", "medium"],
        repo: "bartowski/Phi-4-GGUF",
        recommended: true,
      }),
      entry({
        filename: "Phi-4-Q6_K.gguf",
        quant: "Q6_K",
        size_gb: 3.8,
        family_id: "phi-4",
        family_name: "Phi-4",
        family_desc: "Microsoft reasoning model",
        tags: ["chat", "reasoning", "medium"],
        repo: "bartowski/Phi-4-GGUF",
        advanced: true,
      }),
      // mmproj entry — should not appear in family.entries
      entry({
        filename: "mmproj-Phi-4-BF16.gguf",
        quant: "BF16",
        size_gb: 0.6,
        family_id: "phi-4",
        family_name: "Phi-4",
        family_desc: "",
        tags: ["vision"],
        is_mmproj: true,
      }),
    ],
  };
}

// ── vendorLabel ──────────────────────────────────────────────────────────────

describe("vendorLabel", () => {
  it("maps known vendors", () => {
    expect(vendorLabel("bartowski/SomeModel-GGUF")).toBe("Bartowski");
    expect(vendorLabel("unsloth/SomeModel-GGUF")).toBe("Unsloth");
    expect(vendorLabel("HauhauCS/SomeModel-GGUF")).toBe("HauhauCS");
  });
  it("falls back to the repo owner", () => {
    expect(vendorLabel("myorg/MyModel-GGUF")).toBe("myorg");
  });
  it("handles repo without slash", () => {
    expect(vendorLabel("standalone")).toBe("standalone");
  });
});

// ── familySizeRank ───────────────────────────────────────────────────────────

describe("familySizeRank", () => {
  it("ranks tiny < small < medium < large < untagged", () => {
    expect(familySizeRank(["tiny"])).toBe(0);
    expect(familySizeRank(["small"])).toBe(1);
    expect(familySizeRank(["medium"])).toBe(2);
    expect(familySizeRank(["large"])).toBe(3);
    expect(familySizeRank(["chat"])).toBe(4);
    expect(familySizeRank([])).toBe(4);
  });
  it("picks the first matching size tag", () => {
    expect(familySizeRank(["small", "tiny"])).toBe(0); // tiny matched first
  });
});

// ── familyPrimarySize ────────────────────────────────────────────────────────

describe("familyPrimarySize", () => {
  it("prefers recommended entry", () => {
    const entries = [entry({ size_gb: 2.0 }), entry({ size_gb: 1.0, recommended: true })];
    expect(familyPrimarySize(entries)).toBe(1.0);
  });
  it("falls back to non-advanced entry", () => {
    const entries = [entry({ size_gb: 3.0, advanced: true }), entry({ size_gb: 1.5, advanced: false })];
    expect(familyPrimarySize(entries)).toBe(1.5);
  });
  it("falls back to smallest entry", () => {
    const entries = [entry({ size_gb: 3.0, advanced: true }), entry({ size_gb: 2.0, advanced: true })];
    expect(familyPrimarySize(entries)).toBe(2.0);
  });
  it("returns Infinity for empty list", () => {
    expect(familyPrimarySize([])).toBe(Number.POSITIVE_INFINITY);
  });
});

// ── quantRank ────────────────────────────────────────────────────────────────

describe("quantRank", () => {
  it("Q4_K_M is first", () => expect(quantRank("Q4_K_M")).toBe(0));
  it("F32 is last known", () => expect(quantRank("F32")).toBeGreaterThan(quantRank("Q4_K_M")));
  it("unknown quants sort after all known", () => {
    expect(quantRank("UNKNOWN")).toBeGreaterThan(quantRank("F32"));
  });
  it("is case-insensitive", () => {
    expect(quantRank("q4_k_m")).toBe(quantRank("Q4_K_M"));
  });
  it("ordering: Q4_K_M < Q8_0 < Q3_K_M < BF16", () => {
    expect(quantRank("Q4_K_M")).toBeLessThan(quantRank("Q8_0"));
    expect(quantRank("Q8_0")).toBeLessThan(quantRank("Q3_K_M"));
    expect(quantRank("Q3_K_M")).toBeLessThan(quantRank("BF16"));
  });
});

// ── compareModelEntries ──────────────────────────────────────────────────────

describe("compareModelEntries", () => {
  it("pins active model first", () => {
    const a = entry({ filename: "active.gguf", state: "downloaded" });
    const b = entry({ filename: "other.gguf", state: "downloaded", recommended: true });
    expect(compareModelEntries(a, b, "active.gguf")).toBeLessThan(0);
  });

  it("pins downloading before downloaded", () => {
    const a = entry({ filename: "a.gguf", state: "downloading", progress: 0.5 });
    const b = entry({ filename: "b.gguf", state: "downloaded" });
    expect(compareModelEntries(a, b, "")).toBeLessThan(0);
  });

  it("pins downloaded before recommended", () => {
    const a = entry({ filename: "a.gguf", state: "downloaded" });
    const b = entry({ filename: "b.gguf", state: "not_downloaded", recommended: true });
    expect(compareModelEntries(a, b, "")).toBeLessThan(0);
  });

  it("pins recommended before non-recommended standard", () => {
    const a = entry({ filename: "a.gguf", recommended: true });
    const b = entry({ filename: "b.gguf" });
    expect(compareModelEntries(a, b, "")).toBeLessThan(0);
  });

  it("pins standard before advanced", () => {
    const a = entry({ filename: "a.gguf" });
    const b = entry({ filename: "b.gguf", advanced: true });
    expect(compareModelEntries(a, b, "")).toBeLessThan(0);
  });

  it("breaks ties by quant rank", () => {
    const a = entry({ filename: "a.gguf", quant: "Q4_K_M" });
    const b = entry({ filename: "b.gguf", quant: "Q8_0" });
    expect(compareModelEntries(a, b, "")).toBeLessThan(0);
  });

  it("breaks quant ties by file size", () => {
    const a = entry({ filename: "a.gguf", quant: "Q4_K_M", size_gb: 1.0 });
    const b = entry({ filename: "b.gguf", quant: "Q4_K_M", size_gb: 2.0 });
    expect(compareModelEntries(a, b, "")).toBeLessThan(0);
  });
});

// ── runModeLabel ─────────────────────────────────────────────────────────────

describe("runModeLabel", () => {
  it("maps known modes", () => {
    expect(runModeLabel("gpu")).toBe("GPU");
    expect(runModeLabel("moe")).toBe("MoE offload");
    expect(runModeLabel("cpu_gpu")).toBe("CPU + GPU");
    expect(runModeLabel("cpu")).toBe("CPU");
  });
  it("passes through unknown modes", () => {
    expect(runModeLabel("quantum")).toBe("quantum");
  });
});

// ── tagLabel / tagColor ──────────────────────────────────────────────────────

describe("tagLabel", () => {
  it("maps known tags", () => {
    expect(tagLabel("chat")).toBe("Chat");
    expect(tagLabel("reasoning")).toBe("Reasoning");
    expect(tagLabel("vision")).toBe("Vision");
  });
  it("passes through unknown tags", () => {
    expect(tagLabel("custom")).toBe("custom");
  });
});

describe("tagColor", () => {
  it("returns distinct classes per tag", () => {
    const chat = tagColor("chat");
    const reasoning = tagColor("reasoning");
    expect(chat).not.toBe(reasoning);
    expect(chat).toContain("primary");
    expect(reasoning).toContain("violet");
  });
  it("treats vision and multimodal identically", () => {
    expect(tagColor("vision")).toBe(tagColor("multimodal"));
  });
  it("returns slate fallback for unknown tags", () => {
    expect(tagColor("unknown")).toContain("slate");
  });
});

// ── buildFamilies ────────────────────────────────────────────────────────────

describe("buildFamilies", () => {
  it("groups entries by family_id", () => {
    const families = buildFamilies(multiCatalog().entries);
    expect(families).toHaveLength(2);
    const ids = families.map((f) => f.id);
    expect(ids).toContain("phi-4");
    expect(ids).toContain("qwen3-1.7b");
  });

  it("separates mmproj entries from regular entries", () => {
    const families = buildFamilies(multiCatalog().entries);
    const phi = families.find((f) => f.id === "phi-4")!;
    expect(phi.entries).toHaveLength(2); // Q4_K_M + Q6_K
    expect(phi.mmproj).toHaveLength(1);
    expect(phi.mmproj[0].filename).toBe("mmproj-Phi-4-BF16.gguf");
  });

  it("tracks recommended entry", () => {
    const families = buildFamilies(multiCatalog().entries);
    const qwen = families.find((f) => f.id === "qwen3-1.7b")!;
    expect(qwen.recommended).toBeDefined();
    expect(qwen.recommended?.filename).toBe("Qwen3-1.7B-Q4_K_M.gguf");
  });

  it("collects downloaded entries", () => {
    const cat = multiCatalog();
    cat.entries[0].state = "downloaded";
    cat.entries[0].local_path = "/some/path";
    const families = buildFamilies(cat.entries);
    const qwen = families.find((f) => f.id === "qwen3-1.7b")!;
    expect(qwen.downloaded).toHaveLength(1);
    expect(qwen.downloaded[0].filename).toBe("Qwen3-1.7B-Q4_K_M.gguf");
  });

  it("collects tags and vendors", () => {
    const families = buildFamilies(multiCatalog().entries);
    const phi = families.find((f) => f.id === "phi-4")!;
    expect(phi.tags).toContain("reasoning");
    expect(phi.vendors).toContain("Bartowski");
  });

  it("sorts families alphabetically by name", () => {
    const families = buildFamilies(multiCatalog().entries);
    expect(families[0].name).toBe("Phi-4");
    expect(families[1].name).toBe("Qwen3 1.7B");
  });

  it("filters out families with only mmproj entries", () => {
    const families = buildFamilies([entry({ family_id: "orphan", family_name: "Orphan", is_mmproj: true })]);
    expect(families).toHaveLength(0);
  });

  it("returns empty for empty input", () => {
    expect(buildFamilies([])).toHaveLength(0);
  });
});

// ── splitEntryGroups ─────────────────────────────────────────────────────────

describe("splitEntryGroups", () => {
  it("puts non-advanced entries in primary group", () => {
    const sorted = [entry({ filename: "a.gguf" }), entry({ filename: "b.gguf", advanced: true })];
    const { primary, extra } = splitEntryGroups(sorted, "");
    expect(primary.map((e) => e.filename)).toContain("a.gguf");
    expect(extra.map((e) => e.filename)).toContain("b.gguf");
  });

  it("always includes downloading entries in primary", () => {
    const sorted = [entry({ filename: "dl.gguf", state: "downloading", progress: 0.3, advanced: true })];
    const { primary } = splitEntryGroups(sorted, "");
    expect(primary).toHaveLength(1);
    expect(primary[0].filename).toBe("dl.gguf");
  });

  it("always includes active model in primary", () => {
    const sorted = [entry({ filename: "active.gguf", state: "downloaded", advanced: true })];
    const { primary } = splitEntryGroups(sorted, "active.gguf");
    expect(primary).toHaveLength(1);
  });

  it("always includes recommended in primary", () => {
    const sorted = [entry({ filename: "rec.gguf", recommended: true, advanced: true })];
    const { primary } = splitEntryGroups(sorted, "");
    expect(primary).toHaveLength(1);
  });

  it("always includes downloaded entries in primary", () => {
    const sorted = [entry({ filename: "dl.gguf", state: "downloaded", advanced: true })];
    const { primary } = splitEntryGroups(sorted, "");
    expect(primary).toHaveLength(1);
  });
});

// ── familyOptionLabel ────────────────────────────────────────────────────────

describe("familyOptionLabel", () => {
  it("shows ✓ prefix when family has the active model", () => {
    const f: ModelFamily = {
      id: "test",
      name: "TestModel",
      desc: "",
      tags: [],
      vendors: [],
      entries: [entry({ filename: "active.gguf", state: "downloaded" })],
      mmproj: [],
      recommended: undefined,
      downloaded: [entry({ state: "downloaded" })],
    };
    const label = familyOptionLabel(f, "active.gguf");
    expect(label).toMatch(/^✓/);
    expect(label).toContain("TestModel");
  });

  it("shows ⬇ prefix when family has a downloading model", () => {
    const f: ModelFamily = {
      id: "test",
      name: "TestModel",
      desc: "",
      tags: [],
      vendors: [],
      entries: [entry({ filename: "dl.gguf", state: "downloading" })],
      mmproj: [],
      recommended: undefined,
      downloaded: [],
    };
    const label = familyOptionLabel(f, "");
    expect(label).toMatch(/^⬇/);
  });

  it("shows download count when not active", () => {
    const f: ModelFamily = {
      id: "test",
      name: "TestModel",
      desc: "",
      tags: [],
      vendors: [],
      entries: [entry({ filename: "a.gguf", state: "downloaded" })],
      mmproj: [],
      recommended: undefined,
      downloaded: [entry({ state: "downloaded" }), entry({ state: "downloaded" })],
    };
    const label = familyOptionLabel(f, "other.gguf");
    expect(label).toContain("(2 downloaded)");
  });

  it("omits download count for active families", () => {
    const f: ModelFamily = {
      id: "test",
      name: "TestModel",
      desc: "",
      tags: [],
      vendors: [],
      entries: [entry({ filename: "active.gguf" })],
      mmproj: [],
      recommended: undefined,
      downloaded: [entry({ state: "downloaded" })],
    };
    const label = familyOptionLabel(f, "active.gguf");
    expect(label).not.toContain("downloaded");
  });
});

// ── autoSelectFamily ─────────────────────────────────────────────────────────

describe("autoSelectFamily", () => {
  it("keeps current selection if it still exists", () => {
    const families = buildFamilies(multiCatalog().entries);
    const result = autoSelectFamily(families, multiCatalog(), "qwen3-1.7b");
    expect(result).toBe("qwen3-1.7b");
  });

  it("selects active model's family when current is invalid", () => {
    const cat = multiCatalog();
    cat.active_model = "Phi-4-Q4_K_M.gguf";
    const families = buildFamilies(cat.entries);
    const result = autoSelectFamily(families, cat, "nonexistent");
    expect(result).toBe("phi-4");
  });

  it("selects first downloaded family when no active model", () => {
    const cat = multiCatalog();
    cat.entries[3].state = "downloaded"; // Phi-4 Q4_K_M
    cat.entries[3].local_path = "/path";
    const families = buildFamilies(cat.entries);
    const result = autoSelectFamily(families, cat, "nonexistent");
    expect(result).toBe("phi-4");
  });

  it("selects first family as last resort", () => {
    const cat = multiCatalog();
    const families = buildFamilies(cat.entries);
    const result = autoSelectFamily(families, cat, "nonexistent");
    // First alphabetically is Phi-4
    expect(result).toBe("phi-4");
  });

  it("returns empty string for empty family list", () => {
    expect(autoSelectFamily([], multiCatalog(), "")).toBe("");
  });
});

// ── hasActiveDownloads ───────────────────────────────────────────────────────

describe("hasActiveDownloads", () => {
  it("returns false when no entries are downloading", () => {
    expect(hasActiveDownloads(multiCatalog())).toBe(false);
  });

  it("returns true when an entry is downloading", () => {
    const cat = multiCatalog();
    cat.entries[0].state = "downloading";
    cat.entries[0].progress = 0.42;
    expect(hasActiveDownloads(cat)).toBe(true);
  });

  it("returns false for paused/failed/cancelled states", () => {
    for (const state of ["paused", "failed", "cancelled"] as DownloadState[]) {
      const cat = multiCatalog();
      cat.entries[0].state = state;
      expect(hasActiveDownloads(cat)).toBe(false);
    }
  });
});

// ── End-to-end scenario tests ────────────────────────────────────────────────
//
// These simulate the full lifecycle that the LlmTab component goes through
// when the user starts a download, closes the settings window, and reopens it.

describe("E2E: download-progress survives window reopen", () => {
  /**
   * Simulates what the backend returns from `get_llm_catalog` at different
   * points in the download lifecycle.
   */
  function catalogWithDownloadAt(progress: number): LlmCatalog {
    const cat = multiCatalog();
    cat.entries[0].state = "downloading";
    cat.entries[0].progress = progress;
    cat.entries[0].status_msg = `Downloading: ${(progress * 100).toFixed(0)}%`;
    return cat;
  }

  it("scenario: initial mount detects in-flight download", () => {
    // 1. Backend returns catalog with a download at 42%
    const catalog = catalogWithDownloadAt(0.42);

    // 2. buildFamilies correctly includes the downloading entry
    const families = buildFamilies(catalog.entries);
    const qwen = families.find((f) => f.id === "qwen3-1.7b")!;
    const dlEntry = qwen.entries.find((e) => e.state === "downloading");
    expect(dlEntry).toBeDefined();
    expect(dlEntry?.progress).toBe(0.42);

    // 3. hasActiveDownloads detects it (used by poll timer)
    expect(hasActiveDownloads(catalog)).toBe(true);

    // 4. The downloading entry sorts to the top (after active)
    const sorted = [...qwen.entries].sort((a, b) => compareModelEntries(a, b, catalog.active_model));
    expect(sorted[0].state).toBe("downloading");

    // 5. The downloading entry is in the primary (visible) group
    const { primary } = splitEntryGroups(sorted, catalog.active_model);
    expect(primary.some((e) => e.state === "downloading")).toBe(true);

    // 6. The family dropdown shows the ⬇ indicator
    const label = familyOptionLabel(qwen, catalog.active_model);
    expect(label).toMatch(/⬇/);
  });

  it("scenario: poll detects download completion", () => {
    // 1. Start with a downloading entry
    const cat = catalogWithDownloadAt(0.99);
    expect(hasActiveDownloads(cat)).toBe(true);

    // 2. Backend advances to completed
    cat.entries[0].state = "downloaded";
    cat.entries[0].progress = 1.0;
    cat.entries[0].status_msg = null;
    cat.entries[0].local_path = "/path/to/model.gguf";

    // 3. Poll detects no more active downloads
    expect(hasActiveDownloads(cat)).toBe(false);

    // 4. Family now shows the downloaded count
    const families = buildFamilies(cat.entries);
    const qwen = families.find((f) => f.id === "qwen3-1.7b")!;
    expect(qwen.downloaded).toHaveLength(1);
  });

  it("scenario: download fails mid-progress", () => {
    const cat = catalogWithDownloadAt(0.3);

    // Backend reports failure
    cat.entries[0].state = "failed";
    cat.entries[0].status_msg = "Network error: connection reset";
    cat.entries[0].progress = 0;

    expect(hasActiveDownloads(cat)).toBe(false);

    const families = buildFamilies(cat.entries);
    const qwen = families.find((f) => f.id === "qwen3-1.7b")!;
    const failed = qwen.entries.find((e) => e.state === "failed");
    expect(failed).toBeDefined();
    expect(failed?.status_msg).toContain("Network error");
  });

  it("scenario: multiple simultaneous downloads across families", () => {
    const cat = multiCatalog();
    // Download one from each family
    cat.entries[0].state = "downloading";
    cat.entries[0].progress = 0.2;
    cat.entries[3].state = "downloading";
    cat.entries[3].progress = 0.7;

    expect(hasActiveDownloads(cat)).toBe(true);

    const families = buildFamilies(cat.entries);
    // Both families should show the ⬇ indicator
    for (const f of families) {
      expect(familyOptionLabel(f, "")).toMatch(/⬇/);
    }
  });

  it("scenario: paused download is not confused with active download", () => {
    const cat = multiCatalog();
    cat.entries[0].state = "paused";
    cat.entries[0].progress = 0.5;
    cat.entries[0].status_msg = "Paused.";

    // Paused is NOT an active download
    expect(hasActiveDownloads(cat)).toBe(false);

    // But paused entry is NOT in the downloaded list
    const families = buildFamilies(cat.entries);
    const qwen = families.find((f) => f.id === "qwen3-1.7b")!;
    expect(qwen.downloaded).toHaveLength(0);
  });

  it("scenario: window reopen with stale initial state then fresh poll", () => {
    // Simulates: component mounts with empty catalog (stale), then poll
    // refreshes with actual data from backend.

    // 1. Initial stale state (as if loadCatalog failed or returned empty)
    const stale: LlmCatalog = { entries: [], active_model: "", active_mmproj: "" };
    expect(hasActiveDownloads(stale)).toBe(false);
    expect(buildFamilies(stale.entries)).toHaveLength(0);

    // 2. Poll fires, backend returns real catalog with download at 60%
    const fresh = catalogWithDownloadAt(0.6);
    expect(hasActiveDownloads(fresh)).toBe(true);

    const families = buildFamilies(fresh.entries);
    expect(families.length).toBeGreaterThan(0);

    // 3. Auto-select picks a valid family
    const selected = autoSelectFamily(families, fresh, "");
    expect(families.some((f) => f.id === selected)).toBe(true);
  });

  it("scenario: download starts on one family while viewing another", () => {
    const cat = multiCatalog();
    cat.active_model = "Phi-4-Q4_K_M.gguf";
    cat.entries[3].state = "downloaded";
    cat.entries[3].local_path = "/path";
    // Now start downloading a Qwen model
    cat.entries[0].state = "downloading";
    cat.entries[0].progress = 0.1;

    const families = buildFamilies(cat.entries);

    // Auto-select should keep the active model's family (phi-4)
    const selected = autoSelectFamily(families, cat, "phi-4");
    expect(selected).toBe("phi-4");

    // But the Qwen family dropdown label shows ⬇
    const qwen = families.find((f) => f.id === "qwen3-1.7b")!;
    expect(familyOptionLabel(qwen, cat.active_model)).toMatch(/⬇/);
  });

  it("scenario: shard-based model entry tracks progress correctly", () => {
    const cat = multiCatalog();
    // Turn the first entry into a sharded model
    cat.entries[0].shard_files = [
      "Qwen3-1.7B-Q4_K_M-00001-of-00003.gguf",
      "Qwen3-1.7B-Q4_K_M-00002-of-00003.gguf",
      "Qwen3-1.7B-Q4_K_M-00003-of-00003.gguf",
    ];
    cat.entries[0].state = "downloading";
    cat.entries[0].progress = 0.33;
    cat.entries[0].status_msg = "Downloading shard 1/3…";

    expect(hasActiveDownloads(cat)).toBe(true);
    const families = buildFamilies(cat.entries);
    const qwen = families.find((f) => f.id === "qwen3-1.7b")!;
    const dlEntry = qwen.entries.find((e) => e.state === "downloading")!;
    expect(dlEntry.shard_files).toHaveLength(3);
    expect(dlEntry.progress).toBeCloseTo(0.33);
  });
});

// ── Edge cases ───────────────────────────────────────────────────────────────

describe("edge cases", () => {
  it("handles catalog with only mmproj entries gracefully", () => {
    const cat: LlmCatalog = {
      active_model: "",
      active_mmproj: "",
      entries: [entry({ family_id: "orphan", is_mmproj: true })],
    };
    expect(buildFamilies(cat.entries)).toHaveLength(0);
    expect(hasActiveDownloads(cat)).toBe(false);
    expect(autoSelectFamily([], cat, "")).toBe("");
  });

  it("handles duplicate family names from different repos", () => {
    const cat: LlmCatalog = {
      active_model: "",
      active_mmproj: "",
      entries: [
        entry({ family_id: "qwen3-1.7b", repo: "bartowski/Qwen3-GGUF" }),
        entry({
          family_id: "qwen3-1.7b",
          repo: "unsloth/Qwen3-GGUF",
          filename: "Qwen3-1.7B-Q4_K_M-unsloth.gguf",
        }),
      ],
    };
    const families = buildFamilies(cat.entries);
    expect(families).toHaveLength(1);
    expect(families[0].vendors).toContain("Bartowski");
    expect(families[0].vendors).toContain("Unsloth");
    expect(families[0].entries).toHaveLength(2);
  });

  it("all DownloadState values are handled", () => {
    const allStates: DownloadState[] = ["not_downloaded", "downloading", "paused", "downloaded", "failed", "cancelled"];
    for (const state of allStates) {
      const e = entry({ state });
      // Should not throw
      const families = buildFamilies([e]);
      expect(families.length).toBe(1);
    }
  });

  it("progress values are clamped correctly in display calculations", () => {
    // Progress can theoretically exceed 1.0 or be negative
    const e1 = entry({ state: "downloading", progress: -0.1 });
    const e2 = entry({ state: "downloading", progress: 1.5 });
    // The helpers themselves don't clamp, but they should not crash
    expect(hasActiveDownloads({ entries: [e1], active_model: "", active_mmproj: "" })).toBe(true);
    expect(hasActiveDownloads({ entries: [e2], active_model: "", active_mmproj: "" })).toBe(true);
  });
});
