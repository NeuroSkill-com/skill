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
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline/promises";
import { stableFragmentFromRcNotes } from "./lib/release-notes.mjs";
import { baseVersion, bumpVersion, isRc, readVersionFile } from "./version-utils.mjs";

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

/// Refresh remote branches and tags, so the checks below see what is actually
/// published rather than whatever this clone last fetched. Offline is not fatal:
/// the checks that need the network then work from stale data, and say so.
function fetchRemote() {
  const ok = sh("git", ["fetch", "--quiet", "--tags", "origin"], { capture: true }).status === 0;
  if (!ok) console.error("\x1b[33m!\x1b[0m could not fetch from origin; remote-state checks use stale data.");
  return ok;
}

/// A stable `vX.Y.Z` tag means that version has shipped.
function stableShipped(base) {
  return gitTagExistsLocal(`v${base}`) || gitTagExistsOnAnyRemote(`v${base}`);
}

function tagExists(tag) {
  return gitTagExistsLocal(tag) || gitTagExistsOnAnyRemote(tag);
}

/// How many commits `origin/main` has that HEAD does not (0 if there is no origin/main).
function commitsBehindOriginMain() {
  const n = Number(captureOut("git rev-list --count HEAD..origin/main"));
  return Number.isFinite(n) ? n : 0;
}

/// `origin/<branch>`-style upstream of a local branch, or "" when it has none.
function upstreamOf(branch) {
  return captureOut(`git rev-parse --abbrev-ref ${branch}@{upstream}`);
}

