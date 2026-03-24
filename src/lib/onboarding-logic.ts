// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Onboarding page business logic — extracted from routes/onboarding/+page.svelte.
 *
 * Pure functions for model selection during the onboarding flow.
 */

export interface LlmModelEntry {
  family_id: string;
  family_name: string;
  quant: string;
  is_mmproj: boolean;
  recommended: boolean;
  state: string;
  size_gb: number;
}

/**
 * Pick the best model within a given family by quantization preference.
 *
 * Priority: Q4_K_M > Q8_0 > Q4_0 > any Q4* > recommended > downloaded > first.
 */
export function pickFamilyTarget(
  entries: LlmModelEntry[],
  familyId: string,
  familyRe: RegExp,
): LlmModelEntry | null {
  const family = entries.filter((e) => !e.is_mmproj && (e.family_id === familyId || familyRe.test(e.family_name)));
  if (!family.length) return null;
  const byQuant = (q: string) => family.find((e) => e.quant.toUpperCase() === q);
  return (
    byQuant("Q4_K_M") ??
    byQuant("Q8_0") ??
    byQuant("Q4_0") ??
    family.find((e) => e.quant.toUpperCase().startsWith("Q4")) ??
    family.find((e) => e.recommended) ??
    family.find((e) => e.state === "downloaded") ??
    family[0]
  );
}

/**
 * Pick the default LLM to download during onboarding.
 *
 * Priority:
 *  1. Already-downloaded model (any family) — skip download.
 *  2. Qwen3.5 4B Q4_K_M — best quality for the size.
 *  3. LFM2.5-VL 1.6B Q8_0 — ultra-compact fallback.
 *  4. Any recommended model, smallest first.
 */
export function pickLlmTarget(entries: LlmModelEntry[]): LlmModelEntry | null {
  const downloaded = entries.find((e) => !e.is_mmproj && e.state === "downloaded");
  if (downloaded) return downloaded;

  return (
    pickFamilyTarget(entries, "qwen35-4b", /qwen3\.5\s*4b/i) ??
    pickFamilyTarget(entries, "lfm25-vl-1.6b", /lfm2\.5.*1\.6b/i) ??
    entries
      .filter((e) => !e.is_mmproj && e.recommended)
      .sort((a, b) => a.size_gb - b.size_gb)[0] ??
    null
  );
}
