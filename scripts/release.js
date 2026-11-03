#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
//
// release.js — orchestrate cutting a release candidate and promoting it.
//
// Wraps the existing primitives (`npm run bump`, `npm run tag`) plus git/gh
// commands into a single state-aware command. Detects whether you're on main
// (cutting a new release branch) or already on an existing release branch
// (iterating an RC) and does the right thing.
//
// Usage:
//   npm run release                    promote the current RC to stable
//   npm run release -- --dry-run       run the promotion checks, change nothing
//   npm run release -- --yes           promote without the confirmation prompt
//   npm run release -- --rc            cut/iterate an RC
//   npm run release -- --rc --dry-run  print what would happen, do nothing
//   npm run release -- --rc --force    pass --force through to bump
//
// Promoting rebase-merges the release PR; promote.yml then flips the RC to
// latest. Nothing is rebuilt: the bytes that ship to stable users must be
// byte-identical to the tested RC.

import { execSync, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { createInterface } from "node:readline/promises";
import { baseVersion, bumpVersion, readVersionFile } from "./version-utils.mjs";

// GitHub caps PR/issue bodies at 65_536 chars. Leave headroom for the
// surrounding template; truncate the embedded notes if they exceed this.
const NOTES_MAX_CHARS = 50_000;

// A release is complete only when every one of these finished green for its tag
// and every platform is in latest.json. v0.1.1-rc.2 published without Linux: the
// Linux job failed, the other three still created the release.
const RELEASE_WORKFLOWS = ["release-mac.yml", "release-windows.yml", "release-linux.yml", "release-catalog.yml"];
const UPDATER_PLATFORMS = ["darwin-aarch64", "windows-x86_64", "linux-x86_64"];

const PROMOTE_TIMEOUT_MS = Number(process.env.RELEASE_PROMOTE_TIMEOUT_MS) || 5 * 60_000;
const PROMOTE_POLL_MS = Number(process.env.RELEASE_PROMOTE_POLL_MS) || 10_000;

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

function gitHeadVersion() {
  // Prefer VERSION at HEAD (source of truth); fall back to package.json for older commits.
  const versionOut = sh("git", ["show", "HEAD:VERSION"], { capture: true });
  if (versionOut.status === 0) {
    const line = versionOut.stdout.trim().split(/\r?\n/)[0]?.trim() ?? "";
    if (line) return line;
  }
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

  // Sanity: HEAD's VERSION must match `currentVersion`. If it doesn't,
  // we're not on the bump commit and tagging here would produce a wrong tag.
  const headVersion = gitHeadVersion();
  if (headVersion !== currentVersion) {
    fail(
      `Cannot self-heal: HEAD VERSION (${headVersion ?? "unknown"}) doesn't match the ` +
        `current version (${currentVersion}). Resolve manually: tag the right commit, push, then re-run.`,
    );
  }

  log(`recovering: tag ${tag} is missing — completing the previous iteration first`);

  // The remote-tag check requires HEAD's commit to be reachable on the remote,
  // so the branch must be pushed before we push the tag.
  // --no-verify: bump's preflight already ran the full suite; skip the pre-push hook.
  if (!gitTracksRemote(branchName)) {
    log(`git push -u origin ${branchName} (recovery)`);
    sh("git", ["push", "--no-verify", "-u", "origin", branchName], { check: true });
  } else {
    log("git push (recovery)");
    sh("git", ["push", "--no-verify"], { check: true });
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

function ghJson(args) {
  const r = sh("gh", args, { capture: true });
  if (r.status !== 0) return null;
  try {
    return JSON.parse(r.stdout);
  } catch {
    return null;
  }
}

/// Commit a remote ref points at, peeling annotated tags.
function remoteCommit(ref) {
  const lines = captureOut(`git ls-remote origin ${ref} ${ref}^{}`).split("\n").filter(Boolean);
  return lines.at(-1)?.split(/\s+/)[0] ?? "";
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
  let yes = false;
  for (const a of args) {
    if (a === "--rc") rc = true;
    else if (a === "--dry-run") dryRun = true;
    else if (a === "--force") force = true;
    else if (a === "--yes" || a === "-y") yes = true;
    else if (a === "--help" || a === "-h") {
      printHelp();
      process.exit(0);
    } else fail(`Unknown flag: ${a}`);
  }
  if (force && !rc) fail("--force only applies to --rc.");
  if (yes && rc) fail("--yes only applies to promotion (no --rc).");
  return { rc, dryRun, force, yes };
}

function printHelp() {
  console.log(`
Usage: npm run release [-- --dry-run] [-- --yes]
       npm run release -- --rc [--dry-run] [--force]

Without --rc, promotes the current RC to stable. Run it from the release/X.Y.Z
branch. It checks that:

  - the working tree is clean and HEAD is the RC's bump commit, pushed and tagged
  - the release PR is open and mergeable
  - the GitHub release is still a pre-release, with every platform in latest.json
  - every release workflow passed for the tag

Then it asks for confirmation, rebase-merges the PR and waits for promote.yml to
mark the release as latest. Nothing is rebuilt.

With --rc, cuts or iterates a release candidate. State-aware:

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

Flags:
  --rc          Cut or iterate an RC instead of promoting.
  --dry-run     Print what would happen; do not modify anything.
  --yes, -y     Promote without the confirmation prompt.
  --force       Forwarded to bump (skip the version-tagged-on-remote check).
`);
}

// ── Promote ─────────────────────────────────────────────────────────────────

/// Everything that must hold before an RC may go to stable. Returns one entry
/// per check, so a failed run lists all that's wrong rather than the first.
function promotionChecks({ version, branch, pr }) {
  const tag = `v${version}`;
  const head = captureOut("git rev-parse HEAD");
  const checks = [];
  const check = (name, passed, detail = "") => checks.push({ name, passed, detail });

  check("working tree is clean", !gitDirty());

  // promote.yml matches the head commit message against the bare version, and
  // anything after the bump commit would be on main but not in the tested RC.
  const message = captureOut("git log -1 --format=%B");
  check("HEAD is the RC's bump commit", message === version, `HEAD is "${message.split("\n")[0]}"`);
  check(`HEAD is pushed to origin/${branch}`, remoteCommit(`refs/heads/${branch}`) === head);
  check(`tag ${tag} is on HEAD`, remoteCommit(`refs/tags/${tag}`) === head);

  check("release PR is open", Boolean(pr), `no open PR from ${branch}`);
  if (pr) {
    check(`PR #${pr.number} targets main`, pr.baseRefName === "main", `targets ${pr.baseRefName}`);
    check(`PR #${pr.number} is mergeable`, pr.mergeable === "MERGEABLE", pr.mergeable);
    check(`PR #${pr.number} head is HEAD`, pr.headRefOid === head, pr.headRefOid);
  }

  const release = ghJson(["release", "view", tag, "--json", "isPrerelease,isDraft"]);
  check(`release ${tag} exists`, Boolean(release));
  if (release) {
    check("release is a published pre-release", release.isPrerelease && !release.isDraft, "already stable or a draft");
  }

  for (const workflow of RELEASE_WORKFLOWS) {
    const runs = ghJson([
      "run",
      "list",
      "--workflow",
      workflow,
      "--branch",
      tag,
      "--limit",
      "1",
      "--json",
      "conclusion,status",
    ]);
    const run = runs?.[0];
    check(
      `${workflow} passed`,
      run?.conclusion === "success",
      run ? run.conclusion || run.status : "no run for this tag",
    );
  }

  const latest = sh("gh", ["release", "download", tag, "--pattern", "latest.json", "--output", "-"], { capture: true });
  let platforms = [];
  try {
    platforms = Object.keys(JSON.parse(latest.stdout).platforms);
  } catch {}
  for (const platform of UPDATER_PLATFORMS) {
    check(`latest.json has ${platform}`, platforms.includes(platform), "missing");
  }

  return checks;
}

async function confirmPromotion(version) {
  if (!process.stdin.isTTY)
    fail("Refusing to promote without a terminal to confirm on. Pass --yes to skip the prompt.");
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  const answer = await rl.question(`Type ${version} to ship it to all stable users: `);
  rl.close();
  return answer.trim() === version;
}

async function waitForPromotion(tag) {
  const deadline = Date.now() + PROMOTE_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const release = ghJson(["release", "view", tag, "--json", "isPrerelease"]);
    if (release && !release.isPrerelease) return true;
    await new Promise((resolve) => setTimeout(resolve, PROMOTE_POLL_MS));
  }
  return false;
}

async function promote({ dryRun, yes }) {
  const version = readVersionFile();
  const branch = gitCurrentBranch();
  const tag = `v${version}`;

  if (!version.includes("-rc.")) {
    fail(`VERSION is ${version}, not an RC. Cut one first: npm run release -- --rc`);
  }
  if (branch !== `release/${baseVersion(version)}`) {
    fail(`Promote from release/${baseVersion(version)}. You are on '${branch}'.`);
  }

  log(`promoting ${tag} from ${branch}`);
  const prs = ghJson([
    "pr",
    "list",
    "--head",
    branch,
    "--state",
    "open",
    "--json",
    "number,url,baseRefName,mergeable,headRefOid",
  ]);
  const pr = prs?.[0];

  const checks = promotionChecks({ version, branch, pr });
  for (const { name, passed, detail } of checks) {
    if (passed) ok(name);
    else console.error(`\x1b[1;31m✗\x1b[0m ${name}${detail ? ` (${detail})` : ""}`);
  }
  if (checks.some((c) => !c.passed)) {
    fail(`${tag} can't be promoted. Fix the above; a broken RC needs a new one: npm run release -- --rc`);
  }

  if (dryRun) {
    dim(`[dry-run] would rebase-merge PR #${pr.number}, then wait for ${tag} to become latest`);
    return;
  }
  if (!yes && !(await confirmPromotion(version))) fail("Not confirmed. Nothing changed.");

  // Only a rebase keeps the bare-version commit message promote.yml looks for:
  // a merge commit doesn't, and a squash takes the PR title. --match-head-commit
  // refuses the merge if the branch moved since the checks ran.
  log(`gh pr merge ${pr.number} --rebase`);
  sh("gh", ["pr", "merge", String(pr.number), "--rebase", "--match-head-commit", captureOut("git rev-parse HEAD")], {
    check: true,
  });

  log(`waiting for promote.yml to mark ${tag} as latest`);
  if (!(await waitForPromotion(tag))) {
    fail(
      `PR #${pr.number} merged, but ${tag} is still a pre-release. Check the "Promote RC to stable" run; ` +
        `to finish by hand: gh release edit ${tag} --prerelease=false --latest`,
    );
  }

  console.log("");
  ok(`${tag} is live on the stable channel`);
}

// ── Main ────────────────────────────────────────────────────────────────────

async function main() {
  const { rc, dryRun, force, yes } = parseArgs();

  ensureGhReady();

  if (!rc) {
    await promote({ dryRun, yes });
    return;
  }

  if (gitDirty()) {
    fail("Working tree is dirty. Commit or stash changes before running release.");
  }

  const currentVersion = readVersionFile();
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
  // --no-verify: bump's preflight already ran the full test suite; skip the
  // pre-push hook to avoid re-running the same cargo/vitest checks.
  if (!gitTracksRemote(branchName)) {
    log(`git push -u origin ${branchName}`);
    sh("git", ["push", "--no-verify", "-u", "origin", branchName], { check: true });
  } else {
    log("git push");
    sh("git", ["push", "--no-verify"], { check: true });
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
      "- **`npm run release` promotes the most recent RC to stable.** The promoted binary is byte-identical to the tested RC — no rebuild happens.",
      "",
      "### Merge policy",
      "",
      "Run `npm run release` from this branch rather than merging by hand. If you do merge by hand, use **rebase merge**: a merge commit or a squash loses the commit message `promote.yml` matches on, and nothing is promoted.",
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
  dim("  Watch the build under the repo's Actions tab; `npm run release` promotes the final RC to stable.");
}

main().catch((err) => {
  fail(err.message || String(err));
});
