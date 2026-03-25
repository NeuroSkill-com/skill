#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import fs from "node:fs";
import path from "node:path";

const ROOT = process.cwd();
const I18N_DIR = path.join(ROOT, "src", "lib", "i18n");
const CRITICAL_LOCALES = ["de", "he"];
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

let failed = false;

for (const locale of CRITICAL_LOCALES) {
  const dir = path.join(I18N_DIR, locale);
  if (!fs.existsSync(dir)) {
    console.error(`[i18n-critical] Missing locale directory: ${dir}`);
    failed = true;
    continue;
  }

  const offenders = [];
  for (const file of listTsFiles(dir)) {
    const src = fs.readFileSync(file, "utf8");
    if (src.includes(TODO_MARKER)) offenders.push(file);
  }

  if (offenders.length > 0) {
    failed = true;
    console.error(`[i18n-critical] ${locale} has untranslated fallback markers:`);
    for (const f of offenders) console.error(`  - ${path.relative(ROOT, f)}`);
  }
}

if (failed) process.exit(1);

console.log("[i18n-critical] OK (de/he contain no TODO fallback markers)");
