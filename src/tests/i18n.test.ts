// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * i18n consistency tests.
 *
 * Verifies that every key in en/ (the reference locale) is present in all
 * translated locales, and that no locale has keys that don't exist in en/
 * (which would indicate a typo or a leftover from a rename).
 *
 * These tests run fast (pure file-system reads) and act as a CI gate.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { extractKeysFromDir as extractKeysWithValues, isExempt } from "../lib/i18n/i18n-utils";

const LOCALES_DIR = resolve(__dirname, "../lib/i18n");
const LOCALES = ["de", "es", "fr", "he", "uk"] as const;
const NS_FILES = [
  "common",
  "dashboard",
  "settings",
  "search",
  "calibration",
  "history",
  "hooks",
  "llm",
  "onboarding",
  "screenshots",
  "tts",
  "perm",
  "help",
  "help-ref",
  "ui",
];

/** Extract quoted keys followed by `:` from a TypeScript locale file. */
function extractKeys(filePath: string): Set<string> {
  const content = readFileSync(filePath, "utf8");
  const keys = new Set<string>();
  const re = /"([a-zA-Z][^"]*)"\s*:/g;
  // biome-ignore lint/suspicious/noAssignInExpressions: idiomatic RegExp exec loop
  for (let m: RegExpExecArray | null; (m = re.exec(content)) !== null; ) keys.add(m[1]);
  return keys;
}

/** Extract all keys from a locale directory (all namespace files). */
function extractKeysFromDir(dir: string): Set<string> {
  const keys = new Set<string>();
  for (const ns of NS_FILES) {
    const fp = resolve(dir, `${ns}.ts`);
    for (const k of extractKeys(fp)) keys.add(k);
  }
  return keys;
}

const enKeys = extractKeysFromDir(resolve(LOCALES_DIR, "en"));

describe("i18n locale sync", () => {
  it("en/ has at least 1400 keys (sanity check)", () => {
    expect(enKeys.size).toBeGreaterThanOrEqual(1400);
  });

  for (const locale of LOCALES) {
    const localeKeys = extractKeysFromDir(resolve(LOCALES_DIR, locale));

    it(`${locale}/ has no missing keys vs en/`, () => {
      const missing = [...enKeys].filter((k) => !localeKeys.has(k));
      expect(
        missing,
        `${locale}/ is missing ${missing.length} key(s): ${missing.slice(0, 5).join(", ")}${missing.length > 5 ? "…" : ""}`,
      ).toHaveLength(0);
    });

    it(`${locale}/ has no extra keys vs en/`, () => {
      const extra = [...localeKeys].filter((k) => !enKeys.has(k));
      expect(
        extra,
        `${locale}/ has ${extra.length} extra key(s): ${extra.slice(0, 5).join(", ")}${extra.length > 5 ? "…" : ""}`,
      ).toHaveLength(0);
    });

    it(`${locale}/ key count matches en/`, () => {
      expect(localeKeys.size).toBe(enKeys.size);
    });
  }
});

describe("i18n namespace file sync", () => {
  for (const locale of LOCALES) {
    for (const ns of NS_FILES) {
      const enNsKeys = extractKeys(resolve(LOCALES_DIR, "en", `${ns}.ts`));
      const locNsKeys = extractKeys(resolve(LOCALES_DIR, locale, `${ns}.ts`));

      it(`${locale}/${ns}.ts has same keys as en/${ns}.ts`, () => {
        const missing = [...enNsKeys].filter((k) => !locNsKeys.has(k));
        const extra = [...locNsKeys].filter((k) => !enNsKeys.has(k));
        expect(missing, `${locale}/${ns}.ts missing: ${missing.slice(0, 3).join(", ")}`).toHaveLength(0);
        expect(extra, `${locale}/${ns}.ts extra: ${extra.slice(0, 3).join(", ")}`).toHaveLength(0);
      });
    }
  }
});

describe("i18n placeholder consistency", () => {
  /**
   * Extract simple data-interpolation placeholders from a translation string.
   * We only flag short alphanumeric names (e.g. {n}, {shown}, {total}, {page})
   * that carry real runtime data and must be preserved exactly.
   *
   * We intentionally skip:
   *   • {app}   — app name; translators may legitimately paraphrase without it
   *   • Long / JSON-like patterns (> 20 chars) — code examples, not template vars
   */
  function dataPlaceholders(s: string): string[] {
    const matches = s.match(/\{[^}]+\}/g) ?? [];
    return matches
      .map((m) => m.slice(1, -1))
      .filter((p) => p.length <= 20 && /^[a-zA-Z_][a-zA-Z0-9_]*$/.test(p) && p !== "app");
  }

  it("all locales preserve data-interpolation placeholders from en/", () => {
    // Build en values map from all namespace files
    const enValues = new Map<string, string>();
    for (const ns of NS_FILES) {
      const content = readFileSync(resolve(LOCALES_DIR, "en", `${ns}.ts`), "utf8");
      const valueRe = /"([a-zA-Z][^"]*)"\s*:\s*"([^"]*)"/g;
      // biome-ignore lint/suspicious/noAssignInExpressions: idiomatic RegExp exec loop
      for (let m: RegExpExecArray | null; (m = valueRe.exec(content)) !== null; ) enValues.set(m[1], m[2]);
    }

    const failures: string[] = [];

    for (const locale of LOCALES) {
      for (const ns of NS_FILES) {
        const content = readFileSync(resolve(LOCALES_DIR, locale, `${ns}.ts`), "utf8");
        const valueRe = /"([a-zA-Z][^"]*)"\s*:\s*"([^"]*)"/g;
        // biome-ignore lint/suspicious/noAssignInExpressions: idiomatic RegExp exec loop
        for (let m: RegExpExecArray | null; (m = valueRe.exec(content)) !== null; ) {
          const key = m[1];
          const locVal = m[2];
          const enVal = enValues.get(key);
          if (!enVal) continue;
          const enPh = new Set(dataPlaceholders(enVal));
          const locPh = new Set(dataPlaceholders(locVal));
          for (const p of enPh) {
            if (!locPh.has(p)) failures.push(`${locale}/${ns}.ts["${key}"]: missing {${p}}`);
          }
        }
      }
    }

    expect(failures, failures.slice(0, 10).join("\n")).toHaveLength(0);
  });
});

describe("i18n untranslated value detection", () => {
  const enMap = extractKeysWithValues(resolve(LOCALES_DIR, "en"));

  for (const locale of LOCALES) {
    it(`${locale}/ has no untranslated values (identical to English)`, () => {
      const locMap = extractKeysWithValues(resolve(LOCALES_DIR, locale));
      const untranslated: string[] = [];

      for (const [key, enVal] of enMap) {
        const locVal = locMap.get(key);
        if (locVal === undefined) continue; // missing keys caught by key-sync tests
        if (locVal !== enVal) continue; // translated — value differs
        if (isExempt(key, enVal)) continue; // legitimately identical

        untranslated.push(key);
      }

      expect(
        untranslated,
        `${locale}/ has ${untranslated.length} untranslated key(s) still in English:\n` +
          untranslated
            .slice(0, 20)
            .map((k) => `  ${k}: "${(enMap.get(k) ?? "").substring(0, 60)}"`)
            .join("\n") +
          (untranslated.length > 20 ? `\n  … and ${untranslated.length - 20} more` : ""),
      ).toHaveLength(0);
    });
  }
});
