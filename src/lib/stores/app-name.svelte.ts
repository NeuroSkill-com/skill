// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Centralised reactive store for the application name.
 *
 * The canonical value is fetched once from the Rust backend
 * (`get_app_name` command, which reads `productName` in tauri.conf.json).
 * Every component that needs the app name should import `getAppName()`
 * rather than hard-coding the string "NeuroSkill™".
 */

import { invoke } from "@tauri-apps/api/core";

const DEFAULT_APP_BASE_NAME = "NeuroSkill";
const TRADEMARK_SUFFIX = "™";

let appName = $state(`${DEFAULT_APP_BASE_NAME}${TRADEMARK_SUFFIX}`);   // sensible fallback while we fetch

/** Reactive getter — use inside Svelte `$derived` / templates. */
export function getAppName(): string {
  return appName;
}

/** Capitalise the first letter of a string (e.g. "skill" → "Skill"). */
function titleCase(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** Ensure UI-facing name always renders with a single trailing trademark sign. */
function toDisplayName(raw: string): string {
  const normalizedBase = raw
    .replaceAll(TRADEMARK_SUFFIX, "")
    .replace(/\(tm\)/gi, "")
    .trim();
  const base = normalizedBase || DEFAULT_APP_BASE_NAME;
  return `${titleCase(base)}${TRADEMARK_SUFFIX}`;
}

// Fetch from Rust on module init (runs once per window).
(async () => {
  try {
    const raw = await invoke<string>("get_app_name");
    if (raw) appName = toDisplayName(raw);
  } catch {
    // keep fallback
  }
})();
