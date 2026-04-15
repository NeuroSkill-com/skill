// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Cmd-K usage history — tracks which commands users run most often / recently.
 * Persisted to localStorage so recency/frequency boosts survive reloads.
 */

const STORAGE_KEY = "cmdk-history";
const MAX_ENTRIES = 500;

interface UsageEntry {
  count: number;
  lastUsed: number;
}

let history = $state<Record<string, UsageEntry>>(load());

function load(): Record<string, UsageEntry> {
  if (typeof localStorage === "undefined") return {};
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || "{}");
  } catch {
    return {};
  }
}

function save() {
  if (typeof localStorage === "undefined") return;
  // LRU eviction if too large
  const entries = Object.entries(history);
  if (entries.length > MAX_ENTRIES) {
    entries.sort((a, b) => b[1].lastUsed - a[1].lastUsed);
    history = Object.fromEntries(entries.slice(0, MAX_ENTRIES));
  }
  localStorage.setItem(STORAGE_KEY, JSON.stringify(history));
}

export function recordUsage(id: string) {
  const now = Date.now();
  const prev = history[id];
  history[id] = { count: (prev?.count ?? 0) + 1, lastUsed: now };
  save();
}

export function getUsageStats(): Record<string, UsageEntry> {
  return history;
}

/**
 * Compute a boost score for a command based on usage frequency and recency.
 * Returns 0 for never-used commands.
 */
export function usageBoost(id: string): number {
  const entry = history[id];
  if (!entry) return 0;
  const now = Date.now();
  const recency = Math.max(0, 1 - (now - entry.lastUsed) / (7 * 86_400_000)); // decays over 7 days
  return Math.log2(entry.count + 1) * 3 + recency * 5;
}

/**
 * Get the N most recently used command IDs, sorted by last use time.
 */
export function getRecentIds(n = 5): string[] {
  return Object.entries(history)
    .sort((a, b) => b[1].lastUsed - a[1].lastUsed)
    .slice(0, n)
    .map(([id]) => id);
}
