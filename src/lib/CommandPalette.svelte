<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Command Palette (⌘K / Ctrl+K)
  A quick-access dropdown listing every runnable action in the app.
  Supports fuzzy text filtering and keyboard navigation.
-->
<script lang="ts">
import { onDestroy, onMount } from "svelte";
import { fade } from "svelte/transition";
import { lslDiscover, lslIrohStart, lslIrohStop, retryConnect } from "$lib/daemon/client";
import { getDeviceStatus } from "$lib/daemon/devices";
import { daemonPost } from "$lib/daemon/http";
import { typoMatch } from "$lib/fuzzy-utils";
import { getLocale, t } from "$lib/i18n/index.svelte";
import * as nav from "$lib/navigation";
import { SYNONYMS } from "$lib/settings-search-index";
import { getRecentIds, recordUsage, usageBoost } from "$lib/stores/cmdk-history.svelte";
import { getHighContrast, getResolved, setTheme, toggleHighContrast, toggleTheme } from "$lib/stores/theme.svelte";
import { addToast } from "$lib/stores/toast.svelte";

// Load all locale indexes at build time via Vite glob import
const indexModules = import.meta.glob<{ default: SearchIndexEntry[] }>("$lib/generated/settings-search-index.*.json", {
  eager: true,
});

interface SearchIndexEntry {
  tab: string;
  key: string;
  label: string;
  desc?: string;
}

/** Resolve the index for the current locale, falling back to EN. */
function getSearchIndex(): SearchIndexEntry[] {
  const locale = getLocale();
  // Glob keys look like: /src/lib/generated/settings-search-index.de.json
  const match =
    Object.entries(indexModules).find(([k]) => k.includes(`.${locale}.`)) ??
    Object.entries(indexModules).find(([k]) => k.includes(".en."));
  return match?.[1]?.default ?? [];
}

let open = $state(false);
let query = $state("");
let active = $state(0);
let inputEl: HTMLInputElement | undefined = $state();
let deviceConnected = $state(false);
let semanticResults = $state<ScoredCommand[]>([]);
let semanticDebounce: ReturnType<typeof setTimeout> | undefined;

// ── Command definitions ────────────────────────────────────────────────────

interface Command {
  id: string;
  icon: string;
  label: string;
  section: string;
  keywords?: string;
  shortcut?: string;
  action: () => void | Promise<void>;
  /** If set, Tab key toggles this from the palette without closing. */
  toggle?: { get: () => boolean; set: (v: boolean) => void };
}

const isMac = typeof navigator !== "undefined" && navigator.platform?.includes("Mac");
const mod = isMac ? "⌘" : "Ctrl";

const SETTINGS_TAB_ICONS: Record<string, string> = {
  goals: "🎯",
  devices: "📡",
  exg: "📊",
  lsl: "📶",
  sleep: "🌙",
  calibration: "⚙",
  tts: "🗣",
  llm: "💬",
  tools: "🔧",
  clients: "🔗",
  embeddings: "🔍",
  screenshots: "📷",
  hooks: "🪝",
  appearance: "🎨",
  settings: "⚙",
  shortcuts: "⌨",
  umap: "🗺",
  updates: "⬆",
  permissions: "🔒",
  tokens: "🔑",
};

function settingsTabCommands(): Command[] {
  const index = getSearchIndex();
  return index.map((entry) => ({
    id: `settings-${entry.tab}-${entry.key}`,
    icon: SETTINGS_TAB_ICONS[entry.tab] ?? "⚙",
    section: t("cmdK.sectionSettings"),
    label: `${t(`settingsTabs.${entry.tab}`)} › ${entry.label}`,
    keywords: entry.desc ?? "",
    action: () => nav.openSettingsTab(entry.tab, entry.key),
  }));
}

/**
 * Expand synonyms in the query. Returns the original query plus
 * any synonym expansions as separate alternative queries to try.
 */
function synonymQueries(q: string): string[] {
  const words = q.toLowerCase().split(/\s+/);
  const alts: string[] = [q];
  for (const w of words) {
    if (SYNONYMS[w]) alts.push(SYNONYMS[w]);
  }
  return alts;
}

