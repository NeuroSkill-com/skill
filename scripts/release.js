#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
//
// release.js — cut a release candidate or a stable release.
//
//   npm run release rc        next RC:     0.1.3 → 0.1.4-rc.1,  0.1.4-rc.1 → 0.1.4-rc.2
//   npm run release stable    stable:      0.1.4-rc.2 → 0.1.4   (no RC yet: 0.1.3 → 0.1.4)
//   npm run release status    what VERSION says, and where its tag's release stands
//
// Both cuts run from main (or a release/* branch, for a hotfix on an older line):
// bump (version + changelog + commit "<version>"), push the branch, push the tag.
// The tag starts .github/workflows/release.yml, which builds every platform and
// publishes the GitHub release only when all of them succeeded. RC tags publish
// as pre-releases (the rc update channel); a bare version publishes as latest.
// There is no release PR and no promotion step: every version is its own build.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline/promises";
import { stableFragmentFromRcNotes } from "./lib/release-notes.mjs";
import { baseVersion, bumpVersion, isRc, parseVersion, readVersionFile } from "./version-utils.mjs";

const NOTES_FRAGMENT_DIR = "changes/unreleased";

// ── Helpers ─────────────────────────────────────────────────────────────────

function sh(cmd, args, { capture = false, check = true } = {}) {
  const r = spawnSync(cmd, args, {
    stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
    encoding: "utf8",
    shell: process.platform === "win32",
  });
  if (check && r.status !== 0) throw new Error(`Command failed (exit ${r.status}): ${cmd} ${args.join(" ")}`);
  return r;
}

const out = (cmd, args) => (sh(cmd, args, { capture: true, check: false }).stdout || "").trim();
const git = (...args) => out("git", args);

function fail(msg) {
  console.error(`\x1b[1;31m✗\x1b[0m ${msg}`);
  process.exit(1);
}
const log = (msg) => console.log(`\x1b[1;34m→\x1b[0m ${msg}`);
const ok = (msg) => console.log(`\x1b[1;32m✓\x1b[0m ${msg}`);
const dim = (msg) => console.log(`\x1b[2m${msg}\x1b[0m`);

/// The tag exists locally or on origin (fetched just before this is asked).
const tagExists = (tag) => Boolean(git("rev-parse", "--verify", "--quiet", `refs/tags/${tag}`));

function nextPatch(version) {
  const { major, minor, patch } = parseVersion(version);
  return `${major}.${minor}.${patch + 1}`;
}

// ── Version planning ────────────────────────────────────────────────────────

/// The version a cut produces. A base version whose stable tag exists has
/// shipped, so its RCs are finished and the next cut starts the next patch.
function plan(kind, current) {
  const shipped = (v) => tagExists(`v${baseVersion(v)}`);
  if (kind === "rc") {
    if (isRc(current) && !shipped(current)) return bumpVersion(current, { rc: true });
    return `${nextPatch(baseVersion(current))}-rc.1`;
  }
  if (isRc(current) && !shipped(current)) return baseVersion(current);
  return nextPatch(baseVersion(current));
}

// ── Cut ─────────────────────────────────────────────────────────────────────

async function confirm(version) {
  if (!process.stdin.isTTY) fail("Refusing to ship stable without a terminal to confirm on. Pass --yes.");
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  const answer = await rl.question(`Type ${version} to build and publish it to every stable user: `);
  rl.close();
  return answer.trim() === version;
}

async function cut(kind, { dryRun, check, crossCheck, force, yes }) {
  const branch = git("rev-parse", "--abbrev-ref", "HEAD");
  if (branch !== "main" && !branch.startsWith("release/")) {
    fail(`Release from main (or a release/* hotfix branch). You are on '${branch}'.`);
  }
  if (git("status", "--porcelain", "--untracked-files=no")) {
    if (!dryRun) fail("Working tree is dirty. Commit or stash first.");
    dim("[dry-run] working tree is dirty; a real cut would stop here");
  }

  // Decide from what is published, not from what this clone last saw.
  log("git fetch --tags origin");
  sh("git", ["fetch", "--quiet", "--tags", "origin"]);
  const upstream = git("rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}");
  if (upstream) {
    const behind = Number(git("rev-list", "--count", `HEAD..${upstream}`)) || 0;
    if (behind > 0) fail(`${branch} is ${behind} commit(s) behind ${upstream}. Run: git pull --ff-only`);
  }

  const current = readVersionFile();
  const next = plan(kind, current);
  const tag = `v${next}`;
  if (tagExists(tag)) fail(`${tag} already exists, and a published tag is never moved.`);

  // A previous run that died between its bump commit and its tag left VERSION
  // untagged; bump refuses to go past that, so finish the tag first.
  if (!tagExists(`v${current}`) && git("log", "-1", "--format=%s") === current) {
    log(`v${current} was bumped but never tagged; tagging it first`);
    if (!dryRun) sh("npm", ["run", "-s", "tag"]);
  }

  // A stable release after RCs ships the RCs' combined notes as its own.
  let fragmentPath = null;
  let fragment = null;
  if (kind === "stable" && isRc(current) && baseVersion(current) === next) {
    fragment = stableFragmentFromRcNotes(next);
    fragmentPath = `${NOTES_FRAGMENT_DIR}/release-${next}.md`;
    if (fragment && existsSync(fragmentPath)) fail(`${fragmentPath} already exists. Remove or commit it first.`);
  }

  log(`${current} → ${next}  (branch ${branch}, tag ${tag})`);
  if (dryRun) {
    dim(
      `[dry-run] would ${fragment ? `write ${fragmentPath} from the RC notes, ` : ""}` +
        `run bump ${next}${check ? " --check" : ""}, push ${branch}, push ${tag}`,
    );
    return;
  }
  if (kind === "stable" && !yes && !(await confirm(next))) fail("Not confirmed. Nothing changed.");

  if (crossCheck) {
    log("npm run check:cross (Windows type-check, so a platform-only break fails before tagging)");
    sh("npm", ["run", "check:cross"]);
  }

  if (fragment) {
    mkdirSync(NOTES_FRAGMENT_DIR, { recursive: true });
    writeFileSync(fragmentPath, fragment);
    log(`wrote ${fragmentPath} (${(fragment.match(/^- /gm) ?? []).length} entries from the RC notes)`);
  }

  const bumpArgs = ["run", "bump", "--", next, ...(check ? ["--check"] : []), ...(force ? ["--force"] : [])];
  log(`npm ${bumpArgs.join(" ")}`);
  try {
    sh("npm", bumpArgs);
  } catch (e) {
    // bump reverts its own changes; the notes fragment is ours to remove.
    if (fragmentPath) rmSync(fragmentPath, { force: true });
    throw e;
  }

  log(upstream ? "git push" : `git push -u origin ${branch}`);
  sh("git", upstream ? ["push"] : ["push", "-u", "origin", branch]);
  sh("npm", ["run", "-s", "tag"]);

  console.log("");
  ok(`${tag} pushed — release.yml is building every platform`);
  dim(`  watch:  npm run release status`);
  dim(`  ${isRc(next) ? "RC channel users" : "Every user"} gets it once all platforms are built and published.`);
  if (kind === "stable" && branch !== "main") {
    dim(`  ${branch} is a side branch: merge it back into main so main's VERSION moves on.`);
  }
}

