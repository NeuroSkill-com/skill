// SPDX-License-Identifier: GPL-3.0-only
// Shared sleep-analysis helpers.

import type { SleepStages } from "$lib/types";

export interface SleepAnalysis {
  efficiency:       number;      // % of non-wake time
  onsetLatencyMin:  number;      // minutes to first non-wake
  remLatencyMin:    number;      // minutes to first REM (-1 if none)
  totalMin:         number;      // total recording minutes
  awakenings:       number;      // transitions from sleep → wake
  stageMinutes: {
    wake: number;
    n1:   number;
    n2:   number;
    n3:   number;
    rem:  number;
  };
}

/** Derive sleep-analysis metrics from staged epochs. */
export function analyzeSleep(sleep: SleepStages): SleepAnalysis {
  const eps = sleep.epochs;
  const epochSecs = sleep.summary.epoch_secs || 5;
  const totalMin = (eps.length * epochSecs) / 60;
  const wakeMin = (sleep.summary.wake_epochs * epochSecs) / 60;
  const efficiency = totalMin > 0 ? ((totalMin - wakeMin) / totalMin) * 100 : 0;

  // Onset latency: time until first non-wake epoch
  const onsetIdx = eps.findIndex(e => e.stage !== 0);
  const onsetLatencyMin = onsetIdx >= 0 ? (onsetIdx * epochSecs) / 60 : totalMin;

  // REM latency: time from sleep onset to first REM
  const remIdx = eps.findIndex(e => e.stage === 4);
  const remLatencyMin =
    remIdx >= 0 && onsetIdx >= 0
      ? ((remIdx - onsetIdx) * epochSecs) / 60
      : -1;

  // Awakenings: transitions from sleep (stage 1-4) to wake (stage 0)
  let awakenings = 0;
  for (let i = 1; i < eps.length; i++) {
    if (eps[i].stage === 0 && eps[i - 1].stage > 0) awakenings++;
  }

  const m = (n: number) => (n * epochSecs) / 60;
  const stageMinutes = {
    wake: m(sleep.summary.wake_epochs),
    n1:   m(sleep.summary.n1_epochs),
    n2:   m(sleep.summary.n2_epochs),
    n3:   m(sleep.summary.n3_epochs),
    rem:  m(sleep.summary.rem_epochs),
  };

  return { efficiency, onsetLatencyMin, remLatencyMin, totalMin, awakenings, stageMinutes };
}
