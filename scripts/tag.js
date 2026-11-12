#!/usr/bin/env node
import { execSync } from "node:child_process";
import { readVersionFile } from "./version-utils.mjs";

const tag = `v${readVersionFile()}`;
// `--no-verify` skips the pre-push hook, which re-runs clippy and tests for the
// changed crates. `npm run release -- --rc --skip-checks` passes it through.
const pushFlags = process.argv.slice(2).includes("--no-verify") ? "--no-verify " : "";

try {
  execSync(`git tag ${tag}`, { stdio: "inherit" });

  const remotes = execSync("git remote", { encoding: "utf8" })
    .split("\n")
    .map((name) => name.trim())
    .filter(Boolean);

  for (const remote of remotes) {
    execSync(`git push ${pushFlags}${remote} ${tag}`, { stdio: "inherit" });
  }
} catch {
  process.exit(1);
}
