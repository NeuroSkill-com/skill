<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Onboarding / first-run wizard -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onDestroy, onMount } from "svelte";
import { fade, fly } from "svelte/transition";
import ElectrodeGuide from "$lib/charts/ElectrodeGuide.svelte";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Progress } from "$lib/components/ui/progress";
import { ToggleRow } from "$lib/components/ui/toggle-row";
import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";
import { daemonPost } from "$lib/daemon/http";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import {
  getActiveWindowTracking,
  getCalendarTracking,
  getClipboardTracking,
  getFileActivityTracking,
  getInputActivityTracking,
  getLocationEnabled,
  setActiveWindowTracking,
  setCalendarTracking,
  setClipboardTracking,
  setFileActivityTracking,
  setInputActivityTracking,
  setLocationEnabled,
} from "$lib/daemon/settings";
import { onDaemonEvent } from "$lib/daemon/ws";
import { t } from "$lib/i18n/index.svelte";
import { openSettings } from "$lib/navigation";
import { FONT_SIZE_PRESETS, getFontSize, setFontSize } from "$lib/stores/font-size.svelte";
import { useWindowTitle } from "$lib/stores/window-title.svelte";
import type { DeviceStatus } from "$lib/types";

// ── Types ──────────────────────────────────────────────────────────────────
interface CalibrationAction {
  label: string;
  duration_secs: number;
}
interface CalibrationProfile {
  id: string;
  name: string;
  actions: CalibrationAction[];
  break_duration_secs: number;
  loop_count: number;
  auto_start: boolean;
  last_calibration_utc: number | null;
}
type DownloadState = "not_downloaded" | "downloading" | "downloaded" | "failed" | "cancelled";
interface LlmModelEntry {
  repo?: string;
  filename: string;
  quant: string;
  size_gb: number;
  family_id: string;
  family_name: string;
  is_mmproj: boolean;
  recommended: boolean;
  state: DownloadState;
  progress: number;
}
interface LlmCatalogLite {
  entries: LlmModelEntry[];
  active_model: string;
  active_mmproj: string;
}
interface EegModelStatusLite {
  weights_found: boolean;
  downloading_weights: boolean;
  download_progress: number;
  download_status_msg: string | null;
}
interface NeuttsConfig {
  enabled: boolean;
  backbone_repo: string;
  gguf_file: string;
  voice_preset: string;
  ref_wav_path: string;
  ref_text: string;
}
type OnboardingModelKey = "zuna" | "kitten" | "neutts" | "llm" | "ocr";
type CalPhase = "idle" | "action" | "break" | "done";
interface Phase {
  kind: CalPhase;
  actionIndex: number;
  loop: number;
}

// ── Double-click titlebar maximize/restore ─────────────────────────────────
let _obSavedBounds: { x: number; y: number; width: number; height: number } | null = null;
let _obIsMax = false;
async function toggleMaximizeWindow() {
  const win = getCurrentWindow();
  if (_obIsMax && _obSavedBounds) {
    await win.unmaximize();
    await win.setSize(new LogicalSize(_obSavedBounds.width, _obSavedBounds.height));
    await win.setPosition(new LogicalPosition(_obSavedBounds.x, _obSavedBounds.y));
    _obIsMax = false;
    _obSavedBounds = null;
  } else {
    const pos = await win.outerPosition();
    const size = await win.outerSize();
    const f = await win.scaleFactor();
    _obSavedBounds = { x: pos.x / f, y: pos.y / f, width: size.width / f, height: size.height / f };
    await win.maximize();
    _obIsMax = true;
  }
}

// ── Steps ──────────────────────────────────────────────────────────────────
type Step =
  | "welcome"
  | "enable_bluetooth"
  | "bluetooth"
  | "fit"
  | "calibration"
  | "models"
  | "tray"
  | "permissions"
  | "extensions"
  | "done";

// Each step records the wizard schema version it was introduced in. Compared
// against the user's last-completed version to decide if a NEW badge should
// appear in the progress nav and on the step itself. Bump
// `CURRENT_ONBOARDING_VERSION` in skill-constants whenever a new step is
// added, and set `addedIn` here to that same number.
interface StepMeta {
  id: Step;
  addedIn: number;
}
const STEP_META: StepMeta[] = [
  { id: "welcome", addedIn: 1 },
  { id: "enable_bluetooth", addedIn: 1 },
  { id: "bluetooth", addedIn: 1 },
  { id: "fit", addedIn: 1 },
  { id: "calibration", addedIn: 1 },
  { id: "models", addedIn: 1 },
  { id: "tray", addedIn: 1 },
  { id: "permissions", addedIn: 2 },
  { id: "extensions", addedIn: 2 },
  { id: "done", addedIn: 1 },
];
const STEPS: Step[] = STEP_META.map((s) => s.id);

let step = $state<Step>("welcome");

// ── Onboarding version status (drives "what's new" banner + NEW badges) ────
interface OnboardingStatus {
  completedVersion: number;
  currentVersion: number;
  isReturning: boolean;
}
let onboardingStatus = $state<OnboardingStatus>({
  completedVersion: 0,
  currentVersion: 0,
  isReturning: false,
});
function isNewStep(id: Step): boolean {
  const meta = STEP_META.find((m) => m.id === id);
  if (!meta) return false;
  // Only flag steps as NEW once the user has at least one prior completion;
  // first-run users see every step for the first time, so badges would be noise.
  return onboardingStatus.completedVersion > 0 && meta.addedIn > onboardingStatus.completedVersion;
}

// ── Bluetooth adapter check (OS-level)
let btEnabled = $state<boolean | null>(null);
let stepIdx = $derived(STEPS.indexOf(step));

// ── Font-size A−/A+ control (in top bar) ──────────────────────────────────
//
// Exposes the same global font-size scale as Settings → Appearance, but
// reachable from the top bar of every onboarding step so a low-vision user
// who can't read step 1 can fix it without first navigating Settings.
// The setter persists to localStorage; changes survive into the main app.
let fontSizePct = $state<number>(getFontSize());
const minFontPct = FONT_SIZE_PRESETS[0].value;
const maxFontPct = FONT_SIZE_PRESETS[FONT_SIZE_PRESETS.length - 1].value;
function bumpFontSize(direction: 1 | -1) {
  const idx = FONT_SIZE_PRESETS.findIndex((p) => p.value === fontSizePct);
  // If the current value isn't a preset (e.g. user-set custom from elsewhere),
  // snap to the closest preset before stepping.
  const startIdx = idx >= 0 ? idx : nearestFontPresetIdx(fontSizePct);
  const nextIdx = Math.max(0, Math.min(FONT_SIZE_PRESETS.length - 1, startIdx + direction));
  if (nextIdx === startIdx && idx >= 0) return;
  fontSizePct = FONT_SIZE_PRESETS[nextIdx].value;
  setFontSize(fontSizePct);
}
function nearestFontPresetIdx(pct: number): number {
  let best = 0;
  let bestDist = Infinity;
  FONT_SIZE_PRESETS.forEach((p, i) => {
    const d = Math.abs(p.value - pct);
    if (d < bestDist) {
      bestDist = d;
      best = i;
    }
  });
  return best;
}

// ── Reactive status ────────────────────────────────────────────────────────
let status = $state<DeviceStatus>({
  state: "disconnected",
  device_name: null,
  battery: 0,
  channel_quality: ["no_signal", "no_signal", "no_signal", "no_signal"],
} as DeviceStatus);

const EEG_CH = ["TP9", "AF7", "AF8", "TP10"];
const QC: Record<string, string> = {
  good: "#22c55e",
  fair: "#eab308",
  poor: "#f97316",
  no_signal: "#94a3b8",
};

let isConnected = $derived(status.state === "connected");
let isScanning = $derived(status.state === "scanning");
let allGoodOrFair = $derived(status.channel_quality.every((q: string) => q === "good" || q === "fair"));

// ── Inline calibration state ───────────────────────────────────────────────
let calProfile = $state<CalibrationProfile | null>(null);
let calPhase = $state<Phase>({ kind: "idle", actionIndex: 0, loop: 1 });
let calCountdown = $state(0);
let calTotal = $state(0);
let calRunning = $state(false);
let ttsReady = $state(false);
let ttsDlLabel = $state("");
let unlistenTts: UnlistenFn | null = null;
let modelsTimer: ReturnType<typeof setInterval> | null = null;

// ── Model download step state ─────────────────────────────────────────────
let llmTarget = $state<LlmModelEntry | null>(null);
let zunaStatus = $state<EegModelStatusLite | null>(null);
let modelLoadError = $state("");
let ttsActionBusy = $state(false);
let neuttsDlState = $state<"idle" | "downloading" | "ready" | "error">("idle");
let kittenDlState = $state<"idle" | "downloading" | "ready" | "error">("idle");
let neuttsDlError = $state("");
let kittenDlError = $state("");
let bundleBusy = $state(false);
let ocrDlState = $state<"idle" | "downloading" | "ready" | "error">("idle");
let ocrDlError = $state("");
let screenRecPerm = $state<boolean | null>(null);
const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);

// ── Activity tracking opt-in state (permissions step) ─────────────────────
//
// Each toggle here mirrors a single persistent flag in the daemon settings.
// The toggle handler optimistically updates the local state and persists in
// the background; failures are swallowed so a flaky daemon connection
// doesn't make the wizard feel broken (the next step's load will re-sync).
let trackActiveWindow = $state(false);
let trackInputActivity = $state(true);
let trackFileActivity = $state(false);
let trackClipboard = $state(false);
let trackScreenshots = $state(false);
let trackLocation = $state(false);
let trackCalendar = $state(false);
let permissionsLoaded = $state(false);

interface ScreenshotConfig {
  enabled: boolean;
  [key: string]: unknown;
}

