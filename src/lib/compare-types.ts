// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Shared type definitions and constants for the compare feature.
 *
 * Extracted from `routes/compare/+page.svelte`.
 */

import { BANDS, SESSION_COLORS } from "$lib/constants";
import type { SessionMetrics } from "$lib/dashboard/SessionDetail.svelte";

// ── Types ────────────────────────────────────────────────────────────────────

/** A contiguous recording range discovered from embedding timestamps. */
export interface EmbeddingSession {
  start_utc: number;
  end_utc:   number;
  n_epochs:  number;
  day:       string;
}

/** Metric row descriptor for the advanced metrics table. */
export interface MRow {
  key: keyof SessionMetrics;
  label: string;
  unit: string;
  fmt: (v: number) => string;
}

export interface InsightDelta {
  label: string;
  key: keyof SessionMetrics;
  a: number;
  b: number;
  delta: number;
  pctChange: number;
  direction: "improved" | "declined" | "stable";
}

export interface ClusterAnalysis {
  separationScore: number;
  interCluster: number;
  intraSpreadA: number;
  intraSpreadB: number;
}

// ── Constants ────────────────────────────────────────────────────────────────

export { SESSION_COLORS } from "$lib/constants";

export const bandKeys = ["rel_delta", "rel_theta", "rel_alpha", "rel_beta", "rel_gamma"] as const;
export type BK = typeof bandKeys[number];
export const bandMeta = BANDS.slice(0, 5);

export const scoreKeys = [
  { key: "relaxation" as const, color: "#10b981", label: "dashboard.relaxation" },
  { key: "engagement" as const, color: "#f59e0b", label: "dashboard.engagement" },
];

export const radarMetrics = [
  { key: "relaxation" as const,     label: "Relax",      max: 100, color: "#10b981" },
  { key: "engagement" as const,     label: "Engage",     max: 100, color: "#f59e0b" },
  { key: "meditation" as const,     label: "Meditation", max: 100, color: "#8b5cf6" },
  { key: "cognitive_load" as const, label: "Cog Load",   max: 100, color: "#0ea5e9" },
  { key: "drowsiness" as const,     label: "Drowsiness", max: 100, color: "#ef4444" },
];

const fmtF3 = (v: number) => v.toFixed(3);
const fmtF2 = (v: number) => v.toFixed(2);
const fmtF1 = (v: number) => v.toFixed(1);

