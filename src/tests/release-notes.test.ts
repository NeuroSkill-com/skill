// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error — plain .mjs module without type declarations
import { mergeRcNotes, rcNoteFiles, stableFragmentFromRcNotes } from "../../scripts/lib/release-notes.mjs";

const rc1 = `## [0.1.2-rc.1] — 2026-11-13

### Features

- **A**: first feature.
- **B**: a wrapped bullet
  that continues here.

### Build

- **Old build note**.
`;
const rc2 = `## [0.1.2-rc.2] — 2026-11-13

### Build

- **New build note**.

### Bugfixes

- **Fixed a thing**.
`;

describe("mergeRcNotes", () => {
  it("groups bullets by category in first-seen order", () => {
    const out = mergeRcNotes([rc1, rc2]);
    expect(out.indexOf("### Features")).toBeLessThan(out.indexOf("### Build"));
    expect(out.indexOf("### Build")).toBeLessThan(out.indexOf("### Bugfixes"));
  });

  it("emits each category heading once even when two candidates share it", () => {
    const out = mergeRcNotes([rc1, rc2]);
    expect(out.match(/^### Build$/gm)).toHaveLength(1);
    expect(out).toContain("**Old build note**");
    expect(out).toContain("**New build note**");
  });

  it("keeps a wrapped bullet's continuation lines", () => {
    expect(mergeRcNotes([rc1])).toContain("- **B**: a wrapped bullet\n  that continues here.");
  });

  it("drops a bullet repeated in a later candidate", () => {
    const out = mergeRcNotes([rc1, rc1]);
    expect(out.match(/\*\*A\*\*/g)).toHaveLength(1);
  });

  it("ignores the per-version heading", () => {
    expect(mergeRcNotes([rc1])).not.toContain("0.1.2-rc.1");
  });

  it("returns an empty string when there is nothing to merge", () => {
    expect(mergeRcNotes([])).toBe("");
    expect(mergeRcNotes(["## [0.1.2-rc.1] — 2026-11-13\n"])).toBe("");
  });

  it("ends with exactly one newline", () => {
    expect(mergeRcNotes([rc1, rc2]).endsWith("\n")).toBe(true);
    expect(mergeRcNotes([rc1, rc2]).endsWith("\n\n")).toBe(false);
  });
});

describe("rcNoteFiles", () => {
  const dir = mkdtempSync(join(tmpdir(), "rcnotes-"));
  mkdirSync(dir, { recursive: true });
  for (const f of ["0.1.2-rc.1.md", "0.1.2-rc.10.md", "0.1.2-rc.2.md", "0.1.2.md", "0.1.3-rc.1.md", "0.1.22-rc.1.md"]) {
    writeFileSync(join(dir, f), rc1);
  }

  it("orders candidates numerically, so rc.10 follows rc.2", () => {
    expect(rcNoteFiles("0.1.2", dir).map((f: string) => f.split("/").pop())).toEqual([
      "0.1.2-rc.1.md",
      "0.1.2-rc.2.md",
      "0.1.2-rc.10.md",
    ]);
  });

  it("does not match another patch, the stable file or a longer version", () => {
    const names = rcNoteFiles("0.1.2", dir).map((f: string) => f.split("/").pop());
    expect(names).not.toContain("0.1.2.md");
    expect(names).not.toContain("0.1.3-rc.1.md");
    expect(names).not.toContain("0.1.22-rc.1.md");
  });

  it("is empty for a missing directory", () => {
    expect(rcNoteFiles("0.1.2", join(dir, "nope"))).toEqual([]);
  });

  it("builds a fragment from the real files on disk", () => {
    expect(stableFragmentFromRcNotes("0.1.2", dir)).toContain("### Features");
    expect(stableFragmentFromRcNotes("9.9.9", dir)).toBe("");
  });
});
