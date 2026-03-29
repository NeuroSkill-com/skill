// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * DevicesTab pure logic — extracted from DevicesTab.svelte.
 *
 * Fuzzy matching, device image resolution, channel labeling, and formatting.
 */

// ── Fuzzy search ──────────────────────────────────────────────────────────────

/** Fuzzy substring + character-sequence match. */
export function fuzzyMatch(haystack: string, needle: string): boolean {
  if (!needle) return true;
  const h = haystack.toLowerCase();
  const n = needle.toLowerCase();
  if (h.includes(n)) return true;
  let hIdx = 0;
  for (let i = 0; i < n.length; i++) {
    hIdx = h.indexOf(n[i], hIdx);
    if (hIdx === -1) return false;
    hIdx++;
  }
  return true;
}

// ── Device image resolution ───────────────────────────────────────────────────

/** Resolve the product image path for a Muse-family device. */
export function museImage(name: string, hw?: string | null): string | null {
  const n = name.toLowerCase();
  const isAthena = hw === "p50" || n.includes("muses");
  if (isAthena) return "/devices/muse-s-athena.jpg";
  if (n.includes("muse-s") || n.includes("muse s")) return "/devices/muse-s-gen1.jpg";
  if (n.includes("muse-2") || n.includes("muse2") || n.includes("muse 2")) return "/devices/muse-gen2.jpg";
  if (n.includes("muse")) return "/devices/muse-gen1.jpg";
  if (n.includes("mw75") || n.includes("neurable")) return "/devices/muse-mw75.jpg";
  return null;
}

/** Resolve the product image path for any supported device. */
export function deviceImage(name: string, hw?: string | null): string | null {
  const muse = museImage(name, hw);
  if (muse) return muse;

  const n = name.toLowerCase();
  if (n.includes("idun") || n.includes("guardian") || n.startsWith("ige")) return "/devices/idun-guardian.png";
  if (n.includes("insight")) return "/devices/emotiv-insight.webp";
  if (n.includes("flex")) return "/devices/emotiv-flex-saline.webp";
  if (n.includes("mn8")) return "/devices/emotiv-mn8.webp";
  if (n.includes("epoc")) return "/devices/emotiv-epoc-x.webp";
  if (n.includes("ganglion")) return "/devices/openbci-ganglion.jpg";
  if (n.includes("cyton")) return "/devices/openbci-cyton.jpg";
  if (n.includes("hermes")) return "/devices/hermes.jpg";
  if (n.includes("mendi")) return "/devices/mendi-headband.png";
  if (n.includes("atu") || n.includes("attentivu")) return "/devices/attentivu-glasses.png";
  return null;
}

// ── OpenBCI channel labels ────────────────────────────────────────────────────

const OPENBCI_DEFAULT_LABELS = ["Fp1", "Fp2", "C3", "C4", "T5", "T6", "O1", "O2"];

/** Return the default 10-20 label for an OpenBCI channel index. */
export function openbciChannelLabel(i: number): string {
  return OPENBCI_DEFAULT_LABELS[i] ?? `Ch${i + 1}`;
}

// ── Formatting ────────────────────────────────────────────────────────────────

/** Format a Unix timestamp as a relative "last seen" string. */
export function fmtLastSeen(ts: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - ts;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}
