// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import { fitBadgeClass, fitBadgeIcon, fitBadgeLabel } from "$lib/llm-tab-logic";

describe("fitBadgeClass", () => {
  it("returns emerald for perfect", () => {
    expect(fitBadgeClass("perfect")).toContain("emerald");
  });
  it("returns sky for good", () => {
    expect(fitBadgeClass("good")).toContain("sky");
  });
  it("returns amber for marginal", () => {
    expect(fitBadgeClass("marginal")).toContain("amber");
  });
  it("returns red for too_tight", () => {
    expect(fitBadgeClass("too_tight")).toContain("red");
  });
  it("returns slate for unknown", () => {
    expect(fitBadgeClass("unknown")).toContain("slate");
  });
});

describe("fitBadgeIcon", () => {
  it("returns sparkle for perfect", () => {
    expect(fitBadgeIcon("perfect")).toBe("\u2728");
  });
  it("returns check for good", () => {
    expect(fitBadgeIcon("good")).toBe("\u2705");
  });
  it("returns X for too_tight", () => {
    expect(fitBadgeIcon("too_tight")).toBe("\u274C");
  });
});

describe("fitBadgeLabel", () => {
  it("returns human-readable labels", () => {
    expect(fitBadgeLabel("perfect")).toBe("Perfect fit");
    expect(fitBadgeLabel("good")).toBe("Good fit");
    expect(fitBadgeLabel("marginal")).toBe("Marginal");
    expect(fitBadgeLabel("too_tight")).toBe("Too large");
    expect(fitBadgeLabel("other")).toBe("Unknown");
  });
});
