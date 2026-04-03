#!/usr/bin/env node
/**
 * Compile changelog fragments into CHANGELOG.md.
 *
 * Usage:
 *   node scripts/compile-changelog.js <version> [date]   # compile unreleased + rebuild
 *   node scripts/compile-changelog.js --rebuild          # rebuild from archived releases only
 *   node scripts/compile-changelog.js --check            # validate unreleased fragments only
 */
import { existsSync, mkdirSync, readdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";

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
export const CATEGORY_ORDER = [
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

const CATEGORY_CANONICAL = new Map(CATEGORY_ORDER.map((c) => [c.toLowerCase(), c]));

function categoryRank(name) {
  const idx = CATEGORY_ORDER.findIndex((c) => c.toLowerCase() === name.toLowerCase());
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
  const content = `${HEADER}\n${releases.join("\n\n")}\n`;
  writeFileSync(CHANGELOG_PATH, content, "utf8");
  return releases.length;
}

function parseFragment(file, content) {
  const errors = [];
  const trimmed = content.trim();
  if (!trimmed) {
    errors.push(`${file}: file is empty`);
    return { errors, sections: [], bulletCount: 0 };
  }

  const rawSections = trimmed.split(/^### /m).filter(Boolean);
  if (rawSections.length === 0) {
    errors.push(`${file}: missing category heading(s). Use \`### <Category>\`.`);
    return { errors, sections: [], bulletCount: 0 };
  }

  const sections = [];
  let bulletCount = 0;

  for (const rawSection of rawSections) {
    const newlineIdx = rawSection.indexOf("\n");
    if (newlineIdx === -1) {
      errors.push(`${file}: section has heading but no body: \`${rawSection.trim()}\``);
      continue;
    }

    const rawCategory = rawSection.slice(0, newlineIdx).trim();
    const category = CATEGORY_CANONICAL.get(rawCategory.toLowerCase());
    if (!category) {
      errors.push(`${file}: unknown category \`${rawCategory}\`. Allowed: ${CATEGORY_ORDER.join(", ")}`);
      continue;
    }

    const body = rawSection.slice(newlineIdx + 1).trim();
    if (!body) {
      errors.push(`${file}: category \`${category}\` is empty`);
      continue;
    }

    const bullets = body.split("\n").filter((l) => l.trimStart().startsWith("- "));
    if (bullets.length === 0) {
      errors.push(`${file}: category \`${category}\` must contain at least one \`- \` bullet`);
      continue;
    }

    bulletCount += bullets.length;
    sections.push({ category, body, bulletCount: bullets.length });
  }

  if (sections.length === 0 || bulletCount === 0) {
    errors.push(`${file}: no valid changelog bullet entries found`);
  }

  return { errors, sections, bulletCount };
}

/**
 * Validate all unreleased changelog fragments.
 *
 * @returns {{ files: string[], entryCount: number, categoryCounts: Record<string, number> }}
 * @throws {Error} if any fragment is invalid.
 */
export function validateUnreleasedFragments() {
  if (!existsSync(UNRELEASED_DIR)) {
    return { files: [], entryCount: 0, categoryCounts: {} };
  }

  const files = readdirSync(UNRELEASED_DIR)
    .filter((f) => f.endsWith(".md"))
    .sort();

  if (files.length === 0) {
    return { files: [], entryCount: 0, categoryCounts: {} };
  }

  const categoryCounts = new Map();
  const errors = [];
  let entryCount = 0;

  for (const file of files) {
    const content = readFileSync(join(UNRELEASED_DIR, file), "utf8");
    const parsed = parseFragment(file, content);
    if (parsed.errors.length > 0) {
      errors.push(...parsed.errors);
      continue;
    }

    entryCount += parsed.bulletCount;
    for (const section of parsed.sections) {
      categoryCounts.set(section.category, (categoryCounts.get(section.category) || 0) + section.bulletCount);
    }
  }

  if (errors.length > 0) {
    throw new Error(`Invalid changelog fragment(s):\n- ${errors.join("\n- ")}`);
  }

  return {
    files,
    entryCount,
    categoryCounts: Object.fromEntries(categoryCounts.entries()),
  };
}

/**
 * Normalize `### Category` heading casing in changes/unreleased/*.md fragments.
 *
 * Example fixed heading:
 *   ### bugfixes  ->  ### Bugfixes
 *
 * Unknown categories are left untouched (validation will still fail for them).
 *
 * @returns {{ changedFiles: string[] }}
 */
export function normalizeUnreleasedFragmentCategoryCasing() {
  if (!existsSync(UNRELEASED_DIR)) {
    return { changedFiles: [] };
  }

  const files = readdirSync(UNRELEASED_DIR)
    .filter((f) => f.endsWith(".md"))
    .sort();

  const changedFiles = [];

  for (const file of files) {
    const path = join(UNRELEASED_DIR, file);
    const original = readFileSync(path, "utf8");
    const normalized = original.replace(/^###\s+(.+)$/gm, (full, heading) => {
      const canonical = CATEGORY_CANONICAL.get(String(heading).trim().toLowerCase());
      if (!canonical) return full;
      return `### ${canonical}`;
    });

    if (normalized !== original) {
      writeFileSync(path, normalized, "utf8");
      changedFiles.push(file);
    }
  }

  return { changedFiles };
}

/**
 * Compile unreleased fragments, archive them, and rebuild CHANGELOG.md.
 * @param {string} version  — semver string, e.g. "0.0.38"
 * @param {string} date     — ISO date string, e.g. "2026-03-15"
 * @returns {{ entryCount: number, categories: string[], consumedFiles: string[], categoryCounts: Record<string, number> }}
 */
export function compileChangelog(version, date) {
  const validation = validateUnreleasedFragments();
  const files = validation.files;

  if (files.length === 0) {
    return { entryCount: 0, categories: [], consumedFiles: [], categoryCounts: {} };
  }

  // Parse fragments and group by category
  const categories = new Map();

  for (const file of files) {
    const content = readFileSync(join(UNRELEASED_DIR, file), "utf8").trim();
    const parsed = parseFragment(file, content);
    for (const section of parsed.sections) {
      if (!categories.has(section.category)) {
        categories.set(section.category, []);
      }
      categories.get(section.category).push(section.body);
    }
  }

  // Sort categories by canonical order
  const sortedCategories = [...categories.entries()].sort(([a], [b]) => categoryRank(a) - categoryRank(b));

  // Build the release section
  const lines = [`## [${version}] — ${date}`, ""];
  for (const [category, entries] of sortedCategories) {
    lines.push(`### ${category}`, "");
    for (const entry of entries) {
      lines.push(entry, "");
    }
  }
  const releaseSection = `${lines.join("\n").trimEnd()}\n`;

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
    entryCount: validation.entryCount,
    categories: sortedCategories.map(([c]) => c),
    consumedFiles: files,
    categoryCounts: validation.categoryCounts,
  };
}

// CLI mode
const scriptName = "compile-changelog.js";
if (process.argv[1]?.endsWith(scriptName)) {
  if (process.argv[2] === "--rebuild") {
    rebuildChangelog();
    process.exit(0);
  }

  if (process.argv[2] === "--check") {
    try {
      const result = validateUnreleasedFragments();
      if (result.files.length === 0) {
        console.log("[changelog] No unreleased fragments found.");
      } else {
        console.log(`[changelog] OK: ${result.entryCount} entries across ${result.files.length} fragment(s).`);
      }
      process.exit(0);
    } catch (err) {
      console.error(err instanceof Error ? err.message : String(err));
      process.exit(1);
    }
  }

  const version = process.argv[2];
  const date = process.argv[3] || new Date().toISOString().slice(0, 10);

  if (!version) {
    process.exit(1);
  }

  try {
    const result = compileChangelog(version, date);
    if (result.entryCount === 0) {
      console.log("[changelog] No unreleased fragments to compile.");
    } else {
      console.log(
        `[changelog] Compiled ${result.entryCount} entries from ${result.consumedFiles.length} fragment(s): ${result.consumedFiles.join(", ")}`,
      );
    }
  } catch (err) {
    console.error(err instanceof Error ? err.message : String(err));
    process.exit(1);
  }
}
