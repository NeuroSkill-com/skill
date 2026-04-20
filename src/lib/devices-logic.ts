// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * DevicesTab pure logic — extracted from DevicesTab.svelte.
 *
 * Fuzzy matching, device image resolution, channel labeling, and formatting.
 */

// ── Virtual device detection ────────────────────────────────────────────────────

/**
 * Return true when `dev` is a simulated/virtual device rather than real hardware.
 *
 * Detection rules (any match → virtual):
 *  - name contains "virtual" (case-insensitive) — covers "SkillVirtualEEG",
 *    "Virtual EEG", etc.
 *  - id   contains "virtual" (case-insensitive) — covers daemon id "virtual-eeg"
 */
export function isVirtualDevice(dev: { id: string; name: string }): boolean {
  const n = dev.name.toLowerCase();
  const id = dev.id.toLowerCase();
  return n.includes("virtual") || id.includes("virtual");
}

/**
 * Stable-sort a device list so real hardware always appears above virtual
 * devices.  Within each group the original relative order is preserved.
 */
export function sortDevicesRealFirst<T extends { id: string; name: string }>(devs: T[]): T[] {
  return [...devs].sort((a, b) => {
    const av = isVirtualDevice(a) ? 1 : 0;
    const bv = isVirtualDevice(b) ? 1 : 0;
    return av - bv; // real (0) before virtual (1)
  });
}

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
export function museImage(name: string, _hw?: string | null): string | null {
  const n = name.toLowerCase();
  // Athena (Muse S gen 2) advertises as "MuseS-XXXX" (no space before S).
  // hardware_version is not set by the Muse adapter so we rely on the name only.
  const isAthena = n.includes("muses");
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
  if (n.includes("awear") || n.includes("luca")) return "/devices/awear-eeg.png";
  if (n.includes("atu") || n.includes("attentivu")) return "/devices/attentivu-glasses.png";
  if (n.includes("quick-32") || n.includes("q32")) return "/devices/cgx-quick-32r.png";
  if (n.includes("quick-20r-v1") || n.includes("q20r-v1")) return "/devices/cgx-quick-20r-v1.png";
  if (n.includes("quick-20") || n.includes("q20") || n.includes("cognionics") || n.includes("cgx"))
    return "/devices/cgx-quick-20r.png";
  if (n.includes("quick-8") || n.includes("q8")) return "/devices/cgx-quick-8r.png";
  if (n.includes("aim-2") || n.includes("aim2")) return "/devices/cgx-aim-2.png";
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

// ── Types for merge/preferred helpers ─────────────────────────────────────────

export interface DeviceBase {
  id: string;
  name: string;
  last_seen: number;
  last_rssi: number;
  is_paired: boolean;
  is_preferred: boolean;
  transport?: string;
}

export interface PairedInfo {
  id: string;
  name: string;
  last_seen?: number;
}

// ── Merge & preferred helpers ─────────────────────────────────────────────────

/**
 * Infer transport type from device ID prefix.
 */
function inferTransport(id: string): "ble" | "usb_serial" | "wifi" | undefined {
  if (id.startsWith("ble:")) return "ble";
  if (id.startsWith("usb:")) return "usb_serial";
  if (id.startsWith("wifi:") || (id.includes(":") && id.includes("."))) return "wifi";
  return undefined;
}

/**
 * Merge paired devices from status into a base device list.
 * Devices already in `base` are preserved as-is (including is_preferred).
 * Devices in `paired` but missing from `base` are added with is_preferred=false.
 */
export function mergePairedIntoDevices<T extends DeviceBase>(base: T[], paired: PairedInfo[]): T[] {
  const out = [...base];
  const byId = new Set(out.map((d) => d.id));
  for (const p of paired) {
    if (!byId.has(p.id)) {
      out.push({
        id: p.id,
        name: p.name,
        last_seen: p.last_seen ?? 0,
        last_rssi: 0,
        is_paired: true,
        is_preferred: false,
        transport: inferTransport(p.id),
      } as T);
      byId.add(p.id);
    }
  }
  return out;
}

/**
 * Apply a preferred device selection: sets is_preferred=true on the device
 * matching `targetId`, and is_preferred=false on all others.
 * If `targetId` is empty, clears all preferred flags.
 */
export function applyPreferred<T extends DeviceBase>(devices: T[], targetId: string): T[] {
  return devices.map((d) => ({ ...d, is_preferred: targetId !== "" && d.id === targetId }));
}
