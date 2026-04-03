#!/usr/bin/env node
import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { normalizeUnreleasedFragmentCategoryCasing, validateUnreleasedFragments } from "./compile-changelog.js";

const DOC_ONLY_PATTERN = /^(README\.md|CHANGELOG\.md|CONTRIBUTING\.md|AGENTS\.md|TODO\.md|changes\/|docs\/|\.github\/)/;

function parseArgs() {
  const args = process.argv.slice(2);
  const opts = {
    staged: false,
    base: null,
    head: "HEAD",
    requireOnCodeChanges: false,
    quiet: false,
    fix: false,
  };

  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === "--staged") opts.staged = true;
    else if (a === "--base") opts.base = args[++i] || null;
    else if (a === "--head") opts.head = args[++i] || "HEAD";
    else if (a === "--require-on-code-changes") opts.requireOnCodeChanges = true;
    else if (a === "--quiet") opts.quiet = true;
    else if (a === "--fix") opts.fix = true;
    else if (a === "--help" || a === "-h") {
      console.log(
        "Usage: node scripts/check-changelog-fragments.js [--staged] [--base <ref>] [--head <ref>] [--require-on-code-changes] [--fix]",
      );
      process.exit(0);
    } else {
      throw new Error(`Unknown flag: ${a}`);
    }
  }

  return opts;
}

function gitChangedFiles(opts) {
  try {
    if (opts.staged) {
      return execSync("git diff --cached --name-only --diff-filter=ACMR", { encoding: "utf8" })
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean);
    }

    if (opts.base) {
      return execSync(`git diff --name-only ${opts.base}...${opts.head}`, { encoding: "utf8" })
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean);
    }

    return execSync("git diff --name-only", { encoding: "utf8" })
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);
  } catch {
    return [];
  }
}

function main() {
  const opts = parseArgs();
  const changed = gitChangedFiles(opts);

  const nonDocChanged = changed.filter((f) => !DOC_ONLY_PATTERN.test(f));
  const changedFragments = changed.filter((f) => /^changes\/unreleased\/.*\.md$/.test(f));

  if (opts.requireOnCodeChanges && nonDocChanged.length > 0 && changedFragments.length === 0) {
    throw new Error("Missing changelog fragment: code changes detected but no changes/unreleased/*.md file changed.");
  }

  if (!existsSync("changes/unreleased")) {
    if (!opts.quiet) console.log("[changelog] changes/unreleased does not exist — skipping format validation.");
    return;
  }

  if (opts.fix) {
    const fixed = normalizeUnreleasedFragmentCategoryCasing();
    if (!opts.quiet && fixed.changedFiles.length > 0) {
      console.log(`[changelog] Normalized category casing in: ${fixed.changedFiles.join(", ")}`);
    }
  }

  const validation = validateUnreleasedFragments();
  if (!opts.quiet) {
    if (validation.files.length === 0) {
      console.log("[changelog] No unreleased fragments present.");
    } else {
      console.log(`[changelog] OK: ${validation.entryCount} entries across ${validation.files.length} fragment(s).`);
    }
  }
}

try {
  main();
} catch (err) {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
}
