// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import { deviceImage, fmtLastSeen, fuzzyMatch, museImage, openbciChannelLabel } from "$lib/devices-logic";

describe("fuzzyMatch", () => {
  it("matches empty needle", () => {
    expect(fuzzyMatch("anything", "")).toBe(true);
  });
  it("matches exact substring", () => {
    expect(fuzzyMatch("Muse-S Headband", "muse")).toBe(true);
  });
  it("matches fuzzy character sequence", () => {
    expect(fuzzyMatch("Muse-S Headband", "msh")).toBe(true);
  });
  it("rejects non-matching", () => {
    expect(fuzzyMatch("Muse", "xyz")).toBe(false);
  });
  it("is case-insensitive", () => {
    expect(fuzzyMatch("MUSE-S", "muse")).toBe(true);
  });
});

describe("museImage", () => {
  it("detects Muse S Athena by hw version", () => {
    expect(museImage("Muse-S", "p50")).toContain("athena");
  });
  it("detects Muse 2", () => {
    expect(museImage("Muse-2")).toContain("gen2");
  });
  it("detects Muse S gen1", () => {
    expect(museImage("Muse-S")).toContain("muse-s-gen1");
  });
  it("returns null for unknown device", () => {
    expect(museImage("OpenBCI Ganglion")).toBeNull();
  });
  it("detects MW75 / Neurable", () => {
    expect(museImage("MW75")).toContain("mw75");
  });
});

describe("deviceImage", () => {
  it("delegates to museImage for Muse devices", () => {
    expect(deviceImage("Muse-2")).toContain("gen2");
  });
  it("detects IDUN Guardian", () => {
    expect(deviceImage("IDUN Guardian")).toContain("idun");
  });
  it("detects Emotiv Insight", () => {
    expect(deviceImage("EMOTIV Insight")).toContain("insight");
  });
  it("detects OpenBCI Ganglion", () => {
    expect(deviceImage("OpenBCI Ganglion")).toContain("ganglion");
  });
  it("detects Mendi", () => {
    expect(deviceImage("Mendi")).toContain("mendi-headband");
  });
  it("returns null for unknown", () => {
    expect(deviceImage("Unknown Device XYZ")).toBeNull();
  });
});

describe("deviceImage — OpenBCI boards", () => {
  it("detects Cyton by name", () => {
    expect(deviceImage("Cyton-1234")).toContain("cyton");
    expect(deviceImage("OpenBCI Cyton")).toContain("cyton");
  });
  it("detects Ganglion by name", () => {
    expect(deviceImage("Ganglion-5678")).toContain("ganglion");
    expect(deviceImage("OpenBCI Ganglion")).toContain("ganglion");
  });
  it("returns null for generic OpenBCI display name", () => {
    // "OpenBCI (COM3)" doesn't contain cyton or ganglion
    expect(deviceImage("OpenBCI (COM3)")).toBeNull();
  });
});

describe("openbciChannelLabel", () => {
  it("returns 10-20 labels for standard indices", () => {
    expect(openbciChannelLabel(0)).toBe("Fp1");
    expect(openbciChannelLabel(1)).toBe("Fp2");
    expect(openbciChannelLabel(7)).toBe("O2");
  });
  it("falls back to Ch# for out-of-range", () => {
    expect(openbciChannelLabel(8)).toBe("Ch9");
    expect(openbciChannelLabel(15)).toBe("Ch16");
  });
});

describe("fmtLastSeen", () => {
  it("shows 'just now' for recent timestamps", () => {
    const now = Math.floor(Date.now() / 1000);
    expect(fmtLastSeen(now - 5)).toBe("just now");
  });
  it("shows minutes for <1h", () => {
    const now = Math.floor(Date.now() / 1000);
    expect(fmtLastSeen(now - 300)).toBe("5m ago");
  });
  it("shows hours for <1d", () => {
    const now = Math.floor(Date.now() / 1000);
    expect(fmtLastSeen(now - 7200)).toBe("2h ago");
  });
  it("shows days for >1d", () => {
    const now = Math.floor(Date.now() / 1000);
    expect(fmtLastSeen(now - 172800)).toBe("2d ago");
  });
});
