// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Pure helper functions for the LLM settings tab.
// Extracted so they can be unit-tested without mounting the Svelte component.

// ── Types ────────────────────────────────────────────────────────────────────

export type DownloadState = "not_downloaded" | "downloading" | "paused" | "downloaded" | "failed" | "cancelled";

export interface LlmModelEntry {
  repo: string;
  filename: string;
  quant: string;
  size_gb: number;
  description: string;
  family_id: string;
  family_name: string;
  family_desc: string;
  tags: string[];
  is_mmproj: boolean;
  recommended: boolean;
  advanced: boolean;
  params_b: number;
  max_context_length: number;
  shard_files: string[];
  local_path: string | null;
  state: DownloadState;
  status_msg: string | null;
  progress: number;
}

export interface LlmCatalog {
  entries: LlmModelEntry[];
  active_model: string;
  active_mmproj: string;
}

export interface ModelFamily {
  id: string;
  name: string;
  desc: string;
  tags: string[];
  vendors: string[];
  entries: LlmModelEntry[];
  mmproj: LlmModelEntry[];
  recommended: LlmModelEntry | undefined;
  downloaded: LlmModelEntry[];
}

// ── Helpers ──────────────────────────────────────────────────────────────────

export function vendorLabel(repo: string): string {
  const owner = repo.split("/")[0] ?? repo;
  const labels: Record<string, string> = {
    bartowski: "Bartowski",
    unsloth: "Unsloth",
    HauhauCS: "HauhauCS",
  };
  return labels[owner] ?? owner;
}

export function familySizeRank(tags: string[]): number {
  if (tags.includes("tiny")) return 0;
  if (tags.includes("small")) return 1;
  if (tags.includes("medium")) return 2;
  if (tags.includes("large")) return 3;
  return 4;
}

export function familyPrimarySize(entries: LlmModelEntry[]): number {
  const recommended = entries.find((entry) => entry.recommended);
  if (recommended) return recommended.size_gb;

  const standard = entries.find((entry) => !entry.advanced);
  if (standard) return standard.size_gb;

  return entries.reduce((smallest, entry) => Math.min(smallest, entry.size_gb), Number.POSITIVE_INFINITY);
}

const QUANT_ORDER = [
  "Q4_K_M",
  "Q4_0",
  "Q4_K_S",
  "Q4_K_L",
  "Q4_1",
  "Q5_K_M",
  "Q5_K_S",
  "Q5_K_L",
  "Q6_K",
  "Q6_K_L",
  "Q8_0",
  "IQ4_XS",
  "IQ4_NL",
  "Q3_K_M",
  "Q3_K_L",
  "Q3_K_XL",
  "Q3_K_S",
  "IQ3_M",
  "IQ3_XS",
  "IQ3_XXS",
  "Q2_K",
  "Q2_K_L",
  "IQ2_M",
  "IQ2_S",
  "IQ2_XS",
  "IQ2_XXS",
  "BF16",
  "F16",
  "F32",
];

export function quantRank(quant: string): number {
  const index = QUANT_ORDER.indexOf(quant.toUpperCase());
  return index === -1 ? QUANT_ORDER.length : index;
}

/**
 * Sort comparator for model entries within a family.
 *
 * Pin order: active → downloading → downloaded → recommended → standard → advanced.
 * Then by quant rank, then by file size, then lexicographic.
 */
export function compareModelEntries(a: LlmModelEntry, b: LlmModelEntry, activeModel: string): number {
  const pinScore = (e: LlmModelEntry): number =>
    e.filename === activeModel
      ? 0
      : e.state === "downloading"
        ? 1
        : e.state === "downloaded"
          ? 2
          : e.recommended
            ? 3
            : !e.advanced
              ? 4
              : 5;

  const aPin = pinScore(a);
  const bPin = pinScore(b);
  if (aPin !== bPin) return aPin - bPin;

  const aQ = quantRank(a.quant);
  const bQ = quantRank(b.quant);
  if (aQ !== bQ) return aQ - bQ;

  if (a.size_gb !== b.size_gb) return a.size_gb - b.size_gb;
  return a.quant.localeCompare(b.quant) || a.filename.localeCompare(b.filename);
}

export function runModeLabel(mode: string): string {
  switch (mode) {
    case "gpu":
      return "GPU";
    case "moe":
      return "MoE offload";
    case "cpu_gpu":
      return "CPU + GPU";
    case "cpu":
      return "CPU";
    default:
      return mode;
  }
}

export function tagLabel(tag: string): string {
  const MAP: Record<string, string> = {
    chat: "Chat",
    reasoning: "Reasoning",
    coding: "Coding",
    vision: "Vision",
    multimodal: "Multimodal",
    tiny: "Tiny",
    small: "Small",
    medium: "Medium",
    large: "Large",
  };
  return MAP[tag] ?? tag;
}

