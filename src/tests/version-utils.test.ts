// SPDX-License-Identifier: GPL-3.0-only
//
// Unit tests for scripts/version-utils.mjs — the SemVer-with-RC helpers
// shared by bump.js and release.js.
import { describe, expect, it } from "vitest";

import { baseVersion, bumpVersion, isRc, parseVersion, validateVersion } from "../../scripts/version-utils.mjs";

describe("parseVersion", () => {
  it("parses stable versions", () => {
    expect(parseVersion("0.5.1")).toEqual({ major: 0, minor: 5, patch: 1, rc: null });
    expect(parseVersion("12.34.56")).toEqual({ major: 12, minor: 34, patch: 56, rc: null });
  });

  it("parses RC versions", () => {
    expect(parseVersion("0.5.1-rc.1")).toEqual({ major: 0, minor: 5, patch: 1, rc: 1 });
    expect(parseVersion("0.5.1-rc.42")).toEqual({ major: 0, minor: 5, patch: 1, rc: 42 });
  });

  it("rejects malformed versions", () => {
    expect(() => parseVersion("0.5")).toThrow();
    expect(() => parseVersion("0.5.1-rc")).toThrow();
    expect(() => parseVersion("0.5.1-beta.1")).toThrow();
    expect(() => parseVersion("v0.5.1")).toThrow();
    expect(() => parseVersion("")).toThrow();
  });
});

describe("validateVersion", () => {
  it("accepts valid versions", () => {
    expect(validateVersion("0.5.1")).toBe("0.5.1");
    expect(validateVersion("0.5.1-rc.1")).toBe("0.5.1-rc.1");
  });

  it("throws on invalid versions", () => {
    expect(() => validateVersion("foo")).toThrow();
    expect(() => validateVersion("0.5.1-rc")).toThrow();
  });
});

describe("bumpVersion", () => {
  it("stable → next stable patch", () => {
    expect(bumpVersion("0.5.0", { rc: false })).toBe("0.5.1");
    expect(bumpVersion("0.0.129", { rc: false })).toBe("0.0.130");
  });

  it("stable → first RC of next patch", () => {
    expect(bumpVersion("0.5.0", { rc: true })).toBe("0.5.1-rc.1");
    expect(bumpVersion("0.0.129", { rc: true })).toBe("0.0.130-rc.1");
  });

  it("RC → next RC iteration", () => {
    expect(bumpVersion("0.5.1-rc.1", { rc: true })).toBe("0.5.1-rc.2");
    expect(bumpVersion("0.5.1-rc.2", { rc: true })).toBe("0.5.1-rc.3");
    expect(bumpVersion("0.5.1-rc.42", { rc: true })).toBe("0.5.1-rc.43");
  });

  it("RC → start next stable cycle (drops RC, increments patch)", () => {
    // After an RC has been promoted, the next bump starts the next stable
    // cycle. The promoted bytes shipped without rebuild, so we don't mint
    // a fresh stable for the same patch.
    expect(bumpVersion("0.5.1-rc.3", { rc: false })).toBe("0.5.2");
    expect(bumpVersion("0.5.1-rc.1", { rc: false })).toBe("0.5.2");
  });

  it("defaults to stable bump when options omitted", () => {
    expect(bumpVersion("0.5.0")).toBe("0.5.1");
  });
});

describe("baseVersion", () => {
  it("strips RC suffix", () => {
    expect(baseVersion("0.5.1")).toBe("0.5.1");
    expect(baseVersion("0.5.1-rc.1")).toBe("0.5.1");
    expect(baseVersion("0.5.1-rc.42")).toBe("0.5.1");
  });
});

describe("isRc", () => {
  it("identifies RC versions", () => {
    expect(isRc("0.5.1")).toBe(false);
    expect(isRc("0.5.1-rc.1")).toBe(true);
    expect(isRc("0.5.1-rc.42")).toBe(true);
  });
});