export const advancedMetrics: MRow[] = [
  { key: "tar",                label: "compare.tar",               unit: "",    fmt: fmtF3 },
  { key: "bar",                label: "compare.bar",               unit: "",    fmt: fmtF3 },
  { key: "dtr",                label: "compare.dtr",               unit: "",    fmt: fmtF3 },
  { key: "tbr",                label: "compare.tbr",               unit: "",    fmt: fmtF3 },
  { key: "pse",                label: "compare.pse",               unit: "",    fmt: fmtF3 },
  { key: "apf",                label: "compare.apf",               unit: "Hz",  fmt: fmtF2 },
  { key: "sef95",              label: "compare.sef95",             unit: "Hz",  fmt: fmtF2 },
  { key: "spectral_centroid",  label: "compare.spectralCentroid",  unit: "Hz",  fmt: fmtF2 },
  { key: "bps",                label: "compare.bps",               unit: "",    fmt: fmtF3 },
  { key: "snr",                label: "compare.snr",               unit: "dB",  fmt: fmtF1 },
  { key: "coherence",          label: "compare.coherence",         unit: "",    fmt: fmtF3 },
  { key: "mu_suppression",     label: "compare.muSuppression",     unit: "",    fmt: fmtF3 },
  { key: "mood",               label: "compare.mood",              unit: "",    fmt: fmtF1 },
  { key: "hjorth_activity",    label: "compare.hjorthActivity",    unit: "µV²", fmt: fmtF3 },
  { key: "hjorth_mobility",    label: "compare.hjorthMobility",    unit: "",    fmt: fmtF3 },
  { key: "hjorth_complexity",  label: "compare.hjorthComplexity",  unit: "",    fmt: fmtF3 },
  { key: "permutation_entropy",label: "compare.permEntropy",       unit: "",    fmt: fmtF3 },
  { key: "higuchi_fd",         label: "compare.higuchiFd",         unit: "",    fmt: fmtF3 },
  { key: "dfa_exponent",       label: "compare.dfaExponent",       unit: "",    fmt: fmtF3 },
  { key: "sample_entropy",     label: "compare.sampleEntropy",     unit: "",    fmt: fmtF3 },
  { key: "pac_theta_gamma",    label: "compare.pacThetaGamma",     unit: "",    fmt: fmtF3 },
  { key: "laterality_index",   label: "compare.lateralityIndex",   unit: "",    fmt: fmtF3 },
  { key: "hr",                 label: "compare.hr",                unit: "bpm", fmt: fmtF1 },
  { key: "rmssd",              label: "compare.rmssd",             unit: "ms",  fmt: fmtF1 },
  { key: "sdnn",               label: "compare.sdnn",              unit: "ms",  fmt: fmtF1 },
  { key: "pnn50",              label: "compare.pnn50",             unit: "%",   fmt: fmtF1 },
  { key: "lf_hf_ratio",        label: "compare.lfHfRatio",         unit: "",    fmt: fmtF2 },
  { key: "respiratory_rate",   label: "compare.respiratoryRate",   unit: "bpm", fmt: fmtF1 },
  { key: "spo2_estimate",      label: "compare.spo2",             unit: "%",   fmt: fmtF1 },
  { key: "perfusion_index",    label: "compare.perfusionIndex",    unit: "%",   fmt: fmtF2 },
  { key: "stress_index",       label: "compare.stressIndex",       unit: "",    fmt: fmtF1 },
  { key: "meditation",         label: "compare.meditation",        unit: "",    fmt: fmtF1 },
  { key: "cognitive_load",     label: "compare.cognitiveLoad",     unit: "",    fmt: fmtF1 },
  { key: "drowsiness",         label: "compare.drowsiness",        unit: "",    fmt: fmtF1 },
  { key: "blink_count",        label: "compare.blinkCount",        unit: "",    fmt: fmtF1 },
  { key: "blink_rate",         label: "compare.blinkRate",         unit: "/min",fmt: fmtF1 },
  { key: "head_pitch",         label: "compare.headPitch",         unit: "°",   fmt: fmtF1 },
  { key: "head_roll",          label: "compare.headRoll",          unit: "°",   fmt: fmtF1 },
  { key: "stillness",          label: "compare.stillness",         unit: "",    fmt: fmtF1 },
  { key: "nod_count",          label: "compare.nodCount",          unit: "",    fmt: fmtF1 },
  { key: "shake_count",        label: "compare.shakeCount",        unit: "",    fmt: fmtF1 },
];

/** Metrics where higher B value = improvement. */
export const HIGHER_IS_BETTER = new Set<keyof SessionMetrics>([
  "relaxation", "engagement", "meditation", "coherence", "snr",
  "mu_suppression", "stillness", "spo2_estimate", "pnn50", "rmssd", "sdnn",
]);

/** Metrics where lower B value = improvement. */
export const LOWER_IS_BETTER = new Set<keyof SessionMetrics>([
  "stress_index", "cognitive_load", "drowsiness", "blink_rate", "lf_hf_ratio",
]);

export const insightMetrics: { key: keyof SessionMetrics; label: string }[] = [
  { key: "relaxation", label: "Relaxation" },
  { key: "engagement", label: "Engagement" }, { key: "meditation", label: "Meditation" },
  { key: "cognitive_load", label: "Cog. Load" }, { key: "drowsiness", label: "Drowsiness" },
  { key: "stress_index", label: "Stress" }, { key: "coherence", label: "Coherence" },
  { key: "snr", label: "SNR" }, { key: "mu_suppression", label: "μ Suppression" },
  { key: "hr", label: "Heart Rate" }, { key: "rmssd", label: "RMSSD" },
  { key: "sdnn", label: "SDNN" }, { key: "pnn50", label: "pNN50" },
  { key: "stillness", label: "Stillness" }, { key: "blink_rate", label: "Blink Rate" },
  { key: "lf_hf_ratio", label: "LF/HF" },
];

// ── Helpers ──────────────────────────────────────────────────────────────────

export function bv(m: SessionMetrics | null, k: BK): number { return m ? (m[k] ?? 0) : 0; }
export function pct(v: number): string { return (v * 100).toFixed(1); }

