#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// GIF automation — records animated GIFs of every app interaction:
//   scrolling, tab switching, toggle reveals, expanding panels, editing forms.
//
// Usage:
//   node scripts/screenshots/take-gifs.mjs                         # all GIFs, light+dark
//   node scripts/screenshots/take-gifs.mjs --filter settings-goals  # only matching GIFs
//   node scripts/screenshots/take-gifs.mjs --theme light            # light only
//   node scripts/screenshots/take-gifs.mjs --list                   # list available GIFs
//
// Prerequisites:
//   npx playwright install chromium
//   npm install --save-dev gif-encoder-2 sharp
//
// Output: docs/screenshots/gifs/<name>-<light|dark>.gif

import { chromium }         from "playwright";
import { spawn }            from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath }    from "node:url";
import GIFEncoder           from "gif-encoder-2";
import sharp                from "sharp";
import { buildTauriMock }   from "./tauri-mock.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT      = resolve(__dirname, "../..");
const OUT_DIR   = resolve(ROOT, "docs/screenshots/gifs");
const DEV_PORT  = 1420;
const BASE_URL  = `http://localhost:${DEV_PORT}`;

// ── CLI ─────────────────────────────────────────────────────────────────────
const cliArgs   = process.argv.slice(2);
const filterIdx = cliArgs.indexOf("--filter");
const FILTER    = filterIdx >= 0 ? cliArgs[filterIdx + 1] : null;
const themeIdx  = cliArgs.indexOf("--theme");
const THEMES    = themeIdx >= 0 ? [cliArgs[themeIdx + 1]] : ["light", "dark"];
const LIST_MODE = cliArgs.includes("--list");

// ── Timing constants ────────────────────────────────────────────────────────
const F   = 120;     // default frame delay (ms) in GIF playback
const H   = 1400;    // hold/pause frame delay
const PG  = 3000;    // wait for page to fully render after navigation
const TAB = 1000;    // wait after clicking a tab or toggle for content to render
const SCR = 60;      // delay between scroll sub-frames

// Content area selector used inside settings tabs.
// The settings page has sidebar + main content; we scroll the right panel.
const SC = "main";

// ── Step DSL ────────────────────────────────────────────────────────────────
const w  = (ms)              => ({ a: "wait", ms });
const s  = (d)               => ({ a: "shot", d: d || F });
const h  = (d)               => ({ a: "hold", d: d || H });
const sh = ()                => [s(), h()];  // shot + hold combo
const scroll = (px, n)       => ({ a: "scroll", sel: SC, px, n: n || Math.ceil(px / 80) });
const ct = (text, ms)        => ({ a: "ct", text, ms: ms || TAB });
const ck = (sel, ms)         => ({ a: "ck", sel, ms: ms || TAB });
const type_ = (sel, text)    => ({ a: "type", sel, text });
const tabs = (labels)        => ({ a: "tabs", labels });

