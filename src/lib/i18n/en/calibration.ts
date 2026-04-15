// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** English "calibration" namespace — reference translation. */
const calibration: Record<string, string> = {
  "calibration.title": "Calibration",
  "calibration.profiles": "Calibration Profiles",
  "calibration.newProfile": "New Profile",
  "calibration.editProfile": "Edit Profile",
  "calibration.profileName": "Profile Name",
  "calibration.profileNamePlaceholder": "e.g. Eyes Open / Closed",
  "calibration.addAction": "Add Action",
  "calibration.actionLabel": "Action label…",
  "calibration.breakLabel": "break",
  "calibration.selectProfile": "Profile",
  "calibration.moveUp": "Move up",
  "calibration.moveDown": "Move down",
  "calibration.removeAction": "Remove action",
  "calibration.descriptionN": "This protocol runs {actions}, repeated <strong>{count}</strong> times.",
  "calibration.timingDescN": "{loops} loops · {actions} actions · {breakSecs}s break between each",
  "calibration.notifActionBody": "Loop {loop} of {total}",
  "calibration.notifBreakBody": "Next: {next}",
  "calibration.notifDoneBody": "All {n} loops completed.",
  "calibration.recording": "● Recording",
  "calibration.neverCalibrated": "Never calibrated",
  "calibration.lastAgo": "Last: {ago}",
  "calibration.eegCalibration": "EEG Calibration",
  "calibration.description":
    'This task alternates between <strong class="text-blue-600 dark:text-blue-400">{action1}</strong> and <strong class="text-violet-600 dark:text-violet-400">{action2}</strong> with breaks in between, repeated <strong>{count}</strong> times.',
  "calibration.timingDesc":
    "Each action lasts {actionSecs}s with a {breakSecs}s break. Labels are saved automatically.",
  "calibration.startCalibration": "Start Calibration",
  "calibration.complete": "Calibration Complete",
  "calibration.completeDesc":
    "All {n} iterations completed successfully. Labels have been saved for each action phase.",
  "calibration.runAgain": "Run Again",
  "calibration.iteration": "Iteration",
  "calibration.break": "Break",
  "calibration.nextAction": "Next: {action}",
  "calibration.secondsRemaining": "seconds remaining",
  "calibration.ready": "Ready",
  "calibration.lastCalibrated": "Last calibrated",
  "calibration.lastAtAgo": "Last: {date} ({ago})",
  "calibration.noPrevious": "No previous calibration recorded",
  "calibration.footer": "Esc to close · Events broadcast via WebSocket",
  "calibration.presets": "Quick Presets",
  "calibration.presetsDesc":
    "Select a calibration configuration based on your goal, age, and use case. Settings can still be adjusted below.",
  "calibration.applyPreset": "Apply",
  "calibration.orCustom": "Or configure manually:",
  "calibration.preset.baseline": "Eyes Open / Closed",
  "calibration.preset.baselineDesc":
    "Classic baseline: resting eyes-open vs eyes-closed. Best for beginners & first calibration.",
  "calibration.preset.focus": "Focus / Relax",
  "calibration.preset.focusDesc": "Neurofeedback: mental arithmetic vs. calm breathing. General use.",
  "calibration.preset.meditation": "Meditation",
  "calibration.preset.meditationDesc": "Active thinking vs. mindfulness meditation. For meditators & practitioners.",
  "calibration.preset.sleep": "Pre-sleep / Drowsiness",
  "calibration.preset.sleepDesc": "Alert wakefulness vs. drowsiness. For sleep research & wind-down tracking.",
  "calibration.preset.gaming": "Gaming / Performance",
  "calibration.preset.gamingDesc": "High-demand task vs. passive rest. For esports and peak-performance biofeedback.",
  "calibration.preset.children": "Short Attention",
  "calibration.preset.childrenDesc": "Shorter phases (10 s) for children or users with limited focus endurance.",
  "calibration.preset.clinical": "Clinical / Research",
  "calibration.preset.clinicalDesc":
    "Extended 5-iteration protocol with long action phases for research or clinical baseline.",
  "calibration.preset.stress": "Stress / Anxiety",
  "calibration.preset.stressDesc":
    "Resting calm vs. mild cognitive stressor. For anxiety and stress-response tracking.",
};

export default calibration;
