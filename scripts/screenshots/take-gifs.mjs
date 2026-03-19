#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// GIF automation — records animated GIFs of app interactions:
//   scrolling long views, switching tabs, clicking elements.
//
// Usage:  node scripts/screenshots/take-gifs.mjs [--filter <name>] [--theme light|dark]
//
// Prerequisites:
//   npx playwright install chromium
//   npm install --save-dev gif-encoder-2 sharp
//
// Output: docs/screenshots/gifs/<name>-<light|dark>.gif

import { chromium }         from "playwright";
import { spawn }            from "node:child_process";
import { mkdirSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath }    from "node:url";
import GIFEncoder           from "gif-encoder-2";
import sharp                from "sharp";
import { buildTauriMock }   from "./tauri-mock.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT      = resolve(__dirname, "../..");
const OUT_DIR   = resolve(ROOT, "docs/screenshots/gifs");
const FRAME_DIR = resolve(OUT_DIR, ".frames");
const DEV_PORT  = 1420;
const BASE_URL  = `http://localhost:${DEV_PORT}`;

// ── CLI args ────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const filterIdx = args.indexOf("--filter");
const FILTER    = filterIdx >= 0 ? args[filterIdx + 1] : null;
const themeIdx  = args.indexOf("--theme");
const THEMES    = themeIdx >= 0 ? [args[themeIdx + 1]] : ["light", "dark"];

// ── GIF Config ──────────────────────────────────────────────────────────────
const GIF_WIDTH      = 800;   // output GIF width (height scales proportionally)
const FRAME_DELAY_MS = 120;   // default delay between frames in the GIF
const PAUSE_DELAY_MS = 800;   // delay for "pause" frames (hold on final state)

// ── Interaction Definitions ─────────────────────────────────────────────────
//
// Each entry describes a GIF to record:
//   name:     slug for the output filename
//   route:    the app route to navigate to
//   viewport: { width, height }
//   steps:    array of interaction steps (executed in order)
//
// Step types:
//   { action: "wait", ms }                         — wait N ms
//   { action: "screenshot" }                       — capture a frame
//   { action: "scroll", selector, by, frames }     — scroll element, capturing `frames` frames
//   { action: "click", selector }                  — click an element
//   { action: "tabs", selector, labels, pauseMs }  — click each tab in sequence, capture each
//   { action: "hover", selector }                  — hover over an element
//   { action: "type", selector, text }             — type text into an input
//   { action: "pause" }                            — add a longer-delay frame (hold)