function commands(): Command[] {
  return [
    // ── Navigation ─────────────────────────────────────────────────────
    {
      id: "open-settings",
      icon: "⚙",
      section: t("cmdK.sectionNavigation"),
      label: t("cmdK.openSettings"),
      shortcut: `${mod},`,
      keywords: t("cmdK.kw.settings"),
      action: nav.openSettings,
    },
    {
      id: "open-help",
      icon: "?",
      section: t("cmdK.sectionNavigation"),
      label: t("cmdK.openHelp"),
      keywords: t("cmdK.kw.help"),
      action: nav.openHelp,
    },
    {
      id: "open-history",
      icon: "🕐",
      section: t("cmdK.sectionNavigation"),
      label: t("cmdK.openHistory"),
      keywords: t("cmdK.kw.history"),
      action: nav.openHistory,
    },
    {
      id: "open-compare",
      icon: "⚖",
      section: t("cmdK.sectionNavigation"),
      label: t("cmdK.openCompare"),
      keywords: t("cmdK.kw.compare"),
      action: nav.openCompare,
    },
    {
      id: "open-search",
      icon: "🔍",
      section: t("cmdK.sectionNavigation"),
      label: t("cmdK.openSearch"),
      shortcut: `${mod}⇧S`,
      keywords: t("cmdK.kw.search"),
      action: nav.openSearch,
    },
    {
      id: "open-label",
      icon: "🏷",
      section: t("cmdK.sectionNavigation"),
      label: t("cmdK.openLabel"),
      shortcut: `${mod}⇧L`,
      keywords: t("cmdK.kw.label"),
      action: nav.openLabel,
    },

    // ── Device ─────────────────────────────────────────────────────────
    {
      id: "retry-connect",
      icon: "📡",
      section: t("cmdK.sectionDevice"),
      label: t("cmdK.retryConnect"),
      keywords: t("cmdK.kw.retryConnect"),
      action: () => retryConnect(),
    },
    {
      id: "open-bt-settings",
      icon: "📶",
      section: t("cmdK.sectionDevice"),
      label: t("cmdK.openBtSettings"),
      keywords: t("cmdK.kw.btSettings"),
      action: nav.openBtSettings,
    },

    // ── LSL ────────────────────────────────────────────────────────────
    {
      id: "lsl-scan",
      icon: "📡",
      section: t("cmdK.sectionLsl"),
      label: t("cmdK.lslScan"),
      keywords: "lsl scan discover streams network eeg exg brainflow openbci pylsl",
      action: async () => {
        const streams = await lslDiscover<{ name: string }>();
        if (streams.length === 0) {
          addToast("info", "LSL Scan", "No LSL streams found on the network.");
        } else {
          addToast(
            "success",
            "LSL Scan",
            `Found ${streams.length} stream${streams.length > 1 ? "s" : ""}: ${streams.map((s) => s.name).join(", ")}`,
          );
          nav.openSettingsTab("lsl");
        }
      },
    },
    {
      id: "lsl-settings",
      icon: "⚡",
      section: t("cmdK.sectionLsl"),
      label: t("cmdK.lslOpenSettings"),
      keywords: "lsl settings tab streams pair auto connect iroh",
      action: () => nav.openSettingsTab("lsl"),
    },
    {
      id: "lsl-iroh-start",
      icon: "🌐",
      section: t("cmdK.sectionLsl"),
      label: t("cmdK.lslIrohStart"),
      keywords: "lsl iroh remote quic sink accept tunnel phone",
      action: async () => {
        try {
          const r = await lslIrohStart<{ endpoint_id: string }>();
          addToast("success", "iroh Sink", `Endpoint: ${r.endpoint_id.slice(0, 16)}…`);
          nav.openSettingsTab("lsl");
        } catch (e) {
          addToast("error", "iroh Sink", String(e));
        }
      },
    },
    {
      id: "lsl-iroh-stop",
      icon: "⏹",
      section: t("cmdK.sectionLsl"),
      label: t("cmdK.lslIrohStop"),
      keywords: "lsl iroh stop sink cancel disconnect",
      action: async () => {
        await lslIrohStop();
        addToast("info", "iroh Sink", "Stopped.");
      },
    },

    // ── Calibration ────────────────────────────────────────────────────
    {
      id: "open-calibration",
      icon: "🎯",
      section: t("cmdK.sectionCalibration"),
      label: t("cmdK.openCalibration"),
      shortcut: `${mod}⇧C`,
      keywords: t("cmdK.kw.calibration"),
      action: async () => {
        try {
          await nav.openCalibration();
        } catch (e) {
          addToast("warning", t("cmdK.calibrationError"), String(e));
        }
      },
    },

    {
      id: "open-api",
      icon: "🌐",
      section: t("cmdK.sectionNavigation"),
      label: t("cmdK.openApi"),
      keywords: t("cmdK.kw.api"),
      action: nav.openApi,
    },
    {
      id: "open-labels",
      icon: "🏷",
      section: t("cmdK.sectionNavigation"),
      label: t("labels.openLabels"),
      keywords: "labels annotations notes tags all browse edit delete manage",
      action: nav.openLabels,
    },
    {
      id: "open-focus-timer",
      icon: "⏱",
      section: t("cmdK.sectionNavigation"),
      label: t("focusTimer.openTimer"),
      keywords: "pomodoro focus timer work break productivity neurofeedback session",
      action: nav.openFocusTimer,
    },
    {
      id: "open-downloads",
      icon: "⬇",
      section: t("cmdK.sectionNavigation"),
      label: t("downloads.windowTitle"),
      keywords: "downloads download manager llm models pause resume cancel delete progress",
      action: nav.openDownloads,
    },
    {
      id: "open-onboarding",
      icon: "🧭",
      section: t("cmdK.sectionNavigation"),
      label: t("cmdK.openOnboarding"),
      keywords: t("cmdK.kw.onboarding"),
      action: nav.openOnboarding,
    },
    {
      id: "open-electrodes",
      icon: "🧠",
      section: t("cmdK.sectionNavigation"),
      label: t("cmdK.openElectrodes"),
      keywords: t("cmdK.kw.electrodes"),
      action: nav.openHelp,
    },

    // ── Settings tabs (deep links) ───────────────────────────────────
    ...settingsTabCommands(),

    // ── Utilities ──────────────────────────────────────────────────────
    {
      id: "show-shortcuts",
      icon: "⌨",
      section: t("cmdK.sectionUtilities"),
      label: t("cmdK.showShortcuts"),
      shortcut: "?",
      keywords: t("cmdK.kw.shortcuts"),
      action: () => {
        close();
        // Simulate pressing ? to open the shortcuts overlay
        window.dispatchEvent(new KeyboardEvent("keydown", { key: "?", bubbles: true }));
      },
    },
    {
      id: "toggle-hc",
      icon: "◑",
      section: t("cmdK.sectionUtilities"),
      label: getHighContrast() ? t("cmdK.highContrastOff") : t("cmdK.highContrastOn"),
      keywords: t("cmdK.kw.highContrast"),
      toggle: {
        get: getHighContrast,
        set: (v) => {
          import("$lib/stores/theme.svelte").then((m) => m.setHighContrast(v));
        },
      },
      action: () => {
        toggleHighContrast();
        close();
      },
    },
    {
      id: "toggle-dark-mode",
      icon: "🌗",
      section: t("cmdK.sectionUtilities"),
      label: getResolved() === "dark" ? "Switch to Light Mode" : "Switch to Dark Mode",
      keywords: "dark light mode theme toggle switch appearance color colour",
      toggle: { get: () => getResolved() === "dark", set: (v) => setTheme(v ? "dark" : "light") },
      action: () => {
        toggleTheme();
        close();
      },
    },
    {
      id: "check-updates",
      icon: "⬆",
      section: t("cmdK.sectionUtilities"),
      label: t("cmdK.checkUpdates"),
      keywords: t("cmdK.kw.updates"),
      action: nav.openUpdates,
    },
  ];
}