export function diff(a: number, b: number): string {
  const d = a - b;
  if (Math.abs(d) < 0.001) return "—";
  return `${d > 0 ? "+" : ""}${(d * 100).toFixed(1)}`;
}

export function scoreDiff(a: number, b: number): string {
  const d = a - b;
  if (Math.abs(d) < 0.1) return "—";
  return `${d > 0 ? "+" : ""}${d.toFixed(1)}`;
}

export function dc(a: number, b: number): string {
  const d = a - b;
  if (Math.abs(d) < 0.001) return "text-muted-foreground/40";
  return d > 0 ? "text-emerald-500" : "text-red-400";
}

export function sdc(a: number, b: number): string {
  const d = a - b;
  if (Math.abs(d) < 0.1) return "text-muted-foreground/40";
  return d > 0 ? "text-emerald-500" : "text-red-400";
}

// ── UMAP analysis ────────────────────────────────────────────────────────────

import type { UmapPoint, UmapResult } from "$lib/types";

export function analyzeUmapClusters(result: UmapResult): ClusterAnalysis | null {
  const pts = result.points;
  if (pts.length < 4) return null;
  const ptsA = pts.filter(p => p.session === 0);
  const ptsB = pts.filter(p => p.session === 1);
  if (ptsA.length < 2 || ptsB.length < 2) return null;

  const centroid = (arr: UmapPoint[]) => ({
    x: arr.reduce((s, p) => s + p.x, 0) / arr.length,
    y: arr.reduce((s, p) => s + p.y, 0) / arr.length,
    z: arr.reduce((s, p) => s + p.z, 0) / arr.length,
  });
  const dist = (a: {x:number;y:number;z:number}, b: {x:number;y:number;z:number}) =>
    Math.sqrt((a.x-b.x)**2 + (a.y-b.y)**2 + (a.z-b.z)**2);

  const cA = centroid(ptsA);
  const cB = centroid(ptsB);
  const interCluster = dist(cA, cB);
  const intraSpreadA = Math.sqrt(ptsA.reduce((s, p) => s + dist(p, cA)**2, 0) / ptsA.length);
  const intraSpreadB = Math.sqrt(ptsB.reduce((s, p) => s + dist(p, cB)**2, 0) / ptsB.length);
  const avgIntra = (intraSpreadA + intraSpreadB) / 2;
  const separationScore = avgIntra > 0 ? interCluster / avgIntra : 0;

  return { separationScore, interCluster, intraSpreadA, intraSpreadB };
}

/** Generate a random placeholder UMAP result for loading state. */
export function generateUmapPlaceholder(nA: number, nB: number): UmapResult {
  function gaussRand(): number {
    return Math.sqrt(-2 * Math.log(Math.random() || 1e-10)) * Math.cos(Math.PI * 2 * Math.random());
  }
  const points: UmapPoint[] = [];
  for (let i = 0; i < nA; i++) {
    points.push({ x: -3 + gaussRand()*3, y: gaussRand()*3, z: gaussRand()*3, session: 0, utc: 0 });
  }
  for (let i = 0; i < nB; i++) {
    points.push({ x: 3 + gaussRand()*3, y: gaussRand()*3, z: gaussRand()*3, session: 1, utc: 0 });
  }
  return { points, n_a: nA, n_b: nB, dim: 0 };
}

// ── Insight computation ──────────────────────────────────────────────────────

export function computeInsightDeltas(metricsA: SessionMetrics, metricsB: SessionMetrics): InsightDelta[] {
  return insightMetrics.map(m => {
    const a = Number(metricsA[m.key]) || 0;
    const b = Number(metricsB[m.key]) || 0;
    const delta = b - a;
    const pctChange = a !== 0 ? (delta / Math.abs(a)) * 100 : 0;
    let direction: "improved" | "declined" | "stable" = "stable";
    if (Math.abs(pctChange) > 3) {
      if (HIGHER_IS_BETTER.has(m.key)) direction = delta > 0 ? "improved" : "declined";
      else if (LOWER_IS_BETTER.has(m.key)) direction = delta < 0 ? "improved" : "declined";
    }
    return { label: m.label, key: m.key, a, b, delta, pctChange, direction };
  });
}