// ═══════════════════════════════════════════════════════════════════════════
//  ALL INTERACTIONS
// ═══════════════════════════════════════════════════════════════════════════
const ALL = [

  // ─────────────────── DASHBOARD ──────────────────────────────────────────
  {
    name: "dashboard-full-scroll",
    desc: "Dashboard: full scroll from top (GPU, device hero, signal, electrode guide, brain state, indices, composites, consciousness, PPG, IMU, EEG chart, recording bar)",
    route: "/", vp: [1280, 900],
    steps: [
      w(PG), ...sh(),
      scroll(350, 4), ...sh(),
      scroll(350, 4), ...sh(),
      scroll(350, 4), ...sh(),
      scroll(350, 4), ...sh(),
      scroll(350, 4), ...sh(),
      scroll(350, 4), ...sh(),
      scroll(350, 4), ...sh(),
      scroll(350, 4), ...sh(),
    ],
  },
  {
    name: "dashboard-electrode-guide",
    desc: "Dashboard: expand Electrode Placement Guide, scroll through it, collapse",
    route: "/", vp: [1280, 900],
    steps: [
      w(PG),
      scroll(300, 3),
      ct("Electrode Placement Guide"), s(), h(),
      scroll(400, 4), ...sh(),
      scroll(300, 3), ...sh(),
      ct("Electrode Placement Guide"), s(), // collapse
    ],
  },
  {
    name: "dashboard-collapse-sections",
    desc: "Dashboard: toggle collapse/expand on Brain State, EEG Indices, Composite Scores, Consciousness sections",
    route: "/", vp: [1280, 900],
    steps: [
      w(PG), scroll(700, 6),
      ...sh(),
      ct("Brain State", 600), s(),   ct("Brain State", 600), ...sh(),
      scroll(300, 3),
      ct("EEG Indices", 600), s(),   ct("EEG Indices", 600), ...sh(),
      scroll(300, 3),
      ct("Composite Scores", 600), s(), ct("Composite Scores", 600), ...sh(),
      scroll(400, 3),
      ct("Consciousness", 600), s(), ct("Consciousness", 600), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > GOALS ──────────────────────────────────
  // DND is enabled by default in mock → shows all sub-settings
  {
    name: "settings-goals-slider-and-chart",
    desc: "Settings > Goals: hero, daily recording goal slider, quick presets, 30-day chart, how-it-works",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Goals"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-goals-dnd-full",
    desc: "Settings > Goals: DND automation — toggle ON (already on), threshold slider, sustained duration presets, exit delay presets, focus lookback presets, focus mode picker, exit notification toggle, SNR exit threshold, activation progress bar",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Goals"),
      // scroll past the goal section to DND
      scroll(750, 6), ...sh(),
      scroll(400, 4), ...sh(),   // threshold + duration
      scroll(400, 4), ...sh(),   // exit delay + lookback
      scroll(400, 4), ...sh(),   // focus mode picker + exit notification
      scroll(400, 4), ...sh(),   // SNR exit + activation progress
      scroll(300, 3), ...sh(),   // status indicator
    ],
  },

  // ─────────────────── SETTINGS > DEVICES ────────────────────────────────
  {
    name: "settings-devices-list",
    desc: "Settings > Devices: supported device companies accordion, paired devices, BLE discovered list",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Devices"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-devices-openbci",
    desc: "Settings > Devices: expand OpenBCI section — board type radio buttons, connection config (BLE/Serial/WiFi/Galea)",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Devices"),
      scroll(600, 5),
      ct("OpenBCI"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > SLEEP ──────────────────────────────────
  {
    name: "settings-sleep",
    desc: "Settings > Sleep: clock visualization, bedtime/wake time inputs, schedule presets",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Sleep"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(300, 3), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > CALIBRATION ────────────────────────────
  {
    name: "settings-calibration-profiles",
    desc: "Settings > Calibration: profile list with action summaries, last calibration timestamps",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Calibration"), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-calibration-editor",
    desc: "Settings > Calibration: click New Profile to open editor — name, actions list, duration pickers, break/loop config, auto-start toggle",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Calibration"),
      ct("New Profile", 1200), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      ct("Cancel", 600), s(),
    ],
  },

  // ─────────────────── SETTINGS > VOICE (TTS) ───────────────────────────
  {
    name: "settings-voice",
    desc: "Settings > Voice: backend toggle (KittenTTS / NeuTTS), voice picker grid, preset voices, custom reference wav, test/speak controls",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Voice"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > LLM ────────────────────────────────────
  {
    name: "settings-llm-server",
    desc: "Settings > LLM: server enable/autostart toggles, start/stop server, active model indicator, OpenAI-compatible endpoint list",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("LLM"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-llm-models",
    desc: "Settings > LLM: model family dropdown, quant picker cards with Use/Download buttons, recommended badges, hardware fit indicators",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("LLM"),
      scroll(700, 6), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-llm-advanced",
    desc: "Settings > LLM: expand Advanced/Inference section — GPU layers presets, context size, temperature, verbose toggle, API key, mmproj toggles",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("LLM"),
      scroll(1200, 8),
      // Click "Inference" to expand advanced
      ct("Inference", 800), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > TOOLS ──────────────────────────────────
  {
    name: "settings-tools-toggles",
    desc: "Settings > Tools: master enable toggle, per-tool toggles (date, location, web search, web fetch, bash, read/write/edit file, skill API) with descriptions and warning badges",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Tools"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-tools-web-search",
    desc: "Settings > Tools: web search provider picker (DuckDuckGo/Brave/SearXNG), Brave API key input, SearXNG URL input",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Tools"),
      scroll(500, 5), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-tools-execution",
    desc: "Settings > Tools: execution mode (sequential/parallel), max rounds presets, max calls per round, context compression level, max search results/chars inputs",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Tools"),
      scroll(900, 7), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-tools-skills",
    desc: "Settings > Tools: installed skills list with enable/disable toggles, skills license, sync controls",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Tools"),
      scroll(1400, 10), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > EEG MODEL ──────────────────────────────
  {
    name: "settings-eeg-model",
    desc: "Settings > EEG Model: download/encoder status, weights path, HNSW M and ef_construction presets",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("EEG Model"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > EMBEDDINGS ─────────────────────────────
  {
    name: "settings-embeddings",
    desc: "Settings > Embeddings: model family selector dropdown, active model indicator, re-embed controls, stale label count, progress bar",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Embeddings"), ...sh(),
      scroll(300, 3), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > SCREENSHOTS ────────────────────────────
  {
    name: "settings-screenshots-config",
    desc: "Settings > Screenshots: enable toggle, session-only toggle, interval slider, image size slider, quality slider, embed backend picker, OCR toggle",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Screenshots"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-screenshots-ocr-and-metrics",
    desc: "Settings > Screenshots: OCR engine picker, OCR model download, re-embed controls, pipeline metrics dashboard with capture/OCR/resize/save breakdown",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Screenshots"),
      scroll(900, 7), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > PROACTIVE HOOKS ────────────────────────
  {
    name: "settings-hooks",
    desc: "Settings > Proactive Hooks: hook list with name/enabled/keywords/threshold, scenario examples, add/remove hooks, keyword suggestions, distance suggestions",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Proactive Hooks"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > APPEARANCE ─────────────────────────────
  {
    name: "settings-appearance",
    desc: "Settings > Appearance: font size presets, theme picker (light/dark/system), high contrast toggle, accent color palette, chart color scheme picker",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Appearance"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(300, 3), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > SETTINGS (GENERAL) ─────────────────────
  {
    name: "settings-general-devices",
    desc: "Settings > Settings: data directory, paired devices with pair/forget/prefer controls, serial number and MAC reveal toggles",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Settings"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "settings-general-openbci",
    desc: "Settings > Settings: OpenBCI expandable section — board type, BLE scan timeout, serial port picker, WiFi shield IP, Galea IP",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Settings"),
      scroll(800, 6),
      ct("OpenBCI"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > SHORTCUTS ──────────────────────────────
  {
    name: "settings-shortcuts",
    desc: "Settings > Shortcuts: global keyboard shortcut recorder for each window (settings, help, history, label, search, calibration, focus timer, API)",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Shortcuts"), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > UMAP ───────────────────────────────────
  {
    name: "settings-umap",
    desc: "Settings > UMAP: repulsion strength slider/presets, negative sample rate presets, neighbor presets, epoch presets, timeout presets, random seed, reset defaults button",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("UMAP"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > UPDATES ────────────────────────────────
  {
    name: "settings-updates",
    desc: "Settings > Updates: check-for-updates button, update status, auto-install countdown, update interval presets, auto-start toggle",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Updates"), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS > PERMISSIONS ────────────────────────────
  {
    name: "settings-permissions",
    desc: "Settings > Permissions: accessibility permission status, screen recording permission, Bluetooth permission, notification permission — with open-settings buttons",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG), ct("Permissions"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── SETTINGS: ALL TABS RAPID CYCLE ────────────────────
  {
    name: "settings-all-tabs-cycle",
    desc: "Settings: rapid cycle through all 18 sub-tabs showing sidebar navigation",
    route: "/settings", vp: [1100, 780],
    steps: [
      w(PG),
      tabs([
        "Goals", "Devices", "Sleep", "Calibration", "Voice",
        "LLM", "Tools", "EEG Model", "Embeddings", "Screenshots",
        "Proactive Hooks", "Appearance", "Settings", "Shortcuts", "UMAP",
        "Updates", "Permissions",
      ]),
      h(),
    ],
  },

  // ─────────────────── CHAT ──────────────────────────────────────────────
  {
    name: "chat-conversation",
    desc: "Chat: sidebar with session list, message thread with user/assistant messages, markdown rendering, input bar",
    route: "/chat", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "chat-settings-panel",
    desc: "Chat: toggle open the settings panel overlay (model config, system prompt, temperature)",
    route: "/chat", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      ck("[aria-label='Settings'], [title='Settings']"), ...sh(),
      scroll(300, 3), ...sh(),
      ck("[aria-label='Settings'], [title='Settings']"), s(),
    ],
  },
  {
    name: "chat-tools-panel",
    desc: "Chat: toggle open the tools panel overlay (active tools list, enable/disable each)",
    route: "/chat", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      ck("[aria-label='Tools'], [title='Tools']"), ...sh(),
      scroll(300, 3), ...sh(),
      ck("[aria-label='Tools'], [title='Tools']"), s(),
    ],
  },

  // ─────────────────── SEARCH ────────────────────────────────────────────
  {
    name: "search-eeg-mode",
    desc: "Search > EEG: time range picker, K/EF params, preset buttons, trigger search, streaming results with neighbor cards and distance scores",
    route: "/search?mode=eeg", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      ct("Search", 2500), ...sh(),
      scroll(500, 5), ...sh(),
      scroll(500, 5), ...sh(),
    ],
  },
  {
    name: "search-text-mode",
    desc: "Search > Text: semantic text query input, label results with similarity scores and timestamps",
    route: "/search?mode=text", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      type_("textarea, input[type=text]", "deep focus coding session"), w(500), ...sh(),
    ],
  },
  {
    name: "search-images-mode",
    desc: "Search > Images: screenshot search by text, OCR text matches, similarity ranked thumbnails",
    route: "/search?mode=images", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      type_("textarea, input[type=text]", "code editor"), w(500), ...sh(),
    ],
  },
  {
    name: "search-mode-switching",
    desc: "Search: switch between EEG, Text, and Images modes",
    route: "/search", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      ct("Text"), ...sh(),
      ct("EEG"), ...sh(),
      ct("Images"), ...sh(),
    ],
  },

  // ─────────────────── HISTORY ───────────────────────────────────────────
  {
    name: "history-overview",
    desc: "History: streak counter, stats badges, day navigation, session cards with labels and duration",
    route: "/history", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "history-session-expand",
    desc: "History: expand a session card to show SessionDetail — band power metrics, time-series charts, label timeline",
    route: "/history", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      ck("[data-session-row], .cursor-pointer", 1500), ...sh(),
      scroll(500, 5), ...sh(),
      scroll(500, 5), ...sh(),
      scroll(500, 5), ...sh(),
    ],
  },

  // ─────────────────── SESSION DETAIL ────────────────────────────────────
  {
    name: "session-detail-full",
    desc: "Session detail: metadata header, band power summary, time-series charts (alpha/beta/theta/delta/gamma, focus, relaxation, meditation), sleep staging, label annotations",
    route: "/session?csv_path=/data/session_20260318_120000.csv", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── COMPARE ───────────────────────────────────────────
  {
    name: "compare-overview",
    desc: "Compare: dual side-by-side timeline pickers, day navigation arrows, session bar visualization, datetime range selectors",
    route: "/compare", vp: [1100, 780],
    steps: [
      w(PG), ...sh(),
      scroll(500, 5), ...sh(),
      scroll(500, 5), ...sh(),
    ],
  },

  // ─────────────────── HELP ──────────────────────────────────────────────
  {
    name: "help-all-tabs-cycle",
    desc: "Help: cycle through all 11 tabs — Dashboard, Electrodes, Settings, Windows, API, TTS, LLM, Hooks, Privacy, References, FAQ",
    route: "/help", vp: [1100, 780],
    steps: [
      w(PG),
      tabs([
        "Dashboard", "Electrodes", "Settings", "Windows", "API",
        "TTS", "LLM", "Hooks", "Privacy", "References", "FAQ",
      ]),
      h(),
    ],
  },
  {
    name: "help-dashboard-scroll",
    desc: "Help > Dashboard: status hero, battery, signal quality, EEG channels, band powers, FAA, waveforms, GPU, tray icons — full scroll",
    route: "/help", vp: [1100, 780],
    steps: [
      w(PG), ct("Dashboard"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "help-settings-scroll",
    desc: "Help > Settings: paired devices, signal processing, embedding, calibration, shortcuts, debug logging, updates — full scroll",
    route: "/help", vp: [1100, 780],
    steps: [
      w(PG), ct("Settings"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "help-privacy-scroll",
    desc: "Help > Privacy: data storage, network, third-party, telemetry, permissions — full scroll",
    route: "/help", vp: [1100, 780],
    steps: [
      w(PG), ct("Privacy"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "help-references-scroll",
    desc: "Help > References: academic papers and citations list — full scroll",
    route: "/help", vp: [1100, 780],
    steps: [
      w(PG), ct("References"), ...sh(),
      scroll(500, 5), ...sh(),
      scroll(500, 5), ...sh(),
      scroll(500, 5), ...sh(),
    ],
  },
  {
    name: "help-faq-scroll",
    desc: "Help > FAQ: frequently asked questions — full scroll",
    route: "/help", vp: [1100, 780],
    steps: [
      w(PG), ct("FAQ"), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── CALIBRATION ───────────────────────────────────────
  {
    name: "calibration-electrode-tabs",
    desc: "Calibration: electrode quality indicators, switch between Muse and 10-20 electrode tabs, action timeline",
    route: "/calibration", vp: [800, 640],
    steps: [
      w(PG), ...sh(),
      ct("10-20", TAB), ...sh(),
      ct("Muse", TAB), ...sh(),
      scroll(300, 3), ...sh(),
      scroll(300, 3), ...sh(),
    ],
  },

  // ─────────────────── ONBOARDING ────────────────────────────────────────
  {
    name: "onboarding-wizard",
    desc: "Onboarding: multi-step wizard — welcome, bluetooth pairing, electrode fit, calibration, model downloads",
    route: "/onboarding", vp: [800, 640],
    steps: [
      w(PG), ...sh(),
      ct("bluetooth", TAB), ...sh(),
      ct("fit", TAB), ...sh(),
      ct("calibration", TAB), ...sh(),
      scroll(300, 3), ...sh(),
      ct("models", TAB), ...sh(),
    ],
  },

  // ─────────────────── LABELS ────────────────────────────────────────────
  {
    name: "labels-search-modes",
    desc: "Labels: label list, search bar, toggle between exact and semantic search modes, pagination",
    route: "/labels", vp: [900, 700],
    steps: [
      w(PG), ...sh(),
      ct("semantic", 600), ...sh(),
      ct("exact", 600), s(),
      scroll(400, 4), ...sh(),
    ],
  },

  // ─────────────────── LABEL DIALOG ──────────────────────────────────────
  {
    name: "label-quick-entry",
    desc: "Label: quick label input dialog with textarea, recent labels chips, character count, submit/cancel",
    route: "/label", vp: [700, 520],
    steps: [
      w(PG), ...sh(),
      type_("textarea", "Deep focus — writing documentation for the EEG pipeline"),
      w(500), ...sh(),
    ],
  },

  // ─────────────────── FOCUS TIMER ───────────────────────────────────────
  {
    name: "focus-timer-config",
    desc: "Focus Timer: timer display, work/break/long-break config inputs, preset picker (Pomodoro/Deep Work/Short Focus), auto-label toggle, TTS toggle, session log",
    route: "/focus-timer", vp: [700, 620],
    steps: [
      w(PG), ...sh(),
      scroll(300, 3), ...sh(),
      scroll(300, 3), ...sh(),
      scroll(300, 3), ...sh(),
    ],
  },

  // ─────────────────── DOWNLOADS ─────────────────────────────────────────
  {
    name: "downloads-manager",
    desc: "Downloads: model download list with progress bars, pause/resume/cancel controls, file sizes",
    route: "/downloads", vp: [900, 700],
    steps: [
      w(PG), ...sh(),
      scroll(300, 3), ...sh(),
    ],
  },

  // ─────────────────── API ───────────────────────────────────────────────
  {
    name: "api-status-and-clients",
    desc: "API: WebSocket URL, mDNS discovery command, connected clients table, request log with timestamps",
    route: "/api", vp: [900, 700],
    steps: [
      w(PG), ...sh(),
      scroll(400, 4), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
  {
    name: "api-code-examples",
    desc: "API: code example tabs — Overview, neuroskill CLI, WebSocket, Python, Node.js — with copy button",
    route: "/api", vp: [900, 700],
    steps: [
      w(PG),
      scroll(400, 4),
      ct("Overview", TAB), ...sh(),
      ct("neuroskill", TAB), ...sh(),
      ct("WebSocket", TAB), ...sh(),
      ct("Python", TAB), ...sh(),
      ct("Node.js", TAB), ...sh(),
    ],
  },

  // ─────────────────── ABOUT ─────────────────────────────────────────────
  {
    name: "about-scroll",
    desc: "About: app icon, version, tagline, website/repo/discord links, authors, license, acknowledgements",
    route: "/about", vp: [520, 620],
    steps: [
      w(PG), ...sh(),
      scroll(400, 5), ...sh(),
    ],
  },

  // ─────────────────── WHAT'S NEW ────────────────────────────────────────
  {
    name: "whats-new",
    desc: "What's New: version selector dropdown, changelog markdown content, dismiss button",
    route: "/whats-new", vp: [700, 620],
    steps: [
      w(PG), ...sh(),
      scroll(400, 4), ...sh(),
    ],
  },
];

// ── List mode ───────────────────────────────────────────────────────────────
if (LIST_MODE) {
  console.log(`Available GIF interactions (${ALL.length}):\n`);
  for (const i of ALL) {
    console.log(`  ${i.name.padEnd(42)} ${i.desc}`);
  }
  process.exit(0);
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function escapeRegex(s) { return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"); }

async function waitForServer(url, ms = 60000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    try { const r = await fetch(url); if (r.ok) return; } catch {}
    await new Promise(r => setTimeout(r, 500));
  }
  throw new Error("Dev server timeout");
}

async function resizeFrame(buf, w) {
  const m = await sharp(buf).metadata();
  const h = Math.round(m.height * (w / m.width));
  return { data: await sharp(buf).resize(w, h, { fit: "fill" }).ensureAlpha().raw().toBuffer(), w, h };
}

async function encodeGif(frames, delays, path) {
  if (!frames.length) return;
  const rs = []; let gw, gh;
  for (const f of frames) { const r = await resizeFrame(f, 800); if (!gw) { gw = r.w; gh = r.h; } rs.push(r); }
  const enc = new GIFEncoder(gw, gh, "neuquant", true);
  enc.setTransparent(0x00000000); enc.setRepeat(0); enc.setQuality(10); enc.start();
  for (let i = 0; i < rs.length; i++) { enc.setDelay(delays[i] ?? F); enc.addFrame(rs[i].data); }
  enc.finish();
  const buf = enc.out.getData();
  writeFileSync(path, buf);
  console.log(`    -> ${path} (${frames.length} frames, ${(buf.length/1048576).toFixed(2)} MB)`);
}

async function findByText(page, text) {
  let l = page.locator("button, [role='tab'], a, summary, label, [role='switch']")
    .filter({ hasText: new RegExp(escapeRegex(text), "i") }).first();
  if (await l.count() > 0) return l;
  l = page.getByText(text, { exact: false }).first();
  return (await l.count() > 0) ? l : null;
}

async function findScrollable(page, sel) {
  for (const s of sel.split(",").map(x => x.trim())) {
    try { if (await page.locator(s).count() > 0) return s; } catch {}
  }
  return "body";
}

// ── Main ────────────────────────────────────────────────────────────────────

async function main() {
  mkdirSync(OUT_DIR, { recursive: true });

  let interactions = ALL;
  if (FILTER) {
    interactions = interactions.filter(i => i.name.includes(FILTER));
    if (!interactions.length) {
      console.error(`No match for: ${FILTER}\nAvailable: ${ALL.map(i=>i.name).join(", ")}`);
      process.exit(1);
    }
  }

  const total = interactions.length * THEMES.length;
  console.log(`Recording ${interactions.length} GIF(s) × ${THEMES.length} theme(s) = ${total} total\n`);

  console.log("Starting Vite dev server...");
  const vite = spawn("npx", ["vite", "dev", "--port", String(DEV_PORT)], {
    cwd: ROOT, stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, BROWSER: "none" },
  });
  const cleanup = () => { try { vite.kill("SIGTERM"); } catch {} };
  process.on("exit", cleanup);
  process.on("SIGINT",  () => { cleanup(); process.exit(1); });
  process.on("SIGTERM", () => { cleanup(); process.exit(1); });

  try {
    await waitForServer(BASE_URL);
    console.log("Dev server ready.\n");

    const browser = await chromium.launch({ headless: true });
    const contexts = {};
    for (const theme of THEMES) {
      contexts[theme] = await browser.newContext({ deviceScaleFactor: 2, colorScheme: theme });
      await contexts[theme].addInitScript(buildTauriMock(theme));
      await contexts[theme].addInitScript(`
        localStorage.setItem("skill-theme","${theme}");
        localStorage.removeItem("skill-high-contrast");
      `);
    }

    // Warm up
    console.log("Warming up (compiling frontend)...");
    for (const theme of THEMES) {
      const p = await contexts[theme].newPage();
      await p.setViewportSize({ width: 1280, height: 900 });
      try { await p.goto(BASE_URL, { waitUntil: "networkidle", timeout: 45000 }); await p.waitForTimeout(3000); } catch {}
      await p.close();
    }
    console.log("Warm-up complete.\n");

    let done = 0;
    for (const ix of interactions) {
      for (const theme of THEMES) {
        done++;
        console.log(`[${done}/${total}] ${ix.name} (${theme})`);
        const page = await contexts[theme].newPage();
        await page.setViewportSize({ width: ix.vp[0], height: ix.vp[1] });

        try {
          await page.goto(`${BASE_URL}${ix.route}`, { waitUntil: "domcontentloaded", timeout: 30000 });
        } catch { console.warn("    !! nav timeout"); await page.close(); continue; }

        await page.evaluate((t) => {
          document.documentElement.classList.toggle("dark", t === "dark");
          const s = document.createElement("style");
          s.textContent = "*, *::before, *::after { animation-duration:0s!important; animation-delay:0s!important; transition-duration:0s!important; transition-delay:0s!important; }";
          document.head.appendChild(s);
        }, theme);

        const frames = [], delays = [];
        const cap = async (d) => {
          const b = await page.screenshot({ type: "png", timeout: 10000 });
          frames.push(b); delays.push(d || F);
        };

        // Flatten steps (sh() returns arrays)
        const flat = ix.steps.flat(Infinity);

        for (const step of flat) {
          try {
            switch (step.a) {
              case "wait": await page.waitForTimeout(step.ms); break;
              case "shot": await cap(step.d); break;
              case "hold": await cap(step.d); break;

              case "scroll": {
                const sel = await findScrollable(page, step.sel);
                const per = step.px / step.n;
                for (let i = 0; i < step.n; i++) {
                  await page.evaluate(({s,a})=>{
                    (document.querySelector(s)||document.scrollingElement||document.body).scrollBy({top:a,behavior:"instant"});
                  }, {s:sel,a:per});
                  await page.waitForTimeout(SCR);
                  await cap(F);
                }
                break;
              }

              case "ck": {
                const l = page.locator(step.sel).first();
                if (await l.count()>0) { await l.click(); await page.waitForTimeout(step.ms); }
                else console.warn(`    !! ck not found: ${step.sel}`);
                break;
              }

              case "ct": {
                const el = await findByText(page, step.text);
                if (el) { await el.click(); await page.waitForTimeout(step.ms); }
                else console.warn(`    !! ct not found: "${step.text}"`);
                break;
              }

              case "tabs":
                for (const label of step.labels) {
                  const el = await findByText(page, label);
                  if (el) { await el.click(); await page.waitForTimeout(TAB); await cap(F*2); }
                  else console.warn(`    !! tab: "${label}"`);
                }
                break;

              case "type": {
                for (const sel of step.sel.split(",").map(x=>x.trim())) {
                  const l = page.locator(sel).first();
                  if (await l.count()>0) { await l.fill(step.text); await page.waitForTimeout(300); break; }
                }
                break;
              }
            }
          } catch (e) { console.warn(`    !! ${step.a}: ${e.message}`); }
        }

        if (frames.length) await encodeGif(frames, delays, resolve(OUT_DIR, `${ix.name}-${theme}.gif`));
        else console.warn(`    !! no frames`);
        await page.close();
      }
    }

    for (const t of THEMES) await contexts[t].close();
    await browser.close();
    console.log(`\nDone! ${done} GIFs saved to: ${OUT_DIR}`);
  } finally { cleanup(); }
}

main().catch(e => { console.error("GIF failed:", e); process.exit(1); });