async function loadPermissionsState() {
  if (permissionsLoaded) return;
  try {
    [trackActiveWindow, trackInputActivity, trackFileActivity, trackClipboard, trackLocation, trackCalendar] =
      await Promise.all([
        getActiveWindowTracking(),
        getInputActivityTracking(),
        getFileActivityTracking(),
        getClipboardTracking(),
        getLocationEnabled().catch(() => false),
        getCalendarTracking().catch(() => false),
      ]);
  } catch {}
  // Screenshots have a richer config object; we only care about the enabled bit.
  try {
    const cfg = await daemonInvoke<ScreenshotConfig>("get_screenshot_config");
    trackScreenshots = !!cfg.enabled;
  } catch {}
  permissionsLoaded = true;
}

async function toggleTrackActiveWindow() {
  trackActiveWindow = !trackActiveWindow;
  try {
    await setActiveWindowTracking(trackActiveWindow);
  } catch {}
}
async function toggleTrackInputActivity() {
  trackInputActivity = !trackInputActivity;
  try {
    await setInputActivityTracking(trackInputActivity);
  } catch {}
}
async function toggleTrackFileActivity() {
  trackFileActivity = !trackFileActivity;
  try {
    await setFileActivityTracking(trackFileActivity);
  } catch {}
}
async function toggleTrackClipboard() {
  trackClipboard = !trackClipboard;
  try {
    await setClipboardTracking(trackClipboard);
  } catch {}
}
async function toggleTrackScreenshots() {
  trackScreenshots = !trackScreenshots;
  try {
    // Read-modify-write: the screenshot config has many other tunables
    // (interval, OCR, image size). We only flip `enabled` and preserve the rest.
    const cfg = await daemonInvoke<ScreenshotConfig>("get_screenshot_config");
    cfg.enabled = trackScreenshots;
    await daemonInvoke("set_screenshot_config", { config: cfg });
  } catch {}
}
async function toggleTrackLocation() {
  trackLocation = !trackLocation;
  try {
    await setLocationEnabled(trackLocation);
  } catch {}
}
async function toggleTrackCalendar() {
  trackCalendar = !trackCalendar;
  try {
    await setCalendarTracking(trackCalendar);
  } catch {}
  // First-time enable: trigger the macOS Calendar permission prompt now so the
  // user doesn't get a surprise dialog the first time some other screen tries
  // to read events.
  if (trackCalendar) {
    try {
      await invoke("request_calendar_permission");
    } catch {}
  }
}

$effect(() => {
  if (step === "permissions") loadPermissionsState();
  if (step === "extensions") loadExtensionsState();
});

// ── Extensions opt-in state (extensions step) ─────────────────────────────
//
// Three opt-in installers, each backed by infrastructure that already exists
// in Settings → Extensions / Settings → Terminal. We surface a single primary
// install per category and link out to the full Settings tab for variants
// (multiple VS Code forks, multiple browsers, multiple shells).
type InstallState = "idle" | "installing" | "installed" | "error";
interface VsCodeFork {
  id: string;
  name: string;
  available: boolean;
  installed: boolean;
}
interface ExtensionsCheck {
  vscode_forks: VsCodeFork[];
  vscode: boolean;
  chrome: boolean;
  firefox: boolean;
  safari: boolean;
  edge: boolean;
}
let extensionsChecked = $state(false);
let extensionsCheck = $state<ExtensionsCheck | null>(null);
let vsCodeInstall = $state<InstallState>("idle");
let vsCodeMessage = $state("");
let vsCodePickedFork = $state<string>("vscode");
let browserInstall = $state<InstallState>("idle");
let browserMessage = $state("");
// Default browser pick: Safari on Mac, Chrome elsewhere.
let browserPicked = $state<"chrome" | "firefox" | "safari" | "edge">(isMac ? "safari" : "chrome");
let shellInstall = $state<InstallState>("idle");
let shellMessage = $state("");
// Default shell pick: best-effort guess from navigator.userAgent / platform.
const isWindows = typeof navigator !== "undefined" && /Win/i.test(navigator.platform);
let shellPicked = $state<"zsh" | "bash" | "fish" | "powershell">(isWindows ? "powershell" : "zsh");

async function loadExtensionsState() {
  if (extensionsChecked) return;
  try {
    const c = await invoke<ExtensionsCheck>("check_extensions_installed");
    extensionsCheck = c;
    // Pick the first installed VS Code fork as the default install target,
    // otherwise the first available fork, otherwise leave the default.
    const installed = c.vscode_forks?.find((f) => f.installed);
    const available = c.vscode_forks?.find((f) => f.available);
    if (installed) vsCodePickedFork = installed.id;
    else if (available) vsCodePickedFork = available.id;
    if (installed) vsCodeInstall = "installed";
  } catch {}
  extensionsChecked = true;
}

async function installVsCode() {
  vsCodeInstall = "installing";
  vsCodeMessage = "";
  try {
    const r = await invoke<{ ok: boolean; message: string }>("install_extension", {
      extensionId: vsCodePickedFork,
    });
    vsCodeInstall = r.ok ? "installed" : "error";
    vsCodeMessage = r.message ?? "";
  } catch (e) {
    vsCodeInstall = "error";
    vsCodeMessage = String(e);
  }
}
async function installBrowser() {
  browserInstall = "installing";
  browserMessage = "";
  try {
    const r = await invoke<{ ok: boolean; message: string }>("install_extension", {
      extensionId: browserPicked,
    });
    browserInstall = r.ok ? "installed" : "error";
    browserMessage = r.message ?? "";
  } catch (e) {
    browserInstall = "error";
    browserMessage = String(e);
  }
}
async function installShell() {
  shellInstall = "installing";
  shellMessage = "";
  try {
    const r = await daemonPost<{
      ok: boolean;
      installed?: boolean;
      already_installed?: boolean;
      instructions?: string;
      note?: string;
    }>("/v1/activity/install-shell-hook", { shell: shellPicked });
    shellInstall = r.ok && (r.installed || r.already_installed) ? "installed" : "error";
    shellMessage = r.instructions ?? r.note ?? "";
  } catch (e) {
    shellInstall = "error";
    shellMessage = String(e);
  }
}
let onboardingDownloadOrder = $state<OnboardingModelKey[]>(["zuna", "kitten", "neutts", "llm", "ocr"]);
type AutoModelStage = OnboardingModelKey | "done";
let autoModelStage = $state<AutoModelStage>("zuna");
let autoModelInFlight = $state(false);
let autoModelsStarted = $state(false);

const llmIsDownloading = $derived(llmTarget?.state === "downloading");
const llmIsDownloaded = $derived(llmTarget?.state === "downloaded");
const llmProgressPct = $derived((llmTarget?.progress ?? 0) * 100);
const zunaIsDownloading = $derived(zunaStatus?.downloading_weights ?? false);
const zunaIsDownloaded = $derived(zunaStatus?.weights_found ?? false);
const zunaProgressPct = $derived((zunaStatus?.download_progress ?? 0) * 100);
const allRecommendedReady = $derived(
  llmIsDownloaded &&
    zunaIsDownloaded &&
    neuttsDlState === "ready" &&
    kittenDlState === "ready" &&
    ocrDlState === "ready",
);

const footerModelStatus = $derived.by(() => {
  const fmt = (name: string, ready: boolean, downloading: boolean, pct: number, hasError: boolean) => {
    if (ready) return `${name} ✓`;
    if (hasError) return `${name} ⚠`;
    if (downloading) return `${name} ${Math.round(Math.max(0, Math.min(100, pct)))}%`;
    return `${name} ○`;
  };

  const stagePart = (stage: OnboardingModelKey) => {
    if (stage === "zuna") return fmt("ZUNA", zunaIsDownloaded, zunaIsDownloading, zunaProgressPct, false);
    if (stage === "kitten")
      return fmt("Kitten", kittenDlState === "ready", kittenDlState === "downloading", 0, kittenDlState === "error");
    if (stage === "neutts")
      return fmt("NeuTTS", neuttsDlState === "ready", neuttsDlState === "downloading", 0, neuttsDlState === "error");
    if (stage === "ocr")
      return fmt("OCR", ocrDlState === "ready", ocrDlState === "downloading", 0, ocrDlState === "error");
    return fmt("LLM", llmIsDownloaded, llmIsDownloading, llmProgressPct, false);
  };
  const parts = onboardingDownloadOrder.map(stagePart);

  if (allRecommendedReady) {
    return `Model setup complete • ${parts.join(" • ")}`;
  }
  return `Model setup • ${parts.join(" • ")}`;
});

const calProgressPct = $derived(calTotal > 0 ? ((calTotal - calCountdown) / calTotal) * 100 : 0);

const CAL_COLORS = [
  "text-blue-600 dark:text-blue-400",
  "text-violet-600 dark:text-violet-400",
  "text-emerald-600 dark:text-emerald-400",
  "text-amber-600 dark:text-amber-400",
  "text-rose-600 dark:text-rose-400",
  "text-cyan-600 dark:text-cyan-400",
];
const CAL_BG = ["bg-blue-500", "bg-violet-500", "bg-emerald-500", "bg-amber-500", "bg-rose-500", "bg-cyan-500"];

const calPhaseLabel = $derived.by(() => {
  if (calPhase.kind === "action" && calProfile) return calProfile.actions[calPhase.actionIndex]?.label ?? "";
  if (calPhase.kind === "break") return t("calibration.break");
  if (calPhase.kind === "done") return t("calibration.complete");
  return t("calibration.ready");
});
const calPhaseColor = $derived.by(() => {
  if (calPhase.kind === "action") return CAL_COLORS[calPhase.actionIndex % CAL_COLORS.length];
  if (calPhase.kind === "break") return "text-amber-600 dark:text-amber-400";
  if (calPhase.kind === "done") return "text-emerald-600 dark:text-emerald-400";
  return "text-muted-foreground";
});
const calPhaseBg = $derived.by(() => {
  if (calPhase.kind === "action") return CAL_BG[calPhase.actionIndex % CAL_BG.length];
  if (calPhase.kind === "break") return "bg-amber-500";
  return "bg-emerald-500";
});

