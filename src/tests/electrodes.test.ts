// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Tests for src/lib/data/electrodes.ts
 *
 * Verifies the electrode database, metadata maps, and filtering helpers:
 *   electrodes    — all electrode objects (built from JSON + metadata)
 *   getElectrodes — system-filter helper (10-20, 10-10, 10-5)
 *   regionColors  — BrainRegion → hex colour map
 *   regionLabels  — BrainRegion → display-name map
 */
import { describe, expect, it } from "vitest";
import {
  type BrainRegion,
  type ElectrodeSystem,
  electrodes,
  getElectrodes,
  regionColors,
  regionDescriptions,
  regionLabels,
} from "../../src/lib/data/electrodes";

// ── Known brain regions ───────────────────────────────────────────────────────

const BRAIN_REGIONS: BrainRegion[] = [
  "prefrontal",
  "frontal",
  "central",
  "temporal",
  "parietal",
  "occipital",
  "reference",
];

const SYSTEMS: ElectrodeSystem[] = ["10-20", "10-10", "10-5"];

// ── electrodes array: structural integrity ────────────────────────────────────

describe("electrodes array", () => {
  it("is non-empty", () => {
    expect(electrodes.length).toBeGreaterThan(0);
  });

  it("has more than 60 electrodes (10-5 system is large)", () => {
    // The 10-5 system has ~345 sites; even 10-20 has 21
    expect(electrodes.length).toBeGreaterThan(60);
  });

  it("every electrode has a non-empty name", () => {
    for (const e of electrodes) {
      expect(typeof e.name).toBe("string");
      expect(e.name.length).toBeGreaterThan(0);
    }
  });

  it("electrode names are unique", () => {
    const names = electrodes.map((e) => e.name);
    const unique = new Set(names);
    expect(unique.size).toBe(names.length);
  });

  it("every electrode has a valid region", () => {
    const regionSet = new Set<string>(BRAIN_REGIONS);
    for (const e of electrodes) {
      expect(regionSet.has(e.region), `'${e.name}' has unknown region '${e.region}'`).toBe(true);
    }
  });

  it("every electrode has a 3-element pos array", () => {
    for (const e of electrodes) {
      expect(e.pos).toHaveLength(3);
      for (const coord of e.pos) {
        expect(typeof coord).toBe("number");
        expect(Number.isFinite(coord)).toBe(true);
      }
    }
  });

  it("every electrode has a non-empty lobe string", () => {
    for (const e of electrodes) {
      expect(typeof e.lobe).toBe("string");
      expect(e.lobe.length).toBeGreaterThan(0);
    }
  });

  it("every electrode has a non-empty function string", () => {
    for (const e of electrodes) {
      expect(typeof e.function).toBe("string");
      expect(e.function.length).toBeGreaterThan(0);
    }
  });

  it("every electrode has a non-empty signals string", () => {
    for (const e of electrodes) {
      expect(typeof e.signals).toBe("string");
      expect(e.signals.length).toBeGreaterThan(0);
    }
  });

  it("every electrode belongs to at least one system", () => {
    const systemSet = new Set<string>(SYSTEMS);
    for (const e of electrodes) {
      expect(e.systems.length).toBeGreaterThan(0);
      for (const sys of e.systems) {
        expect(systemSet.has(sys), `'${e.name}' has unknown system '${sys}'`).toBe(true);
      }
    }
  });
});

// ── Muse electrodes ───────────────────────────────────────────────────────────

describe("Muse electrode metadata", () => {
  const MUSE_NAMES = ["TP9", "AF7", "AF8", "TP10"] as const;

  it("all 4 Muse channel electrodes exist in the array", () => {
    const names = new Set(electrodes.map((e) => e.name));
    for (const n of MUSE_NAMES) {
      expect(names.has(n), `Missing Muse electrode: ${n}`).toBe(true);
    }
  });

  it("each Muse electrode has muse: true", () => {
    for (const name of MUSE_NAMES) {
      // biome-ignore lint/style/noNonNullAssertion: known Muse electrode always exists in fixture
      const e = electrodes.find((e) => e.name === name)!;
      expect(e.muse, `${name}.muse should be true`).toBe(true);
    }
  });

  it("each Muse electrode has a non-empty museRole string", () => {
    for (const name of MUSE_NAMES) {
      // biome-ignore lint/style/noNonNullAssertion: known Muse electrode always exists in fixture
      const e = electrodes.find((e) => e.name === name)!;
      expect(typeof e.museRole).toBe("string");
      expect(e.museRole?.length).toBeGreaterThan(0);
    }
  });

  it("non-Muse electrodes do not have muse: true", () => {
    const museSet = new Set<string>(MUSE_NAMES);
    const wronglyFlagged = electrodes.filter((e) => e.muse === true && !museSet.has(e.name));
    expect(wronglyFlagged.map((e) => e.name)).toHaveLength(0);
  });
});

// ── getElectrodes ─────────────────────────────────────────────────────────────

