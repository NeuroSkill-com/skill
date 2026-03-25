#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import fs from "node:fs";
import path from "node:path";

const ROOT = process.cwd();
const I18N_DIR = path.join(ROOT, "src", "lib", "i18n");
const SOURCE_LOCALE = "en";
const TODO_MARKER = "TODO: translate";

function listTsFiles(dir) {
  const out = [];
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      out.push(...listTsFiles(p));
      continue;
    }
    if (ent.isFile() && p.endsWith(".ts")) out.push(p);
  }
  return out;
}

function discoverLocales() {
  if (!fs.existsSync(I18N_DIR)) return [];
  return fs
    .readdirSync(I18N_DIR, { withFileTypes: true })
    .filter((ent) => ent.isDirectory())
    .map((ent) => ent.name)
    .filter((name) => name !== SOURCE_LOCALE)
    .filter((name) => fs.existsSync(path.join(I18N_DIR, name, "index.ts")))
    .sort();
}

const locales = discoverLocales();
let failed = false;

if (locales.length === 0) {
  console.error("[i18n-locales] No non-source locales found under src/lib/i18n");
  process.exit(1);
}

for (const locale of locales) {
  const dir = path.join(I18N_DIR, locale);
  const offenders = [];

  for (const file of listTsFiles(dir)) {
    const src = fs.readFileSync(file, "utf8");
    if (src.includes(TODO_MARKER)) offenders.push(file);
  }

  if (offenders.length > 0) {
    failed = true;
    console.error(`[i18n-locales] ${locale} has untranslated fallback markers:`);
    for (const f of offenders) console.error(`  - ${path.relative(ROOT, f)}`);
  }
}

if (failed) process.exit(1);

console.log(`[i18n-locales] OK (${locales.join(", ")}) contain no TODO fallback markers`);
