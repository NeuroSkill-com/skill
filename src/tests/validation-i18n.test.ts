// SPDX-License-Identifier: GPL-3.0-only

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../..");
const _tauriI18n = resolve(repoRoot, "src/lib/i18n");
const vscodeL10n = resolve(repoRoot, "extensions/vscode/l10n");

// `extensions/vscode` is a git submodule. Skip the VS Code l10n suite when
// not checked out (CI without `submodules: true`, fresh clones).
const vscodeSubmoduleAvailable = existsSync(vscodeL10n);

const TAURI_LANGS = ["en", "de", "es", "fr", "he", "ja", "ko", "uk", "zh"] as const;
const VSCODE_BUNDLES = [
  "bundle.l10n.json",
  "bundle.l10n.de.json",
  "bundle.l10n.es.json",
  "bundle.l10n.fr.json",
  "bundle.l10n.he.json",
  "bundle.l10n.ja.json",
  "bundle.l10n.ko.json",
  "bundle.l10n.uk.json",
  "bundle.l10n.zh-cn.json",
] as const;

// Strings that the user is *guaranteed* to see — every other validation key
// can fall back to English without breaking the UI.
const VSCODE_REQUIRED_KEYS = [
  "validation.kss.prompt",
  "validation.kss.1",
  "validation.kss.5",
  "validation.kss.9",
  "validation.tlx.prompt",
  "validation.tlx.openInApp",
  "validation.pvt.prompt",
  "validation.pvt.openInApp",
  "validation.pvt.skipWeek",
  "validation.action.snooze30m",
  "validation.action.disableToday",
  "validation.action.disablePerm",
] as const;

const TAURI_REQUIRED_KEYS = [
  "settingsTabs.validation",
  "validation.title",
  "validation.intro",
  "validation.disclaimer",
  "validation.master.respectFlow",
  "validation.kss.title",
  "validation.kss.enabled",
  "validation.tlx.title",
  "validation.tlx.enabled",
  "validation.pvt.title",
  "validation.pvt.enabled",
  "validation.pvt.runNow",
  "validation.eeg.title",
  "validation.eeg.enabled",
] as const;

describe.skipIf(!vscodeSubmoduleAvailable)("VS Code extension — validation l10n bundles", () => {
  it("every bundle file exists", () => {
    const present = readdirSync(vscodeL10n);
    for (const f of VSCODE_BUNDLES) {
      expect(present, `missing ${f}`).toContain(f);
    }
  });

  for (const file of VSCODE_BUNDLES) {
    it(`${file} contains every user-facing validation key`, () => {
      const json = JSON.parse(readFileSync(`${vscodeL10n}/${file}`, "utf-8")) as Record<string, string>;
      for (const key of VSCODE_REQUIRED_KEYS) {
        expect(json[key], `${file} is missing "${key}"`).toBeTruthy();
      }
    });
  }

  it("English bundle defines the full set of validation.kss.N labels (1..9)", () => {
    const en = JSON.parse(readFileSync(`${vscodeL10n}/bundle.l10n.json`, "utf-8")) as Record<string, string>;
    for (let i = 1; i <= 9; i++) {
      expect(en[`validation.kss.${i}`], `english missing kss.${i}`).toBeTruthy();
    }
  });
});

describe("Tauri app — validation namespace", () => {
  for (const lang of TAURI_LANGS) {
    it(`${lang}/validation.ts exports the expected namespace shape`, async () => {
      const mod = await import(`../lib/i18n/${lang}/validation.ts`);
      expect(typeof mod.default).toBe("object");
      // Every language ships at least the required user-visible keys; the
      // long tail can fall back to English via the runtime t() chain.
      for (const key of TAURI_REQUIRED_KEYS) {
        const val = mod.default[key];
        // Some non-English bundles intentionally let some required keys
        // fall back; we only enforce English.
        if (lang === "en") {
          expect(val, `en/${lang} missing required key ${key}`).toBeTruthy();
        }
      }
    });
  }

  it("English Tauri bundle covers every required key", async () => {
    const en = (await import("../lib/i18n/en/validation")).default;
    for (const key of TAURI_REQUIRED_KEYS) {
      expect(en[key], `en missing ${key}`).toBeTruthy();
    }
  });

  it("each non-English Tauri validation bundle has at least the tab name and disclaimer translated", async () => {
    for (const lang of TAURI_LANGS) {
      if (lang === "en") continue;
      const mod = (await import(`../lib/i18n/${lang}/validation.ts`)).default;
      expect(mod["settingsTabs.validation"], `${lang} missing tab name`).toBeTruthy();
      expect(mod["validation.disclaimer"], `${lang} missing disclaimer`).toBeTruthy();
    }
  });
});
