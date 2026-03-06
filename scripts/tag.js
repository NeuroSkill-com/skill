#!/usr/bin/env node
import { readFileSync } from "fs";
import { execSync } from "child_process";

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
const tag = `v${pkg.version}`;

try {
  execSync(`git tag ${tag}`, { stdio: "inherit" });
} catch {
  process.exit(1);
}

console.log(`\nTag ${tag} created locally.`);
console.log(`\nTo push it to the remote, run:\n`);
console.log(`  git push origin ${tag}\n`);
