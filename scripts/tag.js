#!/usr/bin/env node
import { execSync } from "node:child_process";
import { readVersionFile } from "./version-utils.mjs";

const tag = `v${readVersionFile()}`;

try {
  execSync(`git tag ${tag}`, { stdio: "inherit" });

  const remotes = execSync("git remote", { encoding: "utf8" })
    .split("\n")
    .map((name) => name.trim())
    .filter(Boolean);

  for (const remote of remotes) {
    execSync(`git push ${remote} ${tag}`, { stdio: "inherit" });
  }
} catch {
  process.exit(1);
}