// ── Fuzzy scoring ──────────────────────────────────────────────────────────

interface ScoredCommand extends Command {
  /** Higher = better. 0 when there is no active query. */
  matchScore: number;
  /** Indices inside cmd.label that matched the query (for highlight rendering). */
  labelPositions: number[];
}

/**
 * fzf-style subsequence fuzzy match.
 *
 * Every character of `query` must appear somewhere in `text`, in order
 * (subsequence / scattered match).  Returns `null` when that condition
 * is not met.  Otherwise returns a numeric score and the matched positions.
 *
 * Scoring bonuses (additive, higher = better):
 *  +15 per char that immediately follows the previous match (consecutive run)
 *  + 5 × run_length on top of the consecutive bonus (rewards longer runs)
 *  +10 first character of the target string matched
 *  + 8 match lands on a word boundary (char before is space / - / _ / . / /)
 *  + 3 matched character is uppercase in the original (camelCase boundary)
 *  − 0.5 × gap_count  gap penalty (scattered matches score lower)
 *  − 0.1 × text_length  length penalty (shorter texts beat longer ones)
 */
function fuzzyScore(query: string, text: string): { score: number; positions: number[] } | null {
  const q = query.toLowerCase();
  const t = text.toLowerCase();

  // Forward greedy pass — find every query char in order
  const positions: number[] = [];
  let ti = 0;
  for (let qi = 0; qi < q.length; qi++) {
    let found = false;
    while (ti < t.length) {
      if (t[ti] === q[qi]) {
        positions.push(ti++);
        found = true;
        break;
      }
      ti++;
    }
    if (!found) return null; // subsequence not present
  }

  // Score the positions
  let score = 0;
  let run = 0;
  for (let i = 0; i < positions.length; i++) {
    const pos = positions[i];
    const prev = i > 0 ? positions[i - 1] : -2;

    if (pos === prev + 1) {
      run++;
      score += 15 + run * 5; // escalating consecutive-run bonus
    } else {
      run = 0;
    }

    if (pos === 0) score += 10; // start-of-string

    if (pos > 0) {
      const pc = t[pos - 1];
      if (pc === " " || pc === "-" || pc === "_" || pc === "." || pc === "/") score += 8; // word-boundary
    }

    if (text[pos] !== t[pos]) score += 3; // uppercase / camelCase boundary
  }

  // Gap penalty — penalise scattered matches
  if (positions.length > 1) {
    const span = positions[positions.length - 1] - positions[0] + 1;
    score -= (span - positions.length) * 0.5;
  }

  // Length penalty — shorter targets rank higher for the same match quality
  score -= t.length * 0.1;

  return { score, positions };
}

