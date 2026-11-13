#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Tag HEAD as v<VERSION> and push the tag to every remote.
//
//   npm run tag [-- --no-verify]
//
// Idempotent, so a run that died after creating the tag can simply be repeated:
// a tag already on HEAD is pushed (or skipped where the remote already has it).
// A tag that exists but points somewhere else is never moved or overwritten: that
// is a published release (or a collision with one), and the script says so.
//
// `--no-verify` skips the pre-push hook, which re-runs clippy and tests for the
// changed crates. `npm run release -- --rc --skip-checks` passes it through.

import { execFileSync } from "node:child_process";
import { readVersionFile } from "./version-utils.mjs";

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

function gitOk(...args) {
  try {
    git(...args);
    return true;
  } catch {
    return false;
  }
}

function die(message) {
  console.error(`\x1b[31m[tag] ${message}\x1b[0m`);
  process.exit(1);
}

const short = (sha) => sha.slice(0, 7);

/// The commit a tag on `remote` resolves to, or null if the remote has no such tag.
/// An annotated tag lists its own object and, on a `^{}` line, the commit it points at.
function remoteTagCommit(remote, tag) {
  const out = git("ls-remote", "--tags", remote, `refs/tags/${tag}`, `refs/tags/${tag}^{}`);
  if (!out) return null;
  const rows = out.split("\n").map((line) => line.split(/\s+/));
  const peeled = rows.find(([, ref]) => ref.endsWith("^{}"));
  return (peeled ?? rows[0])[0];
}

const noVerify = process.argv.slice(2).includes("--no-verify");
const tag = `v${readVersionFile()}`;
const head = git("rev-parse", "HEAD");

const remotes = git("remote")
  .split("\n")
  .map((name) => name.trim())
  .filter(Boolean);
if (remotes.length === 0) die("no git remote is configured, so there is nowhere to push the tag.");

// Look at every remote before changing anything, so a collision leaves no stray tag.
const onRemote = new Map();
for (const remote of remotes) {
  let existing;
  try {
    existing = remoteTagCommit(remote, tag);
  } catch (error) {
    die(`could not reach '${remote}' to check for ${tag}: ${String(error.stderr || error.message).trim()}`);
  }
  if (existing && existing !== head) {
    die(
      `${remote} already has ${tag}, pointing at ${short(existing)}, not HEAD (${short(head)}).\n` +
        `       A release with this version is already published there, and its tag is never overwritten.\n` +
        `       Bump to the next version instead.`,
    );
  }
  onRemote.set(remote, existing);
}

if (gitOk("rev-parse", "--verify", "--quiet", `refs/tags/${tag}`)) {
  const at = git("rev-list", "-n", "1", tag);
  if (at !== head) {
    die(
      `${tag} already exists locally and points at ${short(at)}, not HEAD (${short(head)}).\n` +
        `       Tags are not moved. If ${tag} was already released, VERSION is stale: bump to the next version.\n` +
        `       If that tag was a mistake and never pushed: git tag -d ${tag}, then run this again.`,
    );
  }
  console.log(`[tag] ${tag} is already on HEAD (${short(head)})`);
} else {
  git("tag", tag);
  console.log(`[tag] created ${tag} at ${short(head)}`);
}

for (const [remote, existing] of onRemote) {
  if (existing) {
    console.log(`[tag] ${remote} already has ${tag} on ${short(head)}`);
    continue;
  }
  try {
    execFileSync("git", ["push", ...(noVerify ? ["--no-verify"] : []), remote, tag], { stdio: "inherit" });
  } catch {
    die(
      `pushing ${tag} to '${remote}' failed. The tag exists locally, so once the problem is fixed: git push ${remote} ${tag}`,
    );
  }
  console.log(`[tag] pushed ${tag} to ${remote}`);
}
