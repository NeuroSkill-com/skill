#!/usr/bin/env node
import { execSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { compileChangelog, validateUnreleasedFragments } from "./compile-changelog.js";
import { bumpVersion, readVersionFile, validateVersion, writeVersionFile } from "./version-utils.mjs";

// ── helpers ──────────────────────────────────────────────────────────────────

function readText(path) {
  return readFileSync(path, "utf8");
}

function writeText(path, content) {
  writeFileSync(path, content, "utf8");
}

// ── rlx mode ─────────────────────────────────────────────────────────────────
//
// `scripts/rlx local` path-patches rlx onto sibling checkouts through
// .cargo/rlx-local.toml and hides the rewritten Cargo.lock with skip-worktree.
// The bump commit must carry the committed GitHub-pinned lock (CI and release
// builds use exactly those pins), so a local session is switched to deployed
// for the bump and switched back afterwards.

const RLX_LOCAL_CONFIG = ".cargo/rlx-local.toml";
const RLX_LOCAL_MARK = "# managed by skill/scripts/ensure-rlx.sh";

function rlxLocalActive() {
  return existsSync(RLX_LOCAL_CONFIG) && readText(RLX_LOCAL_CONFIG).includes(RLX_LOCAL_MARK);
}

function rlxMode(mode) {
  execSync(`bash scripts/rlx ${mode}`, { stdio: "inherit" });
}

/**
 * Refuse to bump past a version that was never tagged and pushed: that is how a
 * version gets skipped, or bumped twice.
 */
function checkVersionTagged(version) {
  const tagName = `v${version}`;

  // Check if tag exists locally
  try {
    execSync(`git rev-parse ${tagName}`, { stdio: "ignore" });
  } catch {
    throw new Error(
      `Bump aborted: Current version ${version} is not tagged locally.` +
        `\nRun 'npm run tag' to create and push the tag before bumping to a new version.`,
    );
  }

  // Check if tag exists on at least one remote
  try {
    const remotes = execSync("git remote", { encoding: "utf8" })
      .split("\n")
      .map((name) => name.trim())
      .filter(Boolean);

    let foundOnRemote = false;

    for (const remote of remotes) {
      try {
        // Check if the tag exists on this remote
        execSync(`git ls-remote --tags --exit-code ${remote} refs/tags/${tagName}`, { stdio: "ignore" });
        foundOnRemote = true;
        break;
      } catch {
        // Tag not on this remote, try next one
      }
    }

    if (!foundOnRemote) {
      throw new Error(
        `Bump aborted: Tag ${tagName} exists locally but not on any remote.` +
          `\nRun 'git push --tags' or 'npm run tag' to push the tag before bumping to a new version.`,
      );
    }
  } catch (err) {
    if (err.message.includes("Bump aborted")) {
      throw err;
    }
    throw new Error(`Bump aborted: Could not verify if tag ${tagName} exists on remote: ${err.message}`);
  }
}

function todayIsoDate() {
  return new Date().toISOString().slice(0, 10);
}

function releaseFilePath(version) {
  return `changes/releases/${version}.md`;
}

// ── git helpers ──────────────────────────────────────────────────────────────

/// Tracked files that differ from HEAD, staged or not.
function changedTrackedPaths() {
  try {
    return execSync("git diff --name-only HEAD", { encoding: "utf8" }).split(/\r?\n/).filter(Boolean);
  } catch {
    return [];
  }
}

// ── CLI flags ────────────────────────────────────────────────────────────────

function parseArgs() {
  let dryRun = false;
  let clean = false;
  let versionArg = null;
  let force = false;
  let rc = false;
  let check = false;

  for (const a of process.argv.slice(2)) {
    if (a === "--dry-run") dryRun = true;
    else if (a === "--clean") clean = true;
    else if (a === "--force") force = true;
    else if (a === "--rc") rc = true;
    else if (a === "--check") check = true;
    else if (a === "--help" || a === "-h") {
      console.log(`
Usage: npm run bump [version] [--rc] [--check] [--dry-run] [--clean] [--force]

Writes the new version, compiles the changelog fragments and commits "<version>".
Normally run for you by \`npm run release\`.

Arguments:
  version       Optional specific version (e.g., 1.2.3 or 1.2.3-rc.1).
                If not provided, derives the next version from VERSION.

Flags:
  --rc          Bump to a release candidate.
                  Stable → next-patch-rc.1   (e.g., 0.5.0 → 0.5.1-rc.1)
                  RC     → next RC iteration (e.g., 0.5.1-rc.1 → 0.5.1-rc.2)
                Without --rc, RC versions advance to the next stable patch.
  --check       Run the full check list first (npm test -- full). Off by default:
                CI runs it on the tag before anything is published.
  --dry-run     Run the checks without changing anything
  --clean       Clean Rust build artifacts after a successful bump
  --force       Bypass the "current version is tagged and pushed" check

VERSION is the source of truth. bump writes it and the Cargo workspace version
(root Cargo.toml [workspace.package]), which src-tauri, skill-daemon and
skill-tty inherit and Tauri reads.
`);
      process.exit(0);
    } else if (!a.startsWith("-")) versionArg = a;
    else throw new Error(`Unknown flag: ${a}`);
  }

  return { dryRun, clean, versionArg, force, rc, check };
}

// ── auto-generate changelog fragment from git log ────────────────────────────

function generateFragmentFromGitLog(currentVersion) {
  const UNRELEASED_DIR = "changes/unreleased";

  // Always generate from commit history since last version bump
  // Find the last version bump commit (message exactly matches version string)
  let range = "";
  try {
    // Find the most recent commit whose message is exactly the current version
    const sha = execSync(`git log --all --format=%H --grep="^${currentVersion}$" --fixed-strings -1`, {
      encoding: "utf8",
    }).trim();
    if (sha) {
      range = `${sha}..HEAD`;
    }
  } catch {
    // ignore
  }

  // If we couldn't find a version bump commit, use the last version tag
  if (!range) {
    try {
      // Try to find the last version tag
      const lastTag = execSync("git describe --tags --abbrev=0 2>/dev/null", { encoding: "utf8" }).trim();
      if (lastTag?.startsWith("v")) {
        range = `${lastTag}..HEAD`;
      }
    } catch {
      // ignore
    }
  }

  // Collect commit subjects (skip merge commits)
  let logCmd = "git log --no-merges --format=%B";
  if (range) {
    logCmd += ` ${range}`;
  }

  let commits;
  try {
    commits = execSync(logCmd, { encoding: "utf8" }).trim();
  } catch {
    commits = "";
  }

  // Map a conventional-commit prefix onto a real changelog category.
  //
  // Everything used to be filed under `### Features` regardless of what the
  // commit said, which is how "fixed glib with a patch" and "updTed settings
  // and engine" ended up in the Features list of a shipped release. A wrong
  // category is worse than a vague one: it tells the reader something false.
  const PREFIX_CATEGORY = {
    feat: "Features",
    perf: "Performance",
    fix: "Bugfixes",
    revert: "Bugfixes",
    refactor: "Refactor",
    build: "Build",
    ci: "Build",
    chore: "Build",
    docs: "Docs",
    test: "Build",
    style: "Refactor",
  };

  // Parse commit messages and bucket them by category.
  const commitMessages = commits.split("\n\n").filter(Boolean);
  /** @type {Map<string, string[]>} */
  const byCategory = new Map();
  const seen = new Set();

  for (const commit of commitMessages) {
    const lines = commit.split("\n").filter(Boolean);
    if (lines.length === 0) continue;

    const subject = lines[0];

    // Skip version-only commits (the bump commits themselves).
    if (/^\d+\.\d+\.\d+(-rc\.\d+)?$/.test(subject)) continue;

    if (seen.has(subject)) continue;
    seen.add(subject);

    const m = /^(feat|fix|chore|docs|style|refactor|perf|test|build|ci|revert|WIP)(\(.+?\))?!?:/.exec(subject);
    // No recognisable prefix means nobody labelled it, and guessing "Feature"
    // is exactly the mistake this replaces. `Refactor` is the honest bucket for
    // an unlabelled internal change.
    const category = m ? (PREFIX_CATEGORY[m[1]] ?? "Refactor") : "Refactor";
    const text = m ? commit.replace(/\n/g, " ").replace(/\s+/g, " ").trim() : subject;

    if (!byCategory.has(category)) byCategory.set(category, []);
    byCategory.get(category).push(`- ${text}`);
  }

  const bulletCount = [...byCategory.values()].reduce((n, b) => n + b.length, 0);
  if (bulletCount === 0) {
    byCategory.set("Build", ["- Minor updates and improvements"]);
  }

  // No preamble/marker here on purpose: `parseFragment` splits on /^### /m and
  // treats anything before the first heading as a category name, so a leading
  // comment makes the fragment fail validation. Provenance is carried by the
  // `auto-git-log.md` filename and the warning printed above instead.
  const fragment = [...byCategory.entries()]
    .map(([cat, bullets]) => `### ${cat}\n\n${bullets.join("\n")}\n`)
    .join("\n");

  // Write the auto-generated fragment
  if (!existsSync(UNRELEASED_DIR)) {
    execSync(`mkdir -p ${UNRELEASED_DIR}`);
  }
  writeText(join(UNRELEASED_DIR, "auto-git-log.md"), fragment);
  console.log(`[bump] Generated changelog fragment with ${Math.max(bulletCount, 1)} entries from git log`);
}

// ── main (async) ─────────────────────────────────────────────────────────────

async function main() {
  const { dryRun, clean, versionArg, force, rc, check } = parseArgs();

  // ── resolve new version ─────────────────────────────────────────────────────

  // Source of truth: repo-root VERSION, mirrored into the Cargo workspace.
  const currentVersion = readVersionFile();

  // ── Check if current version is already tagged and pushed ─────────────────
  if (!force) {
    checkVersionTagged(currentVersion);
  } else {
    console.log("[bump] --force flag used: skipping version tag check");
  }

  const newVersion = versionArg ? validateVersion(versionArg) : bumpVersion(currentVersion, { rc });

  // ── Prevent accidental overwrite of an existing archived release ───────────

  const existingReleasePath = releaseFilePath(newVersion);
  if (existsSync(existingReleasePath)) {
    throw new Error(`Bump aborted: ${existingReleasePath} already exists.`);
  }

  // ── Validate changelog fragments early (fast fail) ─────────────────────────

  const validated = validateUnreleasedFragments();
  if (validated.files.length === 0) {
    console.warn("");
    console.warn("  ⚠  No changelog fragment was written for this release.");
    console.warn("     Falling back to raw commit subjects, which become user-visible");
    console.warn("     release notes verbatim — typos, WIP markers and all.");
    console.warn("     Prefer: npm run changes:new -- <slug> --category <Category>");
    console.warn("");
    if (!dryRun) generateFragmentFromGitLog(currentVersion);
  } else {
    console.log(
      `[bump] Found ${validated.files.length} changelog fragment${validated.files.length === 1 ? "" : "s"} — will use these instead of git log`,
    );
  }

  // ── optional checks (must pass before any file is modified) ───────────────

  if (check) {
    execSync("node scripts/check.mjs full", { stdio: "inherit" });
  }

  // ── In dry-run mode stop here — no files are modified ─────────────────────

  if (dryRun) {
    return;
  }

  // A local rlx session hides a path-patched Cargo.lock; the commit needs the
  // GitHub-pinned one. Switched back in `finally`.
  const wasLocal = rlxLocalActive();
  if (wasLocal) rlxMode("deployed");

  // ── Snapshot git state so we can revert on failure ─────────────────────────

  const headBefore = execSync("git rev-parse HEAD", { encoding: "utf8" }).trim();

  // Tracked files that already differ from HEAD. A failed bump must not touch
  // these: they are the user's own uncommitted work, not something bump wrote.
  const dirtyBefore = new Set(changedTrackedPaths());

  // Track whether we've started mutating files
  let mutationStarted = false;

  // Ctrl-C reaches this process and the child it is waiting on (cargo, npm). With
  // no handler node dies on the spot, before the revert below can run, and a
  // half-applied bump is left in the tree. Catching the signal lets the child's
  // death surface as an error, which the handler below turns into a revert.
  let interrupted = false;
  const onSignal = () => {
    interrupted = true;
  };
  process.on("SIGINT", onSignal);
  process.on("SIGTERM", onSignal);
  const checkpoint = () => {
    if (interrupted) throw new Error("Interrupted");
  };

  try {
    mutationStarted = true;

    // ── VERSION (source of truth) ─────────────────────────────────────────────
    writeVersionFile(newVersion);
    console.log(`[bump] Wrote VERSION → ${newVersion}`);

    // ── Cargo workspace version (inherited by the shipped crates + Tauri) ─────
    const cargoToml = readText("Cargo.toml");
    const workspaceVersion = /^(\[workspace\.package\]\s*\nversion\s*=\s*)"[^"]+"/m;
    if (!workspaceVersion.test(cargoToml)) {
      throw new Error("Could not find [workspace.package] version in Cargo.toml");
    }
    writeText("Cargo.toml", cargoToml.replace(workspaceVersion, `$1"${newVersion}"`));
    console.log(`[bump] Wrote Cargo.toml [workspace.package] version → ${newVersion}`);

    checkpoint();

    // ── CHANGELOG.md — compile fragments ─────────────────────────────────────

    const date = todayIsoDate();
    const result = compileChangelog(newVersion, date);

    if (result.entryCount === 0) {
      throw new Error(
        "Bump aborted: no changelog entries were compiled. Ensure changes/unreleased contains valid markdown fragments.",
      );
    }

    console.log(
      `\n[bump] Compiled ${result.entryCount} changelog entr${result.entryCount === 1 ? "y" : "ies"} from ${result.consumedFiles.length} fragment${result.consumedFiles.length === 1 ? "" : "s"}:`,
    );
    for (const file of result.consumedFiles) {
      console.log(`[bump]   - ${file}`);
    }
    console.log("[bump] Category counts:");
    for (const [category, count] of Object.entries(result.categoryCounts)) {
      console.log(`[bump]   - ${category}: ${count}`);
    }

    // Sync Cargo.lock to the bumped crate versions so CI `--locked` builds pass.
    // `cargo metadata` keeps every existing git revision (`generate-lockfile`
    // would float rlx to the tip of main); its JSON output is not needed.
    checkpoint();
    console.log("[bump] Refreshing Cargo.lock (cargo metadata, pinned revisions kept)…");
    execSync("cargo metadata --format-version=1", { stdio: ["ignore", "ignore", "inherit"] });
    checkpoint();

    // ── optionally clean Rust build artifacts to free disk space ──────────────

    if (clean) {
      execSync("npm run clean:rust", { stdio: "inherit" });
    }

    // ── regenerate derived indexes ─────────────────────────────────────────────

    execSync("npm run build:settings-index", { stdio: "inherit" });
    checkpoint();

    // ── create bump commit ────────────────────────────────────────────────────

    execSync("git add -A", { stdio: "inherit" });
    execSync(`git commit -m "${newVersion}"`, { stdio: "inherit" });

    console.log(`\n\x1b[1;32m[bump] ✓ ${newVersion} committed successfully\x1b[0m`);
  } catch (err) {
    // ── Revert what bump changed on failure ───────────────────────────────────
    // node is blocked inside execSync when Ctrl-C arrives, so the handler above
    // has not run yet when the child's death throws; the error carries the signal.
    // A child that was killed is reported as `signal`, or as exit status 128+n when
    // a shell sat between us and it (130 = SIGINT, 143 = SIGTERM).
    const stoppedBySignal =
      interrupted ||
      err?.signal === "SIGINT" ||
      err?.signal === "SIGTERM" ||
      err?.status === 130 ||
      err?.status === 143;
    if (stoppedBySignal) err.interrupted = true;
    if (mutationStarted) {
      console.error(
        `\n\x1b[1;31m[bump] ${stoppedBySignal ? "Interrupted" : "Failed"} — reverting the bump's changes…\x1b[0m`,
      );
      try {
        // Restore only files this bump changed. `git checkout -- .` would also
        // throw away the user's own uncommitted edits to tracked files.
        const touched = changedTrackedPaths().filter((path) => !dirtyBefore.has(path));
        if (touched.length > 0) {
          execSync(`git checkout HEAD -- ${touched.map((path) => JSON.stringify(path)).join(" ")}`, {
            stdio: "inherit",
          });
        }
        const keptDirty = changedTrackedPaths().filter((path) => dirtyBefore.has(path));
        if (keptDirty.length > 0) {
          console.error(`[bump] Left your own uncommitted changes alone: ${keptDirty.join(", ")}`);
        }
        // Remove any untracked files created during bump (e.g. release archive)
        execSync(`git clean -fd changes/releases`, { stdio: "ignore" });
      } catch (revertErr) {
        console.error(`\x1b[31m[bump] Revert failed: ${revertErr.message}. Manual cleanup may be needed.\x1b[0m`);
        console.error(`[bump] To restore manually: git reset --hard ${headBefore}`);
      }
    }
    throw err;
  } finally {
    process.off("SIGINT", onSignal);
    process.off("SIGTERM", onSignal);
    if (wasLocal) rlxMode("local");
  }
}

main().catch((err) => {
  if (err?.message) {
    console.error(`\x1b[31m${err.message}\x1b[0m`);
  }
  // 130 = 128 + SIGINT, the conventional status for "stopped by Ctrl-C".
  process.exit(err?.interrupted ? 130 : 1);
});