/**
 * Score a single command against the query.
 *
 * Searches four fields with different weights:
 *   label (1.0)  keywords (0.7)  id (0.4)  section (0.2)
 *
 * Label match positions are kept for character-level highlight rendering.
 * Returns `matchScore: -Infinity` when no field matches.
 */
// ── Contextual boosting ─────────────────────────────────────────────────────

/** Sections that get a score boost when a device is connected. */
const DEVICE_SECTIONS_PATTERN = /device|exg|eeg|calibration|lsl/i;

function contextBoost(cmd: Command): number {
  if (deviceConnected && DEVICE_SECTIONS_PATTERN.test(cmd.section)) return 12;
  if (!deviceConnected && cmd.id.startsWith("settings-devices")) return 8;
  return 0;
}

function scoreCommand(q: string, cmd: Command): ScoredCommand {
  if (!q) return { ...cmd, matchScore: 0, labelPositions: [] };

  const lm = fuzzyScore(q, cmd.label);
  const km = cmd.keywords ? fuzzyScore(q, cmd.keywords) : null;
  const im = fuzzyScore(q, cmd.id);
  const sm = fuzzyScore(q, cmd.section);

  let best = Math.max(
    lm ? lm.score * 1.0 : -Infinity,
    km ? km.score * 0.7 : -Infinity,
    im ? im.score * 0.4 : -Infinity,
    sm ? sm.score * 0.2 : -Infinity,
  );

  // Typo tolerance fallback — if no fuzzy match, try edit distance
  if (!Number.isFinite(best)) {
    const typoLabel = typoMatch(q, cmd.label);
    const typoKw = cmd.keywords ? typoMatch(q, cmd.keywords) : null;
    const typoScore = Math.max(typoLabel ?? -Infinity, typoKw ?? -Infinity);
    if (Number.isFinite(typoScore)) {
      best = typoScore;
    } else {
      return { ...cmd, matchScore: -Infinity, labelPositions: [] };
    }
  }

  // Apply usage frequency/recency boost
  best += usageBoost(cmd.id);
  // Apply contextual device boost
  best += contextBoost(cmd);

  return {
    ...cmd,
    matchScore: best,
    labelPositions: lm ? lm.positions : [],
  };
}

