#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
//
// release.js — orchestrate cutting a release candidate.
//
// Wraps the existing primitives (`npm run bump`, `npm run tag`) plus git/gh
// commands into a single state-aware command. Detects whether you're on main
// (cutting a new release branch) or already on an existing release branch
// (iterating an RC) and does the right thing.
//
// Usage:
//   npm run release -- --rc            cut/iterate an RC
//   npm run release -- --rc --dry-run  print what would happen, do nothing
//   npm run release -- --rc --force    pass --force through to bump
//
// Stable releases happen by *merging* the release PR (rebase or squash) —
// not by running this command without --rc. That's by design: the bytes that
// ship to stable users must be byte-identical to the tested RC, which only
// works if no rebuild happens at promotion time.

import { execSync, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { baseVersion, bumpVersion } from "./version-utils.mjs";

// GitHub caps PR/issue bodies at 65_536 chars. Leave headroom for the
// surrounding template; truncate the embedded notes if they exceed this.
const NOTES_MAX_CHARS = 50_000;

function readReleaseNotes(version) {
  const path = `changes/releases/${version}.md`;
  if (!existsSync(path)) return null;
  let body = readFileSync(path, "utf8").trim();
  // The file leads with `## [<version>] — <date>` which is redundant with the
  // PR's surrounding heading; strip it so the embedded section starts at the
  // first content heading (Features / Bugfixes / etc.).
  body = body.replace(/^##\s+\[[^\]]+\][^\n]*\n+/, "");
  if (body.length > NOTES_MAX_CHARS) {
    body = `${body.slice(0, NOTES_MAX_CHARS)}\n\n_…notes truncated — see \`changes/releases/${version}.md\` for the full text._`;
  }
  return body;
}

// ── Shell + git helpers ─────────────────────────────────────────────────────

function sh(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, {
    stdio: opts.capture ? ["ignore", "pipe", "pipe"] : "inherit",
    encoding: "utf8",
  });
  if (opts.check && r.status !== 0) {
    const err = new Error(`Command failed (exit ${r.status}): ${cmd} ${args.join(" ")}`);
    err.stdout = r.stdout;
    err.stderr = r.stderr;
    throw err;
  }
  return r;
}

function captureOut(cmd) {
  try {
    return execSync(cmd, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return "";
  }
}

function gitCurrentBranch() {
  return captureOut("git rev-parse --abbrev-ref HEAD");
}
function gitDirty() {
  return captureOut("git status --porcelain").length > 0;
}
function gitBranchExists(b) {
  return captureOut(`git branch --list ${b}`).length > 0;
}
function gitTracksRemote(b) {
  return captureOut(`git for-each-ref --format=%(upstream:short) refs/heads/${b}`).length > 0;
}

function gitTagExistsLocal(tag) {
  return sh("git", ["rev-parse", "--verify", `refs/tags/${tag}`], { capture: true }).status === 0;
}

function gitTagExistsOnAnyRemote(tag) {
  const remotes = captureOut("git remote")
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  for (const remote of remotes) {
    const r = sh("git", ["ls-remote", "--tags", "--exit-code", remote, `refs/tags/${tag}`], { capture: true });
    if (r.status === 0) return true;
  }
  return false;
}

function gitHeadPackageVersion() {
  // Read package.json at HEAD to confirm the current commit is the bump for `currentVersion`.
  const out = sh("git", ["show", "HEAD:package.json"], { capture: true });
  if (out.status !== 0) return null;
  try {
    return JSON.parse(out.stdout).version || null;
  } catch {
    return null;
  }
}

/// Self-heal a half-finished previous iteration: if `currentVersion`'s tag
/// is missing locally or on the remote, push the branch (if needed) and
/// create + push the tag before we try to bump. Without this, every aborted
/// push (failed pre-push hook, killed CI, network blip) wedges the release
/// branch until someone runs `npm run tag` by hand.
function ensureCurrentVersionTagged({ currentVersion, branchName, onReleaseBranch }) {
  if (!onReleaseBranch) return; // Cutting from main — there's no prior version on this branch to tag.

  const tag = `v${currentVersion}`;
  const haveLocal = gitTagExistsLocal(tag);
  const haveRemote = haveLocal && gitTagExistsOnAnyRemote(tag);

  if (haveLocal && haveRemote) return; // Nothing to recover.

  // Sanity: HEAD's package.json must match `currentVersion`. If it doesn't,
  // we're not on the bump commit and tagging here would produce a wrong tag.
  const headVersion = gitHeadPackageVersion();
  if (headVersion !== currentVersion) {
    fail(
      `Cannot self-heal: HEAD's package.json version (${headVersion ?? "unknown"}) doesn't match the ` +
        `current version (${currentVersion}). Resolve manually: tag the right commit, push, then re-run.`,
    );
  }

  log(`recovering: tag ${tag} is missing — completing the previous iteration first`);

  // The remote-tag check requires HEAD's commit to be reachable on the remote,
  // so the branch must be pushed before we push the tag.
  if (!gitTracksRemote(branchName)) {
    log(`git push -u origin ${branchName} (recovery)`);
    sh("git", ["push", "-u", "origin", branchName], { check: true });
  } else {
    log("git push (recovery)");
    sh("git", ["push"], { check: true });
  }

  log("npm run tag (recovery)");
  sh("npm", ["run", "tag"], { check: true });

  ok(`recovered: ${tag} tagged and pushed; resuming next-RC iteration`);
}

function ensureGhReady() {
  if (sh("gh", ["--version"], { capture: true }).status !== 0) {
    fail("`gh` (GitHub CLI) not installed. Install with `brew install gh` then `gh auth login`.");
  }
  if (sh("gh", ["auth", "status"], { capture: true }).status !== 0) {
    fail("`gh` is not authenticated. Run `gh auth login` first.");
  }
}

function fail(msg) {
  console.error(`\x1b[1;31m✗\x1b[0m ${msg}`);
  process.exit(1);
}

function log(msg) {
  console.log(`\x1b[1;34m→\x1b[0m ${msg}`);
}
function ok(msg) {
  console.log(`\x1b[1;32m✓\x1b[0m ${msg}`);
}
function dim(msg) {
  console.log(`\x1b[2m${msg}\x1b[0m`);
}

// ── Args ────────────────────────────────────────────────────────────────────

function parseArgs() {
  const args = process.argv.slice(2);
  let rc = false;
  let dryRun = false;
  let force = false;
  for (const a of args) {
    if (a === "--rc") rc = true;
    else if (a === "--dry-run") dryRun = true;
    else if (a === "--force") force = true;
    else if (a === "--help" || a === "-h") {
      printHelp();
      process.exit(0);
    } else fail(`Unknown flag: ${a}`);
  }
  if (!rc) {
    fail("Releases are RC-driven. Pass --rc to cut a candidate. Stable releases ship by merging the release PR.");
  }
  return { rc, dryRun, force };
}

function printHelp() {
  console.log(`
Usage: npm run release -- --rc [--dry-run] [--force]

Cuts or iterates a release candidate. State-aware:

  From main → first RC of next patch
    1. Compute next version (e.g. 0.5.0 → 0.5.1-rc.1)
    2. Create branch release/<base-version> (release/0.5.1)
    3. Run bump --rc (preflight + commit)
    4. Push branch with -u
    5. Tag v0.5.1-rc.1, push tag → CI fires
    6. Open release PR

  From release/X.Y.Z → next RC iteration
    1. Compute next iteration (rc.1 → rc.2)
    2. Run bump --rc (preflight + commit on same branch)
    3. Push
    4. Tag, push tag → CI fires
    5. Comment on the existing PR

Stable releases happen by merging the release PR using rebase or squash
merge (NOT a merge commit). The merge promotes the most recent RC's bytes
to stable; the promote workflow flips the prerelease flag with no rebuild.

Flags:
  --rc          Required. Marks the run as RC-driven.
  --dry-run     Print what would happen; do not modify anything.
  --force       Forwarded to bump (skip the version-tagged-on-remote check).
`);
}

// ── Main ────────────────────────────────────────────────────────────────────

async function main() {
  const { rc, dryRun, force } = parseArgs();

  ensureGhReady();

  if (gitDirty()) {
    fail("Working tree is dirty. Commit or stash changes before running release.");
  }

  const pkg = JSON.parse(readFileSync("package.json", "utf8"));
  const currentVersion = pkg.version;
  const newVersion = bumpVersion(currentVersion, { rc });
  const base = baseVersion(newVersion);
  const branchName = `release/${base}`;
  const tag = `v${newVersion}`;

  const currentBranch = gitCurrentBranch();
  const onMain = currentBranch === "main" || currentBranch === "master";
  const onReleaseBranch = currentBranch.startsWith("release/");

  if (!onMain && !onReleaseBranch) {
    fail(
      `Run release from main (cuts a new release branch) or from an existing release/* branch (iterates). ` +
        `You are on '${currentBranch}'.`,
    );
  }
  if (onReleaseBranch && currentBranch !== branchName) {
    fail(
      `Branch mismatch: current branch is '${currentBranch}' but the next RC's base version (${base}) ` +
        `implies branch '${branchName}'. Either switch branches or align the version manually.`,
    );
  }

  log(`current: ${currentVersion}    branch: ${currentBranch}`);
  log(`next:    ${newVersion}    tag: ${tag}    target branch: ${branchName}`);

  if (dryRun) {
    dim("[dry-run] would create/checkout branch, run bump --rc, push, run tag, then open or comment on PR");
    return;
  }

  // ── 1. Checkout / create release branch ────────────────────────────────
  if (onMain) {
    if (gitBranchExists(branchName)) {
      fail(
        `Branch ${branchName} already exists locally. Switch to it (\`git checkout ${branchName}\`) ` +
          `to iterate, or delete it first if it's stale.`,
      );
    }
    log(`git checkout -b ${branchName}`);
    sh("git", ["checkout", "-b", branchName], { check: true });
  }

  // ── 2. Self-heal: tag any prior iteration that didn't get pushed ───────
  // The previous run can die mid-flight (failed pre-push hook, killed CI,
  // network blip) after the bump commit but before `npm run tag`. That
  // leaves the release branch in a state where bump's preflight refuses to
  // run because the current version isn't tagged. Detect + recover here so
  // the user doesn't need to remember the manual `npm run tag` dance.
  ensureCurrentVersionTagged({ currentVersion, branchName, onReleaseBranch });

  // ── 3. Run bump (mutates files, runs preflight, creates commit) ────────
  const bumpArgs = ["run", "bump", "--", "--rc"];
  if (force) bumpArgs.push("--force");
  log(`npm ${bumpArgs.join(" ")}`);
  try {
    sh("npm", bumpArgs, { check: true });
  } catch (e) {
    if (onMain) {
      // Bump failed *after* we cut the branch — restore the user to main so
      // they don't get stuck on an empty release branch.
      log("bump failed — restoring main checkout");
      sh("git", ["checkout", "main"], { capture: true });
      sh("git", ["branch", "-D", branchName], { capture: true });
    }
    throw e;
  }

  // ── 3. Push branch ──────────────────────────────────────────────────────
  if (!gitTracksRemote(branchName)) {
    log(`git push -u origin ${branchName}`);
    sh("git", ["push", "-u", "origin", branchName], { check: true });
  } else {
    log("git push");
    sh("git", ["push"], { check: true });
  }

  // ── 4. Tag + push tag (existing primitive) ─────────────────────────────
  log("npm run tag");
  sh("npm", ["run", "tag"], { check: true });

  // ── 5. Open or update PR ────────────────────────────────────────────────
  const prList = sh("gh", ["pr", "list", "--head", branchName, "--state", "open", "--json", "number,url"], {
    capture: true,
  });
  let prs = [];
  try {
    prs = JSON.parse(prList.stdout || "[]");
  } catch {}

  const notes = readReleaseNotes(newVersion);

  if (prs.length === 0) {
    log("gh pr create");
    const sections = [
      `## Release v${base}`,
      "",
      `Tracking release candidates for **v${base}**.`,
      "",
      "- Each push to this branch via `npm run release -- --rc` produces a new RC build.",
      "- Users opted into the **rc** update channel receive each iteration automatically.",
      "- **Merging this PR promotes the most recent RC to stable.** The promoted binary is byte-identical to the tested RC — no rebuild happens.",
      "",
      "### Merge policy",
      "",
      "Use **rebase merge** or **squash merge**. A regular merge commit creates a new commit hash, which would break bit-identity between RC and stable.",
      "",
      "### Iterations",
      "",
      `- ${tag}`,
      "",
      "_(more added as RCs are cut)_",
    ];
    if (notes) {
      sections.push("", "---", "", `## What's in this release (\`${tag}\`)`, "", notes);
    }
    const body = sections.join("\n");
    sh(
      "gh",
      [
        "pr",
        "create",
        "--title",
        `Release v${base}`,
        "--body",
        body,
        "--base",
        "main",
        "--head",
        branchName,
        "--label",
        "release",
      ],
      { check: true },
    );
  } else {
    const pr = prs[0];
    log(`gh pr comment ${pr.number}`);
    const sections = [
      `🚀 New RC: \`${tag}\``,
      "",
      "CI is building. Once the workflow finishes, RC channel users will receive this build automatically on their next update check.",
    ];
    if (notes) {
      sections.push("", "<details><summary>Release notes for this RC</summary>", "", notes, "", "</details>");
    }
    const body = sections.join("\n");
    sh("gh", ["pr", "comment", String(pr.number), "--body", body], { check: true });
  }

  console.log("");
  ok(`Release pipeline kicked off for ${tag}`);
  dim(`  branch: ${branchName}`);
  dim(`  tag:    ${tag}`);
  dim("  Watch the build under the repo's Actions tab; merge the PR to promote the final RC to stable.");
}

main().catch((err) => {
  fail(err.message || String(err));
});
