// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import { applyPreferred, mergePairedIntoDevices, type DeviceBase, type PairedInfo } from "$lib/devices-logic";

// ── Helpers ──────────────────────────────────────────────────────────────────

function makeDevice(id: string, name: string, overrides?: Partial<DeviceBase>): DeviceBase {
  return {
    id,
    name,
    last_seen: 1000,
    last_rssi: -60,
    is_paired: true,
    is_preferred: false,
    ...overrides,
  };
}

// ── applyPreferred ───────────────────────────────────────────────────────────

describe("applyPreferred", () => {
  const muse = makeDevice("ble:muse-123", "MuseS-F921");
  const awear = makeDevice("ble:awear-456", "AWEAR-E04A8471");
  const devices = [muse, awear];

  it("sets is_preferred=true on matching device", () => {
    const result = applyPreferred(devices, "ble:muse-123");
    expect(result[0].is_preferred).toBe(true);
    expect(result[1].is_preferred).toBe(false);
  });

  it("clears all preferred when targetId is empty", () => {
    const withPref = [makeDevice("a", "A", { is_preferred: true }), makeDevice("b", "B")];
    const result = applyPreferred(withPref, "");
    expect(result.every((d) => !d.is_preferred)).toBe(true);
  });

  it("sets only one device as preferred", () => {
    const three = [makeDevice("a", "A"), makeDevice("b", "B"), makeDevice("c", "C")];
    const result = applyPreferred(three, "b");
    expect(result.filter((d) => d.is_preferred)).toHaveLength(1);
    expect(result.find((d) => d.id === "b")?.is_preferred).toBe(true);
  });

  it("does not mutate input array", () => {
    const original = [makeDevice("a", "A")];
    const originalRef = original[0];
    applyPreferred(original, "a");
    expect(originalRef.is_preferred).toBe(false);
  });

  it("preserves all other fields", () => {
    const dev = makeDevice("a", "A", { last_rssi: -42, is_paired: true, transport: "ble" });
    const [result] = applyPreferred([dev], "a");
    expect(result.last_rssi).toBe(-42);
    expect(result.is_paired).toBe(true);
    expect(result.transport).toBe("ble");
    expect(result.name).toBe("A");
  });

  it("handles non-existent targetId gracefully", () => {
    const result = applyPreferred(devices, "ble:nonexistent");
    expect(result.every((d) => !d.is_preferred)).toBe(true);
  });

  it("handles empty device list", () => {
    expect(applyPreferred([], "anything")).toEqual([]);
  });
});

// ── mergePairedIntoDevices ───────────────────────────────────────────────────

describe("mergePairedIntoDevices", () => {
  it("adds paired device not in base list", () => {
    const base: DeviceBase[] = [makeDevice("ble:a", "A")];
    const paired: PairedInfo[] = [{ id: "ble:b", name: "B" }];
    const result = mergePairedIntoDevices(base, paired);
    expect(result).toHaveLength(2);
    expect(result[1].id).toBe("ble:b");
    expect(result[1].is_paired).toBe(true);
  });

  it("does NOT duplicate device already in base", () => {
    const base = [makeDevice("ble:a", "A")];
    const paired: PairedInfo[] = [{ id: "ble:a", name: "A" }];
    const result = mergePairedIntoDevices(base, paired);
    expect(result).toHaveLength(1);
  });

  it("preserves is_preferred on base devices", () => {
    const base = [makeDevice("ble:a", "A", { is_preferred: true })];
    const paired: PairedInfo[] = [{ id: "ble:b", name: "B" }];
    const result = mergePairedIntoDevices(base, paired);
    expect(result[0].is_preferred).toBe(true);
    expect(result[1].is_preferred).toBe(false);
  });

  it("new paired devices default to is_preferred=false", () => {
    const base: DeviceBase[] = [];
    const paired: PairedInfo[] = [{ id: "ble:a", name: "A" }];
    const result = mergePairedIntoDevices(base, paired);
    expect(result[0].is_preferred).toBe(false);
  });

  it("does not mutate base array", () => {
    const base = [makeDevice("ble:a", "A")];
    const paired: PairedInfo[] = [{ id: "ble:b", name: "B" }];
    mergePairedIntoDevices(base, paired);
    expect(base).toHaveLength(1);
  });
});

// ── Integration: applyPreferred survives mergePairedIntoDevices ──────────────

describe("setPreferred end-to-end flow", () => {
  it("preferred flag survives merge with paired status", () => {
    // Simulate: user clicks "Set default" on Muse
    const devices = [
      makeDevice("ble:muse", "Muse"),
      makeDevice("ble:awear", "AWEAR"),
    ];
    const pairedFromStatus: PairedInfo[] = [
      { id: "ble:muse", name: "Muse" },
      { id: "ble:awear", name: "AWEAR" },
    ];

    // Step 1: applyPreferred (optimistic update)
    const afterPreferred = applyPreferred(devices, "ble:muse");
    expect(afterPreferred[0].is_preferred).toBe(true);

    // Step 2: mergePairedIntoDevices (re-derive)
    const afterMerge = mergePairedIntoDevices(afterPreferred, pairedFromStatus);
    // is_preferred must survive the merge
    expect(afterMerge.find((d) => d.id === "ble:muse")?.is_preferred).toBe(true);
    expect(afterMerge.find((d) => d.id === "ble:awear")?.is_preferred).toBe(false);
  });

  it("toggling preferred off clears flag through merge", () => {
    const devices = [makeDevice("ble:muse", "Muse", { is_preferred: true })];
    const paired: PairedInfo[] = [{ id: "ble:muse", name: "Muse" }];

    const afterClear = applyPreferred(devices, "");
    const afterMerge = mergePairedIntoDevices(afterClear, paired);
    expect(afterMerge[0].is_preferred).toBe(false);
  });

  it("preferred flag not lost when offline device is synthesized", () => {
    // Muse is discovered (in devices), AWEAR is offline (only in paired)
    const devices = [makeDevice("ble:muse", "Muse", { is_preferred: true })];
    const paired: PairedInfo[] = [
      { id: "ble:muse", name: "Muse" },
      { id: "ble:awear", name: "AWEAR" },
    ];

    const result = mergePairedIntoDevices(devices, paired);
    expect(result.find((d) => d.id === "ble:muse")?.is_preferred).toBe(true);
    expect(result.find((d) => d.id === "ble:awear")?.is_preferred).toBe(false);
  });

  it("poll response with is_preferred set is preserved through merge", () => {
    // Simulate: daemon returns devices with is_preferred already set
    const polledDevices = [
      makeDevice("ble:muse", "Muse", { is_preferred: true }),
      makeDevice("ble:awear", "AWEAR", { is_preferred: false }),
    ];
    const paired: PairedInfo[] = [
      { id: "ble:muse", name: "Muse" },
      { id: "ble:awear", name: "AWEAR" },
    ];

    const result = mergePairedIntoDevices(polledDevices, paired);
    expect(result.find((d) => d.id === "ble:muse")?.is_preferred).toBe(true);
  });
});