// ── Status ──────────────────────────────────────────────────────────────────

function status() {
  const version = readVersionFile();
  const tag = `v${version}`;
  console.log(`VERSION  ${version}  (${isRc(version) ? "release candidate" : "stable"})`);
  console.log(`branch   ${git("rev-parse", "--abbrev-ref", "HEAD")}`);

  if (!git("ls-remote", "--tags", "origin", `refs/tags/${tag}`)) {
    console.log(`tag      ${tag} is not on origin yet — npm run release rc|stable cuts the next one`);
    return;
  }
  console.log(`tag      ${tag} on origin`);

  const runs = out("gh", [
    "run",
    "list",
    "--workflow",
    "release.yml",
    "--branch",
    tag,
    "--limit",
    "1",
    "--json",
    "status,conclusion,url",
  ]);
  try {
    const [run] = JSON.parse(runs || "[]");
    console.log(`build    ${run ? `${run.conclusion || run.status}  ${run.url}` : "no release.yml run for this tag"}`);
  } catch {
    console.log("build    (gh unavailable)");
  }

  const rel = out("gh", ["release", "view", tag, "--json", "isDraft,isPrerelease,url"]);
  try {
    const r = JSON.parse(rel);
    const state = r.isDraft
      ? "draft (not published)"
      : r.isPrerelease
        ? "published as pre-release (rc channel)"
        : "published (stable)";
    console.log(`release  ${state}  ${r.url}`);
  } catch {
    console.log("release  not published yet");
  }
}

// ── Main ────────────────────────────────────────────────────────────────────

const HELP = `
Usage: npm run release rc     [-- --dry-run] [--check] [--cross-check] [--force]
       npm run release stable [-- --dry-run] [--check] [--cross-check] [--force] [--yes]
       npm run release status

  rc        Cut the next release candidate (pre-release, rc update channel).
  stable    Cut the stable version: the current RC's base version, or the next
            patch if there is no open RC. Asks you to type the version first.
  status    Show VERSION, its tag, the release.yml build and the GitHub release.

Run from main, or from a release/* branch to patch an older line. A cut bumps the
version, compiles changes/unreleased into the changelog, commits "<version>",
pushes the branch and the tag. CI builds all platforms from the tag and publishes
the release only once every platform succeeded.

  --dry-run      Show what would happen; change nothing.
  --check        Run the full check list (npm test -- full) before bumping.
  --cross-check  Type-check the Windows build first (npm run check:cross).
  --force        Passed to bump: skip its "previous version is tagged" check.
  --yes, -y      stable: skip the confirmation prompt.
`;

async function main() {
  const args = process.argv.slice(2);
  const flags = new Set(args.filter((a) => a.startsWith("-")));
  const [command] = args.filter((a) => !a.startsWith("-"));
  const known = new Set(["--dry-run", "--check", "--cross-check", "--force", "--yes", "-y", "--help", "-h"]);
  for (const f of flags) if (!known.has(f)) fail(`Unknown flag: ${f}\n${HELP}`);

  if (!command || flags.has("--help") || flags.has("-h")) {
    console.log(HELP);
    if (!command) status();
    return;
  }
  if (command === "status") return status();
  if (command !== "rc" && command !== "stable") fail(`Unknown command: ${command}\n${HELP}`);

  await cut(command, {
    dryRun: flags.has("--dry-run"),
    check: flags.has("--check"),
    crossCheck: flags.has("--cross-check"),
    force: flags.has("--force"),
    yes: flags.has("--yes") || flags.has("-y"),
  });
}

main().catch((err) => fail(err.message || String(err)));
