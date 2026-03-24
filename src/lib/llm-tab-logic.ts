// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * LlmTab pure logic — extracted from LlmTab.svelte.
 *
 * Hardware-fit badge styling and label resolution.
 */

export type FitLevel = "perfect" | "good" | "marginal" | "too_tight" | string;

/** Tailwind classes for the hardware-fit badge background/text. */
export function fitBadgeClass(level: FitLevel): string {
  switch (level) {
    case "perfect":
      return "bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border-emerald-500/30";
    case "good":
      return "bg-sky-500/15 text-sky-700 dark:text-sky-400 border-sky-500/30";
    case "marginal":
      return "bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30";
    case "too_tight":
      return "bg-red-500/15 text-red-700 dark:text-red-400 border-red-500/30";
    default:
      return "bg-slate-500/10 text-slate-500 border-slate-500/20";
  }
}

/** Emoji icon for the hardware-fit level. */
export function fitBadgeIcon(level: FitLevel): string {
  switch (level) {
    case "perfect":
      return "\u2728"; // ✨
    case "good":
      return "\u2705"; // ✅
    case "marginal":
      return "\u26A0\uFE0F"; // ⚠️
    case "too_tight":
      return "\u274C"; // ❌
    default:
      return "\u2754"; // ❔
  }
}

/** Human-readable label for the hardware-fit level. */
export function fitBadgeLabel(level: FitLevel): string {
  switch (level) {
    case "perfect":
      return "Perfect fit";
    case "good":
      return "Good fit";
    case "marginal":
      return "Marginal";
    case "too_tight":
      return "Too large";
    default:
      return "Unknown";
  }
}
