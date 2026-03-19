#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Screenshot automation — captures every app screen in light & dark mode.
// Usage:  node scripts/screenshots/take-screenshots.mjs
//
// Prerequisites:
//   npx playwright install chromium
//
// Output: docs/screenshots/<route>-<light|dark>.png

import { chromium } from "playwright";
import { spawn }    from "node:child_process";
import { mkdirSync, existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { buildTauriMock } from "./tauri-mock.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT      = resolve(__dirname, "../..");
const OUT_DIR   = resolve(ROOT, "docs/screenshots");
const DEV_PORT  = 1420;
const BASE_URL  = `http://localhost:${DEV_PORT}`;

// ── Routes to screenshot ────────────────────────────────────────────────────
// Each entry: [slug for filename, path, optional viewport override, optional extra wait ms]
const ROUTES = [
  ["dashboard",      "/",               { width: 1280, height: 900 }],
  ["about",          "/about",          { width: 520,  height: 620 }],
  ["settings",       "/settings",       { width: 1100, height: 780 }],
  ["history",        "/history",        { width: 1100, height: 780 }],
  ["chat",           "/chat",           { width: 1100, height: 780 }],
  ["search",              "/search",                    { width: 1100, height: 780 }],
  ["search-text",         "/search?mode=text",          { width: 1100, height: 780 }],
  ["search-eeg",          "/search?mode=eeg",           { width: 1100, height: 780 }],
  ["search-images",       "/search?mode=images",        { width: 1100, height: 780 }],
  ["labels",         "/labels",         { width: 900,  height: 700 }],
  ["label",          "/label",          { width: 700,  height: 520 }],
  ["focus-timer",    "/focus-timer",    { width: 700,  height: 620 }],
  ["help",           "/help",           { width: 1100, height: 780 }],
  ["compare",        "/compare",        { width: 1100, height: 780 }],
  ["downloads",      "/downloads",      { width: 900,  height: 700 }],
  ["calibration",    "/calibration",    { width: 800,  height: 640 }],
  ["onboarding",     "/onboarding",     { width: 800,  height: 640 }],
  ["api",            "/api",            { width: 900,  height: 700 }],
  ["whats-new",      "/whats-new",      { width: 700,  height: 620 }],
  ["session",        "/session?csv_path=/data/session_20260318_120000.csv", { width: 1100, height: 780 }],
];

// ── Settings sub-tabs ───────────────────────────────────────────────────────
const SETTINGS_TABS = [
  "goals", "devices", "exg", "sleep", "calibration", "tts", "llm", "tools",
  "model", "embeddings", "screenshots", "hooks", "appearance", "settings",
  "shortcuts", "umap", "updates", "permissions",
];

// ── Help sub-tabs ───────────────────────────────────────────────────────────
const HELP_TABS = [
  "dashboard", "electrodes", "settings", "windows", "api", "tts", "llm",
  "hooks", "privacy", "references", "faq",
];

// ── Tauri API Mock ──────────────────────────────────────────────────────────
// buildTauriMock is imported from ./tauri-mock.mjs

// ── Helpers ─────────────────────────────────────────────────────────────────

async function waitForServer(url, timeoutMs = 30000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const resp = await fetch(url);
      if (resp.ok) return true;
    } catch { /* not ready yet */ }
    await new Promise(r => setTimeout(r, 500));
  }
  throw new Error(`Dev server not ready after ${timeoutMs}ms`);
}

function slug(name) {
  return name.replace(/[^a-z0-9-]/gi, "-").toLowerCase();
}

// ── Main ────────────────────────────────────────────────────────────────────

