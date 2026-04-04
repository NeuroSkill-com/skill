<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { onDestroy, onMount } from "svelte";
import { fade } from "svelte/transition";
import BandChart, { type BandSnapshot } from "$lib/BandChart.svelte";
// Device capabilities are now pushed as part of DeviceStatus from Rust.
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent, CardFooter, CardHeader } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { Spinner } from "$lib/components/ui/spinner";
import {
  DEFAULT_FILTER_CONFIG,
  EEG_CH,
  EEG_COLOR,
  EMOTIV_CH,
  EMOTIV_COLOR,
  GANGLION_CH,
  GANGLION_COLOR,
  HERMES_CH,
  HERMES_COLOR,
  IDUN_CH,
  IDUN_COLOR,
  MW75_CH,
  MW75_COLOR,
} from "$lib/constants";
import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import {
  ArtifactEvents,
  BrainStateScores,
  CompositeScores,
  ConsciousnessMetrics,
  EegIndices,
  FaaGauge,
  HeadPoseCard,
  PpgMetrics,
} from "$lib/dashboard";
import EegChart, { type EventMarker, type SpectrogramColumn } from "$lib/EegChart.svelte";
import ElectrodeGuide from "$lib/ElectrodeGuide.svelte";
import FnirsChart from "$lib/FnirsChart.svelte";
import GpuChart from "$lib/GpuChart.svelte";
import ImuChart, { type ImuPacket } from "$lib/ImuChart.svelte";
import { t } from "$lib/i18n/index.svelte";
import { openBtSettings, openHistory, openLabel, openSettings, openUpdates } from "$lib/navigation";
import OnboardingChecklist from "$lib/OnboardingChecklist.svelte";
import PpgChart, { type PpgPacket } from "$lib/PpgChart.svelte";
import { setBtOff } from "$lib/stores/bt-status.svelte";
import { addToast } from "$lib/stores/toast.svelte";
import { useWindowTitle } from "$lib/stores/window-title.svelte";
import { C_NEUTRAL, colorForLevel, QUALITY_COLORS, STATE_COLORS } from "$lib/theme";
import type { DeviceStatus, DiscoveredDevice } from "$lib/types";

// ── Model download status (shown as a banner when downloading/retrying) ────
interface ModelDownloadStatus {
  downloading_weights: boolean;
  download_status_msg: string | null;
  download_retry_attempt: number;
  download_retry_in_secs: number;
  encoder_loaded: boolean;
}
let modelDl = $state<ModelDownloadStatus>({
  downloading_weights: false,
  download_status_msg: null,
  download_retry_attempt: 0,
  download_retry_in_secs: 0,
  encoder_loaded: false,
});
let modelDlTimer: ReturnType<typeof setInterval> | null = null;

async function refreshModelDl() {
  try {
    const s = await daemonInvoke<ModelDownloadStatus>("get_eeg_model_status");
    modelDl = s;
    // Stop polling once encoder is loaded.
    if (s.encoder_loaded && modelDlTimer) {
      clearInterval(modelDlTimer);
      modelDlTimer = null;
    }
  } catch {
    /* non-fatal */
  }
}

const modelDlVisible = $derived(modelDl.downloading_weights || modelDl.download_retry_in_secs > 0);

// ── Types ──────────────────────────────────────────────────────────────────
interface EegPacket {
  electrode: number;
  samples: number[];
  timestamp: number;
}

// EEG_CH and EEG_COLOR imported from constants.ts.

// ── Chart refs ─────────────────────────────────────────────────────────────
let chartEl = $state<
  | {
      pushSamples(ch: number, samples: number[]): void;
      pushSpecColumn(col: SpectrogramColumn): void;
      pushMarker(m: EventMarker): void;
      restartRender(): void;
    }
  | undefined
>();
let bandChartEl = $state<{ update(snap: BandSnapshot): void; restartRender(): void } | undefined>();
let ppgChartEl = $state<
  | {
      pushSamples(ch: number, samples: number[]): void;
      pushMarker(m: { timestamp_ms: number; label: string; color: string }): void;
    }
  | undefined
>();
let imuChartEl = $state<
  | {
      pushPacket(pkt: ImuPacket): void;
    }
  | undefined
>();
let fnirsChartEl = $state<
  | {
      pushMetrics(m: { hbo: number; hbr: number; hbt: number; workload: number; oxygenation: number }): void;
    }
  | undefined
>();

// ── Relaxation / Engagement score ─────────────────────────────────────────
//  Relax   = alpha / (beta  + theta)   — high when calm/meditative
//  Engage  = beta  / (alpha + theta)   — high when alert/task-engaged
//  Both are averaged across all channels, smoothed with an EMA, then
//  mapped to 0–100 via a sigmoid so the reading is intuitive.
let focusScore = $state(0); // kept internally for ws/API compat — not shown in UI
let relaxScore = $state(0);
let engagementScore = $state(0);
/** Frontal Alpha Asymmetry: ln(AF8 α) − ln(AF7 α).
 *  Positive → greater right-frontal alpha → left-frontal approach bias.
 *  Range is roughly −1 … +1; smoothed with the same EMA as the other scores. */
let faaScore = $state(0);
let tarScore = $state(0); // Theta/Alpha ratio
let barScore = $state(0); // Beta/Alpha ratio
let dtrScore = $state(0); // Delta/Theta ratio
let pseScore = $state(0); // Power Spectral Entropy
let apfScore = $state(0); // Alpha Peak Frequency
let bpsScore = $state(0); // Band-Power Slope
let snrScore = $state(0); // Signal-to-Noise Ratio
let coherenceScore = $state(0); // Inter-channel coherence
let muScore = $state(1); // Mu suppression (1 = baseline)
let moodScore = $state(50); // Mood index (0–100)
let tbrScore = $state(0); // Theta/Beta ratio (absolute)
let sef95Score = $state(0); // Spectral Edge Frequency 95%
let scScore = $state(0); // Spectral Centroid
let haScore = $state(0); // Hjorth Activity
let hmScore = $state(0); // Hjorth Mobility
let hcScore = $state(0); // Hjorth Complexity
let peScore = $state(0); // Permutation Entropy
let hfdScore = $state(0); // Higuchi Fractal Dimension
let dfaScore = $state(0); // DFA Exponent
let seScore = $state(0); // Sample Entropy
let pacScore = $state(0); // PAC (θ–γ)
let latScore = $state(0); // Laterality Index
let hrScore = $state(0); // Heart Rate (bpm)
let rmssdScore = $state(0); // RMSSD (ms)
let sdnnScore = $state(0); // SDNN (ms)
let pnn50Score = $state(0); // pNN50 (%)
let lfHfScore = $state(0); // LF/HF ratio
let respRateScore = $state(0); // Respiratory Rate (bpm)
let spo2Score = $state(0); // SpO2 estimate (%)
let perfIdxScore = $state(0); // Perfusion Index (%)
let stressIdxScore = $state(0); // Stress Index
// Artifact / event detection
let blinkCount = $state(0);
let blinkRate = $state(0); // blinks/min
// Head pose
let headPitch = $state(0); // degrees
let headRoll = $state(0); // degrees
let stillnessScore = $state(0); // 0–100
let nodCount = $state(0);
let shakeCount = $state(0);
// Composite scores
let meditationScore = $state(0); // 0–100
let cogLoadScore = $state(0); // 0–100
let drowsinessScore = $state(0); // 0–100
// Headache / Migraine EEG correlate indices (0–100)
let headacheScore = $state(0);
let migraineScore = $state(0);
// Consciousness metrics (0–100)
let consciousnessLzc = $state(0);
let consciousnessWakefulness = $state(0);
let consciousnessIntegration = $state(0);
const SCORE_TAU = 0.15; // EMA smoothing factor (0 = frozen, 1 = instant)

function sigmoid100(x: number): number {
  // Maps (0, ∞) → (0, 100) with midpoint at x=1
  return 100 / (1 + Math.exp(-2.5 * (x - 1)));
}