const INTERACTIONS = [
  // ── Dashboard: scroll through all sections ────────────────────────────
  {
    name: "dashboard-scroll",
    route: "/",
    viewport: { width: 1280, height: 900 },
    steps: [
      { action: "wait", ms: 2000 },
      { action: "screenshot" },
      { action: "scroll", selector: "main", by: 2400, frames: 16 },
      { action: "pause" },
    ],
  },

  // ── Settings: cycle through sub-tabs ──────────────────────────────────
  {
    name: "settings-tabs",
    route: "/settings",
    viewport: { width: 1100, height: 780 },
    steps: [
      { action: "wait", ms: 1500 },
      { action: "tabs", containerSelector: "nav, [role='tablist']", labels: [
        "Goals", "Devices", "EXG", "Sleep", "Calibration", "Voice",
        "LLM", "Tools", "EEG Model", "Embeddings", "Screenshots",
        "Proactive Hooks", "Appearance", "Settings", "Shortcuts", "UMAP",
        "Updates", "Permissions",
      ], pauseMs: 400 },
    ],
  },

  // ── Help: cycle through help sections ─────────────────────────────────
  {
    name: "help-tabs",
    route: "/help",
    viewport: { width: 1100, height: 780 },
    steps: [
      { action: "wait", ms: 1500 },
      { action: "tabs", containerSelector: "nav, [role='tablist']", labels: [
        "Dashboard", "Electrodes", "Settings", "Windows", "API",
        "TTS", "LLM", "Hooks", "Privacy", "References", "FAQ",
      ], pauseMs: 400 },
    ],
  },

  // ── History: expand session cards ─────────────────────────────────────
  {
    name: "history-expand",
    route: "/history",
    viewport: { width: 1100, height: 780 },
    steps: [
      { action: "wait", ms: 2000 },
      { action: "screenshot" },
      // Click the first session card to expand it
      { action: "click", selector: "[data-testid='session-card']:first-child, .session-card:first-child, button:has-text('session')" },
      { action: "wait", ms: 600 },
      { action: "screenshot" },
      { action: "scroll", selector: "main", by: 600, frames: 6 },
      { action: "pause" },
    ],
  },

  // ── Chat: scroll through messages ─────────────────────────────────────
  {
    name: "chat-scroll",
    route: "/chat",
    viewport: { width: 1100, height: 780 },
    steps: [
      { action: "wait", ms: 2000 },
      { action: "screenshot" },
      { action: "scroll", selector: "[data-testid='chat-messages'], .chat-messages, main", by: 800, frames: 8 },
      { action: "pause" },
    ],
  },

  // ── Search: switch between search modes ───────────────────────────────
  {
    name: "search-modes",
    route: "/search",
    viewport: { width: 1100, height: 780 },
    steps: [
      { action: "wait", ms: 1500 },
      { action: "screenshot" },
      // Click Text tab
      { action: "click", selector: "button:has-text('Text'), a:has-text('Text')" },
      { action: "wait", ms: 600 },
      { action: "screenshot" },
      // Click EEG tab
      { action: "click", selector: "button:has-text('EEG'), a:has-text('EEG')" },
      { action: "wait", ms: 600 },
      { action: "screenshot" },
      // Click Images tab
      { action: "click", selector: "button:has-text('Images'), a:has-text('Images')" },
      { action: "wait", ms: 600 },
      { action: "screenshot" },
      { action: "pause" },
    ],
  },

  // ── Session: scroll through metrics ───────────────────────────────────
  {
    name: "session-scroll",
    route: "/session?csv_path=/data/session_20260318_120000.csv",
    viewport: { width: 1100, height: 780 },
    steps: [
      { action: "wait", ms: 2000 },
      { action: "screenshot" },
      { action: "scroll", selector: "main", by: 1800, frames: 12 },
      { action: "pause" },
    ],
  },

  // ── Downloads: show download progress ─────────────────────────────────
  {
    name: "downloads-scroll",
    route: "/downloads",
    viewport: { width: 900, height: 700 },
    steps: [
      { action: "wait", ms: 1500 },
      { action: "screenshot" },
      { action: "scroll", selector: "main", by: 600, frames: 6 },
      { action: "pause" },
    ],
  },

  // ── About: scroll the about page ──────────────────────────────────────
  {
    name: "about-scroll",
    route: "/about",
    viewport: { width: 520, height: 620 },
    steps: [
      { action: "wait", ms: 1500 },
      { action: "screenshot" },
      { action: "scroll", selector: "main", by: 600, frames: 8 },
      { action: "pause" },
    ],
  },

  // ── Labels: scroll through labels list ────────────────────────────────
  {
    name: "labels-scroll",
    route: "/labels",
    viewport: { width: 900, height: 700 },
    steps: [
      { action: "wait", ms: 1500 },
      { action: "screenshot" },
      { action: "scroll", selector: "main", by: 600, frames: 6 },
      { action: "pause" },
    ],
  },

  // ── Calibration: scroll ───────────────────────────────────────────────
  {
    name: "calibration-scroll",
    route: "/calibration",
    viewport: { width: 800, height: 640 },
    steps: [
      { action: "wait", ms: 1500 },
      { action: "screenshot" },
      { action: "scroll", selector: "main", by: 500, frames: 6 },
      { action: "pause" },
    ],
  },
];

// ── Helpers ─────────────────────────────────────────────────────────────────

async function waitForServer(url, timeoutMs = 30000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const resp = await fetch(url);
      if (resp.ok) return true;
    } catch { /* not ready */ }
    await new Promise(r => setTimeout(r, 500));
  }
  throw new Error(`Dev server not ready after ${timeoutMs}ms`);
}

/**
 * Resize a PNG buffer to fit GIF_WIDTH while keeping aspect ratio.
 * Returns raw RGBA pixel data + dimensions.
 */
async function resizeFrame(pngBuffer, targetWidth) {
  const metadata = await sharp(pngBuffer).metadata();
  const scale = targetWidth / metadata.width;
  const targetHeight = Math.round(metadata.height * scale);

  const raw = await sharp(pngBuffer)
    .resize(targetWidth, targetHeight, { fit: "fill" })
    .ensureAlpha()
    .raw()
    .toBuffer();

  return { data: raw, width: targetWidth, height: targetHeight };
}

/**
 * Encode an array of PNG buffers into a GIF file.
 */
