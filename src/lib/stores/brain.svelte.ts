// SPDX-License-Identifier: GPL-3.0-only
/**
 * Reactive brain state store — polls daemon `/v1/brain/*` endpoints every 30s.
 * Shared across all components: status bar, dashboard, activity tab, tray.
 */

import { daemonGet, daemonPost } from "$lib/daemon/http";

export interface FlowState {
  in_flow: boolean;
  score: number;
  duration_secs: number;
  avg_focus: number | null;
  file_switches: number;
  edit_velocity: number;
}

export interface FatigueState {
  fatigued: boolean;
  focus_decline_pct: number;
  continuous_work_mins: number;
  suggestion: string;
}

export interface StreakState {
  current_streak_days: number;
  today_deep_mins: number;
  today_qualifies: boolean;
}

// ── Reactive state ───────────────────────────────────────────────────────────

let flow = $state<FlowState | null>(null);
let fatigue = $state<FatigueState | null>(null);
let streak = $state<StreakState | null>(null);
let polling = false;
let timer: ReturnType<typeof setInterval> | undefined;

export function getFlow(): FlowState | null {
  return flow;
}
export function getFatigue(): FatigueState | null {
  return fatigue;
}
export function getStreak(): StreakState | null {
  return streak;
}

async function poll(): Promise<void> {
  try {
    const [f, a, s] = await Promise.allSettled([
      daemonPost<FlowState>("/v1/brain/flow-state", { windowSecs: 300 }),
      daemonGet<FatigueState>("/v1/brain/fatigue"),
      daemonPost<StreakState>("/v1/brain/streak", { minDeepWorkMins: 60 }),
    ]);
    if (f.status === "fulfilled") flow = f.value;
    if (a.status === "fulfilled") fatigue = a.value;
    if (s.status === "fulfilled") streak = s.value;
  } catch {
    // daemon offline
  }
}

let refCount = 0;

export function startBrainPolling(): void {
  refCount++;
  if (polling) return;
  polling = true;
  poll(); // immediate first poll
  timer = setInterval(poll, 30_000);
}

export function stopBrainPolling(): void {
  refCount = Math.max(0, refCount - 1);
  if (refCount > 0) return; // other consumers still need it
  polling = false;
  if (timer) {
    clearInterval(timer);
    timer = undefined;
  }
}
