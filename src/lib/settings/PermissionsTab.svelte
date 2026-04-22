<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Settings tab — System Permissions -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onDestroy, onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { CardContent } from "$lib/components/ui/card";
import { IconExternalLink } from "$lib/components/ui/icons";
import { SectionHeader } from "$lib/components/ui/section-header";
import { SettingsCard } from "$lib/components/ui/settings-card";
import { StatusBadge, type StatusLevel } from "$lib/components/ui/status-badge";
import { t } from "$lib/i18n/index.svelte";

// ── Platform detection ──────────────────────────────────────────────────────
const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
const isLinux = typeof navigator !== "undefined" && /Linux/i.test(navigator.platform);
// Windows = everything else

// ── Permission status ───────────────────────────────────────────────────────
let accessibilityGranted = $state<boolean | null>(null);
let screenRecordingGranted = $state<boolean | null>(null);
let calendarPermissionStatus = $state<"authorized" | "denied" | "restricted" | "not_determined" | "unknown">("unknown");
let locationPermissionStatus = $state<"authorized" | "denied" | "restricted" | "not_determined" | "unknown">("unknown");
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function refreshAccessibility() {
  try {
    accessibilityGranted = await invoke<boolean>("check_accessibility_permission");
  } catch {
    accessibilityGranted = null;
  }
}

async function refreshScreenRecording() {
  try {
    screenRecordingGranted = await invoke<boolean>("check_screen_recording_permission");
  } catch {
    screenRecordingGranted = null;
  }
}

async function refreshCalendarPermission() {
  try {
    const s = await invoke<string>("get_calendar_permission_status");
    if (s === "authorized" || s === "denied" || s === "restricted" || s === "not_determined") {
      calendarPermissionStatus = s;
    } else {
      calendarPermissionStatus = "unknown";
    }
  } catch {
    calendarPermissionStatus = "unknown";
  }
}

async function refreshLocationPermission() {
  try {
    const s = await invoke<string>("get_location_permission_status");
    if (s === "authorized" || s === "denied" || s === "restricted" || s === "not_determined") {
      locationPermissionStatus = s;
    } else {
      locationPermissionStatus = "unknown";
    }
  } catch {
    locationPermissionStatus = "unknown";
  }
}

onMount(() => {
  refreshAccessibility();
  refreshScreenRecording();
  refreshCalendarPermission();
  refreshLocationPermission();
  // Poll every 3 s so the status updates after the user grants it in System Settings
  pollTimer = setInterval(() => {
    refreshAccessibility();
    refreshScreenRecording();
    refreshCalendarPermission();
    refreshLocationPermission();
  }, 3000);
});
onDestroy(() => {
  if (pollTimer) clearInterval(pollTimer);
});

async function openAccessibilitySettings() {
  await invoke("open_accessibility_settings");
}
async function openBluetoothSettings() {
  await invoke("open_bt_settings");
}
async function openNotificationsSettings() {
  await invoke("open_notifications_settings");
}
async function openScreenRecordingSettings() {
  await invoke("open_screen_recording_settings");
}
async function requestCalendarPermission() {
  await invoke("request_calendar_permission").catch(() => false);
  await refreshCalendarPermission();
}
async function openCalendarSettings() {
  await invoke("open_calendar_settings");
}
async function requestLocationPermission() {
  await invoke("request_location_permission").catch(() => false);
  await refreshLocationPermission();
}
async function openLocationSettings() {
  await invoke("open_location_settings");
}
async function openInputMonitoringSettings() {
  await invoke("open_input_monitoring_settings");
}
async function openFocusSettings() {
  await invoke("open_focus_settings");
}
async function openFullDiskAccessSettings() {
  await invoke("open_full_disk_access_settings");
}

// ── Status badge helper ─────────────────────────────────────────────────────
function statusLabel(s: StatusLevel): string {
  return {
    granted: t("perm.granted"),
    denied: t("perm.denied"),
    unknown: t("perm.unknown"),
    not_required: t("perm.notRequired"),
  }[s];
}