/**
 * Split `text` into alternating plain / highlighted segments.
 * Used to render matched characters in a different colour.
 */
function highlightSegments(text: string, positions: number[]): { t: string; hi: boolean }[] {
  if (!positions.length) return [{ t: text, hi: false }];
  const posSet = new Set(positions);
  const out: { t: string; hi: boolean }[] = [];
  let buf = "";
  let bufHi = false;
  for (let i = 0; i < text.length; i++) {
    const hi = posSet.has(i);
    if (hi !== bufHi && buf) {
      out.push({ t: buf, hi: bufHi });
      buf = "";
    }
    bufHi = hi;
    buf += text[i];
  }
  if (buf) out.push({ t: buf, hi: bufHi });
  return out;
}

// ── Filtering & ranking ────────────────────────────────────────────────────

const isFiltering = $derived(query.trim().length > 0);

/**
 * Flat list of matching commands.
 * • No query   → all commands in their original order.
 * • With query → only matching commands, sorted best-score-first.
 */
// ── Prefix-based command modes ────────────────────────────────────────────
// > system commands only, @ settings only, # history/sessions, / skills

function parsePrefixMode(raw: string): { prefix: string; q: string } {
  const trimmed = raw.trim();
  if (trimmed.startsWith(">")) return { prefix: ">", q: trimmed.slice(1).trim() };
  if (trimmed.startsWith("@")) return { prefix: "@", q: trimmed.slice(1).trim() };
  return { prefix: "", q: trimmed };
}

function filterByPrefix(cmds: Command[], prefix: string): Command[] {
  if (prefix === ">") return cmds.filter((c) => !c.id.startsWith("settings-"));
  if (prefix === "@") return cmds.filter((c) => c.id.startsWith("settings-"));
  return cmds;
}

let scored = $derived.by((): ScoredCommand[] => {
  const cmds = commands();
  const { prefix, q } = parsePrefixMode(query);

  if (!q && !prefix) {
    // Show recent commands at top, then the rest
    const recentIds = getRecentIds(5);
    const recentSet = new Set(recentIds);
    const recentCmds = recentIds
      .map((id) => cmds.find((c) => c.id === id))
      .filter((c): c is Command => !!c)
      .map((c) => ({ ...c, matchScore: 0, labelPositions: [] as number[] }));
    const rest = cmds
      .filter((c) => !recentSet.has(c.id))
      .map((c) => ({ ...c, matchScore: 0, labelPositions: [] as number[] }));
    return [...recentCmds, ...rest];
  }

  const filtered = filterByPrefix(cmds, prefix);
  if (!q) {
    return filtered.map((c) => ({ ...c, matchScore: 0, labelPositions: [] }));
  }

  const queries = synonymQueries(q.toLowerCase());
  // Score against original + synonym-expanded queries, take best
  return filtered
    .map((c) => {
      let best = scoreCommand(queries[0], c);
      for (let i = 1; i < queries.length; i++) {
        const alt = scoreCommand(queries[i], c);
        if (alt.matchScore > best.matchScore) {
          best = { ...alt, labelPositions: best.labelPositions.length ? best.labelPositions : alt.labelPositions };
        }
      }
      return best;
    })
    .filter((c) => Number.isFinite(c.matchScore))
    .sort((a, b) => b.matchScore - a.matchScore);
});

/**
 * Sections used for rendering.
 * • No query   → grouped by section with headers (original behaviour).
 * • With query → single nameless group (flat sorted list, no header rendered).
 */
let sections = $derived.by((): [string, ScoredCommand[]][] => {
  if (isFiltering) {
    const result: [string, ScoredCommand[]][] = scored.length ? [["", scored]] : [];
    // Append semantic (NLP) results if available and fuzzy results are sparse
    if (semanticResults.length > 0) {
      const fuzzyIds = new Set(scored.map((c) => c.id));
      const unique = semanticResults.filter((c) => !fuzzyIds.has(c.id));
      if (unique.length > 0) result.push(["Suggested", unique]);
    }
    return result;
  }
  // Hide individual settings entries from the default (unfiltered) view
  const visible = scored.filter((c) => !c.id.startsWith("settings-"));
  // Split recent from the rest
  const recentIds = new Set(getRecentIds(5));
  const recent = visible.filter((c) => recentIds.has(c.id));
  const rest = visible.filter((c) => !recentIds.has(c.id));

  const result: [string, ScoredCommand[]][] = [];
  if (recent.length > 0) result.push(["Recent", recent]);

  const map = new Map<string, ScoredCommand[]>();
  for (const c of rest) {
    if (!map.has(c.section)) map.set(c.section, []);
    map.get(c.section)?.push(c);
  }
  for (const entry of map.entries()) result.push(entry);
  return result;
});

