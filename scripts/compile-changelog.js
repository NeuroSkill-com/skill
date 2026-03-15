#!/usr/bin/env node
/**
 * Compile changelog fragments from changes/unreleased/ into a release section.
 *
 * Usage:
 *   node scripts/compile-changelog.js <version> [date]
 *
 * - Reads all .md files from changes/unreleased/
 * - Groups entries by category (### heading)
 * - Prepends a new ## [version] — date section to CHANGELOG.md
 * - Moves fragments to changes/releases/<version>/
 * - Leaves a fresh ## [Unreleased] header in CHANGELOG.md
 *
 * Can also be imported and called from bump.js.
 */
import { readFileSync, writeFileSync, readdirSync, mkdirSync, renameSync, existsSync } from "fs";
import { join, basename } from "path";

const UNRELEASED_DIR = "changes/unreleased";
const RELEASES_DIR = "changes/releases";
const CHANGELOG_PATH = "CHANGELOG.md";

// Canonical category order
const CATEGORY_ORDER = [
  "Features",
  "Performance",
  "Bugfixes",
  "Refactor",
  "Build",
  "CLI",
  "UI",
  "LLM",
  "Server",
  "i18n",
  "Docs",
  "Dependencies",
];

function categoryRank(name) {
  const idx = CATEGORY_ORDER.findIndex(
    (c) => c.toLowerCase() === name.toLowerCase()
  );
  return idx >= 0 ? idx : CATEGORY_ORDER.length;
}

/**
 * Compile unreleased fragments into a versioned CHANGELOG section.
 * @param {string} version  — semver string, e.g. "0.0.38"
 * @param {string} date     — ISO date string, e.g. "2026-03-15"
 * @returns {{ entryCount: number, categories: string[] }}
 */
export function compileChangelog(version, date) {
  if (!existsSync(UNRELEASED_DIR)) {
    return { entryCount: 0, categories: [] };
  }

  const files = readdirSync(UNRELEASED_DIR)
    .filter((f) => f.endsWith(".md"))
    .sort();

  if (files.length === 0) {
    return { entryCount: 0, categories: [] };
  }

  // Parse fragments and group by category
  const categories = new Map(); // category name → entries[]
  let entryCount = 0;

  for (const file of files) {
    const content = readFileSync(join(UNRELEASED_DIR, file), "utf8").trim();
    // Split by ### headings
    const sections = content.split(/^### /m).filter(Boolean);

    for (const section of sections) {
      const newlineIdx = section.indexOf("\n");
      if (newlineIdx === -1) continue;

      const category = section.slice(0, newlineIdx).trim();
      const body = section.slice(newlineIdx).trim();

      if (!body) continue;

      if (!categories.has(category)) {
        categories.set(category, []);
      }
      categories.get(category).push(body);

      // Count bullet entries
      const bullets = body.split("\n").filter((l) => l.startsWith("- "));
      entryCount += bullets.length;
    }
  }

  // Sort categories by canonical order
  const sortedCategories = [...categories.entries()].sort(
    ([a], [b]) => categoryRank(a) - categoryRank(b)
  );

  // Build the release section
  const lines = [`## [${version}] — ${date}`, ""];

  for (const [category, entries] of sortedCategories) {
    lines.push(`### ${category}`, "");
    for (const entry of entries) {
      lines.push(entry, "");
    }
  }

  const releaseSection = lines.join("\n").trimEnd() + "\n";

  // Update CHANGELOG.md
  const changelog = readFileSync(CHANGELOG_PATH, "utf8");

  // Find ## [Unreleased] and replace with fresh unreleased + new release
  const unreleasedRe = /^## \[Unreleased\].*$/m;
  if (!unreleasedRe.test(changelog)) {
    throw new Error(`Could not find "## [Unreleased]" in ${CHANGELOG_PATH}`);
  }

  // Remove old [Unreleased] content up to the next ## heading
  const unreleasedMatch = changelog.match(unreleasedRe);
  const unreleasedStart = unreleasedMatch.index;
  const afterUnreleased = changelog.slice(
    unreleasedStart + unreleasedMatch[0].length
  );

  // Find the next ## heading (the previous release)
  const nextHeadingMatch = afterUnreleased.match(/^## /m);
  const contentBeforeNextRelease = nextHeadingMatch
    ? afterUnreleased.slice(0, nextHeadingMatch.index)
    : afterUnreleased;
  const restOfChangelog = nextHeadingMatch
    ? afterUnreleased.slice(nextHeadingMatch.index)
    : "";

  const beforeUnreleased = changelog.slice(0, unreleasedStart);

  const updated =
    beforeUnreleased +
    "## [Unreleased]\n\n" +
    releaseSection +
    "\n" +
    restOfChangelog;

  writeFileSync(CHANGELOG_PATH, updated, "utf8");

  // Move fragments to releases/<version>/
  const releaseDir = join(RELEASES_DIR, version);
  mkdirSync(releaseDir, { recursive: true });

  for (const file of files) {
    renameSync(join(UNRELEASED_DIR, file), join(releaseDir, file));
  }

  return {
    entryCount,
    categories: sortedCategories.map(([c]) => c),
  };
}

// CLI mode
if (process.argv[1]?.endsWith("compile-changelog.js")) {
  const version = process.argv[2];
  const date = process.argv[3] || new Date().toISOString().slice(0, 10);

  if (!version) {
    console.error("Usage: node scripts/compile-changelog.js <version> [date]");
    process.exit(1);
  }

  const result = compileChangelog(version, date);

  if (result.entryCount === 0) {
    console.log("No changelog fragments found in changes/unreleased/");
  } else {
    console.log(
      `Compiled ${result.entryCount} entries across ${result.categories.length} categories into CHANGELOG.md`
    );
    console.log(`  Categories: ${result.categories.join(", ")}`);
    console.log(`  Fragments archived to changes/releases/${version}/`);
  }
}