/// First RC of the patch after `base`: 0.1.1 -> 0.1.2-rc.1.
function nextPatchRc(base) {
  return bumpVersion(base, { rc: true });
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
function ensureCurrentVersionTagged({ currentVersion, branchName, onReleaseBranch, skipChecks }) {
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
  sh("npm", tagArgs(skipChecks), { check: true });

  ok(`recovered: ${tag} tagged and pushed; resuming next-RC iteration`);
}

/// `npm run tag` arguments. With `skipChecks` the tag push skips the pre-push
/// hook, which would otherwise re-run clippy and tests for the changed crates.
function tagArgs(skipChecks) {
  return skipChecks ? ["run", "tag", "--", "--no-verify"] : ["run", "tag"];
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
  let stable = false;
  let dryRun = false;
  let force = false;
  let yes = false;
  let skipChecks = false;
  let crossCheck = false;
  for (const a of args) {
    if (a === "--rc") rc = true;
    else if (a === "--stable") stable = true;
    else if (a === "--dry-run") dryRun = true;
    else if (a === "--force") force = true;
    else if (a === "--yes" || a === "-y") yes = true;
    else if (a === "--skip-checks") skipChecks = true;
    else if (a === "--cross-check") crossCheck = true;
    else if (a === "--help" || a === "-h") {
      printHelp();
      process.exit(0);
    } else fail(`Unknown flag: ${a}`);
  }
  if (rc && stable) fail("--rc and --stable are different modes. Pick one.");
  const cuts = rc || stable;
  if (force && !cuts) fail("--force only applies to --rc and --stable.");
  if (yes && rc) fail("--yes only applies to shipping a release (--stable, or no flag), not to --rc.");
  if (skipChecks && !cuts) fail("--skip-checks only applies to --rc and --stable: promotion runs no local checks.");
  if (crossCheck && !cuts) fail("--cross-check only applies to --rc and --stable.");
  return { rc, stable, dryRun, force, yes, skipChecks, crossCheck };
}

function printHelp() {
  console.log(`
Usage: npm run release [-- --dry-run] [-- --yes]
       npm run release -- --rc     [--dry-run] [--force] [--skip-checks] [--cross-check]
       npm run release -- --stable [--dry-run] [--yes] [--force] [--skip-checks] [--cross-check]

Without a mode flag, promotes the current RC to stable. Run it from the release/X.Y.Z
branch. It checks that:

  - the working tree is clean and HEAD is the RC's bump commit, pushed and tagged
  - the release PR is open and mergeable
  - the GitHub release is still a pre-release, with every platform in latest.json
  - every release workflow passed for the tag

Then it asks for confirmation, rebase-merges the PR and waits for promote.yml to
mark the release as latest. Nothing is rebuilt, so the stable build is the RC's own
bytes and still reports its \`-rc.N\` version.

With --stable, ships the BARE version instead (0.1.2, not 0.1.2-rc.2), in two runs:

  Run 1, from release/X.Y.Z while VERSION is an RC:
    1. Merge the notes of every RC (changes/releases/X.Y.Z-rc.*.md) into one fragment
    2. Run bump to the bare version, push, tag vX.Y.Z -> CI builds it as stable
    3. Comment on the release PR
  Run 2, once VERSION is bare and every workflow passed:
    checks the PR, release (published, not a pre-release), workflows and latest.json,
    asks for confirmation (--yes skips it), then rebase-merges the PR.

  The stable build is a fresh build of the same commit, not the bytes tested as the RC.

With --rc, cuts or iterates a release candidate. State-aware:

  From main → first RC of next patch
    1. Compute next version (e.g. 0.5.0 → 0.5.1-rc.1). If v<base> has already shipped
       (main's VERSION still says its last RC) the next patch is used instead.
    2. Create branch release/<base-version> (release/0.5.1)
    3. Run bump (preflight + commit)
    4. Push branch with -u
    5. Tag v0.5.1-rc.1, push tag → CI fires
    6. Open release PR

  From release/X.Y.Z → next RC iteration
    1. Compute next iteration (rc.1 → rc.2)
    2. Run bump (preflight + commit on same branch)
    3. Push
    4. Tag, push tag → CI fires
    5. Comment on the existing PR

  It refuses, with the fix, to: cut an RC on an already-shipped version, reuse an
  existing tag, branch from a local main that is behind origin/main, or push a
  release branch whose upstream is not origin/<branch>.

Flags:
  --rc          Cut or iterate a release candidate.
  --stable      Ship the bare version (see above).
  --dry-run     Print what would happen; do not modify anything.
  --yes, -y     Skip the confirmation prompt when shipping.
  --force       Forwarded to bump (skip the version-tagged-on-remote check).
  --skip-checks Forwarded to bump and tag: no preflight (frontend checks, i18n check,
                vitest, clippy, cargo test) and no pre-push hook on the tag push, so the
                run does no local testing or compilation. The version/tag safety checks
                still run, and CI is the first thing to see the commit. The pre-commit
                hook (i18n locales, cargo fmt) still runs on the bump commit.
  --cross-check Type-check the Windows build first (npm run check:cross), so a break that
                only shows up off macOS fails before anything is tagged.
`);
}

function promotionChecks({ version, branch, pr, stable = false }) {
  const tag = `v${version}`;
  const head = captureOut("git rev-parse HEAD");
  const checks = [];
  const check = (name, passed, detail = "") => checks.push({ name, passed, detail });

  check("working tree is clean", !gitDirty());

  // promote.yml matches the head commit message against the bare version, and
  // anything after the bump commit would be on main but not in the tested RC.
  const message = captureOut("git log -1 --format=%B");
  check("HEAD is the version's bump commit", message === version, `HEAD is "${message.split("\n")[0]}"`);
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
    // An RC is promoted, so it must still be a pre-release. A stable build is
    // published as stable from the start, so it must not be one.
    check(
      stable ? "release is published and stable" : "release is a published pre-release",
      stable ? !release.isPrerelease && !release.isDraft : release.isPrerelease && !release.isDraft,
      stable ? "still a pre-release or a draft" : "already stable or a draft",
    );
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

// ── Stable (bare-version) release ───────────────────────────────────────────
//
// Promoting an RC ships the RC's own bytes, so the app and the updater keep
// reporting `0.1.2-rc.2`. `--stable` builds the bare version instead, in two runs:
//
//   run 1 (VERSION is an RC):  merge the RCs' notes, bump to the bare version,
//                              push, tag. CI builds it as a normal release.
//   run 2 (VERSION is bare):   once the builds are green, rebase-merge the PR.
//
// Cost: the stable build is a fresh build of the same commit, not the exact bytes
// that were tested as the RC. `v0.1.0` shipped this way.

const NOTES_FRAGMENT_DIR = "changes/unreleased";

function runCrossCheck() {
  log("npm run check:cross (Windows type-check, so a platform-only break fails here)");
  sh("npm", ["run", "check:cross"], { check: true });
}

async function cutStable({ dryRun, force, skipChecks, crossCheck }) {
  const rcVersion = readVersionFile();
  const base = baseVersion(rcVersion);
  const branch = gitCurrentBranch();
  const tag = `v${base}`;

  if (branch !== `release/${base}`) fail(`Cut ${tag} from release/${base}. You are on '${branch}'.`);
  if (gitDirty()) fail("Working tree is dirty. Commit or stash changes before running release.");
  fetchRemote();
  if (tagExists(tag)) {
    fail(`${tag} already exists, so ${base} has shipped. Start the next version from main: npm run release -- --rc`);
  }

  const fragment = stableFragmentFromRcNotes(base);
  const fragmentPath = `${NOTES_FRAGMENT_DIR}/release-${base}.md`;
  if (!fragment) {
    fail(
      `No release notes found for ${base}: changes/releases/${base}-rc.*.md are missing or empty. ` +
        `Write them to ${fragmentPath} yourself, commit, and run: npm run bump -- ${base}`,
    );
  }
  if (existsSync(fragmentPath)) fail(`${fragmentPath} already exists. Remove it or commit it first, then re-run.`);
  const entries = (fragment.match(/^- /gm) ?? []).length;

  log(`current: ${rcVersion}    cutting stable: ${base}    tag: ${tag}`);
  if (dryRun) {
    dim(
      `[dry-run] would write ${fragmentPath} (${entries} entries from the RC notes), run bump ${base}` +
        `${skipChecks ? " --skip-checks" : ""}, push, run tag${skipChecks ? " --no-verify" : ""}, then comment on the PR`,
    );
    return;
  }

  if (crossCheck) runCrossCheck();
  ensureCurrentVersionTagged({ currentVersion: rcVersion, branchName: branch, onReleaseBranch: true, skipChecks });

  mkdirSync(NOTES_FRAGMENT_DIR, { recursive: true });
  writeFileSync(fragmentPath, fragment);
  log(`wrote ${fragmentPath} (${entries} entries)`);

  const bumpArgs = ["run", "bump", "--", base];
  if (force) bumpArgs.push("--force");
  if (skipChecks) bumpArgs.push("--skip-checks");
  log(`npm ${bumpArgs.join(" ")}`);
  try {
    sh("npm", bumpArgs, { check: true });
  } catch (e) {
    // bump reverts its own changes but leaves untracked files; this one is ours.
    rmSync(fragmentPath, { force: true });
    throw e;
  }

  if (!gitTracksRemote(branch)) {
    log(`git push -u origin ${branch}`);
    sh("git", ["push", "--no-verify", "-u", "origin", branch], { check: true });
  } else {
    log("git push");
    sh("git", ["push", "--no-verify"], { check: true });
  }
  log(`npm ${tagArgs(skipChecks).join(" ")}`);
  sh("npm", tagArgs(skipChecks), { check: true });

  const prs = ghJson(["pr", "list", "--head", branch, "--state", "open", "--json", "number"]);
  if (prs?.[0]) {
    const body =
      `🚀 Stable build: \`${tag}\`\n\nCI is building it as a normal (non-pre-release) release. ` +
      "When every workflow has passed, run `npm run release -- --stable` again to rebase-merge this PR.";
    sh("gh", ["pr", "comment", String(prs[0].number), "--body", body], { check: true });
  } else {
    console.error(`\x1b[33m!\x1b[0m no open PR from ${branch}; open one against main before the final step.`);
  }

  console.log("");
  ok(`${tag} is building`);
  dim("  When all workflows pass, run `npm run release -- --stable` again to merge the PR.");
}

async function finishStable({ dryRun, yes }) {
  const version = readVersionFile();
  const branch = gitCurrentBranch();
  const tag = `v${version}`;

  if (branch !== `release/${version}`) fail(`Finish ${tag} from release/${version}. You are on '${branch}'.`);
  fetchRemote();
  log(`finishing ${tag} from ${branch}`);

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

  const checks = promotionChecks({ version, branch, pr, stable: true });
  for (const { name, passed, detail } of checks) {
    if (passed) ok(name);
    else console.error(`\x1b[1;31m✗\x1b[0m ${name}${detail ? ` (${detail})` : ""}`);
  }
  if (checks.some((c) => !c.passed)) {
    fail(`${tag} is not ready to finish. Fix the above, or wait for the builds, then re-run.`);
  }

  if (dryRun) {
    dim(`[dry-run] would rebase-merge PR #${pr.number}`);
    return;
  }
  if (!yes && !(await confirmPromotion(version))) fail("Not confirmed. Nothing changed.");

  // Rebase keeps the history linear. promote.yml ignores this merge on purpose:
  // the commit message is the bare version, and the release is already stable.
  log(`gh pr merge ${pr.number} --rebase`);
  sh("gh", ["pr", "merge", String(pr.number), "--rebase", "--match-head-commit", captureOut("git rev-parse HEAD")], {
    check: true,
  });

  const latest = ghJson(["release", "list", "--limit", "5", "--json", "tagName,isLatest"]);
  const isLatest = latest?.some((r) => r.tagName === tag && r.isLatest);
  console.log("");
  if (isLatest) ok(`${tag} is live on the stable channel`);
  else {
    console.error(
      `\x1b[33m!\x1b[0m PR #${pr.number} merged, but ${tag} is not marked "latest". If it should be: ` +
        `gh release edit ${tag} --latest`,
    );
  }
}

// ── Main ────────────────────────────────────────────────────────────────────

async function main() {
  const { rc, stable, dryRun, force, yes, skipChecks, crossCheck } = parseArgs();

  ensureGhReady();

  if (stable) {
    if (isRc(readVersionFile())) await cutStable({ dryRun, force, skipChecks, crossCheck });
    else await finishStable({ dryRun, yes });
    return;
  }

  if (!rc) {
    await promote({ dryRun, yes });
    return;
  }

  if (gitDirty()) {
    fail("Working tree is dirty. Commit or stash changes before running release.");
  }

  // Everything below is decided from what is published, not from what this clone
  // last saw: a stale origin/main or a missing tag is how releases go wrong.
  fetchRemote();

  const currentVersion = readVersionFile();
  let newVersion = bumpVersion(currentVersion, { rc });
  let base = baseVersion(newVersion);

  const currentBranch = gitCurrentBranch();
  const onMain = currentBranch === "main" || currentBranch === "master";
  const onReleaseBranch = currentBranch.startsWith("release/");

  // Promotion leaves `main`'s VERSION at the last RC (0.1.1-rc.3) even after
  // v0.1.1 ships, so "the next RC" would be a second 0.1.1-rc.4 on a released
  // version. A stable tag means that version is done: start the next patch.
  if (onMain) {
    for (let guard = 0; guard < 20 && stableShipped(base); guard++) {
      const next = nextPatchRc(base);
      log(`v${base} has already shipped; starting the next patch: ${next}`);
      newVersion = next;
      base = baseVersion(newVersion);
    }
  } else if (onReleaseBranch && stableShipped(base)) {
    fail(
      `v${base} has already shipped, so release/${base} is finished and a new RC here would collide with it. ` +
        `Start the next patch from main: git checkout main && git pull --ff-only && npm run release -- --rc`,
    );
  }

  const branchName = `release/${base}`;
  const tag = `v${newVersion}`;

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

  if (tagExists(tag)) {
    fail(
      `${tag} already exists, so ${newVersion} cannot be cut again (a published tag is never moved). Pick a version that is free.`,
    );
  }
  if (onMain) {
    const behind = commitsBehindOriginMain();
    if (behind > 0) {
      fail(
        `Local ${currentBranch} is ${behind} commit${behind === 1 ? "" : "s"} behind origin/main, so a release branch cut ` +
          `from it would be missing them (and rebuild history that main already has). Run: git pull --ff-only`,
      );
    }
  }
  if (onReleaseBranch) {
    const upstream = upstreamOf(currentBranch);
    if (upstream && upstream !== `origin/${currentBranch}`) {
      fail(
        `'${currentBranch}' tracks '${upstream}', so a plain \`git push\` refuses to push it. ` +
          `Fix: git branch --set-upstream-to=origin/${currentBranch} ${currentBranch}`,
      );
    }
  }

  log(`current: ${currentVersion}    branch: ${currentBranch}`);
  log(`next:    ${newVersion}    tag: ${tag}    target branch: ${branchName}`);

  if (dryRun) {
    dim(
      `[dry-run] would create/checkout branch, run bump ${newVersion}${skipChecks ? " --skip-checks" : ""}, push, ` +
        `run tag${skipChecks ? " --no-verify" : ""}, then open or comment on PR`,
    );
    return;
  }

  if (crossCheck) runCrossCheck();

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
  ensureCurrentVersionTagged({ currentVersion, branchName, onReleaseBranch, skipChecks });

  // ── 3. Run bump (mutates files, runs preflight, creates commit) ────────
  // The version is passed explicitly: bump derives its own from VERSION, which
  // is exactly what is wrong when main still says an already-shipped RC.
  const bumpArgs = ["run", "bump", "--", newVersion];
  if (force) bumpArgs.push("--force");
  if (skipChecks) bumpArgs.push("--skip-checks");
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
  log(`npm ${tagArgs(skipChecks).join(" ")}`);
  sh("npm", tagArgs(skipChecks), { check: true });

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
      "- **`npm run release` promotes the most recent RC to stable.** The promoted binary is byte-identical to the tested RC — no rebuild happens, so it still reports the `-rc.N` version.",
      "- **`npm run release -- --stable` builds the bare version instead** (a fresh build of the same commit), then merges this PR once the builds pass.",
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
