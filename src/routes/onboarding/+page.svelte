<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Onboarding / first-run wizard -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onDestroy, onMount } from "svelte";
import { fade, fly } from "svelte/transition";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Progress } from "$lib/components/ui/progress";
import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import ElectrodeGuide from "$lib/ElectrodeGuide.svelte";
import { t } from "$lib/i18n/index.svelte";
import { openSettings } from "$lib/navigation";
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
    _obIsMax = false; _obSavedBounds = null;
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
type Step = "welcome" | "enable_bluetooth" | "bluetooth" | "fit" | "calibration" | "models" | "tray" | "done";
const STEPS: Step[] = ["welcome", "enable_bluetooth", "bluetooth", "fit", "calibration", "models", "tray", "done"];

let step = $state<Step>("welcome");

// ── Bluetooth adapter check (OS-level)
let btEnabled = $state<boolean | null>(null);
let stepIdx = $derived(STEPS.indexOf(step));

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

// ── TTS helpers ────────────────────────────────────────────────────────────
async function ttsSpeakWait(text: string): Promise<void> {
  try {
    await invoke("tts_speak", { text });
  } catch (e) {}
}
function ttsSpeak(text: string): void {
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
function sleep(ms: number) {
  return new Promise<void>((r) => setTimeout(r, ms));
}

async function emitCalEvent(event: string, payload: Record<string, unknown> = {}) {
  await invoke("emit_calibration_event", { event, payload });
}

async function runCountdown(secs: number): Promise<boolean> {
  calTotal = secs;
  calCountdown = secs;
  while (calCountdown > 0) {
    await sleep(1000);
    if (!calRunning) return false;
    calCountdown--;
  }
  return true;
}

async function startCalibration() {
  if (!calProfile || !isConnected) return;
  calRunning = true;
  const p = calProfile;

  await ttsSpeakWait(`Calibration starting. ${p.actions.length} actions, ${p.loop_count} loops.`);
  if (!calRunning) return;

  await emitCalEvent("calibration-started", {
    profile_id: p.id,
    profile_name: p.name,
    actions: p.actions.map((a) => a.label),
    loop_count: p.loop_count,
  });

  for (let loop = 1; loop <= p.loop_count; loop++) {
    if (!calRunning) break;
    for (let ai = 0; ai < p.actions.length; ai++) {
      if (!calRunning) break;
      const action = p.actions[ai];

      calPhase = { kind: "action", actionIndex: ai, loop };
      await ttsSpeakWait(action.label);
      if (!calRunning) break;

      await emitCalEvent("calibration-action", {
        action: action.label,
        action_index: ai,
        loop,
        phase: `action_${ai}`,
      });
      const actionStart = Math.floor(Date.now() / 1000);
      if (!(await runCountdown(action.duration_secs))) break;
      try {
        await daemonInvoke("submit_label", { labelStartUtc: actionStart, text: action.label });
      } catch (e) {}

      const isLast = loop === p.loop_count && ai === p.actions.length - 1;
      if (!isLast && calRunning) {
        const nextAction = p.actions[(ai + 1) % p.actions.length];
        calPhase = { kind: "break", actionIndex: ai, loop };

        await ttsSpeakWait("Break.");
        if (!calRunning) break;
        await sleep(300);
        ttsSpeak(`Next: ${nextAction.label}.`);

        await emitCalEvent("calibration-break", { after_action: action.label, loop });
        if (!(await runCountdown(p.break_duration_secs))) break;
      }
    }
  }

  if (calRunning) {
    calPhase = { kind: "done", actionIndex: 0, loop: calProfile.loop_count };
    calRunning = false;
    ttsSpeak(`Calibration complete. ${p.loop_count} loops recorded.`);
    await emitCalEvent("calibration-completed", { loop_count: p.loop_count });
    await invoke("record_calibration_completed", { profileId: p.id });
  } else if (calPhase.kind !== "idle") {
    calPhase = { kind: "idle", actionIndex: 0, loop: 1 };
  }
}

async function cancelCalibration() {
  if (!calRunning) return;
  calRunning = false;
  ttsSpeak("Calibration cancelled.");
  await emitCalEvent("calibration-cancelled", { loop: calPhase.loop });
  calPhase = { kind: "idle", actionIndex: 0, loop: 1 };
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
const unsubs: UnlistenFn[] = [];
onMount(async () => {
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
    calProfile = await invoke<CalibrationProfile | null>("get_active_calibration");
    if (!calProfile) {
      const profiles = await invoke<CalibrationProfile[]>("list_calibration_profiles");
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
  // biome-ignore lint/suspicious/useIterableCallbackReturn: unlisten fns return void-Promise, not a value
  unsubs.forEach((u) => u());
  unlistenTts?.();
  if (calRunning) {
    calRunning = false;
    await emitCalEvent("calibration-cancelled", { loop: calPhase.loop });
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
    <span class="text-[0.78rem] font-bold tracking-tight flex-1">{t("onboarding.title")}</span>
    <!-- TTS readiness indicator (shown on calibration step) -->
    {#if step === "calibration" && !ttsReady}
      <span class="flex items-center gap-1 text-[0.52rem] text-amber-600 dark:text-amber-400
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
          class="text-[0.48rem] font-medium transition-colors
                 {i <= stepIdx ? 'text-foreground cursor-pointer' : 'text-muted-foreground/40 cursor-default'}">
          {t(`onboarding.step.${s}`)}
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
        <h2 class="text-[1.05rem] font-bold">{t("onboarding.welcomeTitle")}</h2>
        <p class="text-[0.72rem] text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.welcomeBody")}
        </p>
        <div class="flex flex-col gap-1.5 w-full max-w-[300px] mt-1">
          {#each ["bluetooth", "fit", "calibration", "models"] as s}
            <div class="flex items-center gap-2.5 rounded-lg border border-border dark:border-white/[0.06]
                        bg-muted dark:bg-[#1a1a28] px-3 py-2">
              <span class="text-base">{s === "bluetooth" ? "📡" : s === "fit" ? "🎧" : s === "calibration" ? "🎯" : "⬇️"}</span>
              <div class="flex flex-col text-left">
                <span class="text-[0.68rem] font-semibold">{t(`onboarding.step.${s}`)}</span>
                <span class="text-[0.55rem] text-muted-foreground">{t(`onboarding.${s}Hint`)}</span>
              </div>
            </div>
          {/each}
        </div>
      </div>

    <!-- ════ ENABLE BLUETOOTH (OS) ═════════════════════════════════════════════════ -->
    {:else if step === "enable_bluetooth"}
      <div class="flex flex-col items-center gap-3 pt-3 text-center" in:fly={{ x: 30, duration: 200 }}>
        <span class="text-3xl">{btEnabled ? '✅' : '🔌'}</span>
        <h2 class="text-[0.95rem] font-bold">{t("onboarding.enableBluetoothTitle")}</h2>
        <p class="text-[0.68rem] text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.enableBluetoothBody")}
        </p>

        <Card class="w-full max-w-[320px] border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-2.5">
            <div class="flex items-center gap-2.5">
              <div class="flex flex-col gap-0 flex-1 min-w-0">
                <span class="text-[0.68rem] font-semibold">{t('onboarding.enableBluetoothStatus')}</span>
              </div>
              <span class="ml-auto inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[0.55rem] font-semibold
                           {btEnabled ? 'bg-green-500/15 text-green-700 dark:text-green-400 border-green-500/30' : 'bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30'}">
                <span class="w-1.5 h-1.5 rounded-full {btEnabled ? 'bg-green-500' : 'bg-amber-400'}"></span>
                {btEnabled ? t('perm.granted') : t('perm.denied')}
              </span>
            </div>
            <p class="text-[0.58rem] text-muted-foreground/80 leading-relaxed mt-2">{t('onboarding.enableBluetoothHint')}</p>
            <div class="flex justify-end mt-2 gap-2">
              <Button size="sm" variant="outline" class="h-7 text-[0.62rem] px-3" onclick={openBt}>
                {t('onboarding.enableBluetoothOpen')}
              </Button>
              <Button size="sm" class="h-7 text-[0.62rem] px-3" onclick={checkBt}>
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
        <h2 class="text-[0.95rem] font-bold">{t("onboarding.bluetoothTitle")}</h2>
        <p class="text-[0.68rem] text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.bluetoothBody")}
        </p>

        <Card class="w-full max-w-[320px] border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-2.5">
            <div class="flex items-center gap-2.5">
              <div class="w-2.5 h-2.5 rounded-full shrink-0
                          {isConnected ? 'bg-green-500' : isScanning ? 'bg-yellow-500 animate-pulse' : 'bg-slate-400'}"></div>
              <div class="flex flex-col gap-0 flex-1 min-w-0">
                <span class="text-[0.68rem] font-semibold">
                  {isConnected
                    ? t("onboarding.btConnected", { name: status.device_name ?? "Muse" })
                    : isScanning ? t("onboarding.btScanning") : t("onboarding.btReady")}
                </span>
                {#if isConnected && status.battery > 0}
                  <span class="text-[0.55rem] text-muted-foreground">{t("dashboard.battery")}: {status.battery.toFixed(0)}%</span>
                {/if}
              </div>
              {#if !isConnected}
                <Button size="sm" class="text-[0.6rem] h-6 px-2.5 shrink-0" onclick={startScan} disabled={isScanning}>
                  {isScanning ? t("onboarding.btScanning") : t("onboarding.btScan")}
                </Button>
              {/if}
            </div>
          </CardContent>
        </Card>

        <div class="w-full max-w-[320px] flex flex-col gap-1.5 text-left">
          <p class="text-[0.5rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("onboarding.btInstructions")}
          </p>
          {#each [1,2,3] as n}
            <div class="flex items-start gap-2">
              <span class="w-4 h-4 rounded-full bg-muted dark:bg-white/[0.06] flex items-center justify-center
                           text-[0.5rem] font-bold text-muted-foreground shrink-0 mt-0.5">{n}</span>
              <p class="text-[0.62rem] text-muted-foreground leading-relaxed">{t(`onboarding.btStep${n}`)}</p>
            </div>
          {/each}
        </div>

        {#if isConnected}
          <div class="flex items-center gap-1.5 text-green-600 dark:text-green-400" in:fade={{ duration: 200 }}>
            <span>✓</span>
            <span class="text-[0.68rem] font-semibold">{t("onboarding.btSuccess")}</span>
          </div>
        {/if}
      </div>

    <!-- ════ FIT CHECK ════════════════════════════════════════════════════════ -->
    {:else if step === "fit"}
      <div class="flex flex-col items-center gap-2 pt-2 text-center" in:fly={{ x: 30, duration: 200 }}>
        <h2 class="text-[0.95rem] font-bold">{t("onboarding.fitTitle")}</h2>
        <p class="text-[0.65rem] text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.fitBody")}
        </p>

        <ElectrodeGuide qualityLabels={status.channel_quality} device={status.device_kind} deviceName={status.device_name ?? ""} />

        {#if !isConnected}
          <p class="text-[0.62rem] text-amber-600 dark:text-amber-400">⚠ {t("onboarding.fitNeedsBt")}</p>
        {/if}

        {#if allGoodOrFair && isConnected}
          <div class="flex items-center gap-1.5 text-green-600 dark:text-green-400" in:fade={{ duration: 200 }}>
            <span>✓</span>
            <span class="text-[0.68rem] font-semibold">{t("onboarding.fitGood")}</span>
          </div>
        {/if}
      </div>

    <!-- ════ CALIBRATION ══════════════════════════════════════════════════════ -->
    {:else if step === "calibration"}
      <div class="flex flex-col items-center gap-4 pt-3 text-center" in:fly={{ x: 30, duration: 200 }}>

        {#if calPhase.kind === "idle"}
          <!-- ── Idle / start screen ─────────────────────────────────────── -->
          <span class="text-3xl">🎯</span>
          <h2 class="text-[0.95rem] font-bold">{t("onboarding.calibrationTitle")}</h2>
          <p class="text-[0.68rem] text-muted-foreground leading-relaxed max-w-[320px]">
            {t("onboarding.calibrationBody")}
          </p>

          {#if calProfile}
            <!-- Action chips -->
            <div class="flex flex-wrap gap-1.5 justify-center max-w-[380px]">
              {#each calProfile.actions as action, i}
                {@const colors = [
                  "border-primary/30 bg-primary/10 text-primary",
                  "border-violet-500/30 bg-violet-500/10 text-violet-600 dark:text-violet-400",
                  "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
                  "border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400",
                  "border-rose-500/30 bg-rose-500/10 text-rose-600 dark:text-rose-400",
                  "border-cyan-500/30 bg-cyan-500/10 text-cyan-600 dark:text-cyan-400",
                ]}
                <span class="rounded-full border px-2.5 py-0.5 text-[0.62rem] font-medium {colors[i % colors.length]}">
                  {action.label} · {action.duration_secs}s
                </span>
              {/each}
              <span class="rounded-full border border-amber-500/30 bg-amber-500/10
                           text-amber-600 dark:text-amber-400 px-2.5 py-0.5 text-[0.62rem] font-medium">
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
            <p class="text-[0.6rem] text-amber-600 dark:text-amber-400">⚠ {t("onboarding.calibrationNeedsBt")}</p>
          {/if}

          <p class="text-[0.6rem] text-muted-foreground/50 max-w-[280px] leading-relaxed">
            {t("onboarding.calibrationSkip")}
          </p>

        {:else if calPhase.kind === "done"}
          <!-- ── Done screen ──────────────────────────────────────────────── -->
          <div class="flex flex-col items-center gap-3">
            <div class="w-14 h-14 rounded-full bg-emerald-500/10 flex items-center justify-center text-2xl">✅</div>
            <h2 class="text-[0.95rem] font-bold text-emerald-600 dark:text-emerald-400">{t("calibration.complete")}</h2>
            <p class="text-[0.68rem] text-muted-foreground leading-relaxed max-w-[300px]">
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
                <span class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground/60">
                  {calProfile.name}
                </span>
                <div class="flex items-center gap-2">
                  <span class="text-[0.58rem] font-semibold tracking-widest uppercase text-muted-foreground">
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
                  <span class="text-[0.62rem] text-muted-foreground tabular-nums">
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
                <span class="text-[0.68rem] text-muted-foreground">
                  {t("calibration.nextAction", { action: calProfile.actions[nextIdx]?.label ?? "" })}
                </span>
              {/if}
            </div>

            <!-- Countdown -->
            <div class="flex flex-col items-center gap-2 w-full">
              <span class="text-[2.8rem] font-bold tabular-nums leading-none">{calCountdown}</span>
              <span class="text-[0.58rem] text-muted-foreground/50">{t("calibration.secondsRemaining")}</span>
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
        <h2 class="text-[0.95rem] font-bold">{t("onboarding.modelsTitle")}</h2>
        <p class="text-[0.68rem] text-muted-foreground leading-relaxed max-w-[340px]">
          {t("onboarding.modelsBody")}
        </p>

        <Card class="w-full max-w-[360px] border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-3 flex flex-col gap-3">
            <div class="flex justify-end">
              <Button size="sm" class="h-7 text-[0.62rem] px-3"
                      onclick={downloadRecommendedBundle}
                      disabled={bundleBusy || allRecommendedReady || ttsActionBusy}>
                {allRecommendedReady
                  ? t("onboarding.models.downloaded")
                  : bundleBusy
                    ? t("onboarding.models.downloading")
                    : t("onboarding.models.downloadAll")}
              </Button>
            </div>

            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-[#1a1a28] px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">🤖</span>
                <span class="text-[0.66rem] font-semibold">{t("onboarding.models.qwenTitle")}</span>
                <span class="text-[0.56rem] text-emerald-600 dark:text-emerald-400 ml-auto">
                  {llmTarget ? llmTarget.quant : "Q4"}
                </span>
              </div>
              <p class="text-[0.58rem] text-muted-foreground/80 leading-relaxed">{t("onboarding.models.qwenDesc")}</p>
              {#if llmTarget?.repo}
                <p class="text-[0.5rem] text-muted-foreground/70 font-mono">🤗 hf download {llmTarget.repo} {llmTarget.filename}</p>
              {/if}
              {#if llmIsDownloading}
                <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                  <div class="h-full rounded-full bg-blue-500 transition-[width] duration-300" style="width:{llmProgressPct.toFixed(1)}%"></div>
                </div>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-[0.62rem] px-3" onclick={downloadLlm}
                        disabled={llmIsDownloaded || llmIsDownloading || !llmTarget}>
                  {llmIsDownloaded ? t("onboarding.models.downloaded") : llmIsDownloading ? t("onboarding.models.downloading") : t("onboarding.models.download")}
                </Button>
              </div>
            </div>

            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-[#1a1a28] px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">🧠</span>
                <span class="text-[0.66rem] font-semibold">{t("onboarding.models.zunaTitle")}</span>
              </div>
              <p class="text-[0.58rem] text-muted-foreground/80 leading-relaxed">{t("onboarding.models.zunaDesc")}</p>
              <p class="text-[0.5rem] text-muted-foreground/70 font-mono">🤗 hf download Zyphra/ZUNA model-00001-of-00001.safetensors</p>
              {#if zunaIsDownloading}
                <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                  <div class="h-full rounded-full bg-blue-500 transition-[width] duration-300" style="width:{zunaProgressPct.toFixed(1)}%"></div>
                </div>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-[0.62rem] px-3" onclick={downloadZuna}
                        disabled={zunaIsDownloaded || zunaIsDownloading}>
                  {zunaIsDownloaded ? t("onboarding.models.downloaded") : zunaIsDownloading ? t("onboarding.models.downloading") : t("onboarding.models.download")}
                </Button>
              </div>
            </div>

            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-[#1a1a28] px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">🗣️</span>
                <span class="text-[0.66rem] font-semibold">{t("onboarding.models.neuttsTitle")}</span>
              </div>
              <p class="text-[0.58rem] text-muted-foreground/80 leading-relaxed">{t("onboarding.models.neuttsDesc")}</p>
              <p class="text-[0.5rem] text-muted-foreground/70 font-mono">🤗 hf download neuphonic/neutts-nano-q4-gguf --include "*.gguf"</p>
              {#if neuttsDlState === "error" && neuttsDlError}
                <p class="text-[0.55rem] text-destructive leading-relaxed">{neuttsDlError}</p>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-[0.62rem] px-3" onclick={() => downloadTtsBackend("neutts")}
                        disabled={ttsActionBusy || neuttsDlState === "ready"}>
                  {neuttsDlState === "ready" ? t("onboarding.models.downloaded") : neuttsDlState === "downloading" ? (ttsDlLabel || t("onboarding.models.downloading")) : t("onboarding.models.download")}
                </Button>
              </div>
            </div>

            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-[#1a1a28] px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">🐱</span>
                <span class="text-[0.66rem] font-semibold">{t("onboarding.models.kittenTitle")}</span>
              </div>
              <p class="text-[0.58rem] text-muted-foreground/80 leading-relaxed">{t("onboarding.models.kittenDesc")}</p>
              <p class="text-[0.5rem] text-muted-foreground/70 font-mono">🤗 hf download KittenML/kitten-tts-mini-0.8</p>
              {#if kittenDlState === "error" && kittenDlError}
                <p class="text-[0.55rem] text-destructive leading-relaxed">{kittenDlError}</p>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-[0.62rem] px-3" onclick={() => downloadTtsBackend("kitten")}
                        disabled={ttsActionBusy || kittenDlState === "ready"}>
                  {kittenDlState === "ready" ? t("onboarding.models.downloaded") : kittenDlState === "downloading" ? (ttsDlLabel || t("onboarding.models.downloading")) : t("onboarding.models.download")}
                </Button>
              </div>
            </div>
            <div class="flex flex-col gap-1.5 rounded-lg border border-border/70 dark:border-white/[0.08] bg-muted/40 dark:bg-[#1a1a28] px-3 py-2.5 text-left">
              <div class="flex items-center gap-2">
                <span class="text-sm">📝</span>
                <span class="text-[0.66rem] font-semibold">{t("onboarding.models.ocrTitle")}</span>
              </div>
              <p class="text-[0.58rem] text-muted-foreground/80 leading-relaxed">{t("onboarding.models.ocrDesc")}</p>
              {#if ocrDlState === "error" && ocrDlError}
                <p class="text-[0.55rem] text-destructive leading-relaxed">{ocrDlError}</p>
              {/if}
              <div class="flex justify-end">
                <Button size="sm" class="h-7 text-[0.62rem] px-3" onclick={downloadOcrModels}
                        disabled={ocrDlState === "ready" || ocrDlState === "downloading"}>
                  {ocrDlState === "ready" ? t("onboarding.models.downloaded") : ocrDlState === "downloading" ? t("onboarding.models.downloading") : t("onboarding.models.download")}
                </Button>
              </div>
            </div>

          </CardContent>
        </Card>

        <!-- ── Screen Recording permission (macOS) ──────────────────── -->
        {#if isMac}
          <Card class="w-full max-w-[360px] border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
            <CardContent class="px-3 py-3 flex flex-col gap-2">
              <div class="flex items-center gap-2">
                <span class="text-sm">🖥️</span>
                <span class="text-[0.66rem] font-semibold">{t("onboarding.screenRecTitle")}</span>
                <span class="ml-auto inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[0.55rem] font-semibold
                             {screenRecPerm ? 'bg-green-500/15 text-green-700 dark:text-green-400 border-green-500/30' : 'bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30'}">
                  <span class="w-1.5 h-1.5 rounded-full {screenRecPerm ? 'bg-green-500' : 'bg-amber-400'}"></span>
                  {screenRecPerm ? t("perm.granted") : t("perm.denied")}
                </span>
              </div>
              <p class="text-[0.58rem] text-muted-foreground/80 leading-relaxed">{t("onboarding.screenRecDesc")}</p>
              {#if !screenRecPerm}
                <div class="flex justify-end">
                  <Button size="sm" variant="outline" class="h-7 text-[0.62rem] px-3"
                          onclick={() => invoke("open_screen_recording_settings")}>
                    {t("onboarding.screenRecOpen")}
                  </Button>
                </div>
              {/if}
            </CardContent>
          </Card>
        {/if}

        {#if modelLoadError}
          <p class="text-[0.56rem] text-destructive/90 max-w-[340px] leading-relaxed">{modelLoadError}</p>
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

        <h2 class="text-[0.95rem] font-bold">{t("onboarding.trayTitle")}</h2>
        <p class="text-[0.68rem] text-muted-foreground leading-relaxed max-w-[320px]">
          {t("onboarding.trayBody")}
        </p>

        <!-- Icon-state reference card -->
        <Card class="w-full max-w-[320px] border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
          <CardContent class="px-3 py-3">
            <p class="text-[0.5rem] font-semibold tracking-widest uppercase text-muted-foreground mb-2.5">
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
                  <span class="text-[0.65rem] text-left">{row.label}</span>
                </div>
              {/each}
            </div>
          </CardContent>
        </Card>

        <!-- How-to tips -->
        <div class="w-full max-w-[320px] flex flex-col gap-1.5 text-left">
          <div class="flex items-start gap-2.5 rounded-lg border border-border dark:border-white/[0.06]
                      bg-muted dark:bg-[#1a1a28] px-3 py-2">
            <span class="text-base shrink-0">👆</span>
            <p class="text-[0.62rem] text-muted-foreground leading-relaxed">{t("onboarding.tray.open")}</p>
          </div>
          <div class="flex items-start gap-2.5 rounded-lg border border-border dark:border-white/[0.06]
                      bg-muted dark:bg-[#1a1a28] px-3 py-2">
            <span class="text-base shrink-0">🖱</span>
            <p class="text-[0.62rem] text-muted-foreground leading-relaxed">{t("onboarding.tray.menu")}</p>
          </div>
        </div>
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
          <p class="text-[0.68rem] text-muted-foreground leading-relaxed max-w-[340px]">
            {t("onboarding.downloadsCompleteBody")} <button onclick={openSettings} class="font-semibold text-blue-600 dark:text-blue-400 hover:underline cursor-pointer">{t("onboarding.downloadMoreSettings")}</button>.
          </p>
        {:else}
          <!-- Default Done View -->
          <span class="text-4xl">🎉</span>
          <h2 class="text-[1.05rem] font-bold">{t("onboarding.doneTitle")}</h2>
          <p class="text-[0.72rem] text-muted-foreground leading-relaxed max-w-[320px]">
            {t("onboarding.doneBody")}
          </p>
        {/if}

        <div class="flex flex-col gap-1.5 w-full max-w-[300px] mt-1">
          {#each ["tray", "shortcuts", "help"] as tip}
            <div class="flex items-start gap-2.5 rounded-lg border border-border dark:border-white/[0.06]
                        bg-muted dark:bg-[#1a1a28] px-3 py-2 text-left">
              <span class="text-base shrink-0">{tip === "tray" ? "🖥" : tip === "shortcuts" ? "⌨" : "❓"}</span>
              <p class="text-[0.6rem] text-muted-foreground leading-relaxed">{t(`onboarding.doneTip.${tip}`)}</p>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>

  <!-- ── Bottom navigation ─────────────────────────────────────────────────── -->
  <div class="flex items-center justify-between px-4 py-2.5
              border-t border-border dark:border-white/[0.07] shrink-0">
    {#if step === "welcome" || calRunning}
      <span></span>
    {:else}
      <Button variant="ghost" size="sm" class="text-[0.65rem] h-7 px-2.5" onclick={prev}>
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
      <Button size="sm" class="text-[0.65rem] h-7 px-4" onclick={finish}>
        {t("onboarding.finish")} →
      </Button>
    {:else if calRunning}
      <!-- Back/Next locked while calibration is in progress -->
      <span></span>
    {:else if step === "calibration" && calPhase.kind === "done"}
      <!-- "Next" shown inline in the done screen — hide duplicate here -->
      <span></span>
    {:else}
      <Button size="sm" class="text-[0.65rem] h-7 px-3" onclick={next}>
        {step === "welcome" ? t("onboarding.getStarted") : t("onboarding.next")} →
      </Button>
    {/if}
  </div>

  <div class="px-4 pb-1.5 shrink-0">
    <p class="text-[0.52rem] text-muted-foreground/75 text-center leading-tight truncate"
       title={footerModelStatus}>
      {footerModelStatus}
    </p>
  </div>

  <DisclaimerFooter />
</main>
