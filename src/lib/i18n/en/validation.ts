// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** English — "validation" namespace. Karolinska / NASA-TLX / PVT / EEG fatigue. */
const validation: Record<string, string> = {
  "settingsTabs.validation": "Validation",
  "validation.title": "Validation & Research",
  "validation.intro":
    "Opt-in research instruments that calibrate the Break Coach and Focus Score against external measures. None of these are required to use NeuroSkill.",
  "validation.disclaimer":
    "Research tool only — not a medical device. Not cleared by the FDA, CE, or any regulatory body. Not for clinical use.",

  // ── Master gates ───────────────────────────────────────────────────────
  "validation.master.title": "Global gates",
  "validation.master.respectFlow": "Respect flow state",
  "validation.master.respectFlowDesc":
    "When you cross into flow, suppress every prompt below. Defaults on — leave it on.",
  "validation.master.quietBefore": "Quiet hours start",
  "validation.master.quietAfter": "Quiet hours end",
  "validation.master.quietDesc":
    "Local time. No prompts fire outside this window. Set start = end to disable quiet hours entirely.",

  // ── KSS ────────────────────────────────────────────────────────────────
  "validation.kss.title": "Karolinska Sleepiness Scale (KSS)",
  "validation.kss.desc":
    "5-second self-report (1–9) of momentary sleepiness. Used to calibrate Break Coach against subjective state.",
  "validation.kss.enabled": "Enable KSS prompts",
  "validation.kss.maxPerDay": "Max prompts per day",
  "validation.kss.minInterval": "Min minutes between prompts",
  "validation.kss.triggerBreakCoach": "Fire when Break Coach detects fatigue",
  "validation.kss.triggerRandom": "Fire occasional uniform-random control samples",
  "validation.kss.triggerRandomDesc":
    "Needed to compute ROC / AUC — without negatives, we only see fatigue-positive cases.",
  "validation.kss.randomWeight": "Random sample weight (0–1)",

  // ── NASA-TLX ───────────────────────────────────────────────────────────
  "validation.tlx.title": "NASA-TLX (workload, raw 6-scale)",
  "validation.tlx.desc":
    "60-second 6-subscale workload self-report after a unit of work. Measures load — complementary to KSS sleepiness.",
  "validation.tlx.enabled": "Enable NASA-TLX prompts",
  "validation.tlx.maxPerDay": "Max prompts per day",
  "validation.tlx.minTaskMin": "Minimum task length (min) to ask",
  "validation.tlx.endOfDay": "Also fire an end-of-day workload summary",

  "validation.tlx.form.title": "Rate the task you just finished",
  "validation.tlx.form.subtitle": "Each scale is 0–100. Move the slider to where the task fell.",
  "validation.tlx.mental": "Mental Demand",
  "validation.tlx.mentalDesc": "How mentally demanding was the task?",
  "validation.tlx.physical": "Physical Demand",
  "validation.tlx.physicalDesc": "How physically demanding was the task?",
  "validation.tlx.temporal": "Temporal Demand",
  "validation.tlx.temporalDesc": "How hurried or rushed was the pace?",
  "validation.tlx.performance": "Performance",
  "validation.tlx.performanceDesc": "How successful were you? (Higher = better outcome.)",
  "validation.tlx.effort": "Effort",
  "validation.tlx.effortDesc": "How hard did you have to work to reach your level of performance?",
  "validation.tlx.frustration": "Frustration",
  "validation.tlx.frustrationDesc": "How frustrated, irritated, stressed, or annoyed were you?",
  "validation.tlx.low": "Very low",
  "validation.tlx.high": "Very high",
  "validation.tlx.failure": "Failure",
  "validation.tlx.perfect": "Perfect",

  // ── PVT ────────────────────────────────────────────────────────────────
  "validation.pvt.title": "Psychomotor Vigilance Task (PVT)",
  "validation.pvt.desc":
    "3-minute reaction-time task. The objective vigilance measure — slow to collect but the strongest signal in the literature.",
  "validation.pvt.enabled": "Enable weekly PVT reminders",
  "validation.pvt.weeklyReminder": "Show a one-line reminder when no PVT this week",
  "validation.pvt.runNow": "Run PVT now (3 min)",

  "validation.pvt.task.title": "Psychomotor Vigilance Task",
  "validation.pvt.task.intro":
    "When the dot appears, click (or press any key) as fast as you can. Stay focused — random pauses are part of the task. Duration: 3 minutes.",
  "validation.pvt.task.start": "Start",
  "validation.pvt.task.cancel": "Cancel",
  "validation.pvt.task.go": "Click / press any key now",
  "validation.pvt.task.wait": "Wait…",
  "validation.pvt.task.tooFast": "Too fast — wait for the dot.",
  "validation.pvt.task.elapsed": "{0}s elapsed of {1}s",
  "validation.pvt.task.results": "PVT complete",
  "validation.pvt.task.meanRt": "Mean RT",
  "validation.pvt.task.medianRt": "Median RT",
  "validation.pvt.task.slowest10": "Slowest 10% mean RT",
  "validation.pvt.task.lapses": "Lapses (RT > 500 ms)",
  "validation.pvt.task.falseStarts": "False starts",
  "validation.pvt.task.close": "Close",

  // ── EEG fatigue index ──────────────────────────────────────────────────
  "validation.eeg.title": "EEG fatigue index (Jap et al. 2009)",
  "validation.eeg.desc":
    "Computed continuously from the band-power stream when a NeuroSkill headset is connected. Formula: (α + θ) / β. Passive — costs nothing.",
  "validation.eeg.enabled": "Compute EEG fatigue index",
  "validation.eeg.windowSecs": "Rolling window (seconds)",
  "validation.eeg.current": "Current value",
  "validation.eeg.noHeadset": "No EEG headset streaming",

  // ── Calibration Week ───────────────────────────────────────────────────
  "validation.calibrationWeek.title": "Calibration Week",
  "validation.calibrationWeek.desc":
    "Opt-in 7-day burst of higher-frequency sampling. Increases KSS to 8/day, fires TLX after every flow block ≥ 20 min, asks for one PVT mid-week. Auto-reverts to your normal settings on day 8.",
  "validation.calibrationWeek.start": "Start a Calibration Week",
  "validation.calibrationWeek.active": "Calibration Week active — auto-reverts in {0} days",
  "validation.calibrationWeek.cancel": "Cancel calibration",

  // ── Recent results / status ────────────────────────────────────────────
  "validation.results.title": "Recent results",
  "validation.results.kssCount": "{0} KSS responses (last 7 days)",
  "validation.results.tlxCount": "{0} TLX responses (last 7 days)",
  "validation.results.pvtCount": "{0} PVT runs (last 7 days)",
  "validation.results.empty": "No data yet — opt in above and prompts will start appearing.",

  // ── References footer ──────────────────────────────────────────────────
  "validation.references.title": "References",
  "validation.references.kss": "Åkerstedt & Gillberg (1990) — KSS",
  "validation.references.tlx": "Hart & Staveland (1988); Hart (2006) — NASA-TLX",
  "validation.references.pvt": "Dinges & Powell (1985) — PVT",
  "validation.references.eeg": "Jap, Lal, Fischer & Bekiaris (2009) — EEG fatigue index",

  // ── Save state ─────────────────────────────────────────────────────────
  "validation.save.saving": "Saving…",
  "validation.save.saved": "Saved",
  "validation.save.failed": "Save failed: {0}",
};
export default validation;