function updateScores(snap: BandSnapshot) {
  if (!snap.channels || snap.channels.length === 0) return;
  let sumFocus = 0,
    sumRelax = 0,
    sumEngage = 0,
    n = 0;
  for (const ch of snap.channels) {
    const a = ch.rel_alpha || 0;
    const b = ch.rel_beta || 0;
    const t = ch.rel_theta || 0;
    const denom1 = a + t;
    const denom2 = b + t;
    const denom3 = a + t; // engagement: beta / (alpha + theta) — same ratio
    if (denom1 > 0.001) sumFocus += b / denom1;
    if (denom2 > 0.001) sumRelax += a / denom2;
    // Engagement index (Pope 1995): β / (α + θ)
    // https://ntrs.nasa.gov/api/citations/19970003078/downloads/19970003078.pdf
    // We use the same ratio as focus but scale differently via a wider sigmoid
    // to separate the two visually — engagement emphasises sustained attention
    // over a longer baseline while focus is the instantaneous reading.
    if (denom3 > 0.001) sumEngage += b / denom3;
    n++;
  }
  if (n === 0) return;
  const rawFocus = sigmoid100(sumFocus / n);
  const rawRelax = sigmoid100(sumRelax / n);
  // Engagement uses a gentler sigmoid (k=2) with midpoint shifted to 0.8
  // so it's less "twitchy" and represents sustained cognitive engagement.
  const rawEngage = 100 / (1 + Math.exp(-2 * (sumEngage / n - 0.8)));
  focusScore = focusScore + SCORE_TAU * (rawFocus - focusScore);
  relaxScore = relaxScore + SCORE_TAU * (rawRelax - relaxScore);
  engagementScore = engagementScore + SCORE_TAU * 0.6 * (rawEngage - engagementScore);

  // ── Frontal Alpha Asymmetry (FAA) ──────────────────────────────────
  // Precomputed on the backend: ln(AF8 α) − ln(AF7 α).
  // Smoothed with the same EMA as the other scores.
  if (snap.faa !== undefined) {
    faaScore = faaScore + SCORE_TAU * (snap.faa - faaScore);
  }
  // ── New indices (all precomputed on backend) ─────────────────────
  if (snap.tar !== undefined) tarScore = tarScore + SCORE_TAU * (snap.tar - tarScore);
  if (snap.bar !== undefined) barScore = barScore + SCORE_TAU * (snap.bar - barScore);
  if (snap.dtr !== undefined) dtrScore = dtrScore + SCORE_TAU * (snap.dtr - dtrScore);
  if (snap.pse !== undefined) pseScore = pseScore + SCORE_TAU * (snap.pse - pseScore);
  if (snap.apf !== undefined && snap.apf > 0) apfScore = apfScore + SCORE_TAU * (snap.apf - apfScore);
  if (snap.bps !== undefined) bpsScore = bpsScore + SCORE_TAU * (snap.bps - bpsScore);
  if (snap.snr !== undefined) snrScore = snrScore + SCORE_TAU * (snap.snr - snrScore);
  if (snap.coherence !== undefined) coherenceScore = coherenceScore + SCORE_TAU * (snap.coherence - coherenceScore);
  if (snap.mu_suppression !== undefined) muScore = muScore + SCORE_TAU * (snap.mu_suppression - muScore);
  if (snap.mood !== undefined) moodScore = moodScore + SCORE_TAU * (snap.mood - moodScore);
  if (snap.tbr !== undefined) tbrScore = tbrScore + SCORE_TAU * (snap.tbr - tbrScore);
  if (snap.sef95 !== undefined) sef95Score = sef95Score + SCORE_TAU * (snap.sef95 - sef95Score);
  if (snap.spectral_centroid !== undefined) scScore = scScore + SCORE_TAU * (snap.spectral_centroid - scScore);
  if (snap.hjorth_activity !== undefined) haScore = haScore + SCORE_TAU * (snap.hjorth_activity - haScore);
  if (snap.hjorth_mobility !== undefined) hmScore = hmScore + SCORE_TAU * (snap.hjorth_mobility - hmScore);
  if (snap.hjorth_complexity !== undefined) hcScore = hcScore + SCORE_TAU * (snap.hjorth_complexity - hcScore);
  if (snap.permutation_entropy !== undefined) peScore = peScore + SCORE_TAU * (snap.permutation_entropy - peScore);
  if (snap.higuchi_fd !== undefined) hfdScore = hfdScore + SCORE_TAU * (snap.higuchi_fd - hfdScore);
  if (snap.dfa_exponent !== undefined) dfaScore = dfaScore + SCORE_TAU * (snap.dfa_exponent - dfaScore);
  if (snap.sample_entropy !== undefined) seScore = seScore + SCORE_TAU * (snap.sample_entropy - seScore);
  if (snap.pac_theta_gamma !== undefined) pacScore = pacScore + SCORE_TAU * (snap.pac_theta_gamma - pacScore);
  if (snap.laterality_index !== undefined) latScore = latScore + SCORE_TAU * (snap.laterality_index - latScore);
  // PPG-derived
  if (snap.hr !== undefined && snap.hr > 0) hrScore = hrScore + SCORE_TAU * (snap.hr - hrScore);
  if (snap.rmssd !== undefined && snap.rmssd > 0) rmssdScore = rmssdScore + SCORE_TAU * (snap.rmssd - rmssdScore);
  if (snap.sdnn !== undefined && snap.sdnn > 0) sdnnScore = sdnnScore + SCORE_TAU * (snap.sdnn - sdnnScore);
  if (snap.pnn50 !== undefined) pnn50Score = pnn50Score + SCORE_TAU * (snap.pnn50 - pnn50Score);
  if (snap.lf_hf_ratio !== undefined && snap.lf_hf_ratio > 0)
    lfHfScore = lfHfScore + SCORE_TAU * (snap.lf_hf_ratio - lfHfScore);
  if (snap.respiratory_rate !== undefined && snap.respiratory_rate > 0)
    respRateScore = respRateScore + SCORE_TAU * (snap.respiratory_rate - respRateScore);
  if (snap.spo2_estimate !== undefined && snap.spo2_estimate > 0)
    spo2Score = spo2Score + SCORE_TAU * (snap.spo2_estimate - spo2Score);
  if (snap.perfusion_index !== undefined && snap.perfusion_index > 0)
    perfIdxScore = perfIdxScore + SCORE_TAU * (snap.perfusion_index - perfIdxScore);
  if (snap.stress_index !== undefined && snap.stress_index > 0)
    stressIdxScore = stressIdxScore + SCORE_TAU * (snap.stress_index - stressIdxScore);
  // Artifact / event detection (counts are absolute, not smoothed)
  if (snap.blink_count !== undefined) blinkCount = snap.blink_count;
  if (snap.blink_rate !== undefined) blinkRate = blinkRate + SCORE_TAU * (snap.blink_rate - blinkRate);
  // Head pose
  if (snap.head_pitch !== undefined) headPitch = headPitch + SCORE_TAU * (snap.head_pitch - headPitch);
  if (snap.head_roll !== undefined) headRoll = headRoll + SCORE_TAU * (snap.head_roll - headRoll);
  if (snap.stillness !== undefined) stillnessScore = stillnessScore + SCORE_TAU * (snap.stillness - stillnessScore);
  if (snap.nod_count !== undefined) nodCount = snap.nod_count;
  if (snap.shake_count !== undefined) shakeCount = snap.shake_count;
  // Composite scores
  if (snap.meditation !== undefined)
    meditationScore = meditationScore + SCORE_TAU * (snap.meditation - meditationScore);
  if (snap.cognitive_load !== undefined) cogLoadScore = cogLoadScore + SCORE_TAU * (snap.cognitive_load - cogLoadScore);
  if (snap.drowsiness !== undefined)
    drowsinessScore = drowsinessScore + SCORE_TAU * (snap.drowsiness - drowsinessScore);
  // Headache / Migraine EEG correlate indices
  if (snap.headache_index !== undefined)
    headacheScore = headacheScore + SCORE_TAU * (snap.headache_index - headacheScore);
  if (snap.migraine_index !== undefined)
    migraineScore = migraineScore + SCORE_TAU * (snap.migraine_index - migraineScore);
  // Consciousness metrics
  if (snap.consciousness_lzc !== undefined)
    consciousnessLzc = consciousnessLzc + SCORE_TAU * (snap.consciousness_lzc - consciousnessLzc);
  if (snap.consciousness_wakefulness !== undefined)
    consciousnessWakefulness =
      consciousnessWakefulness + SCORE_TAU * (snap.consciousness_wakefulness - consciousnessWakefulness);
  if (snap.consciousness_integration !== undefined)
    consciousnessIntegration =
      consciousnessIntegration + SCORE_TAU * (snap.consciousness_integration - consciousnessIntegration);
}

// ── Status ─────────────────────────────────────────────────────────────────
/** Redact all dash-separated segments except the last suffix.
 *  "AAAA-BBBB-CCCC"    → "****-****-CCCC"
 *  "AA-BB-CC-DD-EE-FF" → "**-**-**-**-**-FF" */
function redact(v: string) {
  const parts = v.split("-");
  return [...parts.slice(0, -1).map((p) => "*".repeat(p.length)), parts.at(-1)].join("-");
}
let revealSN = $state(false);
let revealMAC = $state(false);
let showElectrodes = $state(false);
let showDeviceSwitcher = $state(false);

// ── Secondary sessions ─────────────────────────────────────────────────────
interface SecondarySession {
  id: string;
  device_name: string;
  device_kind: string;
  channels: number;
  sample_rate: number;
  sample_count: number;
  csv_path: string;
  started_at: number;
  battery: number;
}
let secondarySessions = $state<SecondarySession[]>([]);

let status = $state<DeviceStatus>({
  state: "disconnected",
  device_name: null,
  device_id: null,
  serial_number: null,
  mac_address: null,
  csv_path: null,
  sample_count: 0,
  battery: 0,
  eeg: [0, 0, 0, 0],
  paired_devices: [],
  device_error: null,
  target_name: null,
  filter_config: { ...DEFAULT_FILTER_CONFIG },
  channel_quality: ["no_signal", "no_signal", "no_signal", "no_signal"],
  retry_attempt: 0,
  retry_countdown_secs: 0,
  ppg: [0, 0, 0],
  ppg_sample_count: 0,
  accel: [0, 0, 0],
  gyro: [0, 0, 0],
  fuel_gauge_mv: 0,
  temperature_raw: 0,
  device_kind: "unknown",
  hardware_version: null,
  channel_names: [],
  ppg_channel_names: [],
  imu_channel_names: [],
  fnirs_channel_names: [],
  fnirs_oxygenation_pct: 0,
  fnirs_workload: 0,
  fnirs_lateralization: 0,
  fnirs_hbo_left: 0,
  fnirs_hbo_right: 0,
  fnirs_hbr_left: 0,
  fnirs_hbr_right: 0,
  fnirs_hbt_left: 0,
  fnirs_hbt_right: 0,
  fnirs_connectivity: 0,
  eeg_channel_count: 0,
  eeg_sample_rate_hz: 0,
  has_ppg: false,
  has_imu: false,
  has_central_electrodes: false,
  has_full_montage: false,
});

/** Capabilities of the currently connected (or last connected) device. */
// Capability flags are derived directly from status (pushed from Rust).

let uptimeSec = $state(0);
let uptimeTimer: ReturnType<typeof setInterval> | null = null;
function startUptime() {
  uptimeSec = 0;
  uptimeTimer = setInterval(() => uptimeSec++, 1000);
}
function stopUptime() {
  if (uptimeTimer) {
    clearInterval(uptimeTimer);
    uptimeTimer = null;
  }
  uptimeSec = 0;
}

// Today's total recording time (fetched once, updates with current session uptime)
let todayRecordedSecs = $state(0); // from past sessions today
async function fetchTodayRecording() {
  try {
    const sessions =
      await daemonInvoke<{ session_start_utc: number | null; session_end_utc: number | null }[]>("list_sessions");
    const todayStart = new Date();
    todayStart.setHours(0, 0, 0, 0);
    const cutoff = Math.floor(todayStart.getTime() / 1000);
    let secs = 0;
    for (const s of sessions) {
      if (!s.session_start_utc || !s.session_end_utc) continue;
      if (s.session_end_utc < cutoff) continue;
      const start = Math.max(s.session_start_utc, cutoff);
      secs += s.session_end_utc - start;
    }
    todayRecordedSecs = secs;
  } catch {
    /* ignore */
  }
}
const todayTotalSecs = $derived(todayRecordedSecs + uptimeSec);

// Daily goal (minutes) — persisted in ~/.skill/settings.json via Rust
let dailyGoalMin = $state(60);
let goalNotified = false; // true once notification fired this session

// Load persisted goal + today's notification state from Rust on mount
daemonInvoke<number>("get_daily_goal")
  .then((v) => {
    if (v > 0) dailyGoalMin = v;
  })
  .catch((_e) => {});
daemonInvoke<string>("get_goal_notified_date")
  .then((stored) => {
    const today = new Date().toISOString().slice(0, 10);
    if (stored === today) goalNotified = true;
  })
  .catch((_e) => {});

const goalPct = $derived(Math.min(100, (todayTotalSecs / 60 / dailyGoalMin) * 100));
const goalReached = $derived(goalPct >= 100);

$effect(() => {
  if (goalReached && !goalNotified && status.state === "connected") {
    goalNotified = true;
    const today = new Date().toISOString().slice(0, 10);
    daemonInvoke("set_goal_notified_date", { date: today }).catch((_e) => {});
    try {
      sendNotification({
        title: "🎯 Daily Goal Reached!",
        body: `You've recorded ${Math.floor(todayTotalSecs / 60)} minutes today. Great job!`,
      });
    } catch {
      /* notification not available */
    }
  }
});
function fmtUptime(s: number) {
  return [Math.floor(s / 3600), Math.floor((s % 3600) / 60), s % 60].map((n) => String(n).padStart(2, "0")).join(":");
}

const fmtEeg = (v: number | null | undefined) =>
  v != null && Number.isFinite(v) ? `${(v >= 0 ? "+" : "") + v.toFixed(1)} µV` : "—";

const battColor = (p: number) => colorForLevel(p);

// Capability flags derived from device_kind — hide irrelevant UI for non-Muse devices
const isMuse = $derived(status.device_kind === "muse" || status.device_kind === "unknown");
const isGanglion = $derived(status.device_kind === "ganglion");
const isMw75 = $derived(status.device_kind === "mw75");
const isHermes = $derived(status.device_kind === "hermes");
const isEmotiv = $derived(status.device_kind === "emotiv");
const isIdun = $derived(status.device_kind === "idun");
const isMendi = $derived(status.device_kind === "mendi");
const hasPpg = $derived(status.has_ppg);
const hasImuCap = $derived(status.has_imu);
const hasEeg = $derived((status.eeg_channel_count ?? 0) > 0);
const hasFnirs = $derived((status.fnirs_channel_names?.length ?? 0) > 0 || status.device_kind === "mendi");
const hasBattery = $derived(isMuse || isMw75 || isEmotiv || isIdun || isMendi);
const hasSecondary = $derived(secondarySessions.length > 0);

/** Short transport/source label for the connected device. */
const sourceLabel = $derived.by(() => {
  switch (status.device_kind) {
    case "lsl":
      return "LSL";
    case "lsl-iroh":
      return "LSL · iroh";
    case "iroh-remote":
      return "iroh";
    case "ganglion":
      return "USB";
    case "emotiv":
      return "Cortex";
    case "muse":
    case "mw75":
    case "hermes":
    case "idun":
    case "mendi":
      return "BLE";
    default:
      return null;
  }
});

