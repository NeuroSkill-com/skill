// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * EEG context injection — builds a compact brain-state summary block
 * suitable for injection into an LLM system prompt.
 *
 * Extracted from `routes/chat/+page.svelte`.
 */

import type { BandSnapshot, BandPowers } from "$lib/chat-types";

/**
 * Build a compact EEG brain-state block suitable for injection into a
 * system prompt.  Averages relative band powers across all channels.
 */
export function buildEegBlock(b: BandSnapshot): string {
  const n = b.channels.length || 1;
  const avg = (key: keyof BandPowers) =>
    b.channels.reduce((s, ch) => s + (ch[key] as number), 0) / n;

  const pct = (v: number) => (v * 100).toFixed(0) + "%";
  const f1  = (v: number) => v.toFixed(1);
  const f2  = (v: number) => (v >= 0 ? "+" : "") + v.toFixed(2);

  const rD = avg("rel_delta");
  const rT = avg("rel_theta");
  const rA = avg("rel_alpha");
  const rB = avg("rel_beta");
  const rG = avg("rel_gamma");

  const dominant = b.channels[0]?.dominant ?? "—";

  const lines: string[] = [
    "--- Live EEG Brain State (auto-updated) ---",
    `Signal quality (SNR): ${f1(b.snr ?? 0)} dB | Dominant band: ${dominant}`,
    `Relative band powers: δ=${pct(rD)} θ=${pct(rT)} α=${pct(rA)} β=${pct(rB)} γ=${pct(rG)}`,
    `Mood: ${(b.mood ?? 0).toFixed(0)}/100 | FAA (approach): ${f2(b.faa)}`,
    `TAR (θ/α — drowsiness): ${f1(b.tar ?? 0)} | BAR (β/α — focus/stress): ${f1(b.bar ?? 0)}`,
    `Coherence (α sync): ${((b.coherence ?? 0) * 100).toFixed(0)}% | Consciousness: wakefulness=${(b.consciousness_wakefulness ?? 0).toFixed(0)} integration=${(b.consciousness_integration ?? 0).toFixed(0)}`,
  ];

  if (b.meditation   != null) lines.push(`Meditation: ${b.meditation.toFixed(0)}/100`);
  if (b.cognitive_load != null) lines.push(`Cognitive load: ${b.cognitive_load.toFixed(0)}/100`);
  if (b.drowsiness   != null) lines.push(`Drowsiness: ${b.drowsiness.toFixed(0)}/100`);
  if (b.hr           != null) lines.push(`Heart rate: ${b.hr.toFixed(0)} bpm${b.rmssd != null ? ` | HRV (RMSSD): ${b.rmssd.toFixed(1)} ms` : ""}`);
  if (b.stress_index != null) lines.push(`Stress index: ${b.stress_index.toFixed(1)}`);
  if (b.respiratory_rate != null) lines.push(`Respiratory rate: ${b.respiratory_rate.toFixed(1)} breaths/min`);

  lines.push("--- End EEG State ---");
  return lines.join("\n");
}