async function encodeGif(frames, outputPath, { delays = [], defaultDelay = FRAME_DELAY_MS } = {}) {
  if (frames.length === 0) {
    console.warn("    No frames to encode, skipping.");
    return;
  }

  // Resize all frames to the same dimensions
  const resized = [];
  let gifWidth, gifHeight;
  for (const frame of frames) {
    const r = await resizeFrame(frame, GIF_WIDTH);
    if (!gifWidth) { gifWidth = r.width; gifHeight = r.height; }
    resized.push(r);
  }

  const encoder = new GIFEncoder(gifWidth, gifHeight, "neuquant", true);
  encoder.setTransparent(0x00000000);
  encoder.setRepeat(0);   // loop forever
  encoder.setQuality(10); // 1=best, 20=fastest
  encoder.start();

  for (let i = 0; i < resized.length; i++) {
    const delay = delays[i] ?? defaultDelay;
    encoder.setDelay(delay);
    encoder.addFrame(resized[i].data);
  }

  encoder.finish();
  const buffer = encoder.out.getData();

  const { writeFileSync } = await import("node:fs");
  writeFileSync(outputPath, buffer);

  const sizeMb = (buffer.length / 1024 / 1024).toFixed(2);
  console.log(`    -> ${outputPath} (${frames.length} frames, ${sizeMb} MB)`);
}

// ── Main ────────────────────────────────────────────────────────────────────

