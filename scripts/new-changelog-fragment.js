#!/usr/bin/env node
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { CATEGORY_ORDER } from "./compile-changelog.js";

function slugify(input) {
  return input
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 80);
}

function toTitle(slug) {
  return slug
    .split("-")
    .filter(Boolean)
    .map((w) => w[0].toUpperCase() + w.slice(1))
    .join(" ");
}

function parseArgs() {
  const args = process.argv.slice(2);
  let name = "";
  let category = "Docs";

  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === "--category" || a === "-c") {
      category = args[++i] || category;
    } else if (a === "--help" || a === "-h") {
      console.log("Usage: npm run changes:new -- <name> [--category <Category>]");
      console.log(`Categories: ${CATEGORY_ORDER.join(", ")}`);
      process.exit(0);
    } else if (!a.startsWith("-")) {
      name = name ? `${name}-${a}` : a;
    } else {
      throw new Error(`Unknown flag: ${a}`);
    }
  }

  if (!name) {
    throw new Error("Missing fragment name. Example: npm run changes:new -- fix-login-bug --category Bugfixes");
  }

  const canonical = CATEGORY_ORDER.find((c) => c.toLowerCase() === category.toLowerCase());
  if (!canonical) {
    throw new Error(`Invalid category \`${category}\`. Allowed: ${CATEGORY_ORDER.join(", ")}`);
  }

  return { name: slugify(name), category: canonical };
}

function nextAvailablePath(baseName) {
  const dir = "changes/unreleased";
  mkdirSync(dir, { recursive: true });

  let attempt = baseName;
  let n = 1;
  while (existsSync(join(dir, `${attempt}.md`))) {
    n += 1;
    attempt = `${baseName}-${n}`;
  }
  return join(dir, `${attempt}.md`);
}

function main() {
  const { name, category } = parseArgs();
  const path = nextAvailablePath(name || `change-${Date.now()}`);
  const title = toTitle(name) || "Short title";

  const content = `### ${category}\n\n- **${title}**: describe what changed and why.\n`;
  writeFileSync(path, content, "utf8");
  console.log(path);
}

try {
  main();
} catch (err) {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
}