// Channel labels and colours — dynamic based on connected device.
// Use dynamic channel names from the device when available (handles
// Emotiv Insight 5ch, EPOC 14ch, Flex 32ch, etc.), fall back to static
// constants for known device kinds.
const chLabels = $derived(
  (() => {
    const dynamic = status.channel_names;
    if (dynamic && dynamic.length > 0) return dynamic;
    if (isMw75) return [...MW75_CH];
    if (isHermes) return [...HERMES_CH];
    if (isEmotiv) return [...EMOTIV_CH];
    if (isIdun) return [...IDUN_CH];
    if (isMendi) return [];
    if (isGanglion) return [...GANGLION_CH];
    return [...EEG_CH];
  })(),
);
const chColors = $derived(
  (() => {
    const n = chLabels.length;
    // Pick the static palette closest to the channel count, then slice/extend.
    const palette = isMw75
      ? MW75_COLOR
      : isHermes
        ? HERMES_COLOR
        : isEmotiv
          ? EMOTIV_COLOR
          : isIdun
            ? IDUN_COLOR
            : isGanglion
              ? GANGLION_COLOR
              : EEG_COLOR;
    if (n <= palette.length) return [...palette].slice(0, n);
    // Extend by cycling for devices with more channels than the palette.
    const out = [...palette];
    while (out.length < n) out.push(palette[out.length % palette.length]);
    return out;
  })(),
);

const ppgLabels = $derived(
  (status.ppg_channel_names?.length
    ? status.ppg_channel_names
    : hasPpg
      ? ["Ambient", "Infrared", "Red"]
      : []) as string[],
);
const imuLabels = $derived((status.imu_channel_names ?? []) as string[]);
const fnirsLabels = $derived((status.fnirs_channel_names ?? []) as string[]);
/**
 * Athena = Muse S gen 2.
 * Detected by hardware_version "p50" (arrives a few seconds after connect)
 * OR by device name "MuseS-XXXX" — Athena advertises without a space/hyphen
 * between "Muse" and "S", e.g. "MuseS-F921".
 */
const isAthena = $derived(
  isMuse &&
    (status.hardware_version === "p50" ||
      // "MuseS-F921" → toLowerCase → "muses-f921" → includes "muses" ✓
      (status.device_name?.toLowerCase().includes("muses") ?? false)),
);
/**
 * Classic Muse S (gen 1) — advertises as "Muse S-XXXX" (space-separated).
 * Only shown when it's definitely NOT an Athena.
 */
const isMuseS = $derived(
  isMuse &&
    !isAthena &&
    ((status.device_name?.toLowerCase().includes("muse-s") ?? false) ||
      (status.device_name?.toLowerCase().includes("muse s") ?? false)),
);
/** Muse 2 — advertises as "Muse-2-XXXX" or has hardware_version "p21". */
const isMuse2 = $derived(
  isMuse &&
    !isAthena &&
    !isMuseS &&
    ((status.device_name?.toLowerCase().includes("muse-2") ?? false) ||
      (status.device_name?.toLowerCase().includes("muse 2") ?? false) ||
      status.hardware_version === "p21"),
);
/**
 * Image path for the currently connected device, or null if no matching image
 * is available.  Checked in specificity order so the most precise match wins.
 */
const deviceImage = $derived(
  (() => {
    if (status.state !== "connected") return null;
    if (isAthena) return "/devices/muse-s-athena.jpg";
    if (isMuseS) return "/devices/muse-s-gen1.jpg";
    if (isMuse2) return "/devices/muse-gen2.jpg";
    if (isMuse) return "/devices/muse-gen1.jpg"; // Muse 1 / generic Muse
    if (isGanglion) return "/devices/openbci-ganglion.jpg";
    if (isMw75) return "/devices/muse-mw75.jpg";
    if (isHermes) return "/devices/re-ak-nucleus-hermes.png";
    if (isEmotiv) return "/devices/emotiv-epoc-x.webp";
    if (isIdun) return "/devices/idun-guardian.png";
    if (isMendi) return "/devices/mendi-headband.png";
    return null;
  })(),
);
const deviceImageAlt = $derived(
  (() => {
    if (isAthena) return "Muse S Athena";
    if (isMuseS) return "Muse S";
    if (isMuse2) return "Muse 2";
    if (isMuse) return "Muse";
    if (isGanglion) return "OpenBCI Ganglion";
    if (isMw75) return "MW75 Neuro";
    if (isHermes) return "Nucleus Hermes";
    if (isEmotiv) return "Emotiv";
    if (isIdun) return "IDUN Guardian";
    if (isMendi) return "Mendi";
    return status.device_name ?? "";
  })(),
);
const csvName = (p: string | null) => (p ? (p.split(/[\\/]/).pop() ?? p) : "");

const QUALITY_LABEL_KEY: Record<string, string> = {
  good: "dashboard.qualityGood",
  fair: "dashboard.qualityFair",
  poor: "dashboard.qualityPoor",
  no_signal: "dashboard.qualityNoSignal",
};
const qualityColor = (q: string) => QUALITY_COLORS[q] ?? C_NEUTRAL;
const qualityLabel = (q: string) => t(QUALITY_LABEL_KEY[q] ?? q);

let appVersion = $state("…");

// ── Recent label (shown inline under REC row) ──────────────────────────────
let recentLabel = $state<string | null>(null);
let recentLabelAt = $state(0); // unix seconds

// ── Card collapse state ────────────────────────────────────────────────────
let ppgOpticalExpanded = $state(true);
let imuExpanded = $state(true);
let eegChExpanded = $state(true);

// ── Onboarding checklist ───────────────────────────────────────────────────
// Persisted in localStorage so it survives reloads.
let onboardDone = $state({
  devicePaired: false,
  calibrated: false,
  firstSession: false, // ≥5-min session completed
  goalSet: false,
  llmDownloaded: false, // at least one LLM model downloaded
  searchRun: false, // similarity search executed at least once
  dndConfigured: false, // DND auto-focus threshold enabled
  apiVisited: false, // API status page opened at least once
});

function loadOnboarding() {
  try {
    const raw = localStorage.getItem("onboardDone");
    if (raw) onboardDone = { ...onboardDone, ...JSON.parse(raw) };
  } catch (e) {}
}
function saveOnboarding() {
  localStorage.setItem("onboardDone", JSON.stringify(onboardDone));
}
function checkOnboarding() {
  // device paired
  if (status.state === "connected" || status.paired_devices.length > 0) {
    if (!onboardDone.devicePaired) {
      onboardDone.devicePaired = true;
      saveOnboarding();
    }
  }
  // first session ≥5 min
  if (todayTotalSecs >= 300 && !onboardDone.firstSession) {
    onboardDone.firstSession = true;
    saveOnboarding();
  }
  // goal set
  if (dailyGoalMin > 0 && !onboardDone.goalSet) {
    onboardDone.goalSet = true;
    saveOnboarding();
  }
  // pick up flags written by other pages (search, api)
  try {
    const stored = JSON.parse(localStorage.getItem("onboardDone") ?? "{}");
    let dirty = false;
    if (stored.searchRun && !onboardDone.searchRun) {
      onboardDone.searchRun = true;
      dirty = true;
    }
    if (stored.apiVisited && !onboardDone.apiVisited) {
      onboardDone.apiVisited = true;
      dirty = true;
    }
    if (stored.llmDownloaded && !onboardDone.llmDownloaded) {
      onboardDone.llmDownloaded = true;
      dirty = true;
    }
    if (stored.dndConfigured && !onboardDone.dndConfigured) {
      onboardDone.dndConfigured = true;
      dirty = true;
    }
    if (dirty) saveOnboarding();
  } catch (e) {}
}

/** One-shot async checks run at startup for steps that don't have live events. */
async function checkOnboardingAsync() {
  // LLM: any model already downloaded?
  if (!onboardDone.llmDownloaded) {
    try {
      const catalog = await daemonInvoke<{ entries: { state: string }[] }>("get_llm_catalog");
      if (catalog.entries.some((e) => e.state === "downloaded")) {
        onboardDone.llmDownloaded = true;
        saveOnboarding();
      }
    } catch (e) {}
  }
  // DND: focus-threshold automation enabled by the user?
  if (!onboardDone.dndConfigured) {
    try {
      const cfg = await daemonInvoke<{ enabled: boolean }>("get_dnd_config");
      if (cfg.enabled) {
        onboardDone.dndConfigured = true;
        saveOnboarding();
      }
    } catch (e) {}
  }
}

let onboardComplete = $derived(Object.values(onboardDone).every(Boolean));
let onboardSteps = $derived([
  { key: "devicePaired", label: t("dashboard.setupDevice"), done: onboardDone.devicePaired },
  { key: "calibrated", label: t("dashboard.setupCalibrate"), done: onboardDone.calibrated },
  { key: "firstSession", label: t("dashboard.setupSession"), done: onboardDone.firstSession },
  { key: "goalSet", label: t("dashboard.setupGoal"), done: onboardDone.goalSet },
  { key: "llmDownloaded", label: t("dashboard.setupLlm"), done: onboardDone.llmDownloaded },
  { key: "searchRun", label: t("dashboard.setupSearch"), done: onboardDone.searchRun },
  { key: "dndConfigured", label: t("dashboard.setupDnd"), done: onboardDone.dndConfigured },
  { key: "apiVisited", label: t("dashboard.setupApi"), done: onboardDone.apiVisited },
]);

// Track unpaired device IDs we've already toasted about so we don't spam.
const knownUnpairedIds = new Set<string>();

async function retryConnect() {
  await daemonInvoke("retry_connect");
}
async function cancelRetry() {
  await daemonInvoke("cancel_retry");
}
async function forgetDevice(id: string) {
  status = await daemonInvoke<DeviceStatus>("forget_device", { id });
}
async function connectDevice(id: string) {
  // If currently connected or scanning, cancel first before switching
  if (status.state === "connected" || status.state === "scanning") {
    await daemonInvoke("cancel_retry");
    // Small delay so the backend finishes teardown before starting a new session
    await new Promise((r) => setTimeout(r, 200));
  }
  await daemonInvoke("set_preferred_device", { id });
  await daemonInvoke("retry_connect");
}

// ── Event markers (labels, calibration, search) ────────────────────────────
/** Push a vertical marker to both EEG and PPG charts simultaneously. */
function pushMarkerToBoth(label: string, color: string) {
  const m = { timestamp_ms: Date.now(), label, color };
  chartEl?.pushMarker(m);
  ppgChartEl?.pushMarker(m);
}

// ── Scroll-driven hero collapse ───────────────────────────────────────────
let compact = $state(false);
function handleScroll(e: Event) {
  if (!compact) compact = (e.currentTarget as HTMLElement).scrollTop > 56;
}

let mainEl: HTMLElement | null = null;
let dashboardContentEl: HTMLDivElement | null = null;
let autoHeightRo: ResizeObserver | null = null;
let autoHeightTimer: ReturnType<typeof setTimeout> | null = null;
let lastAutoHeight = -1;
let mainWindowAutoFit = $state(true);

function scheduleAutoHeightFit() {
  if (!mainWindowAutoFit) return;
  if (autoHeightTimer) clearTimeout(autoHeightTimer);
  autoHeightTimer = setTimeout(() => {
    void fitMainWindowHeight();
  }, 120);
}

async function fitMainWindowHeight() {
  if (!mainWindowAutoFit) return;
  if (!dashboardContentEl) return;
  const host = document.getElementById("main-content");
  const hostCs = host ? getComputedStyle(host) : null;
  const hostPad = hostCs
    ? (Number.parseFloat(hostCs.paddingTop) || 0) + (Number.parseFloat(hostCs.paddingBottom) || 0)
    : 0;
  const mainCs = mainEl ? getComputedStyle(mainEl) : null;
  const mainPad = mainCs
    ? (Number.parseFloat(mainCs.paddingTop) || 0) + (Number.parseFloat(mainCs.paddingBottom) || 0)
    : 0;

  const desiredHeight = Math.ceil(dashboardContentEl.scrollHeight + hostPad + mainPad);
  if (!Number.isFinite(desiredHeight)) return;
  if (Math.abs(desiredHeight - lastAutoHeight) < 8) return;
  lastAutoHeight = desiredHeight;

  try {
    await invoke("autosize_main_window", { desiredHeight });
  } catch {
    // non-fatal on unsupported runtimes
  }
}