describe("getElectrodes(system)", () => {
  it("10-20 returns a non-empty subset", () => {
    const result = getElectrodes("10-20");
    expect(result.length).toBeGreaterThan(0);
  });

  it("10-10 has more electrodes than 10-20", () => {
    expect(getElectrodes("10-10").length).toBeGreaterThan(getElectrodes("10-20").length);
  });

  it("10-5 has more electrodes than 10-10", () => {
    expect(getElectrodes("10-5").length).toBeGreaterThan(getElectrodes("10-10").length);
  });

  it("every returned electrode actually belongs to the requested system", () => {
    for (const sys of SYSTEMS) {
      for (const e of getElectrodes(sys)) {
        expect(e.systems).toContain(sys);
      }
    }
  });

  it("10-20 includes the classic midline electrodes (Fz, Cz, Pz, Oz)", () => {
    const names = new Set(getElectrodes("10-20").map((e) => e.name));
    for (const n of ["Fz", "Cz", "Pz", "Oz"]) {
      expect(names.has(n), `10-20 missing ${n}`).toBe(true);
    }
  });

  it("10-10 includes all 4 Muse electrodes (TP9/AF7/AF8/TP10 are 10-10 sites)", () => {
    const names = new Set(getElectrodes("10-10").map((e) => e.name));
    for (const n of ["TP9", "AF7", "AF8", "TP10"]) {
      expect(names.has(n), `10-10 missing Muse electrode ${n}`).toBe(true);
    }
  });

  it("10-5 also includes all 4 Muse electrodes", () => {
    const names = new Set(getElectrodes("10-5").map((e) => e.name));
    for (const n of ["TP9", "AF7", "AF8", "TP10"]) {
      expect(names.has(n), `10-5 missing Muse electrode ${n}`).toBe(true);
    }
  });

  it("getElectrodes results are a subset of the full electrodes array", () => {
    const allNames = new Set(electrodes.map((e) => e.name));
    for (const sys of SYSTEMS) {
      for (const e of getElectrodes(sys)) {
        expect(allNames.has(e.name)).toBe(true);
      }
    }
  });
});

// ── regionColors ─────────────────────────────────────────────────────────────

describe("regionColors", () => {
  it("has an entry for every BrainRegion", () => {
    for (const region of BRAIN_REGIONS) {
      expect(regionColors).toHaveProperty(region);
    }
  });

  it("all colour values are valid hex strings", () => {
    for (const [, colour] of Object.entries(regionColors)) {
      expect(colour).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });

  it("has no extra keys beyond the known regions", () => {
    const regionSet = new Set<string>(BRAIN_REGIONS);
    for (const key of Object.keys(regionColors)) {
      expect(regionSet.has(key), `unexpected regionColors key: '${key}'`).toBe(true);
    }
  });
});

// ── regionLabels ─────────────────────────────────────────────────────────────

describe("regionLabels", () => {
  it("has an entry for every BrainRegion", () => {
    for (const region of BRAIN_REGIONS) {
      expect(regionLabels).toHaveProperty(region);
    }
  });

  it("all label values are non-empty strings", () => {
    for (const [, label] of Object.entries(regionLabels)) {
      expect(typeof label).toBe("string");
      expect(label.length).toBeGreaterThan(0);
    }
  });

  it("has no extra keys beyond the known regions", () => {
    const regionSet = new Set<string>(BRAIN_REGIONS);
    for (const key of Object.keys(regionLabels)) {
      expect(regionSet.has(key), `unexpected regionLabels key: '${key}'`).toBe(true);
    }
  });
});

// ── regionDescriptions ────────────────────────────────────────────────────────

describe("regionDescriptions", () => {
  it("has an entry for every BrainRegion", () => {
    for (const region of BRAIN_REGIONS) {
      expect(regionDescriptions).toHaveProperty(region);
    }
  });

  it("all description values are non-empty strings (at least 20 chars)", () => {
    for (const [, desc] of Object.entries(regionDescriptions)) {
      expect(typeof desc).toBe("string");
      expect(desc.length).toBeGreaterThanOrEqual(20);
    }
  });
});

// ── Region coverage in data ───────────────────────────────────────────────────

describe("electrode region distribution", () => {
  it("every known BrainRegion is used by at least one electrode", () => {
    const usedRegions = new Set(electrodes.map((e) => e.region));
    for (const region of BRAIN_REGIONS) {
      expect(usedRegions.has(region), `No electrode assigned to region '${region}'`).toBe(true);
    }
  });

  it("occipital region contains O1, Oz, O2", () => {
    const occipital = electrodes.filter((e) => e.region === "occipital").map((e) => e.name);
    const occSet = new Set(occipital);
    for (const n of ["O1", "Oz", "O2"]) {
      expect(occSet.has(n), `Occipital region missing ${n}`).toBe(true);
    }
  });

  it("central region contains Cz", () => {
    const central = electrodes.filter((e) => e.region === "central").map((e) => e.name);
    expect(central).toContain("Cz");
  });
});
