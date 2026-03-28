// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Shared window-navigation helpers.
// Each function calls the corresponding Tauri `open_*_window` command.
// Centralises the repeated `async function openX() { await invoke("open_x_window"); }` pattern.

import { invoke } from "@tauri-apps/api/core";

export async function openSettings(): Promise<void> {
  await invoke("open_settings_window");
}
export async function openHelp(): Promise<void> {
  await invoke("open_help_window");
}
export async function openHistory(): Promise<void> {
  await invoke("open_history_window");
}
export async function openLabel(): Promise<void> {
  await invoke("open_label_window");
}
export async function openLabels(): Promise<void> {
  await invoke("open_labels_window");
}
export async function openSearch(): Promise<void> {
  await invoke("open_search_window");
}
export async function openCompare(): Promise<void> {
  await invoke("open_compare_window");
}
export async function openDownloads(): Promise<void> {
  await invoke("open_downloads_window");
}
export async function openCalibration(): Promise<void> {
  await invoke("open_calibration_window");
}
export async function openFocusTimer(): Promise<void> {
  await invoke("open_focus_timer_window");
}
export async function openOnboarding(): Promise<void> {
  await invoke("open_onboarding_window");
}
export async function openUpdates(): Promise<void> {
  await invoke("open_updates_window");
}
export async function openApi(): Promise<void> {
  await invoke("open_api_window");
}
export async function openWhatsNew(): Promise<void> {
  await invoke("open_whats_new_window");
}
export async function openBtSettings(): Promise<void> {
  await invoke("open_bt_settings");
}

/** Open the settings window and navigate to a specific tab. */
export async function openSettingsTab(tab: string): Promise<void> {
  await invoke("open_settings_window");
  // Small delay to ensure the window is ready to receive events
  await new Promise((r) => setTimeout(r, 150));
  const { emit } = await import("@tauri-apps/api/event");
  await emit("switch-tab", tab);
}