// Reset keyboard selection whenever the result set changes
$effect(() => {
  void scored;
  active = 0;
});

// Debounced semantic search — fires when fuzzy results are sparse
$effect(() => {
  const q = query.trim();
  const fuzzyCount = scored.length;
  clearTimeout(semanticDebounce);
  semanticResults = [];

  if (q.length < 6 || fuzzyCount > 5) return;

  semanticDebounce = setTimeout(async () => {
    try {
      const cmds = commands();
      const candidates = cmds.map((c) => ({
        id: c.id,
        text: `${c.label} ${c.keywords ?? ""} ${c.section}`,
      }));
      const res = await daemonPost<{ results: { id: string; score: number }[] }>(
        "/v1/search/commands",
        { query: q, candidates },
        5000,
      );
      if (res?.results?.length) {
        const cmdMap = new Map(cmds.map((c) => [c.id, c]));
        semanticResults = res.results
          .slice(0, 5)
          .map((r): ScoredCommand | undefined => {
            const cmd = cmdMap.get(r.id);
            if (!cmd) return undefined;
            return { ...cmd, matchScore: r.score, labelPositions: [] as number[] };
          })
          .filter((c): c is ScoredCommand => !!c);
      }
    } catch {
      // Daemon unavailable or route not yet deployed — gracefully degrade
    }
  }, 300);
});

// ── Keyboard handling ──────────────────────────────────────────────────────

function handleGlobalKeydown(e: KeyboardEvent) {
  // Cmd/Ctrl+K to toggle
  if (e.key === "k" && (isMac ? e.metaKey : e.ctrlKey)) {
    e.preventDefault();
    e.stopPropagation();
    if (open) close();
    else openPalette();
    return;
  }
  // Escape to close
  if (e.key === "Escape" && open) {
    e.preventDefault();
    e.stopPropagation();
    close();
  }
}

function handleInputKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    active = Math.min(active + 1, scored.length - 1);
    scrollActiveIntoView();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    active = Math.max(active - 1, 0);
    scrollActiveIntoView();
  } else if (e.key === "Tab" && scored.length > 0) {
    // Tab toggles the active command if it has a toggle
    const cmd = scored[active];
    if (cmd?.toggle) {
      e.preventDefault();
      cmd.toggle.set(!cmd.toggle.get());
    }
  } else if (e.key === "Enter" && scored.length > 0) {
    e.preventDefault();
    runCommand(scored[active]);
  }
}

function scrollActiveIntoView() {
  requestAnimationFrame(() => {
    const el = document.querySelector(`[data-cmd-index="${active}"]`);
    el?.scrollIntoView({ block: "nearest" });
  });
}

function openPalette() {
  query = "";
  active = 0;
  semanticResults = [];
  open = true;
  requestAnimationFrame(() => inputEl?.focus());
  // Refresh device status for contextual boosting
  getDeviceStatus<{ state: string }>()
    .then((s) => {
      deviceConnected = s.state === "connected";
    })
    .catch(() => {
      deviceConnected = false;
    });
}

function close() {
  open = false;
}

function runCommand(cmd: Command) {
  recordUsage(cmd.id);
  close();
  cmd.action();
}

// Compute flat index for each command across grouped sections
function flatIndex(sectionIdx: number, itemIdx: number): number {
  let idx = 0;
  for (let s = 0; s < sectionIdx; s++) {
    idx += sections[s][1].length;
  }
  return idx + itemIdx;
}

onMount(() => {
  window.addEventListener("keydown", handleGlobalKeydown, true);
});
onDestroy(() => {
  window.removeEventListener("keydown", handleGlobalKeydown, true);
});
</script>

