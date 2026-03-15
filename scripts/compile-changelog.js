#!/usr/bin/env node
/**
 * Compile changelog fragments into CHANGELOG.md.
 *
 * Usage:
 *   node scripts/compile-changelog.js <version> [date]   # compile unreleased + rebuild
 *   node scripts/compile-changelog.js --rebuild           # rebuild from archived releases only
 *
 * - Reads all .md files from changes/unreleased/
 * - Groups entries by category (### heading)
 * - Writes changes/releases/<version>.md
 * - Deletes consumed fragment files
 * - Rebuilds CHANGELOG.md from all archived releases
 *
 * Can also be imported and called from bump.js.
 */
import {
  readFileSync,
  writeFileSync,
  readdirSync,
  mkdirSync,
  unlinkSync,
  existsSync,
} from "fs";
import { join } from "path";

const UNRELEASED_DIR = "changes/unreleased";
const RELEASES_DIR = "changes/releases";
const CHANGELOG_PATH = "CHANGELOG.md";

const HEADER = `# Changelog

All notable changes to NeuroSkill™ are documented here.
Pending changes live as fragments in [\`changes/unreleased/\`](changes/unreleased/).
Past releases are archived in [\`changes/releases/\`](changes/releases/).

---

## [Unreleased]
`;

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

/** Compare semver strings descending (newest first). */
function semverCompareDesc(a, b) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    if ((pa[i] || 0) !== (pb[i] || 0)) return (pb[i] || 0) - (pa[i] || 0);
  }
  return 0;
}

/**
 * Read all archived releases from changes/releases/*.md,
 * sorted newest-first.
 */
function loadArchivedReleases() {
  if (!existsSync(RELEASES_DIR)) return [];

  return readdirSync(RELEASES_DIR)
    .filter((f) => f.endsWith(".md"))
    .map((f) => f.replace(/\.md$/, ""))
    .sort(semverCompareDesc)
    .map((v) => readFileSync(join(RELEASES_DIR, `${v}.md`), "utf8").trimEnd());
}

/**
 * Rebuild CHANGELOG.md from header + all archived releases.
 */
export function rebuildChangelog() {
  const releases = loadArchivedReleases();
  const content = HEADER + "\n" + releases.join("\n\n") + "\n";
  writeFileSync(CHANGELOG_PATH, content, "utf8");
  return releases.length;
}

/**
 * Compile unreleased fragments, archive them, and rebuild CHANGELOG.md.
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
  const categories = new Map();
  let entryCount = 0;

  for (const file of files) {
    const content = readFileSync(join(UNRELEASED_DIR, file), "utf8").trim();
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

  // Write compiled release file
  mkdirSync(RELEASES_DIR, { recursive: true });
  writeFileSync(join(RELEASES_DIR, `${version}.md`), releaseSection, "utf8");

  // Delete consumed fragments
  for (const file of files) {
    unlinkSync(join(UNRELEASED_DIR, file));
  }

  // Rebuild full CHANGELOG.md from all archived releases
  rebuildChangelog();

  return {
    entryCount,
    categories: sortedCategories.map(([c]) => c),
  };
}

// CLI mode
const scriptName = "compile-changelog.js";
if (process.argv[1]?.endsWith(scriptName)) {
  if (process.argv[2] === "--rebuild") {
    const count = rebuildChangelog();
    console.log(`Rebuilt CHANGELOG.md from ${count} archived releases`);
    process.exit(0);
  }

  const version = process.argv[2];
  const date = process.argv[3] || new Date().toISOString().slice(0, 10);

  if (!version) {
    console.error(
      `Usage:\n  node scripts/${scriptName} <version> [date]\n  node scripts/${scriptName} --rebuild`
    );
    process.exit(1);
  }

  const result = compileChangelog(version, date);

  if (result.entryCount === 0) {
    console.log("No changelog fragments found in changes/unreleased/");
  } else {
    console.log(
      `Compiled ${result.entryCount} entries across ${result.categories.length} categories`
    );
    console.log(`  Categories: ${result.categories.join(", ")}`);
    console.log(`  Archived to changes/releases/${version}.md`);
    console.log(`  CHANGELOG.md rebuilt`);
  }
}
