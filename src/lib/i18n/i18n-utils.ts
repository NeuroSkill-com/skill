// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Shared i18n utilities for scripts (sync-i18n, audit-i18n).
 */
import fs from "fs";
import path from "path";

/**
 * Extract {key → value} map from a .ts locale file (namespace or flat).
 * Handles both:  "key.name":   "value"
 * and:           "key.name":   `template string`
 */
export function extractKeys(filePath: string): Map<string, string> {
  const src = fs.readFileSync(filePath, "utf8");
  const map = new Map<string, string>();
  const re = /^\s*"([^"]+)"\s*:\s*(?:"((?:[^"\\]|\\.)*)"|`((?:[^`\\]|\\.)*)`)/gm;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const key = m[1];
    const val = m[2] !== undefined ? m[2] : m[3];
    map.set(key, val);
  }
  return map;
}

/** Namespace files in the expected order. */
export const NS_FILES = [
  "common", "dashboard", "settings", "search", "calibration",
  "history", "hooks", "llm", "onboarding", "screenshots",
  "tts", "perm", "help", "help-ref", "ui",
];

/**
 * Extract keys from a locale directory (merges all namespace .ts files).
 */
export function extractKeysFromDir(dirPath: string): Map<string, string> {
  const map = new Map<string, string>();
  if (!fs.existsSync(dirPath) || !fs.statSync(dirPath).isDirectory()) {
    // Fall back to single-file extraction
    return extractKeys(dirPath);
  }
  for (const ns of NS_FILES) {
    const fp = path.join(dirPath, `${ns}.ts`);
    if (fs.existsSync(fp)) {
      const sub = extractKeys(fp);
      for (const [k, v] of sub) map.set(k, v);
    }
  }
  return map;
}
