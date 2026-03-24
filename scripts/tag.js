#!/usr/bin/env node
import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
const tag = `v${pkg.version}`;

try {
  execSync(`git tag ${tag}`, { stdio: "inherit" });
} catch {
  process.exit(1);
}