const unlisteners: UnlistenFn[] = [];
async function refreshStatus() {
  const prev = status.state;
  status = await daemonInvoke<DeviceStatus>("get_status");
  if (prev !== "connected" && status.state === "connected") startUptime();
  if (prev === "connected" && status.state !== "connected") stopUptime();
}

onMount(async () => {
  loadOnboarding();
  // Async checks for onboarding steps that need backend queries
  checkOnboardingAsync();
  // Fetch today's prior recording time
  fetchTodayRecording();

  // Auto-check for updates (daily, if enabled)
  try {
    const autoCheck = localStorage.getItem("autoCheckUpdates") !== "false";
    const lastCheck = Number(localStorage.getItem("lastUpdateCheckUtc") || "0");
    const nowSecs = Math.floor(Date.now() / 1000);
    if (autoCheck && nowSecs - lastCheck >= 86400) {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      localStorage.setItem("lastUpdateCheckUtc", String(nowSecs));
      if (update) {
        // Update available — open the updates tab so user sees it
        await openUpdates();
      }
    }
  } catch {
    /* updater not available in dev / offline */
  }

  // Request notification permission on first launch (macOS shows a one-time dialog).
  try {
    if (!(await isPermissionGranted())) {
      await requestPermission();
    }
  } catch (_) {
    /* notification plugin unavailable or denied — non-fatal */
  }

  // EEG/PPG/IMU streaming via daemon WebSocket
  const { subscribeEeg, subscribePpg, subscribeImu } = await import("$lib/daemon/eeg-stream");
  unlisteners.push(subscribeEeg((pkt) => chartEl?.pushSamples(pkt.electrode, pkt.samples)));
  unlisteners.push(subscribePpg((pkt) => ppgChartEl?.pushSamples(pkt.channel, pkt.samples)));
  unlisteners.push(subscribeImu((pkt) => imuChartEl?.pushPacket(pkt)));

  await refreshStatus();
  appVersion = await invoke<string>("get_app_version");
  mainWindowAutoFit = await daemonInvoke<boolean>("get_main_window_auto_fit").catch(() => true);

  // Auto-fit window height to dashboard content as cards expand/collapse.
  autoHeightRo = new ResizeObserver(() => scheduleAutoHeightFit());
  if (dashboardContentEl) autoHeightRo.observe(dashboardContentEl);
  if (mainEl) autoHeightRo.observe(mainEl);
  window.addEventListener("resize", scheduleAutoHeightFit);
  unlisteners.push(() => window.removeEventListener("resize", scheduleAutoHeightFit));
  scheduleAutoHeightFit();

  // Poll model download status every 2 s until the encoder is loaded.
  await refreshModelDl();
  if (!modelDl.encoder_loaded) {
    modelDlTimer = setInterval(refreshModelDl, 2000);
  }

  // Seed the band chart with whatever the backend already has (may be null
  // on a fresh start or before the 2-second warmup period completes).
  const { getLatestBands, subscribeBands } = await import("$lib/daemon/eeg-stream");
  const latestBands = await getLatestBands();
  if (latestBands) {
    bandChartEl?.update(latestBands);
    updateScores(latestBands);
  }

  unlisteners.push(
    await listen<number>("daily-goal-changed", (ev) => {
      dailyGoalMin = ev.payload;
    }),
  );

  unlisteners.push(
    await listen<boolean>("main-window-auto-fit-changed", (ev) => {
      mainWindowAutoFit = ev.payload;
      if (mainWindowAutoFit) scheduleAutoHeightFit();
    }),
  );

  unlisteners.push(
    await listen<DeviceStatus>("status", (ev) => {
      const prev = status.state;
      status = ev.payload;
      if ((status.fnirs_channel_names?.length ?? 0) > 0) {
        fnirsChartEl?.pushMetrics({
          hbo:
            (((status.fnirs_hbo_left as number | undefined) ?? 0) +
              ((status.fnirs_hbo_right as number | undefined) ?? 0)) /
            2,
          hbr:
            (((status.fnirs_hbr_left as number | undefined) ?? 0) +
              ((status.fnirs_hbr_right as number | undefined) ?? 0)) /
            2,
          hbt:
            (((status.fnirs_hbt_left as number | undefined) ?? 0) +
              ((status.fnirs_hbt_right as number | undefined) ?? 0)) /
            2,
          workload: (status.fnirs_workload as number | undefined) ?? 0,
          oxygenation: (status.fnirs_oxygenation_pct as number | undefined) ?? 0,
        });
      }
      if (prev !== "connected" && status.state === "connected") startUptime();
      if (prev === "connected" && status.state !== "connected") {
        stopUptime();
        showDeviceSwitcher = false;
      }
    }),
  );

  // Secondary (background) sessions
  try {
    secondarySessions = await daemonInvoke<SecondarySession[]>("list_secondary_sessions");
  } catch {
    /* ignore — command may not exist in older builds */
  }
  unlisteners.push(
    await listen<SecondarySession[]>("secondary-sessions", (ev) => {
      secondarySessions = ev.payload;
    }),
  );

  // When the background scanner discovers a new unpaired device, let the user
  // know so they can go to Settings → Devices and pair it.
  unlisteners.push(
    await listen<DiscoveredDevice[]>("devices-updated", (ev) => {
      const unpaired = ev.payload.filter((d) => !d.is_paired && d.last_rssi !== 0);
      for (const dev of unpaired) {
        if (!knownUnpairedIds.has(dev.id)) {
          knownUnpairedIds.add(dev.id);
          addToast("info", t("settings.newDeviceNotice"), `${dev.name} — ${t("settings.newDeviceNoticeHint")}`, 8_000);
        }
      }
    }),
  );

  // Spectrogram columns: 8 Hz, one column per filter hop (HOP=32 @ 256 Hz).
  // Forwarded directly to EegChart which writes them into the tape canvases.
  unlisteners.push(
    await listen<SpectrogramColumn>("eeg-spectrogram", (ev) => {
      chartEl?.pushSpecColumn(ev.payload);
    }),
  );

  // Live band power updates (4 Hz, via daemon WebSocket).
  unlisteners.push(
    subscribeBands((snap) => {
      bandChartEl?.update(snap);
      updateScores(snap);
    }),
  );

  // BLE device disconnect — fired from the btleplug adapter event stream
  // for immediate visibility even before the muse_rs notification stream
  // closes.  Triggers an immediate status refresh so the UI reacts fast.
  unlisteners.push(
    await listen<{ device_name: string; device_id: string; reason: string }>("device-disconnected", (_ev) => {
      refreshStatus();
    }),
  );

  // BLE device connected.
  unlisteners.push(
    await listen<{ device_name: string; device_id: string }>("device-connected", (_ev) => {
      refreshStatus();
    }),
  );

  // ── Event markers on EEG/PPG charts ──────────────────────────────────────

  // Calibration phase boundaries
  unlisteners.push(
    await listen<{ action: string; iteration: number }>("calibration-action", (ev) => {
      pushMarkerToBoth(ev.payload.action, "#f59e0b"); // amber
    }),
  );
  unlisteners.push(
    await listen<{ iteration: number }>("calibration-break", () => {
      pushMarkerToBoth("Break", "#94a3b8"); // slate
    }),
  );
  unlisteners.push(
    await listen("calibration-completed", () => {
      pushMarkerToBoth("✓ Done", "#22c55e"); // green
      if (!onboardDone.calibrated) {
        onboardDone.calibrated = true;
        saveOnboarding();
      }
    }),
  );
  unlisteners.push(
    await listen("calibration-cancelled", () => {
      pushMarkerToBoth("✗ Cancel", "#ef4444"); // red
    }),
  );

  // Label submissions — the label window calls submit_label; we listen
  // for a custom event emitted when a label is saved.
  unlisteners.push(
    await listen<{ text: string }>("label-created", (ev) => {
      pushMarkerToBoth(`🏷 ${ev.payload.text}`, "#a78bfa"); // violet
      recentLabel = ev.payload.text;
      recentLabelAt = Math.floor(Date.now() / 1000);
    }),
  );

  // Search result hits — emitted when the search window highlights results.
  unlisteners.push(
    await listen<{ query: string }>("search-hit", (ev) => {
      pushMarkerToBoth(`🔍 ${ev.payload.query ?? "search"}`, "#38bdf8"); // sky
    }),
  );

  // LLM model becomes available (server running = model downloaded & loaded).
  unlisteners.push(
    await listen<{ status: string }>("llm:status", (ev) => {
      if (ev.payload.status === "running" && !onboardDone.llmDownloaded) {
        onboardDone.llmDownloaded = true;
        saveOnboarding();
      }
    }),
  );

  const onVisible = () => {
    if (!document.hidden) {
      refreshStatus();
      // Restart canvas render loops in case they died while the window was hidden
      // (e.g. after wake-from-sleep or an unhandled exception during a frame).
      chartEl?.restartRender();
      bandChartEl?.restartRender();
    }
  };
  document.addEventListener("visibilitychange", onVisible);
  unlisteners.push(() => document.removeEventListener("visibilitychange", onVisible));
});

onDestroy(() => {
  // biome-ignore lint/suspicious/useIterableCallbackReturn: unlisten fns return void-Promise, not a value
  unlisteners.forEach((u) => u());
  stopUptime();
  if (modelDlTimer) {
    clearInterval(modelDlTimer);
    modelDlTimer = null;
  }
  autoHeightRo?.disconnect();
  autoHeightRo = null;
  if (autoHeightTimer) {
    clearTimeout(autoHeightTimer);
    autoHeightTimer = null;
  }
});

// Keep onboarding state up-to-date as status/times change.
$effect(() => {
  checkOnboarding();
});

const sc = $derived(STATE_COLORS[status.state]);

// Keep the shared BT-off store in sync so the titlebar can react.
$effect(() => {
  setBtOff(status.state === "bt_off");
});

useWindowTitle("window.title.main");
</script>

