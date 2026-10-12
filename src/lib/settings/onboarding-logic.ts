// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Onboarding model-selection logic — the single definition.
 *
 * This file used to be a COPY: `routes/onboarding/+page.svelte` carried its own
 * `pickFamilyTarget` / `pickLlmTarget`, and those were what actually ran. The
 * two drifted — the copy here still bootstrapped `lfm25-1.2b-instruct` long
 * after the page moved to Qwen3.5 0.8B — so `tests/onboarding-logic.test.ts`
 * was testing selection rules the app did not use. That is how two onboarding
 * bugs shipped unnoticed: a card titled with a model we do not download, and a
 * model chosen through the CLI being ignored in favour of the default.
 *
 * The page now imports from here. Keep it that way: these are pure functions
 * precisely so they can be tested without a Svelte component.
 */

/** Download state of a catalog entry, as the daemon reports it. */
export type DownloadState = "not_downloaded" | "downloading" | "downloaded" | "failed" | "cancelled";

export interface LlmModelEntry {
  repo?: string;
  filename: string;
  quant: string;
  size_gb: number;
  family_id: string;
  family_name: string;
  is_mmproj: boolean;
  recommended: boolean;
  state: DownloadState;
  progress: number;
  tags?: string[];
  local_path?: string | null;
}

/** Whether an entry was discovered from another app's cache (tagged "discovered"). */
export function isDiscoveredEntry(e: LlmModelEntry): boolean {
  return (e.tags ?? []).includes("discovered");
}

/**
 * Pick the best model within a given family by quantization preference.
 *
 * Priority: Q4_K_M > Q8_0 > Q4_0 > any Q4* > recommended > downloaded > first.
 */
export function pickFamilyTarget(entries: LlmModelEntry[], familyId: string, familyRe: RegExp): LlmModelEntry | null {
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
 * Priority chain:
 *  1. `activeModel` — whatever is already selected (`llm model set`, Models tab).
 *  2. Already-downloaded model (any family) — skip the download.
 *  3. Qwen3.5 0.8B — default bootstrap family.
 *  4. Qwen3 0.6B — fallback tiny family.
 *  5. Any recommended model, smallest first.
 */
export function pickLlmTarget(entries: LlmModelEntry[], activeModel = ""): LlmModelEntry | null {
  // An already-chosen model wins. Substituting onboarding's own default here is
  // how a model selected through the CLI ended up ignored, with Qwen3 0.6B
  // fetched instead.
  const active = activeModel ? entries.find((e) => !e.is_mmproj && e.filename === activeModel) : undefined;
  if (active) return active;

  // Discovered (external-app) models are handled separately as an explicit
  // reuse choice, so they don't silently satisfy the default card.
  const downloaded = entries.find((e) => !e.is_mmproj && !isDiscoveredEntry(e) && e.state === "downloaded");
  if (downloaded) return downloaded;

  return (
    pickFamilyTarget(entries, "qwen35-0.8b", /qwen3\.?5\s*0\.8b/i) ??
    pickFamilyTarget(entries, "qwen3-0.6b", /qwen3\s*0\.6b/i) ??
    entries.filter((e) => !e.is_mmproj && e.recommended).sort((a, b) => a.size_gb - b.size_gb)[0] ??
    null
  );
}

/**
 * Title for onboarding's LLM card: derived from the model we will ACTUALLY
 * fetch. A hard-coded title drifts — this card read "Qwen3.5 4B (Q4_K_M)" out
 * of the i18n bundle in every locale while the download was Qwen3.5 0.8B, with
 * the quant badge beside it already showing the truth.
 */
export function llmCardTitle(target: LlmModelEntry | null, fallback: string): string {
  if (!target) return fallback;
  return `${target.family_name || target.filename} (${target.quant})`;
}
