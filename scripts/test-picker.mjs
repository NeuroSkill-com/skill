#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Interactive test suite picker — cross-platform (Node.js).
// Used by `npm test`. Falls back to numbered menu when no TTY.

import { execSync, spawn } from "child_process";
import { createInterface } from "readline";

const SUITES = [
  ["fast", "Fast (fmt + lint + clippy + vitest + rust + ci + types)"],
  ["all", "Everything (all suites)"],
  ["hooks", "Git hooks (pre-commit + pre-push)"],
  null,
  ["fmt", "Formatting (cargo fmt + biome)"],
  ["lint", "Frontend lint (biome check)"],
  ["clippy", "Rust lint (cargo clippy)"],
  ["deny", "Dependency audit (cargo deny)"],
  ["vitest", "Frontend tests (vitest)"],
  ["rust", "Rust tests tier 1"],
  ["rust:all", "Rust tests all tiers"],
  ["types", "Type checking (svelte-check)"],
  ["ci", "CI script self-test"],
  ["a11y", "Accessibility audit"],
  ["i18n", "i18n key validation"],
  ["changelog", "Changelog fragment check"],
  null,
  ["smoke", "Smoke test"],
  ["daemon", "Daemon packaging"],
  ["e2e", "LLM E2E test"],
  null,
  ["pre-commit", "Pre-commit hook checks"],
  ["pre-push", "Full pre-push hook checks"],
];

// Non-interactive: run vitest
if (!process.stdin.isTTY || !process.stdout.isTTY) {
  const child = spawn("npx", ["vitest", "run"], { stdio: "inherit", shell: true });
  child.on("exit", (code) => process.exit(code ?? 1));
} else {
  main();
}

function main() {
  let cursor = 0;
  const selected = new Array(SUITES.length).fill(false);

  function skipSep(dir) {
    while (SUITES[cursor] === null) {
      cursor = (cursor + dir + SUITES.length) % SUITES.length;
    }
  }

  function draw() {
    // Clear screen + cursor home
    process.stdout.write("\x1b[2J\x1b[H");
    process.stdout.write("\x1b[1m  Test Suite Picker\x1b[0m\n");
    process.stdout.write(
      "  \x1b[2m↑↓/jk navigate · space toggle · enter run · q quit · a all · n none\x1b[0m\n\n",
    );

    for (let i = 0; i < SUITES.length; i++) {
      const item = SUITES[i];
      if (item === null) {
        process.stdout.write(`  \x1b[2m${"─".repeat(44)}\x1b[0m\n`);
        continue;
      }
      const [, label] = item;
      const arrow = i === cursor ? "\x1b[1;36m❯ \x1b[0m" : "  ";
      const check = selected[i] ? "\x1b[32m◉\x1b[0m " : "\x1b[2m○\x1b[0m ";
      const text = i === cursor ? `\x1b[1m${label}\x1b[0m` : label;
      process.stdout.write(`${arrow}${check}${text}\n`);
    }

    const chosen = SUITES.filter((s, i) => s && selected[i])
      .map((s) => s[0])
      .join(" ");
    const display = chosen || (SUITES[cursor] ? SUITES[cursor][0] : "(nothing selected)");
    process.stdout.write(`\n  \x1b[2mWill run: ${display}\x1b[0m\n`);
  }

  function runSelected() {
    const chosen = SUITES.filter((s, i) => s && selected[i]).map((s) => s[0]);
    process.stdout.write("\x1b[2J\x1b[H");
    if (chosen.length === 0) {
      console.log("No suites selected.");
      process.exit(0);
    }
    console.log(`Running: ${chosen.join(" ")}\n`);
    const child = spawn("bash", ["scripts/test-all.sh", ...chosen], {
      stdio: "inherit",
    });
    child.on("exit", (code) => process.exit(code ?? 1));
  }

  // Enable raw mode for keypress handling
  process.stdin.setRawMode(true);
  process.stdin.resume();
  process.stdin.setEncoding("utf8");

  draw();

  process.stdin.on("data", (key) => {
    // Ctrl-C
    if (key === "\x03" || key === "q" || key === "Q") {
      process.stdout.write("\x1b[2J\x1b[H");
      process.stdin.setRawMode(false);
      process.exit(0);
    }

    // Enter
    if (key === "\r" || key === "\n") {
      process.stdin.setRawMode(false);
      process.stdin.pause();
      // If nothing was toggled, run the highlighted item
      if (!selected.some(Boolean) && SUITES[cursor] !== null) {
        selected[cursor] = true;
      }
      runSelected();
      return;
    }

    // Space — toggle
    if (key === " ") {
      if (SUITES[cursor] !== null) selected[cursor] = !selected[cursor];
    }

    // a/A — select all
    if (key === "a" || key === "A") {
      for (let i = 0; i < SUITES.length; i++) {
        if (SUITES[i] !== null) selected[i] = true;
      }
    }

    // n/N — select none
    if (key === "n" || key === "N") {
      selected.fill(false);
    }

    // Arrow up / k
    if (key === "\x1b[A" || key === "k" || key === "K") {
      cursor = (cursor - 1 + SUITES.length) % SUITES.length;
      skipSep(-1);
    }

    // Arrow down / j
    if (key === "\x1b[B" || key === "j" || key === "J") {
      cursor = (cursor + 1) % SUITES.length;
      skipSep(1);
    }

    draw();
  });
}