<main bind:this={mainEl} class="h-full min-h-0 overflow-y-auto p-2 flex flex-col items-center" onscroll={handleScroll}
      aria-label="Dashboard">
  <div bind:this={dashboardContentEl} class="w-full flex flex-col items-center">
  <!-- GPU utilisation chart — always visible when GPU stats are available -->
  <div class="w-full max-w-[1200px]">
    <GpuChart />
  </div>

  <!-- ── Connected iroh client banner ──────────────────────────────────── -->
  {#if status.iroh_client_name && status.state === "connected"}
    {@const pi = status.phone_info}
    <div class="w-full max-w-[1200px] mb-1">
      <div class="flex items-center gap-2.5 rounded-xl
                  border border-indigo-400/30 bg-indigo-50/70 dark:bg-indigo-950/20
                  px-3 py-2">
        <!-- iroh icon -->
        <div class="flex items-center justify-center w-7 h-7 rounded-lg shrink-0
                    bg-gradient-to-br from-indigo-500 to-violet-500
                    shadow-sm shadow-indigo-500/20">
          <svg viewBox="0 0 24 24" fill="none" stroke="white"
               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
               class="w-3.5 h-3.5">
            <path d="M12 2L2 7l10 5 10-5-10-5z"/>
            <path d="M2 17l10 5 10-5"/>
            <path d="M2 12l10 5 10-5"/>
          </svg>
        </div>

        <div class="flex flex-col gap-0 flex-1 min-w-0">
          <div class="flex items-center gap-1.5">
            <span class="text-[0.7rem] font-semibold text-indigo-800 dark:text-indigo-300 truncate">
              {status.iroh_client_name}
            </span>
            <span class="relative flex h-1.5 w-1.5 shrink-0">
              <span class="absolute inline-flex h-full w-full rounded-full bg-green-500 opacity-75 animate-ping"></span>
              <span class="relative inline-flex rounded-full h-1.5 w-1.5 bg-green-500"></span>
            </span>
          </div>
          <span class="text-[0.54rem] text-indigo-600/60 dark:text-indigo-400/50 truncate">
            {#if pi?.phone_model}
              {pi.phone_model}{#if pi?.os_version} · {pi.os} {pi.os_version}{/if}{#if pi?.app_version} · v{pi.app_version}{/if}
            {:else}
              iroh remote client
            {/if}
          </span>
        </div>

        {#if pi?.battery_level != null && (pi?.battery_level ?? 0) > 0}
          <div class="flex items-center gap-1 shrink-0">
            <span class="text-[0.54rem] text-indigo-600/50 dark:text-indigo-400/40">📱</span>
            <span class="text-[0.58rem] font-semibold tabular-nums text-indigo-700/70 dark:text-indigo-300/60">
              {Math.round((pi?.battery_level ?? 0) * 100)}%
            </span>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- ── ZUNA model download / retry progress banner ─────────────────────── -->
  {#if modelDlVisible}
    <div class="w-full max-w-[1200px] mb-1">
      {#if modelDl.downloading_weights}
        <!-- Downloading -->
        <div class="flex items-center gap-2.5 rounded-xl
                    border border-blue-400/30 bg-blue-50/80 dark:bg-blue-950/25
                    px-3 py-2.5">
          <span class="w-2 h-2 rounded-full bg-blue-500 animate-pulse shrink-0"></span>
          <div class="flex flex-col gap-0 flex-1 min-w-0">
            <span class="text-[0.68rem] font-semibold text-blue-700 dark:text-blue-300 leading-tight">
              {t("model.downloading")}
            </span>
            {#if modelDl.download_status_msg}
              <span class="text-[0.58rem] text-blue-600/70 dark:text-blue-400/70 truncate">
                {modelDl.download_status_msg}
              </span>
            {/if}
          </div>
          <button onclick={() => invoke("open_model_tab")}
                  aria-label={t("settingsTabs.eegModel")}
                  class="shrink-0 text-[0.6rem] font-semibold text-blue-600 dark:text-blue-400
                         hover:text-blue-800 dark:hover:text-blue-200 transition-colors">
            {t("settingsTabs.eegModel")} ↗
          </button>
        </div>
      {:else if modelDl.download_retry_in_secs > 0}
        <!-- Auto-retrying with countdown -->
        <div class="flex items-center gap-2.5 rounded-xl
                    border border-amber-400/30 bg-amber-50/80 dark:bg-amber-950/25
                    px-3 py-2.5">
          <span class="w-2 h-2 rounded-full bg-amber-500 shrink-0"></span>
          <div class="flex flex-col gap-0 flex-1 min-w-0">
            <span class="text-[0.68rem] font-semibold text-amber-700 dark:text-amber-300 leading-tight">
              {t("model.autoRetryIn", { secs: String(modelDl.download_retry_in_secs) })}
              <span class="font-normal opacity-70">
                · {t("model.autoRetryAttempt", { n: String(modelDl.download_retry_attempt + 1) })}
              </span>
            </span>
            {#if modelDl.download_status_msg && modelDl.download_status_msg !== "Download cancelled."}
              <span class="text-[0.58rem] text-amber-600/70 dark:text-amber-400/70 truncate">
                {modelDl.download_status_msg}
              </span>
            {/if}
          </div>
          <button onclick={() => invoke("open_model_tab")}
                  aria-label={t("settingsTabs.eegModel")}
                  class="shrink-0 text-[0.6rem] font-semibold text-amber-600 dark:text-amber-400
                         hover:text-amber-800 dark:hover:text-amber-200 transition-colors">
            {t("settingsTabs.eegModel")} ↗
          </button>
        </div>
      {/if}
    </div>
  {/if}

  <Card class="w-full max-w-[1200px] gap-0 py-0
               border-border dark:border-white/[0.06]
               bg-white dark:bg-[#14141e]">

    <!-- ── Header ──────────────────────────────────────────────────────────── -->
    <CardHeader class="relative overflow-hidden px-4 transition-[padding] duration-300
                       {compact ? 'py-2' : 'pt-5 pb-3'}">

      {#if compact}
        <!-- ── Compact one-liner ─────────────────────────────────────────────
             All items in one flex row so vertical centering is automatic.
             Buttons live at the right end — no absolute positioning needed. -->
        <div class="flex items-center gap-2 w-full" transition:fade={{ duration: 150 }}>
          <div class="status-ring-sm shrink-0" style="--rc:{sc.ring}">
            <div class="status-dot-sm" style="background:{sc.ring}"></div>
          </div>

          <Badge
            variant="outline"
            class="text-[0.55rem] font-semibold tracking-widest uppercase px-2 py-0 rounded-full shrink-0"
            style="background:{sc.badge}; color:{sc.text}; border-color:{sc.border}"
          >
            {#if status.state === "scanning"}{t("dashboard.scanning")}
            {:else if status.state === "connected"}● {t("dashboard.connected")}
            {:else if status.state === "bt_off"}⚠ {t("dashboard.btOff")}
            {:else}{t("dashboard.disconnected")}{/if}
          </Badge>
          {#if deviceImage}
            <img
              src={deviceImage}
              alt={deviceImageAlt}
              class="h-5 w-auto rounded object-cover shrink-0
                     border border-border dark:border-white/[0.06]"
            />
          {/if}
          {#if status.device_name && status.state === "connected"}
            <span class="text-[0.65rem] text-muted-foreground truncate min-w-0 flex-1">
              {status.device_name}
              {#if sourceLabel}
                <span class="ml-1 text-[0.48rem] font-bold tracking-widest uppercase px-1 py-0.5
                             rounded bg-foreground/[0.06] dark:bg-white/[0.06] text-muted-foreground/60">{sourceLabel}</span>
              {/if}
              {#if hasSecondary}
                <span class="ml-0.5 text-[0.44rem] font-bold tracking-widest uppercase px-1 py-0.5
                             rounded bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">{t("dashboard.primary")}</span>
                <span class="text-[0.48rem] text-violet-500/60">+{secondarySessions.length}</span>
              {/if}
            </span>
          {:else}
            <span class="flex-1"></span>
          {/if}
          <!-- Chevron button for expand (label/history moved to titlebar) -->
          <div class="flex items-center gap-0.5 shrink-0">
            <button onclick={() => compact = false} title={t("common.expand")}
              class="flex items-center justify-center w-6 h-6 rounded-md
                     text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                <path d="M6 9l6 6 6-6"/>
              </svg>
            </button>
          </div>
        </div>

      {:else}
        <!-- Expanded: chevron button outermost (theme/language moved to titlebar) -->
        <div class="absolute top-2 right-2 z-10 flex items-center gap-0.5">
          <!-- Chevron — always outermost -->
          <button onclick={() => compact = true} title={t("common.minimise")}
            class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground
                   hover:text-foreground hover:bg-accent transition-colors">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                 stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                 class="w-3 h-3 rotate-180">
              <path d="M6 9l6 6 6-6"/>
            </svg>
          </button>
        </div>

        <!-- ── Expanded vertical hero ─────────────────────────────────────── -->
        <div class="flex flex-col items-center gap-2.5 w-full" transition:fade={{ duration: 150 }}>

          <!-- Label and history buttons moved to titlebar -->

          <!-- Status ring -->
          <div class="status-ring bg-slate-100 dark:bg-[#1a1a28]" style="--rc:{sc.ring}">
            <div class="status-dot" style="background:{sc.ring}"></div>
          </div>



          <Badge
            variant="outline"
            class="text-[0.65rem] font-semibold tracking-widest uppercase px-3 py-0.5 rounded-full transition-all"
            style="background:{sc.badge}; color:{sc.text}; border-color:{sc.border}"
          >
            {#if status.state === "scanning"}
              {t("dashboard.scanning")}
            {:else if status.state === "connected"}
              ● {t("dashboard.connected")}
            {:else if status.state === "bt_off"}
              ⚠ {t("dashboard.bluetoothUnavailable")}
            {:else}
              {t("dashboard.disconnected")}
            {/if}
          </Badge>

          {#if deviceImage}
            <img
              src={deviceImage}
              alt={deviceImageAlt}
              class="max-h-[72px] w-auto rounded-xl object-cover
                     border border-border dark:border-white/[0.06] shadow-sm"
            />
          {/if}

          {#if status.device_name && status.state === "connected"}
            <p class="text-[0.73rem] text-muted-foreground font-medium -mt-1">
              {status.device_name}
              {#if sourceLabel}
                <span class="ml-1.5 text-[0.5rem] font-bold tracking-widest uppercase px-1.5 py-0.5
                             rounded bg-foreground/[0.06] dark:bg-white/[0.06] text-muted-foreground/60">{sourceLabel}</span>
              {/if}
              {#if hasSecondary}
                <span class="ml-1 text-[0.46rem] font-bold tracking-widest uppercase px-1.5 py-0.5
                             rounded bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">{t("dashboard.primary")}</span>
              {/if}
            </p>
            {#if status.serial_number || status.mac_address}
              <div class="flex flex-wrap justify-center gap-x-3 gap-y-0.5 -mt-0.5">
                {#if status.serial_number}
                  <button
                    onclick={() => revealSN = !revealSN}
                    title={revealSN ? t("common.clickToHide") : t("common.clickToReveal")}
                    class="font-mono text-[0.6rem] text-muted-foreground/70 hover:text-muted-foreground
                           cursor-pointer select-none transition-colors">
                    SN&nbsp;{revealSN ? status.serial_number : redact(status.serial_number)}
                  </button>
                {/if}
                {#if status.mac_address}
                  <button
                    onclick={() => revealMAC = !revealMAC}
                    title={revealMAC ? t("common.clickToHide") : t("common.clickToReveal")}
                    class="font-mono text-[0.6rem] text-muted-foreground/70 hover:text-muted-foreground
                           cursor-pointer select-none transition-colors">
                    {revealMAC ? status.mac_address : redact(status.mac_address)}
                  </button>
                {/if}
              </div>
            {/if}

            <!-- Switch device (inline, right under device info) -->
            {#if status.paired_devices.length > 1}
              {#if showDeviceSwitcher}
                <div class="w-full max-w-[280px] flex flex-col gap-1.5 mt-0.5" transition:fade={{ duration: 120 }}>
                  {#each status.paired_devices.filter(d => d.id !== status.device_id) as dev}
                    <button
                      onclick={() => { showDeviceSwitcher = false; connectDevice(dev.id); }}
                      class="flex items-center justify-between gap-2 rounded-lg
                             border border-border dark:border-white/[0.06]
                             bg-muted/60 dark:bg-[#1a1a28] px-3 py-1.5
                             hover:border-primary/40 hover:bg-primary/5
                             transition-colors group">
                      <span class="text-[0.65rem] font-medium text-foreground/70 group-hover:text-foreground truncate">
                        {dev.name}
                      </span>
                      <span class="text-[0.52rem] font-semibold text-primary/70 group-hover:text-primary shrink-0">
                        {t("dashboard.switchTo")}
                      </span>
                    </button>
                  {/each}
                  <button onclick={() => showDeviceSwitcher = false}
                          class="text-[0.5rem] text-muted-foreground/40 hover:text-muted-foreground/70
                                 transition-colors self-center mt-0.5">
                    {t("common.cancel")}
                  </button>
                </div>
              {:else}
                <button
                  onclick={() => showDeviceSwitcher = true}
                  class="text-[0.55rem] text-muted-foreground/50 hover:text-primary/80
                         transition-colors mt-0.5 flex items-center gap-1">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                       stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                       class="w-2.5 h-2.5">
                    <polyline points="16 3 21 3 21 8"/><line x1="4" y1="20" x2="21" y2="3"/>
                    <polyline points="21 16 21 21 16 21"/><line x1="15" y1="15" x2="21" y2="21"/>
                  </svg>
                  {t("dashboard.switchDevice")}
                </button>
              {/if}
            {/if}

            <!-- Disconnect button -->
            <button
              onclick={cancelRetry}
              class="text-[0.55rem] text-muted-foreground/50 hover:text-destructive
                     transition-colors mt-0.5 flex items-center gap-1">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                   class="w-2.5 h-2.5">
                <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
              {t("common.disconnect")}
            </button>
          {/if}
        </div>
      {/if}
    </CardHeader>

    <Separator class="bg-border dark:bg-white/[0.06]" />

    <!-- ── Main content ─────────────────────────────────────────────────────── -->
    <CardContent class="flex flex-col gap-3 px-4 py-3">

      <!-- ════ BT OFF ═══════════════════════════════════════════════════════ -->
      {#if status.state === "bt_off"}
        <div class="flex flex-col items-center gap-3 py-5 px-4 rounded-2xl
                    border border-red-200/60 dark:border-red-400/20
                    bg-red-50/70 dark:bg-red-950/20">

          <!-- Bluetooth icon with slash overlay -->
          <div class="relative w-12 h-12">
            <div class="w-12 h-12 rounded-full flex items-center justify-center
                        bg-red-100 dark:bg-red-900/40 text-red-500 dark:text-red-400">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                   class="w-6 h-6">
                <polyline points="6.5 6.5 17.5 17.5 12 23 12 1 17.5 6.5 6.5 17.5"/>
              </svg>
            </div>
            <!-- diagonal slash -->
            <svg viewBox="0 0 48 48" class="absolute inset-0 w-12 h-12
                                            text-red-500 dark:text-red-400"
                 stroke="currentColor" stroke-width="3.5" stroke-linecap="round">
              <line x1="38" y1="10" x2="10" y2="38"/>
            </svg>
          </div>

          <div class="flex flex-col items-center gap-1 text-center">
            <p class="text-[0.85rem] font-semibold text-red-700 dark:text-red-400">
              {t("dashboard.bluetoothIsOff")}
            </p>
            <p class="text-[0.72rem] text-muted-foreground leading-relaxed max-w-[220px]">
              {t("dashboard.turnOnBluetooth")}
            </p>
          </div>

          <div class="flex gap-2">
            <Button size="sm" onclick={retryConnect}>{t("common.retry")}</Button>
            <Button size="sm" variant="outline" onclick={openBtSettings}>
              {t("dashboard.openSettings")}
            </Button>
          </div>
        </div>

      <!-- ════ CONNECTION ERROR ══════════════════════════════════════════════ -->
      {:else if status.device_error && status.state === "disconnected"}
        {#if status.device_error === "NO_MUSE_NEARBY"}
          <div class="rounded-xl border border-amber-400/30 bg-amber-50 dark:bg-amber-950/20 p-3.5 flex flex-col gap-3">
            <p class="text-[0.8rem] font-semibold text-amber-800 dark:text-amber-300">
              {t("dashboard.noMuseNearbyTitle")}
            </p>
            <ul class="flex flex-col gap-1 text-[0.7rem] text-muted-foreground leading-relaxed pl-1">
              <li>• {t("dashboard.noMuseNearbyHint1")}</li>
              <li>• {t("dashboard.noMuseNearbyHint2")}</li>
              <li>• {t("dashboard.noMuseNearbyHint3")}</li>
            </ul>
            <div class="flex gap-2">
              <Button size="sm" onclick={retryConnect}>{t("common.retry")}</Button>
              <Button size="sm" variant="outline" onclick={openBtSettings}>{t("dashboard.openSettings")}</Button>
            </div>
          </div>
        {:else if status.device_error.includes("EEG stream not available") || status.device_error.includes("-32230")}
          <div class="rounded-xl border border-violet-400/30 bg-violet-50 dark:bg-violet-950/20 p-3.5 flex flex-col gap-3">
            <p class="text-[0.8rem] font-semibold text-violet-800 dark:text-violet-300">
              EEG Access Not Available
            </p>
            <p class="text-[0.7rem] text-muted-foreground leading-relaxed">
              Your Emotiv Cortex App does not have raw EEG data access enabled for this headset.
              To stream EEG, enable the <strong>Raw EEG</strong> data stream in your Cortex App settings.
            </p>
            <div class="flex gap-2 flex-wrap">
              <Button size="sm" onclick={() => { import("@tauri-apps/plugin-opener").then(m => m.openUrl("https://www.emotiv.com/my-account/cortex-apps/")); }}>
                Manage Emotiv Account
              </Button>
              <Button size="sm" variant="outline" onclick={retryConnect}>{t("common.retry")}</Button>
            </div>
          </div>
        {:else}
          <div class="rounded-xl border border-red-400/30 bg-red-50 dark:bg-[#1a0a0a] p-3.5 flex flex-col gap-3">
            <pre class="font-mono text-[0.67rem] text-red-600 dark:text-red-400 leading-relaxed whitespace-pre-wrap">{status.device_error}</pre>
            <div class="flex gap-2">
              <Button size="sm" onclick={retryConnect}>{t("common.retry")}</Button>
              <Button size="sm" variant="outline" onclick={openBtSettings}>{t("dashboard.openSettings")}</Button>
            </div>
          </div>
        {/if}

      <!-- ════ SCANNING / RETRY COUNTDOWN ════════════════════════════════ -->
      {:else if status.state === "scanning"}
        <div class="flex flex-col items-center gap-3 py-3">
          {#if status.retry_countdown_secs > 0}
            <!-- Retry countdown -->
            <div class="relative w-12 h-12 flex items-center justify-center">
              <svg class="absolute inset-0 w-12 h-12 -rotate-90" viewBox="0 0 48 48">
                <circle cx="24" cy="24" r="20" fill="none" stroke="currentColor"
                  stroke-width="2" opacity="0.1" />
                <circle cx="24" cy="24" r="20" fill="none"
                  stroke-width="2.5" stroke-linecap="round"
                  class="text-amber-500 dark:text-amber-400"
                  stroke="currentColor"
                  stroke-dasharray="{2 * Math.PI * 20}"
                  stroke-dashoffset="{2 * Math.PI * 20 * (1 - status.retry_countdown_secs / Math.max(status.retry_countdown_secs, 3))}"
                  style="transition: stroke-dashoffset 1s linear" />
              </svg>
              <span class="text-[1rem] font-bold tabular-nums text-amber-600 dark:text-amber-400">
                {status.retry_countdown_secs}
              </span>
            </div>
            <p class="text-[0.73rem] text-muted-foreground text-center leading-relaxed">
              {t("dashboard.retryCountdown", { secs: String(status.retry_countdown_secs) })}
            </p>
            {#if status.retry_attempt > 0}
              <p class="text-[0.55rem] text-muted-foreground/50 text-center">
                {t("dashboard.retryAttempt", { n: String(status.retry_attempt) })}
              </p>
            {/if}
            <div class="flex gap-2">
              <Button size="sm" onclick={retryConnect}>{t("dashboard.retryNow")}</Button>
              <Button size="sm" variant="outline" onclick={cancelRetry}>{t("common.cancel")}</Button>
            </div>
          {:else}
            <!-- Normal scanning spinner -->
            <Spinner size="w-6 h-6" class="text-yellow-500 dark:text-yellow-400" />
            <p class="text-[0.73rem] text-muted-foreground text-center leading-relaxed">
              {status.target_name ? t("dashboard.connectingTo", { name: status.target_name }) : isGanglion ? t("dashboard.lookingForGanglion") : isEmotiv ? t("dashboard.connectingEmotiv") : isMw75 ? t("dashboard.connectingTo", { name: "MW75 Neuro" }) : isHermes ? t("dashboard.connectingTo", { name: "Hermes" }) : isIdun ? t("dashboard.connectingTo", { name: "IDUN Guardian" }) : isMendi ? t("dashboard.connectingTo", { name: "Mendi" }) : t("dashboard.lookingForMuse")}
            </p>
            <Button size="sm" variant="outline" onclick={cancelRetry}>{t("common.cancel")}</Button>

            <!-- Switch to a different paired device while scanning -->
            {#if status.paired_devices.length > 1 || (status.paired_devices.length > 0 && !status.target_name)}
              <div class="w-full mt-1">
                <p class="text-[0.52rem] font-semibold tracking-widest uppercase text-muted-foreground mb-1.5 text-center">
                  {t("dashboard.connectDifferent")}
                </p>
                <div class="flex flex-col gap-1">
                  {#each status.paired_devices.filter(d => d.name !== status.target_name) as dev}
                    <button
                      onclick={() => connectDevice(dev.id)}
                      class="flex items-center justify-between gap-2 rounded-lg
                             border border-border dark:border-white/[0.06]
                             bg-muted dark:bg-[#1a1a28] px-3 py-1.5
                             hover:border-primary/40 hover:bg-primary/5
                             transition-colors group">
                      <span class="text-[0.68rem] font-medium text-foreground/70 group-hover:text-foreground truncate">
                        {dev.name}
                      </span>
                      <span class="text-[0.52rem] font-semibold text-primary/70 group-hover:text-primary shrink-0">
                        {t("common.connect")}
                      </span>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          {/if}
        </div>

      <!-- ════ CONNECTED ════════════════════════════════════════════════════ -->
      {:else if status.state === "connected"}

        <div class="grid grid-cols-1 xl:grid-cols-2 gap-3 auto-rows-min">

        <!-- Battery (devices with battery reporting) -->
        {#if hasBattery}
        <div class="flex items-center gap-2.5" role="meter" aria-label={t("dashboard.battery")}
             aria-valuenow={status.battery ?? 0} aria-valuemin={0} aria-valuemax={100}>
          <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">{t("dashboard.battery")}</span>
          <div class="flex-1 h-1.5 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden" aria-hidden="true">
            <div class="h-full rounded-full transition-all duration-500"
              style="width:{status.battery}%; background:{battColor(status.battery)}"></div>
          </div>
          <span class="text-[0.56rem] font-semibold text-muted-foreground tabular-nums w-8 text-right">
            {(status.battery ?? 0).toFixed(0)}%
          </span>
          {#if status.temperature_raw > 0}
            <span class="text-[0.42rem] text-muted-foreground/50 tabular-nums" title={t("dashboard.temperature")}>
              🌡 {status.temperature_raw}
            </span>
          {/if}
        </div>
        {/if}

        <!-- Device info badge (non-Muse devices) -->
        {#if isGanglion}
          {@const sr = (status.eeg_sample_rate_hz ?? 0) > 0 ? Math.round(status.eeg_sample_rate_hz ?? 0) : 200}
          <div class="flex items-center gap-1.5 px-2 py-1 rounded-lg
                      bg-emerald-500/10 border border-emerald-500/20 w-fit">
            <span class="text-[0.55rem] font-semibold text-emerald-600 dark:text-emerald-400 tracking-wide">
              OpenBCI Ganglion · {chLabels.length}ch · {sr} Hz
            </span>
          </div>
        {:else if isEmotiv}
          {@const sr = (status.eeg_sample_rate_hz ?? 0) > 0 ? Math.round(status.eeg_sample_rate_hz ?? 0) : null}
          <div class="flex items-center gap-1.5 px-2 py-1 rounded-lg
                      bg-violet-500/10 border border-violet-500/20 w-fit">
            <span class="text-[0.55rem] font-semibold text-violet-600 dark:text-violet-400 tracking-wide">
              {status.device_name ?? "Emotiv"} · {chLabels.length}ch{#if sr} · {sr} Hz{/if}
            </span>
          </div>
        {:else if isIdun}
          {@const sr = (status.eeg_sample_rate_hz ?? 0) > 0 ? Math.round(status.eeg_sample_rate_hz ?? 0) : 250}
          <div class="flex items-center gap-1.5 px-2 py-1 rounded-lg
                      bg-cyan-500/10 border border-cyan-500/20 w-fit">
            <span class="text-[0.55rem] font-semibold text-cyan-600 dark:text-cyan-400 tracking-wide">
              IDUN Guardian · {chLabels.length}ch · {sr} Hz
            </span>
          </div>
        {:else if isMendi}
          <div class="flex items-center gap-1.5 px-2 py-1 rounded-lg
                      bg-fuchsia-500/10 border border-fuchsia-500/20 w-fit">
            <span class="text-[0.55rem] font-semibold text-fuchsia-600 dark:text-fuchsia-400 tracking-wide">
              Mendi · fNIRS + IMU
            </span>
          </div>
        {:else if isHermes}
          {@const sr = (status.eeg_sample_rate_hz ?? 0) > 0 ? Math.round(status.eeg_sample_rate_hz ?? 0) : 250}
          <div class="flex items-center gap-1.5 px-2 py-1 rounded-lg
                      bg-amber-500/10 border border-amber-500/20 w-fit">
            <span class="text-[0.55rem] font-semibold text-amber-600 dark:text-amber-400 tracking-wide">
              Nucleus Hermes · {chLabels.length}ch · {sr} Hz
            </span>
          </div>
        {/if}

        {#if hasEeg}
          <!-- Signal quality row -->
          <div class="rounded-xl border border-border dark:border-white/[0.04]
                      bg-muted dark:bg-[#1a1a28] px-3 py-2.5"
              role="group" aria-label={t("dashboard.signal")}>
            <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground block mb-1.5">
              {t("dashboard.signal")}
            </span>
            <div class="grid gap-1.5" class:grid-cols-2={chLabels.length <= 4} class:grid-cols-3={chLabels.length > 4 && chLabels.length <= 6} class:grid-cols-4={chLabels.length > 6}>
              {#each chLabels as ch, i}
                {@const q = status.channel_quality[i] ?? 'no_signal'}
                <div class="flex items-center gap-1.5">
                  <svg width="8" height="8" viewBox="0 0 8 8" class="shrink-0">
                    <circle cx="4" cy="4" r="4" fill={qualityColor(q)}>
                      {#if q === 'fair' || q === 'poor'}
                        <animate attributeName="opacity" values="1;0.4;1" dur="1.6s" repeatCount="indefinite"/>
                      {/if}
                    </circle>
                  </svg>
                  <span class="text-[0.58rem] font-semibold text-muted-foreground">{ch}</span>
                  <span class="text-[0.52rem] text-muted-foreground/60 leading-none"
                        style="color:{qualityColor(q)}">{qualityLabel(q)}</span>
                </div>
              {/each}
            </div>
          </div>

          <!-- Electrode placement toggle -->
          <button
            onclick={() => showElectrodes = !showElectrodes}
            class="flex items-center gap-1.5 text-[0.52rem] font-medium text-muted-foreground/60
                  hover:text-muted-foreground transition-colors -mt-0.5"
            aria-expanded={showElectrodes}
            aria-controls="electrode-guide">
            <span class="transition-transform {showElectrodes ? 'rotate-90' : ''}"
                  style="display:inline-block">▸</span>
            {t("electrode.title")}
          </button>

          {#if showElectrodes}
            <div id="electrode-guide" class="-mt-0.5 xl:col-span-2">
              <ElectrodeGuide qualityLabels={status.channel_quality} device={status.device_kind} channelNames={chLabels} deviceName={status.device_name ?? ""} />
            </div>
          {/if}

          <!-- EXG electrode-placement disclaimer -->
          <div class="rounded-lg border border-blue-500/20 dark:border-blue-400/15
                      bg-blue-50/60 dark:bg-blue-500/[0.07]
                      px-3 py-2 flex gap-2 items-start xl:col-span-2">
            <span class="text-blue-500 dark:text-blue-400 shrink-0 text-[0.7rem] leading-none mt-px">ℹ</span>
            <p class="text-[0.58rem] leading-relaxed text-blue-900/70 dark:text-blue-200/55">
              {t("disclaimer.exgPlacement")}
            </p>
          </div>
        {:else}
          <div class="rounded-xl border border-border dark:border-white/[0.04]
                      bg-muted dark:bg-[#1a1a28] px-3 py-2.5 flex flex-col gap-1.5 xl:col-span-2">
            <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">Modalities</span>
            {#if hasFnirs && fnirsLabels.length > 0}
              <div class="text-[0.58rem] text-muted-foreground">fNIRS: {fnirsLabels.join(" · ")}</div>
              <div class="grid grid-cols-3 gap-1.5 mt-1">
                <div class="rounded-md border border-border/60 px-1.5 py-1">
                  <div class="text-[0.45rem] uppercase tracking-wider text-muted-foreground/70">Oxy</div>
                  <div class="text-[0.62rem] font-semibold text-foreground">{(status.fnirs_oxygenation_pct ?? 0).toFixed(1)}%</div>
                </div>
                <div class="rounded-md border border-border/60 px-1.5 py-1">
                  <div class="text-[0.45rem] uppercase tracking-wider text-muted-foreground/70">Workload</div>
                  <div class="text-[0.62rem] font-semibold text-foreground">{(status.fnirs_workload ?? 0).toFixed(1)}</div>
                </div>
                <div class="rounded-md border border-border/60 px-1.5 py-1">
                  <div class="text-[0.45rem] uppercase tracking-wider text-muted-foreground/70">Lat</div>
                  <div class="text-[0.62rem] font-semibold text-foreground">{(status.fnirs_lateralization ?? 0).toFixed(1)}</div>
                </div>
                <div class="rounded-md border border-border/60 px-1.5 py-1">
                  <div class="text-[0.45rem] uppercase tracking-wider text-muted-foreground/70">ΔHbO</div>
                  <div class="text-[0.62rem] font-semibold text-foreground">{((((status.fnirs_hbo_left ?? 0) + (status.fnirs_hbo_right ?? 0)) / 2)).toFixed(3)}</div>
                </div>
                <div class="rounded-md border border-border/60 px-1.5 py-1">
                  <div class="text-[0.45rem] uppercase tracking-wider text-muted-foreground/70">ΔHbR</div>
                  <div class="text-[0.62rem] font-semibold text-foreground">{((((status.fnirs_hbr_left ?? 0) + (status.fnirs_hbr_right ?? 0)) / 2)).toFixed(3)}</div>
                </div>
                <div class="rounded-md border border-border/60 px-1.5 py-1">
                  <div class="text-[0.45rem] uppercase tracking-wider text-muted-foreground/70">Conn</div>
                  <div class="text-[0.62rem] font-semibold text-foreground">{(status.fnirs_connectivity ?? 0).toFixed(3)}</div>
                </div>
              </div>
            {/if}
            {#if ppgLabels.length > 0}
              <div class="text-[0.58rem] text-muted-foreground">PPG: {ppgLabels.join(" · ")}</div>
            {/if}
            {#if imuLabels.length > 0}
              <div class="text-[0.58rem] text-muted-foreground">IMU: {imuLabels.join(" · ")}</div>
            {/if}
          </div>
          {#if hasFnirs}
            <div class="xl:col-span-2">
              <FnirsChart bind:this={fnirsChartEl} />
            </div>
          {/if}
        {/if}

        {#if hasEeg}
          <!-- Focus / Relaxation / Engagement scores -->
          <BrainStateScores relaxation={relaxScore} engagement={engagementScore} />

          <!-- Frontal Alpha Asymmetry (FAA) gauge -->
          <FaaGauge faa={faaScore} />

          <!-- Advanced EEG Indices grid -->
          <div class="xl:col-span-2">
            <EegIndices tar={tarScore} bar={barScore} dtr={dtrScore} pse={pseScore} apf={apfScore}
              mood={moodScore} bps={bpsScore} snr={snrScore} coherence={coherenceScore} mu={muScore}
              tbr={tbrScore} sef95={sef95Score} sc={scScore} ha={haScore} hm={hmScore} hc={hcScore}
              pe={peScore} hfd={hfdScore} dfa={dfaScore} se={seScore} pac={pacScore} lat={latScore}
              headache={headacheScore} migraine={migraineScore}
              showMu={status.has_central_electrodes} />
          </div>

          <!-- Composite Scores -->
          <CompositeScores meditation={meditationScore} cognitiveLoad={cogLoadScore} drowsiness={drowsinessScore} />

          <!-- Consciousness Metrics -->
          <ConsciousnessMetrics
            lzc={consciousnessLzc}
            wakefulness={consciousnessWakefulness}
            integration={consciousnessIntegration} />

          <!-- Artifact Events -->
          <ArtifactEvents {blinkCount} {blinkRate} />
        {/if}

        <!-- Head Pose (IMU-equipped devices only) -->
        {#if hasImuCap}
        <HeadPoseCard pitch={headPitch} roll={headRoll} stillness={stillnessScore} {nodCount} {shakeCount} />
        {/if}

        <!-- PPG Metrics -->
        {#if hasPpg && (hrScore > 0 || status.ppg_sample_count > 0)}
          <PpgMetrics hr={hrScore} rmssd={rmssdScore} sdnn={sdnnScore} pnn50={pnn50Score}
            lfHf={lfHfScore} respRate={respRateScore} spo2={spo2Score}
            perfIdx={perfIdxScore} stressIdx={stressIdxScore} />
        {/if}

        <!-- PPG Optical (single tile, 3 channels — devices with PPG) -->
        {#if hasPpg && (status.ppg_sample_count > 0 || status.state === "connected")}
          <div class="rounded-xl border border-border dark:border-white/[0.04]
                      bg-muted dark:bg-[#1a1a28] px-3 py-2.5 flex flex-col gap-1.5">
            <button class="flex items-center gap-1.5 w-full group"
                    onclick={() => (ppgOpticalExpanded = !ppgOpticalExpanded)}
                    aria-expanded={ppgOpticalExpanded}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                   stroke-linecap="round" stroke-linejoin="round"
                   class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-muted-foreground/70
                          transition-transform duration-150 shrink-0
                          {ppgOpticalExpanded ? 'rotate-90' : ''}">
                <path d="M9 18l6-6-6-6"/>
              </svg>
              <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground
                           group-hover:text-foreground transition-colors">
                {t("dashboard.ppg")}
              </span>
              {#if status.ppg_sample_count > 0}
                <span class="text-[0.5rem] text-red-500 live-blink" aria-hidden="true">●</span>
              {/if}
              <span class="ml-auto text-[0.48rem] text-muted-foreground/40 tabular-nums">
                {status.ppg_sample_count.toLocaleString()} {t("dashboard.ppgSamples")}
              </span>
            </button>
            {#if ppgOpticalExpanded}
              <PpgChart bind:this={ppgChartEl} />
            {/if}
          </div>
        {/if}

        <!-- IMU (Accelerometer + Gyroscope) — only for devices with IMU -->
        {#if hasImuCap}
        {@const hasImuData = status.accel.some(v => v !== 0) || status.gyro.some(v => v !== 0)}
        <div class="rounded-xl border border-border dark:border-white/[0.04]
                    bg-muted dark:bg-[#1a1a28] px-3 py-2.5 flex flex-col gap-1.5">
          <button class="flex items-center gap-1.5 w-full group"
                  onclick={() => (imuExpanded = !imuExpanded)}
                  aria-expanded={imuExpanded}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                 stroke-linecap="round" stroke-linejoin="round"
                 class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-muted-foreground/70
                        transition-transform duration-150 shrink-0
                        {imuExpanded ? 'rotate-90' : ''}">
              <path d="M9 18l6-6-6-6"/>
            </svg>
            <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground
                         group-hover:text-foreground transition-colors">
              {t("dashboard.imu")}
            </span>
            {#if hasImuData}
              <span class="text-[0.45rem] text-sky-500 live-blink shrink-0" aria-hidden="true">●</span>
            {/if}
          </button>
          {#if imuExpanded}
            <ImuChart bind:this={imuChartEl} />
          {/if}
        </div>
        {/if}

        {#if hasEeg}
          <!-- EEG channel grid -->
          <div class="rounded-xl border border-border dark:border-white/[0.04]
                      bg-muted dark:bg-[#1a1a28] px-3 py-2 flex flex-col gap-1.5">
            <button class="flex items-center gap-1.5 w-full group"
                    onclick={() => (eegChExpanded = !eegChExpanded)}
                    aria-expanded={eegChExpanded}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                   stroke-linecap="round" stroke-linejoin="round"
                   class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-muted-foreground/70
                          transition-transform duration-150 shrink-0
                          {eegChExpanded ? 'rotate-90' : ''}">
                <path d="M9 18l6-6-6-6"/>
              </svg>
              <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground
                           group-hover:text-foreground transition-colors">
                {t("dashboard.eegChannels")}
              </span>
              <span class="text-[0.45rem] text-emerald-500 live-blink shrink-0" aria-hidden="true">●</span>
            </button>
            {#if eegChExpanded}
              <div class="grid gap-1.5" class:grid-cols-2={chLabels.length <= 4} class:grid-cols-3={chLabels.length > 4 && chLabels.length <= 8} class:grid-cols-4={chLabels.length > 8}>
                {#each chLabels as ch, i}
                  <div class="min-w-0 rounded-lg border border-border dark:border-white/[0.04]
                              bg-muted dark:bg-[#1a1a28] px-2 py-1.5 flex flex-col gap-0.5"
                    style="border-left-color:{chColors[i]}; border-left-width:2px">
                    <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground truncate">{ch}</span>
                    <span class="font-mono text-[0.72rem] font-semibold truncate" style="color:{chColors[i]}">{fmtEeg(status.eeg[i])}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

        <!-- Daily goal progress -->
        <div class="rounded-lg border border-border dark:border-white/[0.06]
                    bg-muted dark:bg-[#1a1a28] px-3 py-2 flex items-center gap-2.5">
          <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">
            {t("dashboard.dailyGoal")}
          </span>
          <div class="flex-1 h-2 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
            <div class="h-full rounded-full transition-all duration-700"
                 style="width:{goalPct}%; background:{goalReached ? '#22c55e' : '#3b82f6'}"></div>
          </div>
          <span class="text-[0.58rem] font-bold tabular-nums shrink-0
                       {goalReached ? 'text-emerald-500' : 'text-muted-foreground'}">
            {Math.floor(todayTotalSecs / 60)}m / {dailyGoalMin}m
          </span>
          {#if goalReached}
            <span class="text-xs" title={t("common.goalReached")}>🎯</span>
          {/if}
        </div>

        <!-- Stats row -->
        <div class="flex gap-1">
          {#each [
            [t("dashboard.uptime"), fmtUptime(uptimeSec)],
            [t("dashboard.samples"), (status.sample_count ?? 0).toLocaleString()],
            [t("dashboard.todayTotal"), fmtUptime(todayTotalSecs)],
          ] as [label, val]}
            <div class="flex-1 min-w-0 rounded-lg border border-border dark:border-white/[0.06]
                        bg-muted dark:bg-[#1a1a28] px-2 py-1.5 flex flex-col gap-0.5">
              <span class="text-[0.42rem] font-semibold tracking-widest uppercase text-muted-foreground truncate">{label}</span>
              <span class="font-mono text-[0.6rem] text-muted-foreground truncate">{val}</span>
            </div>
          {/each}
        </div>

        <!-- CSV recording row -->
        {#if status.csv_path}
          <div class="flex flex-col gap-1">
            <div class="flex items-center gap-2.5 rounded-lg border border-border dark:border-white/[0.06]
                        bg-muted dark:bg-[#0f0f1a] px-3 py-2">
              <span class="text-[0.57rem] font-bold text-red-500 shrink-0">{t("dashboard.rec")}</span>
              <span class="font-mono text-[0.6rem] text-muted-foreground overflow-hidden text-ellipsis whitespace-nowrap">
                {csvName(status.csv_path)}
              </span>
            </div>

            <!-- Recent label badge -->
            {#if recentLabel}
              <div class="flex items-center gap-1.5 px-2.5 py-1
                          rounded-md border border-violet-500/25 bg-violet-500/8 dark:bg-violet-500/10">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                     stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                     class="w-2.5 h-2.5 shrink-0 text-violet-400">
                  <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
                  <line x1="7" y1="7" x2="7.01" y2="7"/>
                </svg>
                <span class="text-[0.58rem] text-violet-600 dark:text-violet-300 truncate flex-1">
                  {recentLabel}
                </span>
                <button onclick={() => recentLabel = null}
                        class="text-muted-foreground/30 hover:text-muted-foreground/70 transition-colors shrink-0"
                        aria-label="Dismiss">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                       stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                       class="w-2.5 h-2.5">
                    <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                  </svg>
                </button>
              </div>
            {:else}
              <button
                onclick={openLabel}
                class="flex items-center gap-1.5 px-2.5 py-1 rounded-md
                       border border-dashed border-border dark:border-white/[0.08]
                       text-[0.58rem] text-muted-foreground/50
                       hover:text-muted-foreground hover:border-muted-foreground/40
                       transition-colors w-full"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                     stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                     class="w-2.5 h-2.5 shrink-0">
                  <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                </svg>
                {t("dashboard.addNote")}
              </button>
            {/if}
          </div>
        {/if}

        <!-- ── Onboarding checklist (hides once all steps done) ─────────── -->
        {#if !onboardComplete}
          <div class="xl:col-span-2">
            <OnboardingChecklist
              steps={onboardSteps}
              onDismiss={() => { onboardDone = { devicePaired: true, calibrated: true, firstSession: true, goalSet: true, llmDownloaded: true, searchRun: true, dndConfigured: true, apiVisited: true }; saveOnboarding(); }}
            />
          </div>
        {/if}

        </div>

      <!-- ════ DISCONNECTED ══════════════════════════════════════════════════ -->
      {:else}
        {#if status.paired_devices.length > 0}
          <p class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">{t("dashboard.pairedDevices")}</p>
          <div class="flex flex-col gap-1.5">
            {#each status.paired_devices as dev (dev.id)}
              <div class="flex items-center justify-between gap-2 rounded-xl
                          border border-border dark:border-white/[0.06]
                          bg-muted dark:bg-[#1a1a28] px-3 py-2">
                <div class="flex flex-col gap-0.5 min-w-0">
                  <span class="text-[0.77rem] font-semibold text-foreground/80 dark:text-slate-400 truncate">{dev.name}</span>
                  <span class="font-mono text-[0.57rem] text-muted-foreground/60 truncate">{dev.id}</span>
                </div>
                <div class="flex items-center gap-1 shrink-0">
                  <Button size="sm"
                    class="h-5 px-2 text-[0.56rem]"
                    onclick={() => connectDevice(dev.id)}>
                    {t("common.connect")}
                  </Button>
                  <Button size="icon-sm" variant="ghost"
                    class="text-muted-foreground hover:text-red-500"
                    onclick={() => forgetDevice(dev.id)}>✕</Button>
                </div>
              </div>
            {/each}
          </div>
          <Button size="sm" variant="outline" class="w-full text-[0.65rem]" onclick={retryConnect}>
            {t("dashboard.scanForNew")}
          </Button>
        {:else}
          <div class="flex flex-col items-center gap-3 py-4">
            <p class="text-[0.72rem] text-muted-foreground text-center leading-relaxed">
              {t("dashboard.noDevicesPaired")}
            </p>
            <Button size="sm" onclick={retryConnect}>{t("dashboard.scanForMuse")}</Button>
          </div>
        {/if}
      {/if}

      <!-- ════ Band Powers & EEG Waveforms — only during active EEG session ════ -->
      {#if status.state === "connected" && hasEeg}
      <Separator class="bg-border dark:bg-white/[0.06]" />

      <div class="flex flex-col gap-2">
        <div class="flex items-center gap-1.5">
          <p class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("dashboard.bandPowers")}
          </p>
          <span class="text-[0.5rem] text-green-500 live-blink">●</span>
        </div>
        <BandChart bind:this={bandChartEl} chNames={chLabels} chColors={chColors} />
      </div>

      <Separator class="bg-border dark:bg-white/[0.06]" />

      <div class="flex flex-col gap-2">
        <div class="flex items-center gap-1.5">
          <p class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">{t("dashboard.eegWaveforms")}</p>
          <span class="text-[0.5rem] text-red-500 live-blink">●</span>
        </div>
        <EegChart bind:this={chartEl} numChannels={chLabels.length} chLabels={chLabels} chColors={chColors} sampleRate={status.filter_config?.sample_rate ?? 256} />
      </div>
      {/if}

    </CardContent>

    <!-- ── Footer ──────────────────────────────────────────────────────────── -->
    <Separator class="bg-border dark:bg-white/[0.06]" />
    <!-- ── Secondary Sessions Strip ──────────────────────────────────── -->
    {#if hasSecondary}
      <div class="border-t border-border dark:border-white/[0.05]">
        <div class="px-4 pt-2.5 pb-1">
          <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground/50">
            {t("dashboard.backgroundRecordings")}
          </span>
        </div>
        {#each secondarySessions as sess (sess.id)}
          <div class="flex items-center gap-2.5 px-4 py-2 hover:bg-muted/30 dark:hover:bg-white/[0.02] transition-colors">
            <!-- Pulsing dot -->
            <span class="relative flex h-2 w-2 shrink-0">
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-violet-400 opacity-60"></span>
              <span class="relative inline-flex rounded-full h-2 w-2 bg-violet-500"></span>
            </span>

            <!-- Name + meta -->
            <div class="flex items-center gap-1.5 flex-1 min-w-0">
              <span class="text-[0.62rem] font-medium text-foreground truncate">
                {sess.device_name}
              </span>
              <span class="text-[0.46rem] font-bold tracking-widest uppercase px-1 py-0.5 rounded
                           bg-violet-500/10 text-violet-600 dark:text-violet-400 shrink-0">
                {sess.device_kind === "lsl" ? "LSL" : sess.device_kind === "lsl-iroh" ? "iroh" : sess.device_kind.toUpperCase()}
              </span>
            </div>

            <!-- Stats -->
            <span class="text-[0.54rem] text-muted-foreground/60 tabular-nums shrink-0">
              {sess.channels}ch · {sess.sample_rate % 1 === 0 ? sess.sample_rate : sess.sample_rate.toFixed(1)} Hz
            </span>
            <span class="text-[0.56rem] text-muted-foreground tabular-nums shrink-0">
              {sess.sample_count.toLocaleString()}
            </span>

            <!-- Stop -->
            <button
              class="text-muted-foreground/30 hover:text-red-500 transition-colors cursor-pointer text-[0.65rem] shrink-0"
              onclick={() => daemonInvoke("lsl_cancel_secondary", { sessionId: sess.id })}
              title={t("dashboard.stopSecondary")}
            >
              ✕
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <CardFooter class="px-5 py-3 flex items-center justify-between gap-2">
      <p class="text-[0.63rem] text-muted-foreground leading-relaxed truncate">
        {#if status.state === "connected"}
          {t("dashboard.streamingCsv")}
        {:else if status.state === "scanning"}
          {t("dashboard.scanningFooter")}
        {/if}
      </p>
      <div class="flex items-center gap-2 shrink-0">
        <span class="text-[0.56rem] text-muted-foreground/40 tabular-nums">v{appVersion}</span>
      </div>
    </CardFooter>

  </Card>

  <DisclaimerFooter />
  </div>
</main>

<style>
  /* Status ring — background set via Tailwind class on the element */
  .status-ring {
    width: 48px; height: 48px; border-radius: 50%;
    border: 2.5px solid var(--rc, #cbd5e1);
    display: flex; align-items: center; justify-content: center;
    animation: ring-pulse 2s ease-in-out infinite;
  }
  @keyframes ring-pulse {
    0%,100% { box-shadow: 0 0 0 3px color-mix(in oklch, var(--rc, transparent) 10%, transparent); }
    50%      { box-shadow: 0 0 0 8px color-mix(in oklch, var(--rc, transparent) 20%, transparent); }
  }
  .status-dot { width: 18px; height: 18px; border-radius: 50%; transition: background .4s; }

  /* Compact ring — used in the collapsed hero */
  .status-ring-sm {
    width: 22px; height: 22px; border-radius: 50%;
    border: 1.5px solid var(--rc, #cbd5e1);
    display: flex; align-items: center; justify-content: center;
  }
  .status-dot-sm { width: 8px; height: 8px; border-radius: 50%; transition: background .4s; }





  .live-blink { animation: blink 1s step-start infinite; }
  @keyframes blink { 0%,100%{opacity:1} 50%{opacity:0} }
</style>