async function main() {
  mkdirSync(OUT_DIR, { recursive: true });

  // Start Vite dev server
  console.log("Starting Vite dev server...");
  const vite = spawn("npx", ["vite", "dev", "--port", String(DEV_PORT)], {
    cwd: ROOT,
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, BROWSER: "none" },
  });

  let viteOutput = "";
  vite.stdout.on("data", d => { viteOutput += d.toString(); });
  vite.stderr.on("data", d => { viteOutput += d.toString(); });

  // Ensure cleanup
  const cleanup = () => { try { vite.kill("SIGTERM"); } catch {} };
  process.on("exit", cleanup);
  process.on("SIGINT",  () => { cleanup(); process.exit(1); });
  process.on("SIGTERM", () => { cleanup(); process.exit(1); });

  try {
    await waitForServer(BASE_URL, 60000);
    console.log("Dev server ready.");

    const browser = await chromium.launch({ headless: true });

    // Create ONE context per theme — reuse across all pages (much faster).
    const contexts = {};
    for (const theme of ["light", "dark"]) {
      contexts[theme] = await browser.newContext({
        deviceScaleFactor: 2,
        colorScheme: theme,
      });
      // Pre-inject mock & theme into every new page created in this context
      await contexts[theme].addInitScript(buildTauriMock(theme));
      await contexts[theme].addInitScript(`
        localStorage.setItem("skill-theme", "${theme}");
        localStorage.removeItem("skill-high-contrast");
      `);
    }

    /** Helper: capture a single page screenshot with a hard timeout. */
    async function capturePage({ name, path, viewport, theme, fullPage = false }) {
      const HARD_TIMEOUT = 25000; // 25s max per page
      const ctx = contexts[theme];
      const page = await ctx.newPage();
      await page.setViewportSize(viewport || { width: 1280, height: 900 });

      console.log(`  Capturing ${name} (${theme})...`);
      const hardTimer = setTimeout(async () => {
        console.warn(`    !! HARD TIMEOUT for ${name} (${theme}), forcing close.`);
        try { await page.close(); } catch {}
      }, HARD_TIMEOUT);
      try {
        await page.goto(`${BASE_URL}${path}`, { waitUntil: "domcontentloaded", timeout: 15000 });
      } catch (e) {
        console.warn(`    !! Navigation timeout for ${name} (${theme}), skipping.`);
        clearTimeout(hardTimer);
        try { await page.close(); } catch {}
        return;
      }

      // Ensure theme class is applied & kill animations + kill timers
      await page.evaluate((t) => {
        document.documentElement.classList.toggle("dark", t === "dark");
        const style = document.createElement("style");
        style.textContent = "*, *::before, *::after { animation-duration: 0s !important; animation-delay: 0s !important; transition-duration: 0s !important; transition-delay: 0s !important; }";
        document.head.appendChild(style);
      }, theme);

      // Wait for Svelte to render & mocked data to load.
      // The first page in a fresh context needs extra bootstrap time.
      const isFirstPage = name === "dashboard";
      await page.waitForTimeout(isFirstPage ? 3500 : 1500);

      // ── Post-navigation actions for specific pages ──
      // Search EEG: click the Search button to trigger a search
      if (name === "search-eeg") {
        try {
          const searchBtn = page.locator("button").filter({ hasText: /^.*Search$/i }).first();
          if (await searchBtn.count() > 0) {
            await searchBtn.click();
            await page.waitForTimeout(2000);
          }
        } catch {}
      }
      // Search images: trigger search then replace broken images with placeholders
      if (name === "search-images") {
        try {
          const input = page.locator("input[type=text], input[type=search]").first();
          if (await input.count() > 0) {
            await input.fill("code editor");
          }
          const searchBtn = page.locator("button").filter({ hasText: /Images/i }).first();
          if (await searchBtn.count() > 0) {
            await searchBtn.click();
            await page.waitForTimeout(1200);
          }
          // Replace broken <img> elements with coloured placeholder SVGs
          await page.evaluate(() => {
            const colors = ["#6366f1","#3b82f6","#10b981","#f59e0b","#ef4444"];
            const apps   = ["VS Code","Firefox","Terminal"];
            document.querySelectorAll('img[alt="Screenshot"]').forEach((img, i) => {
              const c = colors[i % colors.length];
              const a = apps[i % apps.length];
              const svg = [
                '<svg xmlns="http://www.w3.org/2000/svg" width="640" height="400">',
                '<rect width="640" height="400" fill="' + c + '22"/>',
                '<rect x="10" y="10" width="620" height="30" rx="6" fill="' + c + '44"/>',
                '<circle cx="28" cy="25" r="6" fill="#ef4444"/>',
                '<circle cx="46" cy="25" r="6" fill="#f59e0b"/>',
                '<circle cx="64" cy="25" r="6" fill="#22c55e"/>',
                '<text x="320" y="25" text-anchor="middle" font-family="system-ui" font-size="12" fill="' + c + '">' + a + '</text>',
                '<rect x="20" y="60" width="280" height="14" rx="3" fill="' + c + '33"/>',
                '<rect x="20" y="84" width="200" height="14" rx="3" fill="' + c + '22"/>',
                '<rect x="20" y="108" width="350" height="14" rx="3" fill="' + c + '28"/>',
                '<rect x="20" y="132" width="160" height="14" rx="3" fill="' + c + '22"/>',
                '<rect x="20" y="170" width="400" height="10" rx="3" fill="' + c + '18"/>',
                '<rect x="20" y="190" width="320" height="10" rx="3" fill="' + c + '18"/>',
                '<rect x="20" y="210" width="380" height="10" rx="3" fill="' + c + '18"/>',
                '</svg>',
              ].join("");
              img.src = "data:image/svg+xml," + encodeURIComponent(svg);
            });
          });
          await page.waitForTimeout(300);
        } catch {}
      }
      // Search modes: type a query and press Ctrl+Enter to trigger search
      // search-text: handled above in post-navigation actions
      if (name === "search-text") {
        try {
          const textarea = page.locator("textarea").first();
          if (await textarea.count() > 0) {
            await textarea.fill("deep focus coding");
            await textarea.press("Control+Enter");
            await page.waitForTimeout(800);
          }
        } catch {}
      }
      // search-images: handled above (new handler with placeholder images)

      const filename = `${name}-${theme}.png`;
      try {
        await page.screenshot({ path: resolve(OUT_DIR, filename), fullPage, timeout: 10000 });
        console.log(`    -> ${filename}`);
      } catch {
        console.warn(`    !! Screenshot timeout for ${name} (${theme}), skipping.`);
      }
      clearTimeout(hardTimer);
      try { await page.close(); } catch {}
    }

    // ── Warm-up: pre-load the app so Vite finishes compiling ────────────────
    {
      console.log("  Warming up app (pre-compile)...");
      const warmPage = await contexts.light.newPage();
      await warmPage.setViewportSize({ width: 1280, height: 900 });
      try {
        await warmPage.goto(BASE_URL, { waitUntil: "networkidle", timeout: 30000 });
        await warmPage.waitForTimeout(3000);
      } catch { /* non-fatal */ }
      await warmPage.close();
      // Also warm the dark context
      const warmPageDark = await contexts.dark.newPage();
      await warmPageDark.setViewportSize({ width: 1280, height: 900 });
      try {
        await warmPageDark.goto(BASE_URL, { waitUntil: "networkidle", timeout: 30000 });
        await warmPageDark.waitForTimeout(2000);
      } catch { /* non-fatal */ }
      await warmPageDark.close();
      console.log("  Warm-up complete.");
    }

    // ── Main routes ─────────────────────────────────────────────────────────
    for (const [name, path, viewport] of ROUTES) {
      const fullPage = ["dashboard", "settings", "help", "api", "downloads"].includes(name);
      for (const theme of ["light", "dark"]) {
        await capturePage({ name, path, viewport, theme, fullPage });
      }
    }

    // ── Settings sub-tabs ─────────────────────────────────────────────────
    for (const tab of SETTINGS_TABS) {
      for (const theme of ["light", "dark"]) {
        const ctx = contexts[theme];
        const page = await ctx.newPage();
        await page.setViewportSize({ width: 1100, height: 780 });

        console.log(`  Capturing settings/${tab} (${theme})...`);
        try {
          await page.goto(`${BASE_URL}/settings`, { waitUntil: "domcontentloaded", timeout: 15000 });
        } catch {
          console.warn(`    !! Settings page timeout, skipping tab ${tab}.`);
          await page.close();
          continue;
        }
        await page.evaluate((t) => {
          document.documentElement.classList.toggle("dark", t === "dark");
          const s = document.createElement("style");
          s.textContent = "*, *::before, *::after { animation-duration: 0s !important; animation-delay: 0s !important; transition-duration: 0s !important; transition-delay: 0s !important; }";
          document.head.appendChild(s);
        }, theme);

        // Wait for the settings page to fully render
        await page.waitForTimeout(1200);

        // Click the tab
        try {
          const tabBtn = page.locator(`button, [role="tab"], a`).filter({ hasText: new RegExp(`^${tab}$`, "i") }).first();
          if (await tabBtn.count() > 0) {
            await tabBtn.click();
          } else {
            const idx = SETTINGS_TABS.indexOf(tab);
            if (idx < 9) {
              await page.keyboard.press(`Control+${idx + 1}`);
            }
          }
        } catch { /* tab click may fail, continue anyway */ }

        await page.waitForTimeout(800);

        const filename = `settings-${tab}-${theme}.png`;
        try {
          await page.screenshot({ path: resolve(OUT_DIR, filename), fullPage: true, timeout: 10000 });
          console.log(`    -> ${filename}`);
        } catch {
          console.warn(`    !! Screenshot timeout for settings/${tab} (${theme}), skipping.`);
        }
        await page.close();
      }
    }

    // ── Help sub-tabs ─────────────────────────────────────────────────────
    for (const tab of HELP_TABS) {
      for (const theme of ["light", "dark"]) {
        const ctx = contexts[theme];
        const page = await ctx.newPage();
        await page.setViewportSize({ width: 1100, height: 780 });

        console.log(`  Capturing help/${tab} (${theme})...`);
        try {
          await page.goto(`${BASE_URL}/help`, { waitUntil: "domcontentloaded", timeout: 15000 });
        } catch {
          console.warn(`    !! Help page timeout, skipping tab ${tab}.`);
          await page.close();
          continue;
        }
        await page.evaluate((t) => {
          document.documentElement.classList.toggle("dark", t === "dark");
          const s = document.createElement("style");
          s.textContent = "*, *::before, *::after { animation-duration: 0s !important; animation-delay: 0s !important; transition-duration: 0s !important; transition-delay: 0s !important; }";
          document.head.appendChild(s);
        }, theme);

        // Wait for help page to render
        await page.waitForTimeout(1200);

        try {
          const tabBtn = page.locator(`button, [role="tab"], a`).filter({ hasText: new RegExp(`^${tab}$`, "i") }).first();
          if (await tabBtn.count() > 0) {
            await tabBtn.click();
          }
        } catch { /* continue */ }

        await page.waitForTimeout(800);

        const filename = `help-${tab}-${theme}.png`;
        try {
          await page.screenshot({ path: resolve(OUT_DIR, filename), fullPage: true, timeout: 10000 });
          console.log(`    -> ${filename}`);
        } catch {
          console.warn(`    !! Screenshot timeout for help/${tab} (${theme}), skipping.`);
        }
        await page.close();
      }
    }

    // Close contexts and browser
    await contexts.light.close();
    await contexts.dark.close();
    await browser.close();
    console.log(`\nDone! Screenshots saved to: ${OUT_DIR}`);
    console.log(`Total: ${(ROUTES.length + SETTINGS_TABS.length + HELP_TABS.length) * 2} screenshots`);

  } finally {
    cleanup();
  }
}

main().catch(err => {
  console.error("Screenshot script failed:", err);
  process.exit(1);
});
