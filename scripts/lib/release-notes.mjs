// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Merge the notes of a release's candidates into one changelog fragment.
//
// Every RC's notes land in `changes/releases/<base>-rc.<n>.md`, holding only the
// fragments that went into *that* candidate (rc.2's file may be a single bullet).
// A stable release is the sum of all of them, so `npm run release -- --stable`
// builds its fragment from every RC file, grouped by category, with repeats removed.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

/// Category headings in the order they first appear across the files.
/// Bullets are kept verbatim, including wrapped continuation lines.
export function mergeRcNotes(files) {
  /** @type {Map<string, string[]>} */
  const categories = new Map();
  const seen = new Map();

  for (const text of files) {
    let category = null;
    let bullet = null;
    const flush = () => {
      if (!category || !bullet) return;
      const body = bullet.join("\n").trimEnd();
      if (!seen.get(category).has(body)) {
        seen.get(category).add(body);
        categories.get(category).push(body);
      }
      bullet = null;
    };

    for (const line of text.split(/\r?\n/)) {
      if (line.startsWith("## ")) continue; // the `## [version] — date` heading
      const heading = line.match(/^### (.+?)\s*$/);
      if (heading) {
        flush();
        category = heading[1];
        if (!categories.has(category)) {
          categories.set(category, []);
          seen.set(category, new Set());
        }
        continue;
      }
      if (category === null) continue;
      if (line.startsWith("- ")) {
        flush();
        bullet = [line];
      } else if (line.trim() && bullet) {
        bullet.push(line);
      } else if (!line.trim()) {
        flush();
      }
    }
    flush();
  }

  const sections = [];
  for (const [category, bullets] of categories) {
    if (bullets.length === 0) continue;
    sections.push(`### ${category}\n\n${bullets.join("\n")}`);
  }
  return sections.length ? `${sections.join("\n\n")}\n` : "";
}

/// RC note files for `base` (e.g. "0.1.2") in `dir`, oldest candidate first.
export function rcNoteFiles(base, dir = "changes/releases") {
  if (!existsSync(dir)) return [];
  const rx = new RegExp(`^${base.replace(/\./g, "\\.")}-rc\\.(\\d+)\\.md$`);
  return readdirSync(dir)
    .map((name) => ({ name, m: name.match(rx) }))
    .filter((x) => x.m)
    .sort((a, b) => Number(a.m[1]) - Number(b.m[1]))
    .map((x) => join(dir, x.name));
}

/// The merged fragment for `base`, or "" when no RC left notes.
export function stableFragmentFromRcNotes(base, dir = "changes/releases") {
  return mergeRcNotes(rcNoteFiles(base, dir).map((f) => readFileSync(f, "utf8")));
}