// Derive accessibility status from the polled boolean
const accessStatus = $derived<StatusLevel>(
  accessibilityGranted === null ? "unknown" : accessibilityGranted ? "granted" : "denied",
);

const screenRecordingStatus = $derived<StatusLevel>(
  screenRecordingGranted === null ? "unknown" : screenRecordingGranted ? "granted" : "denied",
);

const calendarStatus = $derived<StatusLevel>(
  calendarPermissionStatus === "authorized"
    ? "granted"
    : calendarPermissionStatus === "denied" || calendarPermissionStatus === "restricted"
      ? "denied"
      : "unknown",
);

const locationStatus = $derived<StatusLevel>(
  locationPermissionStatus === "authorized"
    ? "granted"
    : locationPermissionStatus === "denied" || locationPermissionStatus === "restricted"
      ? "denied"
      : "unknown",
);

// Bluetooth: we don't have a live API to check it — always show "system-managed"
// (the device connection status on the dashboard already shows BT state)
const bluetoothStatus: StatusLevel = "unknown";

// Notifications: not queried yet — direct user to OS settings
const notifStatus: StatusLevel = "unknown";
</script>

<div class="flex flex-col gap-5">

  <!-- ── Header ──────────────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-1">
    <p class="text-ui-md text-muted-foreground leading-relaxed max-w-prose">
      {t("perm.intro")}
    </p>
  </section>

  <!-- ── Accessibility ─────────────────────────────────────────────────────── -->
  {#if isMac}
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("perm.accessibility")}</SectionHeader>
    <SettingsCard>
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <!-- Status row -->
        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">⌨️</span>
            <span class="text-ui-lg font-semibold text-foreground">{t("perm.accessibility")}</span>
          </div>
          <div class="flex items-center gap-2">
            <StatusBadge status={accessStatus} label={statusLabel(accessStatus)} />
            <Button size="sm" variant="outline"
                    class="h-6 px-2 text-ui-base"
                    onclick={refreshAccessibility}>
              {t("common.retry")}
            </Button>
          </div>
        </div>

        <!-- Description -->
        <p class="text-ui-base text-muted-foreground leading-relaxed">
          {t("perm.accessibilityDesc")}
        </p>

        {#if accessStatus === "denied"}
        <!-- Denied — step-by-step guide -->
        <div class="rounded-lg bg-red-50 dark:bg-red-900/10 border border-red-200 dark:border-red-800/30 px-3 py-2.5
                    text-ui-base text-red-800 dark:text-red-300 leading-relaxed flex flex-col gap-1">
          <strong>{t("perm.howToGrant")}</strong>
          <ol class="flex flex-col gap-0.5 list-decimal list-inside">
            <li>{t("perm.accessStep1")}</li>
            <li>{t("perm.accessStep2")}</li>
            <li>{t("perm.accessStep3")}</li>
            <li>{t("perm.accessStep4")}</li>
          </ol>
        </div>
        {:else if accessStatus === "granted"}
        <p class="text-ui-base text-green-700 dark:text-green-400 leading-relaxed">
          {t("perm.accessibilityOk")}
        </p>
        {:else}
        <p class="text-ui-base text-muted-foreground leading-relaxed">
          {t("perm.accessibilityPending")}
        </p>
        {/if}

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-ui-md h-7"
                  onclick={openAccessibilitySettings}>
            {t("perm.openAccessibilitySettings")}
            <IconExternalLink class="w-3 h-3 ml-1 shrink-0" />
          </Button>
        </div>

      </CardContent>
    </SettingsCard>
  </section>
  {/if}

  <!-- ── Screen Recording ────────────────────────────────────────────────── -->
  {#if isMac}
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("perm.screenRecording")}</SectionHeader>
    <SettingsCard>
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">🖥️</span>
            <span class="text-ui-lg font-semibold text-foreground">{t("perm.screenRecording")}</span>
          </div>
          <div class="flex items-center gap-2">
            <StatusBadge status={screenRecordingStatus} label={statusLabel(screenRecordingStatus)} />
            <Button size="sm" variant="outline"
                    class="h-6 px-2 text-ui-base"
                    onclick={refreshScreenRecording}>
              {t("common.retry")}
            </Button>
          </div>
        </div>

        <p class="text-ui-base text-muted-foreground leading-relaxed">
          {t("perm.screenRecordingDesc")}
        </p>

        {#if screenRecordingStatus === "denied"}
        <div class="rounded-lg bg-red-50 dark:bg-red-900/10 border border-red-200 dark:border-red-800/30 px-3 py-2.5
                    text-ui-base text-red-800 dark:text-red-300 leading-relaxed flex flex-col gap-1">
          <strong>{t("perm.howToGrant")}</strong>
          <ol class="flex flex-col gap-0.5 list-decimal list-inside">
            <li>{t("perm.screenRecordingStep1")}</li>
            <li>{t("perm.screenRecordingStep2")}</li>
            <li>{t("perm.screenRecordingStep3")}</li>
          </ol>
        </div>
        {:else if screenRecordingStatus === "granted"}
        <p class="text-ui-base text-green-700 dark:text-green-400 leading-relaxed">
          {t("perm.screenRecordingOk")}
        </p>
        {/if}

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-ui-md h-7"
                  onclick={openScreenRecordingSettings}>
            {t("perm.openScreenRecordingSettings")}
            <IconExternalLink class="w-3 h-3 ml-1 shrink-0" />
          </Button>
        </div>

      </CardContent>
    </SettingsCard>
  </section>
  {/if}

  <!-- ── Full Disk Access ──────────────────────────────────────────────────── -->
  {#if isMac}
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("perm.fullDiskAccess")}</SectionHeader>
    <SettingsCard>
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">🔒</span>
            <span class="text-ui-lg font-semibold text-foreground">{t("perm.fullDiskAccess")}</span>
          </div>
        </div>

        <p class="text-ui-base text-muted-foreground leading-relaxed">
          {t("perm.fullDiskAccessDesc")}
        </p>

        <div class="rounded-lg bg-amber-50 dark:bg-amber-900/10 border border-amber-200 dark:border-amber-800/30 px-3 py-2.5
                    text-ui-base text-amber-800 dark:text-amber-300 leading-relaxed flex flex-col gap-1">
          <strong>{t("perm.howToGrant")}</strong>
          <ol class="flex flex-col gap-0.5 list-decimal list-inside">
            <li>{t("perm.fullDiskAccessStep1")}</li>
            <li>{t("perm.fullDiskAccessStep2")}</li>
            <li>{t("perm.fullDiskAccessStep3")}</li>
          </ol>
        </div>

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-ui-md h-7"
                  onclick={openFullDiskAccessSettings}>
            {t("perm.openFullDiskAccessSettings")}
            <IconExternalLink class="w-3 h-3 ml-1 shrink-0" />
          </Button>
        </div>

      </CardContent>
    </SettingsCard>
  </section>
  {/if}

  <!-- ── Calendar ─────────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("perm.calendar")}</SectionHeader>
    <SettingsCard>
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">🗓️</span>
            <span class="text-ui-lg font-semibold text-foreground">{t("perm.calendar")}</span>
          </div>
          <div class="flex items-center gap-2">
            <StatusBadge status={calendarStatus} label={statusLabel(calendarStatus)} />
            <Button size="sm" variant="outline"
                    class="h-6 px-2 text-ui-base"
                    onclick={refreshCalendarPermission}>
              {t("common.retry")}
            </Button>
          </div>
        </div>

        <p class="text-ui-base text-muted-foreground leading-relaxed">
          {t("perm.calendarDesc")}
        </p>

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-ui-md h-7"
                  onclick={requestCalendarPermission}>
            {t("perm.requestCalendarPermission")}
          </Button>
          {#if calendarPermissionStatus === "denied"}
            <Button size="sm" variant="outline"
                    class="text-ui-md h-7"
                    onclick={openCalendarSettings}>
              {t("perm.openCalendarSettings")}
              <IconExternalLink class="w-3 h-3 ml-1 shrink-0" />
            </Button>
          {/if}
        </div>

      </CardContent>
    </SettingsCard>
  </section>

  <!-- ── Location ──────────────────────────────────────────────────────────── -->
  {#if isMac}
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("perm.location")}</SectionHeader>
    <SettingsCard>
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">📍</span>
            <span class="text-ui-lg font-semibold text-foreground">{t("perm.location")}</span>
          </div>
          <div class="flex items-center gap-2">
            <StatusBadge status={locationStatus} label={statusLabel(locationStatus)} />
            <Button size="sm" variant="outline"
                    class="h-6 px-2 text-ui-base"
                    onclick={refreshLocationPermission}>
              {t("common.retry")}
            </Button>
          </div>
        </div>

        <p class="text-ui-base text-muted-foreground leading-relaxed">
          {t("perm.locationDesc")}
        </p>

        {#if locationStatus === "denied"}
        <div class="rounded-lg bg-amber-50 dark:bg-amber-900/10 border border-amber-200 dark:border-amber-800/30 px-3 py-2.5
                    text-ui-base text-amber-800 dark:text-amber-300 leading-relaxed flex flex-col gap-1">
          <strong>{t("perm.locationFallback")}</strong>
          <ol class="flex flex-col gap-0.5 list-decimal list-inside">
            <li>{t("perm.locationStep1")}</li>
            <li>{t("perm.locationStep2")}</li>
            <li>{t("perm.locationStep3")}</li>
          </ol>
        </div>
        {:else if locationStatus === "granted"}
        <p class="text-ui-base text-green-700 dark:text-green-400 leading-relaxed">
          {t("perm.locationOk")}
        </p>
        {/if}

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-ui-md h-7"
                  onclick={requestLocationPermission}>
            {t("perm.requestLocationPermission")}
          </Button>
          {#if locationPermissionStatus === "denied"}
            <Button size="sm" variant="outline"
                    class="text-ui-md h-7"
                    onclick={openLocationSettings}>
              {t("perm.openLocationSettings")}
              <IconExternalLink class="w-3 h-3 ml-1 shrink-0" />
            </Button>
          {/if}
        </div>

      </CardContent>
    </SettingsCard>
  </section>
  {/if}

  <!-- ── Bluetooth ─────────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("perm.bluetooth")}</SectionHeader>
    <SettingsCard>
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">📶</span>
            <span class="text-ui-lg font-semibold text-foreground">{t("perm.bluetooth")}</span>
          </div>
          <StatusBadge status="not_required" label={t("perm.systemManaged")} />
        </div>

        <p class="text-ui-base text-muted-foreground leading-relaxed">
          {t("perm.bluetoothDesc")}
        </p>

        {#if isMac}
        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-ui-md h-7"
                  onclick={openBluetoothSettings}>
            {t("perm.openBluetoothSettings")}
            <IconExternalLink class="w-3 h-3 ml-1 shrink-0" />
          </Button>
        </div>
        {/if}

      </CardContent>
    </SettingsCard>
  </section>

  <!-- ── Notifications ─────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("perm.notifications")}</SectionHeader>
    <SettingsCard>
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">🔔</span>
            <span class="text-ui-lg font-semibold text-foreground">{t("perm.notifications")}</span>
          </div>
          <StatusBadge status="not_required" label={t("perm.systemManaged")} />
        </div>

        <p class="text-ui-base text-muted-foreground leading-relaxed">
          {t("perm.notificationsDesc")}
        </p>

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-ui-md h-7"
                  onclick={openNotificationsSettings}>
            {t("perm.openNotificationsSettings")}
            <IconExternalLink class="w-3 h-3 ml-1 shrink-0" />
          </Button>
        </div>

      </CardContent>
    </SettingsCard>
  </section>

  <!-- ── Platform permission matrix ────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("perm.matrix")}</SectionHeader>
    <SettingsCard>
      <CardContent class="p-0">
        <table class="w-full text-ui-base">
          <thead>
            <tr class="divide-x divide-border dark:divide-white/[0.05]
                       bg-muted/40 dark:bg-white/[0.02] border-b border-border dark:border-white/[0.06]">
              <th class="px-3 py-2 text-left font-semibold text-muted-foreground">{t("perm.feature")}</th>
              <th class="px-3 py-2 text-center font-semibold text-muted-foreground">macOS</th>
              <th class="px-3 py-2 text-center font-semibold text-muted-foreground">Linux</th>
              <th class="px-3 py-2 text-center font-semibold text-muted-foreground">Windows</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border dark:divide-white/[0.05]">
            {#each [
              [t("perm.matrixBluetooth"),        "✅ " + t("perm.matrixNone"), "✅ " + t("perm.matrixNone"), "✅ " + t("perm.matrixNone")],
              [t("perm.matrixKeyboardMouse"),     "🔑 " + t("perm.matrixAccessibility"), "✅ libxtst", "✅ " + t("perm.matrixNone")],
              [t("perm.matrixActiveWindow"),      "✅ " + t("perm.matrixNone"), "✅ xdotool", "✅ " + t("perm.matrixNone")],
              [t("perm.matrixNotifications"),     "⚙️ " + t("perm.matrixOsPrompt"), "✅ " + t("perm.matrixNone"), "⚙️ " + t("perm.matrixOsPrompt")],
              [t("perm.matrixScreenRecording"),  "🔑 " + t("perm.matrixScreenRecordingReq"), "✅ " + t("perm.matrixNone"), "✅ " + t("perm.matrixNone")],
              [t("perm.matrixLocation"),            "🔑 " + t("perm.matrixLocationReq"),        "✅ " + t("perm.matrixNone"), "✅ " + t("perm.matrixNone")],
            ] as [feat, mac, linux, win]}
              <tr class="divide-x divide-border dark:divide-white/[0.05]">
                <td class="px-3 py-2 text-foreground/80">{feat}</td>
                <td class="px-3 py-2 text-center text-muted-foreground">{mac}</td>
                <td class="px-3 py-2 text-center text-muted-foreground">{linux}</td>
                <td class="px-3 py-2 text-center text-muted-foreground">{win}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <div class="px-4 py-2.5 border-t border-border dark:border-white/[0.06]
                    text-ui-sm text-muted-foreground/60 flex flex-wrap gap-x-3 gap-y-1">
          <span>✅ {t("perm.legendNone")}</span>
          <span>🔑 {t("perm.legendRequired")}</span>
          <span>⚙️ {t("perm.legendPrompt")}</span>
        </div>
      </CardContent>
    </SettingsCard>
  </section>

  <!-- ── "Why does this app need X?" explainer ─────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("perm.why")}</SectionHeader>
    <SettingsCard>
      <CardContent class="px-4 py-3.5">
        <div class="rounded-xl bg-muted/50 dark:bg-surface-3 px-4 py-4
                    text-ui-base text-muted-foreground leading-relaxed flex flex-col gap-2">
          <p>🔵 <strong class="text-foreground">{t("perm.whyBluetooth")}</strong> — {t("perm.whyBluetoothDesc")}</p>
          <p>⌨️ <strong class="text-foreground">{t("perm.whyAccessibility")}</strong> — {t("perm.whyAccessibilityDesc")}</p>
          <p>🔔 <strong class="text-foreground">{t("perm.whyNotifications")}</strong> — {t("perm.whyNotificationsDesc")}</p>
          <p>🖥️ <strong class="text-foreground">{t("perm.whyScreenRecording")}</strong> — {t("perm.whyScreenRecordingDesc")}</p>
          <p>📍 <strong class="text-foreground">{t("perm.whyLocation")}</strong> — {t("perm.whyLocationDesc")}</p>
          <p class="pt-1 text-ui-sm">{t("perm.privacyNote")}</p>
        </div>
      </CardContent>
    </SettingsCard>
  </section>

</div>