async function main() {
  mkdirSync(OUT_DIR, { recursive: true });
  mkdirSync(FRAME_DIR, { recursive: true });

  // Filter interactions if --filter provided
  let interactions = INTERACTIONS;
  if (FILTER) {
    interactions = interactions.filter(i => i.name.includes(FILTER));
    if (interactions.length === 0) {
      console.error(`No interactions matching filter: ${FILTER}`);
      console.log("Available:", INTERACTIONS.map(i => i.name).join(", "));
      process.exit(1);
    }
  }

  // Start Vite dev server
  console.log("Starting Vite dev server...");
  const vite = spawn("npx", ["vite", "dev", "--port", String(DEV_PORT)], {
    cwd: ROOT,
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, BROWSER: "none" },
  });
  const cleanup = () => { try { vite.kill("SIGTERM"); } catch {} };
  process.on("exit", cleanup);
  process.on("SIGINT",  () => { cleanup(); process.exit(1); });
  process.on("SIGTERM", () => { cleanup(); process.exit(1); });

  try {
    await waitForServer(BASE_URL, 60000);
    console.log("Dev server ready.\n");

    const browser = await chromium.launch({ headless: true });

    // Create contexts per theme
    const contexts = {};
    for (const theme of THEMES) {
      contexts[theme] = await browser.newContext({
        deviceScaleFactor: 2,
        colorScheme: theme,
      });
      await contexts[theme].addInitScript(buildTauriMock(theme));
      await contexts[theme].addInitScript(`
        localStorage.setItem("skill-theme", "${theme}");
        localStorage.removeItem("skill-high-contrast");
      `);
    }

    // Warm up
    console.log("Warming up...");
    for (const theme of THEMES) {
      const page = await contexts[theme].newPage();
      await page.setViewportSize({ width: 1280, height: 900 });
      try {
        await page.goto(BASE_URL, { waitUntil: "networkidle", timeout: 45000 });
        await page.waitForTimeout(3000);
      } catch {}
      await page.close();
    }
    console.log("Warm-up complete.\n");

    // ── Record each interaction ───────────────────────────────────────────
    for (const interaction of interactions) {
      for (const theme of THEMES) {
        console.log(`Recording: ${interaction.name} (${theme})`);
        const ctx = contexts[theme];
        const page = await ctx.newPage();
        await page.setViewportSize(interaction.viewport || { width: 1280, height: 900 });

        // Navigate
        try {
          await page.goto(`${BASE_URL}${interaction.route}`, {
            waitUntil: "domcontentloaded",
            timeout: 30000,
          });
        } catch (e) {
          console.warn(`    !! Navigation timeout, skipping.`);
          await page.close();
          continue;
        }

        // Apply theme + disable animations
        await page.evaluate((t) => {
          document.documentElement.classList.toggle("dark", t === "dark");
          const s = document.createElement("style");
          s.textContent = `
            *, *::before, *::after {
              animation-duration: 0s !important;
              animation-delay: 0s !important;
              transition-duration: 0s !important;
              transition-delay: 0s !important;
            }
          `;
          document.head.appendChild(s);
        }, theme);

        // Execute steps and collect frames
        const frames = [];
        const delays = [];

        async function captureFrame(delay = FRAME_DELAY_MS) {
          const buf = await page.screenshot({ type: "png", timeout: 10000 });
          frames.push(buf);
          delays.push(delay);
        }

        for (const step of interaction.steps) {
          try {
            switch (step.action) {
              case "wait":
                await page.waitForTimeout(step.ms);
                break;

              case "screenshot":
                await captureFrame(step.delay || FRAME_DELAY_MS);
                break;

              case "pause":
                // Capture current state with a longer delay (hold frame)
                await captureFrame(step.ms || PAUSE_DELAY_MS);
                break;

              case "scroll": {
                const sel = step.selector || "main";
                const totalScroll = step.by || 1000;
                const numFrames = step.frames || 10;
                const scrollPerFrame = totalScroll / numFrames;

                for (let i = 0; i < numFrames; i++) {
                  await page.evaluate(({ sel, amount }) => {
                    const el = document.querySelector(sel) || document.scrollingElement || document.body;
                    el.scrollBy({ top: amount, behavior: "instant" });
                  }, { sel, amount: scrollPerFrame });
                  await page.waitForTimeout(50); // let render settle
                  await captureFrame(step.frameDelay || FRAME_DELAY_MS);
                }
                break;
              }

              case "click": {
                const loc = page.locator(step.selector).first();
                if (await loc.count() > 0) {
                  await loc.click();
                  await page.waitForTimeout(step.waitAfter || 300);
                  if (step.capture !== false) {
                    await captureFrame(step.delay || FRAME_DELAY_MS);
                  }
                } else {
                  console.warn(`    !! Click target not found: ${step.selector}`);
                }
                break;
              }

              case "tabs": {
                const labels = step.labels || [];
                const pauseMs = step.pauseMs || 400;
                for (const label of labels) {
                  // Try multiple strategies to find the tab button
                  let tabBtn = page.locator("button, [role='tab'], a")
                    .filter({ hasText: new RegExp(escapeRegex(label), "i") })
                    .first();
                  if (await tabBtn.count() === 0) {
                    // Try with getByText (more lenient)
                    tabBtn = page.getByText(label, { exact: false }).first();
                  }
                  if (await tabBtn.count() > 0) {
                    await tabBtn.click();
                    await page.waitForTimeout(pauseMs);
                    await captureFrame(step.frameDelay || FRAME_DELAY_MS * 2);
                  } else {
                    console.warn(`    !! Tab not found: "${label}"`);
                  }
                }
                break;
              }

              case "hover": {
                const loc = page.locator(step.selector).first();
                if (await loc.count() > 0) {
                  await loc.hover();
                  await page.waitForTimeout(step.waitAfter || 200);
                  await captureFrame(step.delay || FRAME_DELAY_MS);
                }
                break;
              }

              case "type": {
                const input = page.locator(step.selector).first();
                if (await input.count() > 0) {
                  await input.fill(step.text);
                  await page.waitForTimeout(step.waitAfter || 300);
                  if (step.capture !== false) {
                    await captureFrame(step.delay || FRAME_DELAY_MS);
                  }
                }
                break;
              }

              default:
                console.warn(`    !! Unknown action: ${step.action}`);
            }
          } catch (e) {
            console.warn(`    !! Step failed (${step.action}): ${e.message}`);
          }
        }

        // Encode GIF
        if (frames.length > 0) {
          const outputPath = resolve(OUT_DIR, `${interaction.name}-${theme}.gif`);
          await encodeGif(frames, outputPath, { delays });
        } else {
          console.warn(`    !! No frames captured for ${interaction.name} (${theme})`);
        }

        await page.close();
      }
    }

    // Cleanup
    for (const theme of THEMES) {
      await contexts[theme].close();
    }
    await browser.close();

    // Remove temp frame dir
    try {
      const { rmSync } = await import("node:fs");
      rmSync(FRAME_DIR, { recursive: true, force: true });
    } catch {}

    console.log(`\nDone! GIFs saved to: ${OUT_DIR}`);
    const total = interactions.length * THEMES.length;
    console.log(`Total: ${total} GIFs`);

  } finally {
    cleanup();
  }
}

function escapeRegex(str) {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

main().catch(err => {
  console.error("GIF script failed:", err);
  process.exit(1);
});