// ── TTS helper ────────────────────────────────────────────────────────────
let _lastTtsText = "";
let _lastTtsTime = 0;
function ttsSpeak(text: string): void {
  const now = Date.now();
  if (text === _lastTtsText && now - _lastTtsTime < 5000) return;
  _lastTtsText = text;
  _lastTtsTime = now;
  invoke("tts_speak", { text }).catch((_e) => {});
}

// ── Model download helpers ────────────────────────────────────────────────

/** Pick the best family match by id or name regex, preferring Q4_K_M. */
function pickFamilyTarget(entries: LlmModelEntry[], familyId: string, familyRe: RegExp): LlmModelEntry | null {
  const family = entries.filter((e) => !e.is_mmproj && (e.family_id === familyId || familyRe.test(e.family_name)));
  if (!family.length) return null;
  const byQuant = (q: string) => family.find((e) => e.quant.toUpperCase() === q);
  return (
    byQuant("Q4_K_M") ??
    byQuant("Q8_0") ??
    byQuant("Q4_0") ??
    family.find((e) => e.quant.toUpperCase().startsWith("Q4")) ??
    family.find((e) => e.recommended) ??
    family.find((e) => e.state === "downloaded") ??
    family[0]
  );
}

/**
 * Pick the default LLM to download during onboarding.
 *
 * Priority chain:
 *  1. Already-downloaded model (any family) — skip download.
 *  2. LFM2.5 1.2B Instruct — default bootstrap family.
 *  3. Any recommended model, smallest first.
 */
function pickLlmTarget(entries: LlmModelEntry[]): LlmModelEntry | null {
  // If any model is already downloaded, prefer it (skip download).
  const downloaded = entries.find((e) => !e.is_mmproj && e.state === "downloaded");
  if (downloaded) return downloaded;

  return (
    pickFamilyTarget(entries, "lfm25-1.2b-instruct", /lfm2\.5\s*1\.2b.*instruct/i) ??
    entries.filter((e) => !e.is_mmproj && e.recommended).sort((a, b) => a.size_gb - b.size_gb)[0] ??
    null
  );
}

async function refreshModelDownloads() {
  try {
    const [catalog, eeg, ocrReady] = await Promise.all([
      daemonInvoke<LlmCatalogLite>("get_llm_catalog"),
      daemonInvoke<EegModelStatusLite>("get_eeg_model_status"),
      daemonInvoke<boolean>("check_ocr_models_ready"),
    ]);
    llmTarget = pickLlmTarget(catalog.entries);
    zunaStatus = eeg;
    if (ocrReady && ocrDlState !== "ready") ocrDlState = "ready";
    modelLoadError = "";
  } catch (e) {
    modelLoadError = String(e);
  }
}

async function downloadLlm() {
  if (!llmTarget || llmTarget.state === "downloading" || llmTarget.state === "downloaded") return;
  await daemonInvoke("download_llm_model", { filename: llmTarget.filename });
  await refreshModelDownloads();
}

async function downloadZuna() {
  if (zunaStatus?.downloading_weights || zunaStatus?.weights_found) return;
  await daemonInvoke("trigger_weights_download");
  await refreshModelDownloads();
}

async function downloadTtsBackend(target: "neutts" | "kitten") {
  if (ttsActionBusy) return;
  ttsActionBusy = true;
  if (target === "neutts") {
    neuttsDlState = "downloading";
    neuttsDlError = "";
  } else {
    kittenDlState = "downloading";
    kittenDlError = "";
  }

  let previous: NeuttsConfig | null = null;
  try {
    previous = await daemonInvoke<NeuttsConfig>("get_neutts_config");
    const nextCfg: NeuttsConfig =
      target === "neutts"
        ? {
            ...previous,
            enabled: true,
            backbone_repo: "neuphonic/neutts-nano-q4-gguf",
            gguf_file: "",
            voice_preset: previous.voice_preset || "jo",
          }
        : { ...previous, enabled: false };

    await daemonInvoke("set_neutts_config", { config: nextCfg });
    await invoke("tts_init");

    if (target === "neutts") neuttsDlState = "ready";
    else kittenDlState = "ready";
  } catch (e) {
    if (target === "neutts") {
      neuttsDlState = "error";
      neuttsDlError = String(e);
    } else {
      kittenDlState = "error";
      kittenDlError = String(e);
    }
  } finally {
    if (previous) {
      daemonInvoke("set_neutts_config", { config: previous }).catch((_e) => {});
    }
    ttsActionBusy = false;
  }
}

async function downloadOcrModels() {
  if (ocrDlState === "ready" || ocrDlState === "downloading") return;
  ocrDlState = "downloading";
  ocrDlError = "";
  try {
    const ok = await daemonInvoke<boolean>("download_ocr_models");
    ocrDlState = ok ? "ready" : "error";
    if (!ok) ocrDlError = "OCR model download failed";
  } catch (e) {
    ocrDlState = "error";
    ocrDlError = String(e);
  }
}

async function downloadRecommendedBundle() {
  if (bundleBusy) return;
  bundleBusy = true;
  modelLoadError = "";
  try {
    await refreshModelDownloads();
    for (const stage of onboardingDownloadOrder) {
      if (stage === "zuna") {
        if (!zunaIsDownloaded && !zunaIsDownloading) await downloadZuna();
      } else if (stage === "kitten") {
        if (kittenDlState !== "ready") await downloadTtsBackend("kitten");
      } else if (stage === "neutts") {
        if (neuttsDlState !== "ready") await downloadTtsBackend("neutts");
      } else if (stage === "ocr") {
        if (ocrDlState !== "ready") await downloadOcrModels();
      } else if (!llmIsDownloaded && !llmIsDownloading) {
        await downloadLlm();
      }
    }
    await refreshModelDownloads();
  } catch (e) {
    modelLoadError = String(e);
  } finally {
    bundleBusy = false;
  }
}

function isStageReady(stage: OnboardingModelKey): boolean {
  if (stage === "zuna") return zunaIsDownloaded;
  if (stage === "kitten") return kittenDlState === "ready";
  if (stage === "neutts") return neuttsDlState === "ready";
  if (stage === "ocr") return ocrDlState === "ready";
  return llmIsDownloaded;
}

function isStageDownloading(stage: OnboardingModelKey): boolean {
  if (stage === "zuna") return zunaIsDownloading;
  if (stage === "kitten") return kittenDlState === "downloading" || (ttsActionBusy && autoModelStage === "kitten");
  if (stage === "neutts") return neuttsDlState === "downloading" || (ttsActionBusy && autoModelStage === "neutts");
  if (stage === "ocr") return ocrDlState === "downloading";
  return llmIsDownloading;
}

function advanceAutoModelStage() {
  const nextStage = onboardingDownloadOrder.find((stage) => !isStageReady(stage));
  if (nextStage) {
    autoModelStage = nextStage;
  } else {
    autoModelStage = "done";
  }
}

async function driveAutoModelQueue() {
  if (!autoModelsStarted || autoModelInFlight || autoModelStage === "done") return;

  advanceAutoModelStage();

  // Wait while current stage is already actively downloading.
  if (isStageDownloading(autoModelStage)) return;

  autoModelInFlight = true;
  try {
    if (autoModelStage === "zuna" && !zunaIsDownloaded) {
      await downloadZuna();
    } else if (autoModelStage === "kitten" && kittenDlState !== "ready") {
      await downloadTtsBackend("kitten");
    } else if (autoModelStage === "neutts" && neuttsDlState !== "ready") {
      await downloadTtsBackend("neutts");
    } else if (autoModelStage === "ocr" && ocrDlState !== "ready") {
      await downloadOcrModels();
    } else if (autoModelStage === "llm" && !llmIsDownloaded && !llmIsDownloading) {
      await downloadLlm();
    }
  } catch (e) {
    modelLoadError = String(e);
  } finally {
    autoModelInFlight = false;
    advanceAutoModelStage();
  }
}

// ── Calibration helpers ────────────────────────────────────────────────────
async function startCalibration() {
  if (!calProfile || !isConnected) return;
  calRunning = true;
  try {
    const result = await daemonInvoke<{ ok: boolean; error?: string }>("calibration_start_session", {
      profile_id: calProfile.id,
    });
    if (!result?.ok) {
      calRunning = false;
      ttsSpeak(result?.error ?? "Failed to start calibration.");
    }
  } catch (e) {
    calRunning = false;
    ttsSpeak("Error: Could not start calibration session.");
  }
}

