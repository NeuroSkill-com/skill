// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error — plain .mjs-style module without type declarations
import { findUnregisteredTests } from "../../scripts/check-test-targets.js";

/** A throwaway repo with one crate: its Cargo.toml text, and the files under tests/. */
function repo(manifest: string, files: Record<string, string>) {
  const root = mkdtempSync(join(tmpdir(), "tt-"));
  const crate = join(root, "crates", "demo");
  mkdirSync(join(crate, "tests"), { recursive: true });
  writeFileSync(join(crate, "Cargo.toml"), manifest);
  for (const [name, body] of Object.entries(files)) writeFileSync(join(crate, "tests", name), body);
  return root;
}

const MERGED = `[package]\nname = "demo"\nautotests = false\n\n[[test]]\nname = "it"\npath = "tests/it.rs"\n`;

describe("findUnregisteredTests", () => {
  it("passes when every file is a module of it.rs", () => {
    const root = repo(MERGED, {
      "it.rs": '#[path = "a_tests.rs"]\nmod a_tests;\nmod b_tests;\n',
      "a_tests.rs": "",
      "b_tests.rs": "",
    });
    expect(findUnregisteredTests(root)).toEqual([]);
  });

  it("flags a test file nothing registers", () => {
    const root = repo(MERGED, { "it.rs": "mod a_tests;\n", "a_tests.rs": "", "forgotten.rs": "" });
    expect(findUnregisteredTests(root)).toEqual([{ crate: "crates/demo", file: "forgotten.rs" }]);
  });

  it("accepts a file declared as its own [[test]]", () => {
    const manifest = `${MERGED}\n[[test]]\nname = "llm_e2e"\npath = "tests/llm_e2e.rs"\n`;
    const root = repo(manifest, { "it.rs": "", "llm_e2e.rs": "" });
    expect(findUnregisteredTests(root)).toEqual([]);
  });

  it("ignores crates that still auto-discover their tests", () => {
    const root = repo(`[package]\nname = "demo"\n`, { "anything.rs": "" });
    expect(findUnregisteredTests(root)).toEqual([]);
  });

  it("does not treat a commented-out autotests line as the setting", () => {
    const root = repo(`[package]\nname = "demo"\n# autotests = false\n`, { "anything.rs": "" });
    expect(findUnregisteredTests(root)).toEqual([]);
  });

  it("does not count a [[bin]] path as a registered test", () => {
    const manifest = `${MERGED}\n[[bin]]\nname = "tool"\npath = "src/bin/tool.rs"\n`;
    const root = repo(manifest, { "it.rs": "", "tool.rs": "" });
    expect(findUnregisteredTests(root)).toEqual([{ crate: "crates/demo", file: "tool.rs" }]);
  });

  it("only looks at top-level files: subdirectories are module folders", () => {
    const root = repo(MERGED, { "it.rs": "mod nested;\n" });
    mkdirSync(join(root, "crates", "demo", "tests", "nested"), { recursive: true });
    writeFileSync(join(root, "crates", "demo", "tests", "nested", "mod.rs"), "");
    expect(findUnregisteredTests(root)).toEqual([]);
  });

  it("reports several crates and files in a stable order", () => {
    const root = repo(MERGED, { "it.rs": "", "z.rs": "", "a.rs": "" });
    expect(findUnregisteredTests(root).map((p: { file: string }) => p.file)).toEqual(["a.rs", "z.rs"]);
  });

  it("passes on this repository", () => {
    expect(findUnregisteredTests()).toEqual([]);
  });
});