{#if open}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-[9998] bg-black/40 backdrop-blur-sm"
    transition:fade={{ duration: 100 }}
    onclick={close}
    onkeydown={handleGlobalKeydown}
  ></div>

  <!-- Palette -->
  <div
    class="fixed top-[15%] left-[50%] z-[9999] w-full max-w-[480px]
           translate-x-[-50%]
           rounded-2xl border border-border dark:border-white/[0.1]
           bg-white dark:bg-[#18181f] shadow-2xl
           flex flex-col overflow-hidden"
    transition:fade={{ duration: 100 }}
    role="dialog"
    aria-modal="true"
    aria-label={t("cmdK.title")}
  >
    <!-- Search input -->
    <div class="flex items-center gap-2.5 px-4 py-3 border-b border-border dark:border-white/[0.06]">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
           stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
           class="w-4 h-4 text-muted-foreground shrink-0">
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={handleInputKeydown}
        type="text"
        placeholder={t("cmdK.placeholder")}
        class="flex-1 bg-transparent text-[0.82rem] text-foreground
               placeholder:text-muted-foreground/50
               focus:outline-none"
        spellcheck="false"
        autocomplete="off"
      />
      <kbd class="text-[0.55rem] font-mono text-muted-foreground/50 border border-border
                  dark:border-white/[0.08] rounded px-1.5 py-0.5 shrink-0">
        Esc
      </kbd>
    </div>

    <!-- Results -->
    <div class="max-h-[50vh] overflow-y-auto py-1.5">
      {#if scored.length === 0}
        <p class="text-center text-[0.75rem] text-muted-foreground/50 py-6">
          {t("cmdK.noResults")}
        </p>
      {:else}
        {#each sections as [sectionLabel, cmds], sIdx}
          {#if sectionLabel}
            <div class="px-3 pt-2 pb-1">
              <p class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground/60 px-1">
                {sectionLabel}
              </p>
            </div>
          {/if}
          {#each cmds as cmd, cIdx}
            {@const fi = flatIndex(sIdx, cIdx)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              data-cmd-index={fi}
              class="flex items-center gap-2.5 mx-1.5 px-3 py-2 rounded-lg cursor-pointer
                     transition-colors
                     {fi === active
                       ? 'bg-blue-500/10 dark:bg-blue-500/15 text-foreground'
                       : 'text-foreground/80 hover:bg-accent'}"
              onclick={() => runCommand(cmd)}
              onmouseenter={() => (active = fi)}
              onkeydown={(e) => { if (e.key === 'Enter') runCommand(cmd); }}
              role="option"
              aria-selected={fi === active}
              tabindex="-1"
            >
              <span class="w-5 text-center text-[0.85rem] shrink-0">{cmd.icon}</span>
              <span class="flex-1 text-[0.78rem] font-medium truncate">
                {#each highlightSegments(cmd.label, cmd.labelPositions) as seg}
                  {#if seg.hi}<span class="text-blue-500 dark:text-blue-400 font-bold">{seg.t}</span>{:else}{seg.t}{/if}
                {/each}
              </span>
              {#if cmd.toggle}
                <span class="text-[0.6rem] font-mono shrink-0 px-1.5 py-0.5 rounded
                             {cmd.toggle.get()
                               ? 'bg-blue-500/20 text-blue-500 dark:text-blue-400'
                               : 'bg-muted-foreground/10 text-muted-foreground/50'}">
                  {cmd.toggle.get() ? "ON" : "OFF"}
                </span>
              {/if}
              {#if cmd.shortcut}
                <kbd class="text-[0.55rem] font-mono text-muted-foreground/50 border border-border
                            dark:border-white/[0.08] rounded px-1.5 py-0.5 shrink-0 whitespace-nowrap">
                  {cmd.shortcut}
                </kbd>
              {/if}
            </div>
          {/each}
        {/each}
      {/if}
    </div>

    <!-- Footer -->
    <div class="flex items-center gap-3 px-4 py-2 border-t border-border dark:border-white/[0.06]
                text-[0.55rem] text-muted-foreground/50">
      <span>↑↓ {t("cmdK.navigate")}</span>
      <span>↵ {t("cmdK.run")}</span>
      <span>⇥ Toggle</span>
      <span class="opacity-50">@ settings</span>
      <span class="opacity-50">&gt; commands</span>
      <span class="ml-auto">{t("cmdK.footerHint")}</span>
    </div>
  </div>
{/if}
