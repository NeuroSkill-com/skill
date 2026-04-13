// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.

/**
 * Locale-aware loader for help markdown content.
 *
 * Loads `.md` files from `./content/{locale}/{tab}.md` using Vite's
 * `import.meta.glob` with `?raw` query. Falls back to English when a
 * locale-specific file is missing.
 */

import { getAppName } from "$lib/stores/app-name.svelte";
import type { FaqEntry, HelpSectionData } from "./parse-help-md";
import { parseFaqMd, parseHelpMd } from "./parse-help-md";

// Eager-load all markdown files as raw strings at build time.
const modules = import.meta.glob("./content/**/*.md", {
  query: "?raw",
  eager: true,
}) as Record<string, { default: string }>;

/** Replace `{app}` with the canonical app name (same as i18n `t()`). */
function interpolate(raw: string): string {
  return raw.replaceAll("{app}", getAppName());
}

function getRaw(tab: string, locale: string): string {
  const key = `./content/${locale}/${tab}.md`;
  const fallback = `./content/en/${tab}.md`;
  const mod = modules[key] ?? modules[fallback];
  return mod?.default ?? "";
}

/** Get parsed help sections for a tab and locale. */
export function getHelpContent(tab: string, locale: string): HelpSectionData[] {
  return parseHelpMd(interpolate(getRaw(tab, locale)));
}

/** Get parsed FAQ entries for a locale. */
export function getFaqContent(locale: string): FaqEntry[] {
  return parseFaqMd(interpolate(getRaw("faq", locale)));
}
