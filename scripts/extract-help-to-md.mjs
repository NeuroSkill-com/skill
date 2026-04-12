#!/usr/bin/env node
/**
 * One-time extraction script: reads each language's help.ts (and tts.ts for helpTts keys)
 * and generates markdown files matching the English structure.
 *
 * Usage: node scripts/extract-help-to-md.mjs
 */
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const I18N = join(ROOT, "src/lib/i18n");
const CONTENT = join(ROOT, "src/lib/help/content");

const LANGS = ["de", "es", "fr", "he", "uk", "zh"];

// ── Parse a TS help file into a key→value map ──────────────────────────────
function parseHelpTs(filePath) {
  if (!existsSync(filePath)) return {};
  const src = readFileSync(filePath, "utf-8");
  const map = {};
  // Match "key": "value" or "key": 'value' patterns (possibly multi-line via concatenation)
  const re = /"([^"]+)":\s*\n?\s*"((?:[^"\\]|\\.)*)"/g;
  let m;
  while ((m = re.exec(src))) {
    map[m[1]] = m[2].replace(/\\"/g, '"').replace(/\\n/g, "\n");
  }
  // Also try single-quoted values
  const re2 = /"([^"]+)":\s*\n?\s*'((?:[^'\\]|\\.)*)'/g;
  while ((m = re2.exec(src))) {
    map[m[1]] = m[2].replace(/\\'/g, "'").replace(/\\n/g, "\n");
  }
  return map;
}

// ── Read English markdown to extract structure ──────────────────────────────
function readEnglishMd(tab) {
  return readFileSync(join(CONTENT, "en", `${tab}.md`), "utf-8");
}

// ── Map from English text → i18n key for a given prefix ─────────────────────
function buildReverseMap(enMap, prefix) {
  const rev = {};
  for (const [k, v] of Object.entries(enMap)) {
    if (k.startsWith(prefix)) {
      rev[v] = k;
    }
  }
  return rev;
}

// ── Replace English text in markdown with translated text ───────────────────
function translateMd(enMd, enMap, langMap, prefixes) {
  // Build a map of English value → translated value
  const translations = {};
  for (const prefix of prefixes) {
    for (const [key, enVal] of Object.entries(enMap)) {
      if (key.startsWith(prefix) && langMap[key]) {
        translations[enVal] = langMap[key];
      }
    }
  }

  // For each line, try to replace English content with translation
  const lines = enMd.split("\n");
  const result = [];
  for (const line of lines) {
    if (line.startsWith("# ") || line.startsWith("## ")) {
      const prefix = line.startsWith("## ") ? "## " : "# ";
      const heading = line.slice(prefix.length);
      const translated = translations[heading];
      result.push(translated ? `${prefix}${translated}` : line);
    } else if (line.trim() === "") {
      result.push(line);
    } else {
      // Body text line - try to find it in translations
      const translated = translations[line.trim()];
      result.push(translated ?? line);
    }
  }
  return result.join("\n");
}

// ── Tab config: which i18n prefixes map to which markdown file ──────────────
const TABS = [
  { tab: "dashboard",  prefixes: ["helpDash."] },
  { tab: "settings",   prefixes: ["helpSettings."] },
  { tab: "windows",    prefixes: ["helpWindows."] },
  { tab: "api",        prefixes: ["helpApi."] },
  { tab: "privacy",    prefixes: ["helpPrivacy."] },
  { tab: "hooks",      prefixes: ["helpHooks."] },
  { tab: "llm",        prefixes: ["helpLlm."] },
  { tab: "tts",        prefixes: ["helpTts."] },
  { tab: "faq",        prefixes: ["helpFaq."] },
  { tab: "faq-old",    prefixes: ["helpOld."] },
];

// ── Load English map ────────────────────────────────────────────────────────
const enHelp = parseHelpTs(join(I18N, "en/help.ts"));
const enTts = parseHelpTs(join(I18N, "en/tts.ts"));
const enMap = { ...enHelp, ...enTts };

// ── Process each language ───────────────────────────────────────────────────
for (const lang of LANGS) {
  const langHelp = parseHelpTs(join(I18N, `${lang}/help.ts`));
  const langTts = parseHelpTs(join(I18N, `${lang}/tts.ts`));
  const langMap = { ...langHelp, ...langTts };

  const outDir = join(CONTENT, lang);
  mkdirSync(outDir, { recursive: true });

  for (const { tab, prefixes } of TABS) {
    const enMd = readEnglishMd(tab);
    const translated = translateMd(enMd, enMap, langMap, prefixes);
    writeFileSync(join(outDir, `${tab}.md`), translated, "utf-8");
    const keyCount = Object.keys(langMap).filter(k => prefixes.some(p => k.startsWith(p))).length;
    console.log(`  ${lang}/${tab}.md — ${keyCount} keys found`);
  }
}

console.log("\nDone! Markdown files written to src/lib/help/content/");