export function tagColor(tag: string): string {
  switch (tag) {
    case "chat":
      return "bg-primary/10 text-primary border-primary/20";
    case "reasoning":
      return "bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20";
    case "coding":
      return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20";
    case "vision":
    case "multimodal":
      return "bg-amber-500/10 text-amber-700 dark:text-amber-400 border-amber-500/20";
    default:
      return "bg-slate-500/10 text-slate-500 border-slate-500/20";
  }
}

// ── Family grouping ──────────────────────────────────────────────────────────

/**
 * Build `ModelFamily[]` from a flat catalog entry list.
 * Filters out families that have no non-mmproj entries.
 * Sorted by name → primary size → size-rank tag → id.
 */
export function buildFamilies(entries: LlmModelEntry[]): ModelFamily[] {
  const map = new Map<string, ModelFamily>();
  for (const e of entries) {
    if (!map.has(e.family_id)) {
      map.set(e.family_id, {
        id: e.family_id,
        name: e.family_name || e.family_id,
        desc: e.family_desc || "",
        tags: [],
        vendors: [],
        entries: [],
        mmproj: [],
        recommended: undefined,
        downloaded: [],
      });
    }
    // biome-ignore lint/style/noNonNullAssertion: family always set in the loop above
    const f = map.get(e.family_id)!;
    for (const tag of e.tags) {
      if (!f.tags.includes(tag)) f.tags.push(tag);
    }
    const vendor = vendorLabel(e.repo);
    if (!f.vendors.includes(vendor)) f.vendors.push(vendor);
    if (e.is_mmproj || e.filename.toLowerCase().includes("mmproj")) {
      f.mmproj.push(e);
    } else {
      f.entries.push(e);
      if (e.recommended && !f.recommended) f.recommended = e;
      if (e.state === "downloaded") f.downloaded.push(e);
    }
  }
  return Array.from(map.values())
    .filter((f) => f.entries.length > 0)
    .sort((a, b) => {
      const byName = a.name.localeCompare(b.name);
      if (byName !== 0) return byName;
      const aSize = familyPrimarySize(a.entries);
      const bSize = familyPrimarySize(b.entries);
      if (aSize !== bSize) return aSize - bSize;
      const aTagSize = familySizeRank(a.tags);
      const bTagSize = familySizeRank(b.tags);
      if (aTagSize !== bTagSize) return aTagSize - bTagSize;
      return a.id.localeCompare(b.id);
    });
}

/**
 * Split sorted entries into `primary` (always visible) and `extra`
 * (hidden behind "Show all quants").
 */
export function splitEntryGroups(
  sortedEntries: LlmModelEntry[],
  activeModel: string,
): { primary: LlmModelEntry[]; extra: LlmModelEntry[] } {
  const pinned = new Set<string>();
  for (const entry of sortedEntries) {
    if (
      entry.filename === activeModel ||
      entry.recommended ||
      entry.state === "downloaded" ||
      entry.state === "downloading" ||
      !entry.advanced
    ) {
      pinned.add(entry.filename);
    }
  }
  return {
    primary: sortedEntries.filter((e) => pinned.has(e.filename)),
    extra: sortedEntries.filter((e) => !pinned.has(e.filename)),
  };
}

/**
 * Option label for the family `<select>` dropdown.
 */
export function familyOptionLabel(f: ModelFamily, activeModel: string): string {
  const active = f.entries.some((e) => e.filename === activeModel);
  const dlCount = f.downloaded.length;
  const loading = f.entries.some((e) => e.state === "downloading");
  let prefix = "";
  if (active) prefix = "✓ ";
  else if (loading) prefix = "⬇ ";
  let suffix = "";
  if (dlCount > 0 && !active) suffix = ` (${dlCount} downloaded)`;
  return `${prefix}${f.name}${suffix}`;
}

/**
 * Choose which family to auto-select when the tab opens.
 *
 * Priority: currently selected (if still valid) → active model's family
 * → first family with downloads → first family.
 */
export function autoSelectFamily(families: ModelFamily[], catalog: LlmCatalog, currentSelection: string): string {
  if (families.length === 0) return "";
  if (currentSelection && families.some((f) => f.id === currentSelection)) {
    return currentSelection;
  }
  const activeEntry = catalog.entries.find((e) => !e.is_mmproj && e.filename === catalog.active_model);
  if (activeEntry) return activeEntry.family_id;
  const dlFamily = families.find((f) => f.downloaded.length > 0);
  return (dlFamily ?? families[0]).id;
}

/**
 * Returns true when at least one catalog entry is actively downloading.
 * Used to decide whether the poll timer should refresh the catalog.
 */
export function hasActiveDownloads(catalog: LlmCatalog): boolean {
  return catalog.entries.some((e) => e.state === "downloading");
}
