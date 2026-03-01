// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Reactive toast notification store.
 *
 * Toasts are added via `addToast()` and auto-dismissed after a configurable
 * duration.  The `toasts` array is a Svelte 5 `$state` so the UI re-renders
 * automatically when toasts are added or removed.
 */

export type ToastLevel = "info" | "success" | "warning" | "error";

export interface Toast {
  id: number;
  level: ToastLevel;
  title: string;
  message: string;
  /** Auto-dismiss duration in milliseconds. 0 = manual dismiss only. */
  duration: number;
}

let nextId = 0;

// Exported $state must never be reassigned — only mutated via splice/push.
export const toasts = $state<Toast[]>([]);

const DEFAULT_DURATIONS: Record<ToastLevel, number> = {
  info:    5_000,
  success: 4_000,
  warning: 6_000,
  error:   8_000,
};

/**
 * Add a toast notification.  Returns the toast id (useful for programmatic dismissal).
 */
export function addToast(
  level: ToastLevel,
  title: string,
  message: string,
  duration?: number,
): number {
  const id = nextId++;
  const dur = duration ?? DEFAULT_DURATIONS[level];
  toasts.push({ id, level, title, message, duration: dur });

  if (dur > 0) {
    setTimeout(() => dismissToast(id), dur);
  }
  return id;
}

/** Remove a toast by id. */
export function dismissToast(id: number) {
  const idx = toasts.findIndex((t) => t.id === id);
  if (idx !== -1) toasts.splice(idx, 1);
}