async function cancelCalibration() {
  if (!calRunning) return;
  calRunning = false;
  calPhase = { kind: "idle", actionIndex: 0, loop: 1 };
  try {
    await daemonInvoke("calibration_cancel_session");
  } catch {
    // ignore
  }
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
const unsubs: UnlistenFn[] = [];
onMount(async () => {
  window.addEventListener("keydown", onArrowKey);

  // Load onboarding status (drives "what's new" banner + per-step NEW badges).
  // Best-effort: if the call fails the wizard still works, just without badges.
  try {
    onboardingStatus = await invoke<OnboardingStatus>("get_onboarding_status");
  } catch {}

  status = await daemonInvoke<DeviceStatus>("get_status");
  unsubs.push(
    await listen<DeviceStatus>("status", (ev) => {
      status = ev.payload;
    }),
  );

  // Load default calibration profile for inline calibration
  try {
    const order = await invoke<string[]>("get_onboarding_model_download_order");
    const valid = order.filter(
      (stage): stage is OnboardingModelKey =>
        stage === "zuna" || stage === "kitten" || stage === "neutts" || stage === "llm" || stage === "ocr",
    );
    if (valid.length) onboardingDownloadOrder = valid;
  } catch (e) {}

  try {
    calProfile = await daemonInvoke<CalibrationProfile | null>("get_active_calibration");
    if (!calProfile) {
      const profiles = await daemonInvoke<CalibrationProfile[]>("list_calibration_profiles");
      calProfile = profiles[0] ?? null;
    }
  } catch (e) {}

  // Pre-warm TTS engine
  unlistenTts = await listen<{ phase: string; label: string }>("tts-progress", (ev) => {
    if (ev.payload.phase === "ready") {
      ttsReady = true;
      ttsDlLabel = "";
    } else {
      ttsReady = false;
      ttsDlLabel = ev.payload.label ?? "";
    }
  });
  invoke("tts_init").catch((_e) => {});

  // ── Daemon WS event subscriptions for calibration ─────────────────────
  const calUnsubs: (() => void)[] = [];

  calUnsubs.push(
    onDaemonEvent("calibration-phase", (ev) => {
      const p = ev.payload as Record<string, number | string | boolean>;
      if (!p) return;
      calCountdown = (p.countdown as number) ?? 0;
      calTotal = (p.total_secs as number) ?? 0;
      calRunning = (p.running as boolean) ?? false;
      if (p.kind === "action" || p.kind === "break" || p.kind === "done") {
        calPhase = {
          kind: p.kind as Phase["kind"],
          actionIndex: (p.action_index as number) ?? 0,
          loop: (p.loop_number as number) ?? 1,
        };
      }
    }),
  );

  calUnsubs.push(
    onDaemonEvent("calibration-tts", (ev) => {
      const text = ev.payload?.text as string | undefined;
      if (text) ttsSpeak(text);
    }),
  );

  calUnsubs.push(
    onDaemonEvent("calibration-started", () => {
      calRunning = true;
    }),
  );

  calUnsubs.push(
    onDaemonEvent("calibration-completed", async () => {
      calRunning = false;
      calPhase = { kind: "done", actionIndex: 0, loop: calProfile?.loop_count ?? 1 };
      // Reload profile to get updated last_calibration_utc
      try {
        calProfile = await daemonInvoke<CalibrationProfile | null>("get_active_calibration");
      } catch {}
    }),
  );

  calUnsubs.push(
    onDaemonEvent("calibration-cancelled", () => {
      calRunning = false;
      calPhase = { kind: "idle", actionIndex: 0, loop: 1 };
    }),
  );

  calUnsubs.push(
    onDaemonEvent("calibration-error", () => {
      calRunning = false;
      calPhase = { kind: "idle", actionIndex: 0, loop: 1 };
    }),
  );

  // Store unsubs for cleanup
  unsubs.push(...calUnsubs.map((fn) => fn as unknown as UnlistenFn));

  await refreshModelDownloads();
  if (isMac) {
    try {
      screenRecPerm = await invoke<boolean>("check_screen_recording_permission");
    } catch (e) {}
  }

  // Check OS-level bluetooth adapter state (macOS only)
  try {
    btEnabled = await invoke<boolean>("check_bluetooth_power");
  } catch (e) {
    btEnabled = true;
  }

  autoModelsStarted = true;
  void driveAutoModelQueue();
  modelsTimer = setInterval(() => {
    refreshModelDownloads();
    if (isMac)
      invoke<boolean>("check_screen_recording_permission")
        .then((v) => {
          screenRecPerm = v;
        })
        .catch((_e) => {});
    invoke<boolean>("check_bluetooth_power")
      .then((v) => {
        btEnabled = v;
      })
      .catch((_e) => {});
  }, 2000);
});

// Re-check bluetooth whenever the user navigates to that step
$effect(() => {
  if (step === "enable_bluetooth") {
    invoke<boolean>("check_bluetooth_power")
      .then((v) => {
        btEnabled = v;
      })
      .catch(() => {
        btEnabled = true;
      });
  }
});

async function checkBt() {
  try {
    btEnabled = await invoke<boolean>("check_bluetooth_power");
  } catch (e) {
    btEnabled = true;
  }
}

async function openBt() {
  await invoke("open_bt_settings");
}

onDestroy(async () => {
  window.removeEventListener("keydown", onArrowKey);
  // biome-ignore lint/suspicious/useIterableCallbackReturn: unlisten fns return void-Promise, not a value
  unsubs.forEach((u) => u());
  unlistenTts?.();
  if (calRunning) {
    calRunning = false;
    daemonInvoke("calibration_cancel_session").catch(() => {});
  }
  if (modelsTimer) clearInterval(modelsTimer);
});

$effect(() => {
  zunaStatus;
  llmTarget;
  kittenDlState;
  neuttsDlState;
  ocrDlState;
  ttsActionBusy;
  autoModelsStarted;
  autoModelStage;

  if (!autoModelsStarted) return;
  void driveAutoModelQueue();
});

// ── Navigation ─────────────────────────────────────────────────────────────
function next() {
  const i = stepIdx;
  if (i < STEPS.length - 1) step = STEPS[i + 1];
}
function prev() {
  const i = stepIdx;
  if (i > 0) step = STEPS[i - 1];
}

function onArrowKey(e: KeyboardEvent) {
  if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  const t = e.target as HTMLElement | null;
  const tag = t?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
  if (t?.isContentEditable) return;
  e.preventDefault();
  if (e.key === "ArrowRight") next();
  else prev();
}
async function startScan() {
  await daemonInvoke("retry_connect");
}
async function finish() {
  await invoke("complete_onboarding");
}

useWindowTitle("window.title.onboarding");
</script>

<main class="h-full min-h-0 flex flex-col overflow-hidden select-none bg-background text-foreground"
      aria-label={t("onboarding.title")}>

  <!-- ── Top bar ───────────────────────────────────────────────────────────── -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="flex items-center gap-2 px-4 pt-3 pb-1.5 shrink-0" data-tauri-drag-region
       ondblclick={toggleMaximizeWindow}>
    <span class="text-ui-lg font-bold tracking-tight flex-1">{t("onboarding.title")}</span>

    <!-- ── Accessibility: text-size A−/A+ (always visible) ───────────────── -->
    <!-- The wrapper opts out of the parent drag-region so buttons are clickable. -->
    <div data-tauri-drag-region="false"
         class="flex items-center gap-0.5 rounded-md border border-border/60 bg-surface-2/60
                dark:bg-white/[0.04] px-0.5 py-0.5 select-none shrink-0"
         role="group" aria-label={t("onboarding.fontSizeLabel")}>
      <button
        type="button"
        onclick={() => bumpFontSize(-1)}
        disabled={fontSizePct <= minFontPct}
        title={t("onboarding.fontSizeDecrease")}
        aria-label={t("onboarding.fontSizeDecrease")}
        class="w-6 h-6 flex items-center justify-center rounded text-ui-base font-semibold
               text-muted-foreground hover:text-foreground hover:bg-accent/50 transition-colors
               disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer">
        A−
      </button>
      <span class="w-10 text-center text-ui-2xs font-mono tabular-nums text-muted-foreground"
            aria-live="polite">{fontSizePct}%</span>
      <button
        type="button"
        onclick={() => bumpFontSize(1)}
        disabled={fontSizePct >= maxFontPct}
        title={t("onboarding.fontSizeIncrease")}
        aria-label={t("onboarding.fontSizeIncrease")}
        class="w-6 h-6 flex items-center justify-center rounded text-ui-lg font-semibold
               text-muted-foreground hover:text-foreground hover:bg-accent/50 transition-colors
               disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer">
        A+
      </button>
    </div>

    <!-- TTS readiness indicator (shown on calibration step) -->
    {#if step === "calibration" && !ttsReady}
      <span class="flex items-center gap-1 text-ui-xs text-amber-600 dark:text-amber-400
                   font-medium animate-pulse" title={ttsDlLabel || "Preparing voice engine…"}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
             class="w-3 h-3 shrink-0">
          <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
          <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
        </svg>
        {ttsDlLabel || "Voice loading…"}
      </span>
    {/if}
  </div>

  <!-- ── Progress ──────────────────────────────────────────────────────────── -->
  <div class="px-4 pb-2 shrink-0">
    <Progress value={((stepIdx) / (STEPS.length - 1)) * 100} class="h-1"
              aria-label="Setup progress" />
    <div class="flex justify-between mt-1">
      {#each STEPS as s, i}
        <button
          onclick={() => { if (i <= stepIdx && !calRunning) step = s; }}
          class="relative text-ui-2xs font-medium transition-colors
                 {i <= stepIdx ? 'text-foreground cursor-pointer' : 'text-muted-foreground/40 cursor-default'}">
          {t(`onboarding.step.${s}`)}
          {#if isNewStep(s)}
            <span class="absolute -top-1.5 -right-1.5 text-[0.55rem] font-bold tracking-widest
                         px-1 py-px rounded-sm bg-violet-500 text-white shadow-sm uppercase
                         leading-none"
                  aria-label={t("onboarding.newBadge")}>
              {t("onboarding.newBadge")}
            </span>
          {/if}
        </button>
      {/each}
    </div>
  </div>

  <!-- ── Step content ──────────────────────────────────────────────────────── -->
  <div class="flex-1 min-h-0 overflow-y-auto px-4 pb-3">

    <!-- ════ WELCOME ══════════════════════════════════════════════════════════ -->
    {#if step === "welcome"}
      <div class="flex flex-col items-center gap-3 pt-4 text-center" in:fly={{ x: 30, duration: 200 }}>
        <span class="text-4xl">🧠</span>
        <h2 class="text-[1.05rem] font-bold">
          {onboardingStatus.isReturning ? t("onboarding.welcomeBackTitle") : t("onboarding.welcomeTitle")}
        </h2>
        <p class="text-ui-md text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.welcomeBody")}
        </p>

        {#if onboardingStatus.isReturning}
          <!-- "Why you're seeing this again" banner — only for users who already onboarded
               at an earlier wizard version. Lists which steps are new since their last run. -->
          <div class="w-full max-w-[340px] rounded-lg border border-violet-500/25 bg-violet-500/[0.06]
                      px-3 py-2.5 flex flex-col gap-1.5 text-left">
            <div class="flex items-center gap-2">
              <span class="text-base">✨</span>
              <span class="text-ui-base font-semibold text-violet-700 dark:text-violet-300">
                {t("onboarding.whatsNewTitle")}
              </span>
            </div>
            <p class="text-ui-sm text-violet-700/90 dark:text-violet-300/90 leading-relaxed">
              {t("onboarding.whatsNewBody")}
            </p>
            <ul class="text-ui-sm text-violet-700/90 dark:text-violet-300/90 leading-relaxed list-disc pl-5 mt-0.5">
              {#each STEP_META.filter((m) => m.addedIn > onboardingStatus.completedVersion && m.id !== "done" && m.id !== "welcome") as m}
                <li>{t(`onboarding.step.${m.id}`)}</li>
              {/each}
            </ul>
          </div>
        {/if}
        <div class="flex flex-col gap-1.5 w-full max-w-[320px] mt-1">
          {#each [
            { id: "bluetooth",   icon: "📡" },
            { id: "fit",         icon: "🎧" },
            { id: "calibration", icon: "🎯" },
            { id: "models",      icon: "⬇️" },
            { id: "tray",        icon: "🖥" },
            { id: "permissions", icon: "🔒" },
            { id: "extensions",  icon: "🧩" },
          ] as s}
            <div class="relative flex items-center gap-2.5 rounded-lg border border-border dark:border-white/[0.06]
                        bg-muted dark:bg-surface-2 px-3 py-2">
              <span class="text-base">{s.icon}</span>
              <div class="flex flex-col text-left flex-1 min-w-0">
                <span class="text-ui-base font-semibold">{t(`onboarding.step.${s.id}`)}</span>
                <span class="text-ui-xs text-muted-foreground">{t(`onboarding.${s.id}Hint`)}</span>
              </div>
              {#if isNewStep(s.id as Step)}
                <span class="text-[0.55rem] font-bold tracking-widest px-1 py-px rounded-sm
                             bg-violet-500 text-white shadow-sm uppercase leading-none shrink-0">
                  {t("onboarding.newBadge")}
                </span>
              {/if}
            </div>
          {/each}
        </div>
      </div>

    <!-- ════ ENABLE BLUETOOTH (OS) ═════════════════════════════════════════════════ -->
    {:else if step === "enable_bluetooth"}
      <div class="flex flex-col items-center gap-3 pt-3 text-center" in:fly={{ x: 30, duration: 200 }}>
        <span class="text-3xl">{btEnabled ? '✅' : '🔌'}</span>
        <h2 class="text-ui-xl font-bold">{t("onboarding.enableBluetoothTitle")}</h2>
        <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.enableBluetoothBody")}
        </p>

        <Card class="w-full max-w-[320px] border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-2.5">
            <div class="flex items-center gap-2.5">
              <div class="flex flex-col gap-0 flex-1 min-w-0">
                <span class="text-ui-base font-semibold">{t('onboarding.enableBluetoothStatus')}</span>
              </div>
              <span class="ml-auto inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-ui-xs font-semibold
                           {btEnabled ? 'bg-green-500/15 text-green-700 dark:text-green-400 border-green-500/30' : 'bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30'}">
                <span class="w-1.5 h-1.5 rounded-full {btEnabled ? 'bg-green-500' : 'bg-amber-400'}"></span>
                {btEnabled ? t('perm.granted') : t('perm.denied')}
              </span>
            </div>
            <p class="text-ui-sm text-muted-foreground/80 leading-relaxed mt-2">{t('onboarding.enableBluetoothHint')}</p>
            <div class="flex justify-end mt-2 gap-2">
              <Button size="sm" variant="outline" class="h-7 text-ui-sm px-3" onclick={openBt}>
                {t('onboarding.enableBluetoothOpen')}
              </Button>
              <Button size="sm" class="h-7 text-ui-sm px-3" onclick={checkBt}>
                {t('onboarding.btScan')}
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>

    <!-- ════ BLUETOOTH ════════════════════════════════════════════════════════ -->
    {:else if step === "bluetooth"}
      <div class="flex flex-col items-center gap-3 pt-3 text-center" in:fly={{ x: 30, duration: 200 }}>
        <span class="text-3xl">{isConnected ? "✅" : "📡"}</span>
        <h2 class="text-ui-xl font-bold">{t("onboarding.bluetoothTitle")}</h2>
        <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.bluetoothBody")}
        </p>

        <Card class="w-full max-w-[320px] border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-2.5">
            <div class="flex items-center gap-2.5">
              <div class="w-2.5 h-2.5 rounded-full shrink-0
                          {isConnected ? 'bg-green-500' : isScanning ? 'bg-yellow-500 animate-pulse' : 'bg-slate-400'}"></div>
              <div class="flex flex-col gap-0 flex-1 min-w-0">
                <span class="text-ui-base font-semibold">
                  {isConnected
                    ? t("onboarding.btConnected", { name: status.device_name ?? "Muse" })
                    : isScanning ? t("onboarding.btScanning") : t("onboarding.btReady")}
                </span>
                {#if isConnected && status.battery > 0}
                  <span class="text-ui-xs text-muted-foreground">{t("dashboard.battery")}: {status.battery.toFixed(0)}%</span>
                {/if}
              </div>
              {#if !isConnected}
                <Button size="sm" class="text-ui-sm h-6 px-2.5 shrink-0" onclick={startScan} disabled={isScanning}>
                  {isScanning ? t("onboarding.btScanning") : t("onboarding.btScan")}
                </Button>
              {/if}
            </div>
          </CardContent>
        </Card>

        <div class="w-full max-w-[320px] flex flex-col gap-1.5 text-left">
          <p class="text-ui-2xs font-semibold tracking-widest uppercase text-muted-foreground">
            {t("onboarding.btInstructions")}
          </p>
          {#each [1,2,3] as n}
            <div class="flex items-start gap-2">
              <span class="w-4 h-4 rounded-full bg-muted dark:bg-white/[0.06] flex items-center justify-center
                           text-ui-2xs font-bold text-muted-foreground shrink-0 mt-0.5">{n}</span>
              <p class="text-ui-sm text-muted-foreground leading-relaxed">{t(`onboarding.btStep${n}`)}</p>
            </div>
          {/each}
        </div>

        {#if isConnected}
          <div class="flex items-center gap-1.5 text-green-600 dark:text-green-400" in:fade={{ duration: 200 }}>
            <span>✓</span>
            <span class="text-ui-base font-semibold">{t("onboarding.btSuccess")}</span>
          </div>
        {/if}
      </div>

    <!-- ════ FIT CHECK ════════════════════════════════════════════════════════ -->
    {:else if step === "fit"}
      <div class="flex flex-col items-center gap-2 pt-2 text-center" in:fly={{ x: 30, duration: 200 }}>
        <h2 class="text-ui-xl font-bold">{t("onboarding.fitTitle")}</h2>
        <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.fitBody")}
        </p>

        <ElectrodeGuide qualityLabels={status.channel_quality} device={status.device_kind} deviceName={status.device_name ?? ""} />

        {#if !isConnected}
          <p class="text-ui-sm text-amber-600 dark:text-amber-400">⚠ {t("onboarding.fitNeedsBt")}</p>
        {/if}

        {#if allGoodOrFair && isConnected}
          <div class="flex items-center gap-1.5 text-green-600 dark:text-green-400" in:fade={{ duration: 200 }}>
            <span>✓</span>
            <span class="text-ui-base font-semibold">{t("onboarding.fitGood")}</span>
          </div>
        {/if}
      </div>

    <!-- ════ CALIBRATION ══════════════════════════════════════════════════════ -->
    {:else if step === "calibration"}
      <div class="flex flex-col items-center gap-4 pt-3 text-center" in:fly={{ x: 30, duration: 200 }}>

        {#if calPhase.kind === "idle"}
          <!-- ── Idle / start screen ─────────────────────────────────────── -->
          <span class="text-3xl">🎯</span>
          <h2 class="text-ui-xl font-bold">{t("onboarding.calibrationTitle")}</h2>
          <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[320px]">
            {t("onboarding.calibrationBody")}
          </p>

          {#if calProfile}
            <!-- Action chips -->
            <div class="flex flex-wrap gap-1.5 justify-center max-w-[380px]">
              {#each calProfile.actions as action, i}
                {@const colors = [
                  "border-violet-500/30 bg-violet-500/10 text-violet-600 dark:text-violet-400",
                  "border-violet-500/30 bg-violet-500/10 text-violet-600 dark:text-violet-400",
                  "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
                  "border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400",
                  "border-rose-500/30 bg-rose-500/10 text-rose-600 dark:text-rose-400",
                  "border-cyan-500/30 bg-cyan-500/10 text-cyan-600 dark:text-cyan-400",
                ]}
                <span class="rounded-full border px-2.5 py-0.5 text-ui-sm font-medium {colors[i % colors.length]}">
                  {action.label} · {action.duration_secs}s
                </span>
              {/each}
              <span class="rounded-full border border-amber-500/30 bg-amber-500/10
                           text-amber-600 dark:text-amber-400 px-2.5 py-0.5 text-ui-sm font-medium">
                {t("calibration.break")} · {calProfile.break_duration_secs}s
              </span>
            </div>

            <Button class="px-6 h-9 mt-1" onclick={startCalibration} disabled={!isConnected}>
              {t("calibration.startCalibration")}
            </Button>
          {:else}
            <Button class="px-6 h-9" onclick={startCalibration} disabled={!isConnected}>
              {t("calibration.startCalibration")}
            </Button>
          {/if}

          {#if !isConnected}
            <p class="text-ui-sm text-amber-600 dark:text-amber-400">⚠ {t("onboarding.calibrationNeedsBt")}</p>
          {/if}

          <p class="text-ui-sm text-muted-foreground/50 max-w-[280px] leading-relaxed">
            {t("onboarding.calibrationSkip")}
          </p>

        {:else if calPhase.kind === "done"}
          <!-- ── Done screen ──────────────────────────────────────────────── -->
          <div class="flex flex-col items-center gap-3">
            <div class="w-14 h-14 rounded-full bg-emerald-500/10 flex items-center justify-center text-2xl">✅</div>
            <h2 class="text-ui-xl font-bold text-emerald-600 dark:text-emerald-400">{t("calibration.complete")}</h2>
            <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[300px]">
              {t("calibration.completeDesc", { n: String(calProfile?.loop_count ?? 1) })}
            </p>
            <div class="flex gap-2.5 mt-1">
              <Button variant="outline" size="sm"
                      onclick={() => { calPhase = { kind: "idle", actionIndex: 0, loop: 1 }; }}>
                {t("calibration.runAgain")}
              </Button>
              <Button size="sm" onclick={next}>
                {t("onboarding.next")} →
              </Button>
            </div>
          </div>

        {:else}
          <!-- ── Active calibration phase ────────────────────────────────── -->
          <div class="flex flex-col items-center gap-4 w-full max-w-[380px]">

            <!-- Profile name + loop dots -->
            {#if calProfile}
              <div class="flex flex-col items-center gap-1.5">
                <span class="text-ui-sm font-semibold uppercase tracking-widest text-muted-foreground/60">
                  {calProfile.name}
                </span>
                <div class="flex items-center gap-2">
                  <span class="text-ui-sm font-semibold tracking-widest uppercase text-muted-foreground">
                    {t("calibration.iteration")}
                  </span>
                  <div class="flex gap-1">
                    {#each Array(calProfile.loop_count) as _, i}
                      <div class="w-2.5 h-2.5 rounded-full transition-colors
                                  {i < calPhase.loop - 1 ? 'bg-emerald-500' :
                                   i === calPhase.loop - 1 ? calPhaseBg :
                                   'bg-muted dark:bg-white/[0.08]'}"></div>
                    {/each}
                  </div>
                  <span class="text-ui-sm text-muted-foreground tabular-nums">
                    {calPhase.loop}/{calProfile.loop_count}
                  </span>
                </div>
              </div>

              <!-- Action progress dots -->
              {#if calPhase.kind === "action" && calProfile.actions.length > 1}
                <div class="flex items-center gap-2">
                  {#each calProfile.actions as _, i}
                    <div class="flex items-center gap-1">
                      <div class="w-2 h-2 rounded-full transition-colors
                                  {i < calPhase.actionIndex ? 'bg-emerald-500' :
                                   i === calPhase.actionIndex ? 'bg-blue-500' :
                                   'bg-muted dark:bg-white/[0.08]'}"></div>
                      {#if i < calProfile.actions.length - 1}
                        <div class="w-3 h-px bg-muted dark:bg-white/[0.08]"></div>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            {/if}

            <!-- Phase label -->
            <div class="flex flex-col items-center gap-1">
              <span class="text-[1.8rem] font-bold tracking-tight {calPhaseColor}">{calPhaseLabel}</span>
              {#if calPhase.kind === "break" && calProfile}
                {@const nextIdx = (calPhase.actionIndex + 1) % calProfile.actions.length}
                <span class="text-ui-base text-muted-foreground">
                  {t("calibration.nextAction", { action: calProfile.actions[nextIdx]?.label ?? "" })}
                </span>
              {/if}
            </div>

            <!-- Countdown -->
            <div class="flex flex-col items-center gap-2 w-full">
              <span class="text-[2.8rem] font-bold tabular-nums leading-none">{calCountdown}</span>
              <span class="text-ui-sm text-muted-foreground/50">{t("calibration.secondsRemaining")}</span>
              <div class="w-full"><Progress value={calProgressPct} class="h-2" /></div>
            </div>

            <Button variant="outline" size="sm" onclick={cancelCalibration}>
              {t("common.cancel")}
            </Button>
          </div>
        {/if}
      </div>

    <!-- ════ MODELS ══════════════════════════════════════════════════════════ -->
    {:else if step === "models"}
      <div class="flex flex-col items-center gap-3 pt-3 text-center" in:fly={{ x: 30, duration: 200 }}>
        <span class="text-3xl">⬇️</span>
        <h2 class="text-ui-xl font-bold">{t("onboarding.modelsTitle")}</h2>
        <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[340px]">
          {t("onboarding.modelsBody")}
        </p>

        <Card class="w-full max-w-[360px] border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-3 flex flex-col gap-3">
            <div class="flex justify-end">
              <Button size="sm" class="h-7 text-ui-sm px-3"
                      onclick={downloadRecommendedBundle}
                      disabled={bundleBusy || allRecommendedReady || ttsActionBusy}>
                {allRecommendedReady
                  ? t("onboarding.models.downloaded")
                  : bundleBusy
                    ? t("onboarding.models.downloading")
                    : t("onboarding.models.downloadAll")}
              </Button>
            </div>

            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-surface-2 px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">🤖</span>
                <span class="text-ui-base font-semibold">{t("onboarding.models.qwenTitle")}</span>
                <span class="text-ui-xs text-emerald-600 dark:text-emerald-400 ml-auto">
                  {llmTarget ? llmTarget.quant : "Q4"}
                </span>
              </div>
              <p class="text-ui-sm text-muted-foreground/80 leading-relaxed">{t("onboarding.models.qwenDesc")}</p>
              {#if llmTarget?.repo}
                <p class="text-ui-2xs text-muted-foreground/70 font-mono">🤗 hf download {llmTarget.repo} {llmTarget.filename}</p>
              {/if}
              {#if llmIsDownloading}
                <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                  <div class="h-full rounded-full bg-blue-500 transition-[width] duration-300" style="width:{llmProgressPct.toFixed(1)}%"></div>
                </div>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-ui-sm px-3" onclick={downloadLlm}
                        disabled={llmIsDownloaded || llmIsDownloading || !llmTarget}>
                  {llmIsDownloaded ? t("onboarding.models.downloaded") : llmIsDownloading ? t("onboarding.models.downloading") : t("onboarding.models.download")}
                </Button>
              </div>
            </div>

            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-surface-2 px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">🧠</span>
                <span class="text-ui-base font-semibold">{t("onboarding.models.zunaTitle")}</span>
              </div>
              <p class="text-ui-sm text-muted-foreground/80 leading-relaxed">{t("onboarding.models.zunaDesc")}</p>
              <p class="text-ui-2xs text-muted-foreground/70 font-mono">🤗 hf download Zyphra/ZUNA model-00001-of-00001.safetensors</p>
              {#if zunaIsDownloading}
                <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                  <div class="h-full rounded-full bg-blue-500 transition-[width] duration-300" style="width:{zunaProgressPct.toFixed(1)}%"></div>
                </div>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-ui-sm px-3" onclick={downloadZuna}
                        disabled={zunaIsDownloaded || zunaIsDownloading}>
                  {zunaIsDownloaded ? t("onboarding.models.downloaded") : zunaIsDownloading ? t("onboarding.models.downloading") : t("onboarding.models.download")}
                </Button>
              </div>
            </div>

            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-surface-2 px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">🗣️</span>
                <span class="text-ui-base font-semibold">{t("onboarding.models.neuttsTitle")}</span>
              </div>
              <p class="text-ui-sm text-muted-foreground/80 leading-relaxed">{t("onboarding.models.neuttsDesc")}</p>
              <p class="text-ui-2xs text-muted-foreground/70 font-mono">🤗 hf download neuphonic/neutts-nano-q4-gguf --include "*.gguf"</p>
              {#if neuttsDlState === "error" && neuttsDlError}
                <p class="text-ui-xs text-destructive leading-relaxed">{neuttsDlError}</p>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-ui-sm px-3" onclick={() => downloadTtsBackend("neutts")}
                        disabled={ttsActionBusy || neuttsDlState === "ready"}>
                  {neuttsDlState === "ready" ? t("onboarding.models.downloaded") : neuttsDlState === "downloading" ? (ttsDlLabel || t("onboarding.models.downloading")) : t("onboarding.models.download")}
                </Button>
              </div>
            </div>

            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-surface-2 px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">🐱</span>
                <span class="text-ui-base font-semibold">{t("onboarding.models.kittenTitle")}</span>
              </div>
              <p class="text-ui-sm text-muted-foreground/80 leading-relaxed">{t("onboarding.models.kittenDesc")}</p>
              <p class="text-ui-2xs text-muted-foreground/70 font-mono">🤗 hf download KittenML/kitten-tts-mini-0.8</p>
              {#if kittenDlState === "error" && kittenDlError}
                <p class="text-ui-xs text-destructive leading-relaxed">{kittenDlError}</p>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-ui-sm px-3" onclick={() => downloadTtsBackend("kitten")}
                        disabled={ttsActionBusy || kittenDlState === "ready"}>
                  {kittenDlState === "ready" ? t("onboarding.models.downloaded") : kittenDlState === "downloading" ? (ttsDlLabel || t("onboarding.models.downloading")) : t("onboarding.models.download")}
                </Button>
              </div>
            </div>
            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-surface-2 px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">📝</span>
                <span class="text-ui-base font-semibold">{t("onboarding.models.ocrTitle")}</span>
              </div>
              <p class="text-ui-sm text-muted-foreground/80 leading-relaxed">{t("onboarding.models.ocrDesc")}</p>
              {#if ocrDlState === "error" && ocrDlError}
                <p class="text-ui-xs text-destructive leading-relaxed">{ocrDlError}</p>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-ui-sm px-3" onclick={downloadOcrModels}
                        disabled={ocrDlState === "ready" || ocrDlState === "downloading"}>
                  {ocrDlState === "ready" ? t("onboarding.models.downloaded") : ocrDlState === "downloading" ? t("onboarding.models.downloading") : t("onboarding.models.download")}
                </Button>
              </div>
            </div>

          </CardContent>
        </Card>

        <!-- ── Screen Recording permission (macOS) ──────────────────── -->
        {#if isMac}
          <Card class="w-full max-w-[360px] border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
            <CardContent class="px-3 py-3 flex flex-col gap-2">
              <div class="flex items-center gap-2">
                <span class="text-sm">🖥️</span>
                <span class="text-ui-base font-semibold">{t("onboarding.screenRecTitle")}</span>
                <span class="ml-auto inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-ui-xs font-semibold
                             {screenRecPerm ? 'bg-green-500/15 text-green-700 dark:text-green-400 border-green-500/30' : 'bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30'}">
                  <span class="w-1.5 h-1.5 rounded-full {screenRecPerm ? 'bg-green-500' : 'bg-amber-400'}"></span>
                  {screenRecPerm ? t("perm.granted") : t("perm.denied")}
                </span>
              </div>
              <p class="text-ui-sm text-muted-foreground/80 leading-relaxed">{t("onboarding.screenRecDesc")}</p>
              {#if !screenRecPerm}
                <div class="flex justify-end">
                  <Button size="sm" variant="outline" class="h-7 text-ui-sm px-3"
                          onclick={() => invoke("open_screen_recording_settings")}>
                    {t("onboarding.screenRecOpen")}
                  </Button>
                </div>
              {/if}
            </CardContent>
          </Card>
        {/if}

        {#if modelLoadError}
          <p class="text-ui-xs text-destructive/90 max-w-[340px] leading-relaxed">{modelLoadError}</p>
        {/if}
      </div>

    <!-- ════ TRAY ════════════════════════════════════════════════════════════ -->
    {:else if step === "tray"}
      <div class="flex flex-col items-center gap-4 pt-3 text-center" in:fly={{ x: 30, duration: 200 }}>
        <!-- Icon with a subtle glow ring -->
        <div class="relative flex items-center justify-center">
          <div class="absolute w-14 h-14 rounded-full bg-slate-400/10 dark:bg-white/[0.04] blur-sm"></div>
          <span class="relative text-3xl">🖥</span>
        </div>

        <h2 class="text-ui-xl font-bold">{t("onboarding.trayTitle")}</h2>
        <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.trayBody")}
        </p>

        <!-- Icon-state reference card -->
        <Card class="w-full max-w-[320px] border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-3">
            <p class="text-ui-2xs font-semibold tracking-widest uppercase text-muted-foreground mb-2.5">
              {t("onboarding.tray.states")}
            </p>
            <div class="flex flex-col gap-2">
              {#each [
                { dot: "bg-slate-400",                     label: t("onboarding.tray.grey")  },
                { dot: "bg-yellow-400 animate-pulse",      label: t("onboarding.tray.amber") },
                { dot: "bg-green-500",                     label: t("onboarding.tray.green") },
                { dot: "bg-red-500",                       label: t("onboarding.tray.red")   },
              ] as row}
                <div class="flex items-center gap-2.5">
                  <div class="w-3 h-3 rounded-full shrink-0 {row.dot}"></div>
                  <span class="text-ui-base text-left">{row.label}</span>
                </div>
              {/each}
            </div>
          </CardContent>
        </Card>

        <!-- How-to tips -->
        <div class="w-full max-w-[320px] flex flex-col gap-1.5 text-left">
          <div class="flex items-start gap-2.5 rounded-lg border border-border dark:border-white/[0.06]
                      bg-muted dark:bg-surface-2 px-3 py-2">
            <span class="text-base shrink-0">👆</span>
            <p class="text-ui-sm text-muted-foreground leading-relaxed">{t("onboarding.tray.open")}</p>
          </div>
          <div class="flex items-start gap-2.5 rounded-lg border border-border dark:border-white/[0.06]
                      bg-muted dark:bg-surface-2 px-3 py-2">
            <span class="text-base shrink-0">🖱</span>
            <p class="text-ui-sm text-muted-foreground leading-relaxed">{t("onboarding.tray.menu")}</p>
          </div>
        </div>
      </div>

    <!-- ════ PERMISSIONS (optional activity tracking) ════════════════════════ -->
    {:else if step === "permissions"}
      <div class="flex flex-col items-center gap-3 pt-3 text-center" in:fly={{ x: 30, duration: 200 }}>
        <span class="text-3xl">🔒</span>
        <h2 class="text-ui-xl font-bold">{t("onboarding.permissionsTitle")}</h2>
        <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[360px]">
          {t("onboarding.permissionsBody")}
        </p>

        <!-- Privacy callout (always visible, sets the tone) -->
        <div class="w-full max-w-[360px] rounded-lg border border-emerald-500/25 bg-emerald-500/[0.06]
                    px-3 py-2.5 flex items-start gap-2.5 text-left">
          <span class="text-base shrink-0 mt-0.5">🛡️</span>
          <p class="text-ui-sm text-emerald-700 dark:text-emerald-300/90 leading-relaxed">
            {t("onboarding.permissionsPrivacy")}
          </p>
        </div>

        <!-- Three opt-in toggles -->
        <Card class="w-full max-w-[360px] border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
          <CardContent class="px-0 py-0">
            <ToggleRow
              checked={trackActiveWindow}
              label={t("settings.activeWindowToggle")}
              description={t("onboarding.permissionsActiveWindowDesc")}
              ontoggle={toggleTrackActiveWindow}
            />
            <div class="border-t border-border dark:border-white/[0.06]"></div>
            <ToggleRow
              checked={trackInputActivity}
              label={t("settings.inputActivityToggle")}
              description={t("onboarding.permissionsInputDesc")}
              ontoggle={toggleTrackInputActivity}
            />
            <div class="border-t border-border dark:border-white/[0.06]"></div>
            <ToggleRow
              checked={trackFileActivity}
              label={t("settings.fileActivityToggle")}
              description={t("onboarding.permissionsFileDesc")}
              ontoggle={toggleTrackFileActivity}
            />
            {#if isMac}
              <div class="border-t border-border dark:border-white/[0.06]"></div>
              <ToggleRow
                checked={trackClipboard}
                label={t("settings.clipboardToggle")}
                description={t("onboarding.permissionsClipboardDesc")}
                ontoggle={toggleTrackClipboard}
              />
            {/if}
            <div class="border-t border-border dark:border-white/[0.06]"></div>
            <ToggleRow
              checked={trackScreenshots}
              label={t("settings.screenshotsToggle")}
              description={t("onboarding.permissionsScreenshotsDesc")}
              ontoggle={toggleTrackScreenshots}
            />
            <div class="border-t border-border dark:border-white/[0.06]"></div>
            <ToggleRow
              checked={trackLocation}
              label={t("settings.locationToggle")}
              description={t("onboarding.permissionsLocationDesc")}
              ontoggle={toggleTrackLocation}
            />
            <div class="border-t border-border dark:border-white/[0.06]"></div>
            <ToggleRow
              checked={trackCalendar}
              label={t("settings.calendarToggle")}
              description={t("onboarding.permissionsCalendarDesc")}
              ontoggle={toggleTrackCalendar}
            />
          </CardContent>
        </Card>

        <p class="text-ui-sm text-muted-foreground/60 max-w-[340px] leading-relaxed">
          {t("onboarding.permissionsSkip")}
        </p>
      </div>

    <!-- ════ EXTENSIONS (opt-in companion installers) ═══════════════════════ -->
    {:else if step === "extensions"}
      <div class="flex flex-col items-center gap-3 pt-3 text-center" in:fly={{ x: 30, duration: 200 }}>
        <span class="text-3xl">🧩</span>
        <h2 class="text-ui-xl font-bold">{t("onboarding.extensionsTitle")}</h2>
        <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[360px]">
          {t("onboarding.extensionsBody")}
        </p>

        <!-- Same privacy callout as the permissions step — these all feed local activity.sqlite. -->
        <div class="w-full max-w-[360px] rounded-lg border border-emerald-500/25 bg-emerald-500/[0.06]
                    px-3 py-2.5 flex items-start gap-2.5 text-left">
          <span class="text-base shrink-0 mt-0.5">🛡️</span>
          <p class="text-ui-sm text-emerald-700 dark:text-emerald-300/90 leading-relaxed">
            {t("onboarding.extensionsPrivacy")}
          </p>
        </div>

        <!-- ── VS Code card ─────────────────────────────────────────────── -->
        <Card class="w-full max-w-[360px] border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-3 flex flex-col gap-2 text-left">
            <div class="flex items-center gap-2">
              <span class="text-base">💻</span>
              <span class="text-ui-base font-semibold flex-1">{t("onboarding.extensions.vscodeTitle")}</span>
              {#if vsCodeInstall === "installed"}
                <span class="inline-flex items-center gap-1 rounded-full border border-emerald-500/30
                             bg-emerald-500/15 px-2 py-0.5 text-ui-xs font-semibold
                             text-emerald-700 dark:text-emerald-400">
                  ✓ {t("extensions.installed")}
                </span>
              {/if}
            </div>
            <p class="text-ui-sm text-muted-foreground/85 leading-relaxed">
              {t("onboarding.extensions.vscodeDesc")}
            </p>

            {#if extensionsCheck && extensionsCheck.vscode_forks?.filter((f) => f.available).length > 1}
              <!-- Multiple VS Code forks detected — let the user pick which one to install into. -->
              <div class="flex flex-wrap gap-1.5">
                {#each extensionsCheck.vscode_forks.filter((f) => f.available) as fork}
                  <button
                    onclick={() => { vsCodePickedFork = fork.id; }}
                    class="text-ui-xs px-2 py-0.5 rounded-full border transition-colors cursor-pointer
                           {vsCodePickedFork === fork.id
                             ? 'border-violet-500/60 bg-violet-500/15 text-violet-700 dark:text-violet-300'
                             : 'border-border bg-surface-2 text-muted-foreground hover:bg-accent/50'}">
                    {fork.name}{fork.installed ? " ✓" : ""}
                  </button>
                {/each}
              </div>
            {:else if extensionsCheck && extensionsCheck.vscode_forks?.filter((f) => f.available).length === 0}
              <p class="text-ui-xs text-amber-600 dark:text-amber-400 leading-relaxed">
                {t("extensions.noIdeDetected")}
              </p>
            {/if}

            {#if vsCodeMessage}
              <p class="text-ui-xs leading-relaxed
                        {vsCodeInstall === 'error' ? 'text-destructive/90' : 'text-muted-foreground/70'}">
                {vsCodeMessage}
              </p>
            {/if}

            <div class="flex justify-end">
              <Button size="sm" class="h-7 text-ui-sm px-3"
                      disabled={vsCodeInstall === "installing"
                                || (extensionsCheck?.vscode_forks?.filter((f) => f.available).length === 0)}
                      onclick={installVsCode}>
                {vsCodeInstall === "installing"
                  ? t("extensions.installing")
                  : vsCodeInstall === "installed"
                    ? t("extensions.reinstall")
                    : t("extensions.install")}
              </Button>
            </div>
          </CardContent>
        </Card>

        <!-- ── Browser card ─────────────────────────────────────────────── -->
        <Card class="w-full max-w-[360px] border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-3 flex flex-col gap-2 text-left">
            <div class="flex items-center gap-2">
              <span class="text-base">🌐</span>
              <span class="text-ui-base font-semibold flex-1">{t("onboarding.extensions.browserTitle")}</span>
              {#if browserInstall === "installed"}
                <span class="inline-flex items-center gap-1 rounded-full border border-emerald-500/30
                             bg-emerald-500/15 px-2 py-0.5 text-ui-xs font-semibold
                             text-emerald-700 dark:text-emerald-400">
                  ✓ {t("extensions.installed")}
                </span>
              {/if}
            </div>
            <p class="text-ui-sm text-muted-foreground/85 leading-relaxed">
              {t("onboarding.extensions.browserDesc")}
            </p>

            <div class="flex flex-wrap gap-1.5">
              {#each (isMac ? ["safari", "chrome", "firefox", "edge"] : ["chrome", "firefox", "edge"]) as id}
                <button
                  onclick={() => { browserPicked = id as typeof browserPicked; }}
                  class="text-ui-xs px-2 py-0.5 rounded-full border transition-colors cursor-pointer capitalize
                         {browserPicked === id
                           ? 'border-violet-500/60 bg-violet-500/15 text-violet-700 dark:text-violet-300'
                           : 'border-border bg-surface-2 text-muted-foreground hover:bg-accent/50'}">
                  {t(`extensions.${id}`)}
                </button>
              {/each}
            </div>

            {#if browserMessage}
              <p class="text-ui-xs leading-relaxed
                        {browserInstall === 'error' ? 'text-destructive/90' : 'text-muted-foreground/70'}">
                {browserMessage}
              </p>
            {/if}

            <div class="flex justify-end">
              <Button size="sm" class="h-7 text-ui-sm px-3"
                      disabled={browserInstall === "installing"}
                      onclick={installBrowser}>
                {browserInstall === "installing"
                  ? t("extensions.installing")
                  : browserInstall === "installed"
                    ? t("extensions.reinstall")
                    : t("extensions.install")}
              </Button>
            </div>
          </CardContent>
        </Card>

        <!-- ── Terminal / shell hooks card ──────────────────────────────── -->
        <Card class="w-full max-w-[360px] border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-3 flex flex-col gap-2 text-left">
            <div class="flex items-center gap-2">
              <span class="text-base">⌨️</span>
              <span class="text-ui-base font-semibold flex-1">{t("onboarding.extensions.terminalTitle")}</span>
              {#if shellInstall === "installed"}
                <span class="inline-flex items-center gap-1 rounded-full border border-emerald-500/30
                             bg-emerald-500/15 px-2 py-0.5 text-ui-xs font-semibold
                             text-emerald-700 dark:text-emerald-400">
                  ✓ {t("extensions.installed")}
                </span>
              {/if}
            </div>
            <p class="text-ui-sm text-muted-foreground/85 leading-relaxed">
              {t("onboarding.extensions.terminalDesc")}
            </p>

            <div class="flex flex-wrap gap-1.5">
              {#each ["zsh", "bash", "fish", "powershell"] as id}
                <button
                  onclick={() => { shellPicked = id as typeof shellPicked; }}
                  class="text-ui-xs px-2 py-0.5 rounded-full border transition-colors cursor-pointer
                         {shellPicked === id
                           ? 'border-violet-500/60 bg-violet-500/15 text-violet-700 dark:text-violet-300'
                           : 'border-border bg-surface-2 text-muted-foreground hover:bg-accent/50'}">
                  {id === "powershell" ? "PowerShell" : id.charAt(0).toUpperCase() + id.slice(1)}
                </button>
              {/each}
            </div>

            {#if shellMessage}
              <p class="text-ui-xs text-muted-foreground/70 leading-relaxed font-mono whitespace-pre-wrap">
                {shellMessage}
              </p>
            {/if}

            <div class="flex justify-end">
              <Button size="sm" class="h-7 text-ui-sm px-3"
                      disabled={shellInstall === "installing"}
                      onclick={installShell}>
                {shellInstall === "installing"
                  ? t("extensions.installing")
                  : shellInstall === "installed"
                    ? t("extensions.reinstall")
                    : t("extensions.install")}
              </Button>
            </div>
          </CardContent>
        </Card>

        <p class="text-ui-sm text-muted-foreground/60 max-w-[340px] leading-relaxed">
          {t("onboarding.extensionsSkip")}
        </p>
      </div>

    <!-- ════ DONE ═════════════════════════════════════════════════════════════ -->
    {:else if step === "done"}
      <div class="flex flex-col items-center gap-3 pt-4 text-center" in:fly={{ x: 30, duration: 200 }}>
        {#if allRecommendedReady}
          <!-- Downloads Complete View -->
          <div class="flex items-center justify-center w-16 h-16 rounded-full bg-green-500/10 mb-1">
            <span class="text-5xl text-green-600 dark:text-green-400">✓</span>
          </div>
          <h2 class="text-[1.05rem] font-bold text-green-600 dark:text-green-400">{t("onboarding.downloadsComplete")}</h2>
          <p class="text-ui-base text-muted-foreground leading-relaxed max-w-[340px]">
            {t("onboarding.downloadsCompleteBody")} <button onclick={openSettings} class="font-semibold text-blue-600 dark:text-blue-400 hover:underline cursor-pointer">{t("onboarding.downloadMoreSettings")}</button>.
          </p>
        {:else}
          <!-- Default Done View -->
          <span class="text-4xl">🎉</span>
          <h2 class="text-[1.05rem] font-bold">{t("onboarding.doneTitle")}</h2>
          <p class="text-ui-md text-muted-foreground leading-relaxed max-w-[320px]">
            {t("onboarding.doneBody")}
          </p>
        {/if}

        <div class="flex flex-col gap-1.5 w-full max-w-[300px] mt-1">
          {#each ["tray", "shortcuts", "help"] as tip}
            <div class="flex items-start gap-2.5 rounded-lg border border-border dark:border-white/[0.06]
                        bg-muted dark:bg-surface-2 px-3 py-2 text-left">
              <span class="text-base shrink-0">{tip === "tray" ? "🖥" : tip === "shortcuts" ? "⌨" : "❓"}</span>
              <p class="text-ui-sm text-muted-foreground leading-relaxed">{t(`onboarding.doneTip.${tip}`)}</p>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>

  <!-- ── Bottom navigation ─────────────────────────────────────────────────── -->
  <div class="flex items-center justify-between px-4 py-2.5
              border-t border-border dark:border-white/[0.06] shrink-0">
    {#if step === "welcome" || calRunning}
      <span></span>
    {:else}
      <Button variant="ghost" size="sm" class="text-ui-base h-7 px-2.5" onclick={prev}>
        ← {t("onboarding.back")}
      </Button>
    {/if}

    <div class="flex gap-1.5">
      {#each STEPS as _, i}
        <div class="w-1.5 h-1.5 rounded-full transition-colors
                    {i === stepIdx ? 'bg-foreground' : i < stepIdx ? 'bg-foreground/30' : 'bg-muted-foreground/20'}"></div>
      {/each}
    </div>

    {#if step === "done"}
      <Button size="sm" class="text-ui-base h-7 px-4" onclick={finish}>
        {t("onboarding.finish")} →
      </Button>
    {:else if calRunning}
      <!-- Back/Next locked while calibration is in progress -->
      <span></span>
    {:else if step === "calibration" && calPhase.kind === "done"}
      <!-- "Next" shown inline in the done screen — hide duplicate here -->
      <span></span>
    {:else}
      <Button size="sm" class="text-ui-base h-7 px-3" onclick={next}>
        {step === "welcome" ? t("onboarding.getStarted") : t("onboarding.next")} →
      </Button>
    {/if}
  </div>

  <div class="px-4 pb-1.5 shrink-0">
    <p class="text-ui-xs text-muted-foreground/75 text-center leading-tight truncate"
       title={footerModelStatus}>
      {footerModelStatus}
    </p>
  </div>

  <DisclaimerFooter />
</main>
