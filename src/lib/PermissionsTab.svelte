<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Settings tab — System Permissions -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { Card, CardContent }  from "$lib/components/ui/card";
  import { Button }             from "$lib/components/ui/button";
  import { t }                  from "$lib/i18n/index.svelte";

  // ── Platform detection ──────────────────────────────────────────────────────
  const isMac   = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
  const isLinux = typeof navigator !== "undefined" && /Linux/i.test(navigator.platform);
  // Windows = everything else

  // ── Permission status ───────────────────────────────────────────────────────
  let accessibilityGranted = $state<boolean | null>(null);
  let screenRecordingGranted = $state<boolean | null>(null);
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

  onMount(() => {
    refreshAccessibility();
    refreshScreenRecording();
    // Poll every 3 s so the status updates after the user grants it in System Settings
    pollTimer = setInterval(() => {
      refreshAccessibility();
      refreshScreenRecording();
    }, 3000);
  });
  onDestroy(() => { if (pollTimer) clearInterval(pollTimer); });

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

  // ── Status badge helper ─────────────────────────────────────────────────────
  type Status = "granted" | "denied" | "unknown" | "not_required";
  function statusClass(s: Status): string {
    return {
      granted:      "bg-green-500/15 text-green-700 dark:text-green-400 border-green-500/30",
      denied:       "bg-red-500/15 text-red-700 dark:text-red-400 border-red-500/30",
      unknown:      "bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30",
      not_required: "bg-muted/60 text-muted-foreground border-border",
    }[s];
  }
  function statusLabel(s: Status): string {
    return {
      granted:      t("perm.granted"),
      denied:       t("perm.denied"),
      unknown:      t("perm.unknown"),
      not_required: t("perm.notRequired"),
    }[s];
  }
  function statusDot(s: Status): string {
    return {
      granted:      "bg-green-500",
      denied:       "bg-red-500",
      unknown:      "bg-amber-400",
      not_required: "bg-muted-foreground/40",
    }[s];
  }

  // Derive accessibility status from the polled boolean
  const accessStatus = $derived<Status>(
    accessibilityGranted === null ? "unknown"
      : accessibilityGranted       ? "granted"
      : "denied"
  );

  const screenRecordingStatus = $derived<Status>(
    screenRecordingGranted === null ? "unknown"
      : screenRecordingGranted       ? "granted"
      : "denied"
  );

  // Bluetooth: we don't have a live API to check it — always show "system-managed"
  // (the device connection status on the dashboard already shows BT state)
  const bluetoothStatus: Status = "unknown";

  // Notifications: not queried yet — direct user to OS settings
  const notifStatus: Status = "unknown";
</script>

<div class="flex flex-col gap-5">

  <!-- ── Header ──────────────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-1">
    <p class="text-[0.72rem] text-muted-foreground leading-relaxed max-w-prose">
      {t("perm.intro")}
    </p>
  </section>

  <!-- ── Accessibility ─────────────────────────────────────────────────────── -->
  {#if isMac}
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("perm.accessibility")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <!-- Status row -->
        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">⌨️</span>
            <span class="text-[0.8rem] font-semibold text-foreground">{t("perm.accessibility")}</span>
          </div>
          <div class="flex items-center gap-2">
            <span class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[0.65rem] font-semibold
                         {statusClass(accessStatus)}">
              <span class="w-1.5 h-1.5 rounded-full {statusDot(accessStatus)}"></span>
              {statusLabel(accessStatus)}
            </span>
            <Button size="sm" variant="outline"
                    class="h-6 px-2 text-[0.65rem]"
                    onclick={refreshAccessibility}>
              {t("common.retry")}
            </Button>
          </div>
        </div>

        <!-- Description -->
        <p class="text-[0.68rem] text-muted-foreground leading-relaxed">
          {t("perm.accessibilityDesc")}
        </p>

        {#if accessStatus === "denied"}
        <!-- Denied — step-by-step guide -->
        <div class="rounded-lg bg-red-50 dark:bg-red-900/10 border border-red-200 dark:border-red-800/30 px-3 py-2.5
                    text-[0.68rem] text-red-800 dark:text-red-300 leading-relaxed flex flex-col gap-1">
          <strong>{t("perm.howToGrant")}</strong>
          <ol class="flex flex-col gap-0.5 list-decimal list-inside">
            <li>{t("perm.accessStep1")}</li>
            <li>{t("perm.accessStep2")}</li>
            <li>{t("perm.accessStep3")}</li>
            <li>{t("perm.accessStep4")}</li>
          </ol>
        </div>
        {:else if accessStatus === "granted"}
        <p class="text-[0.67rem] text-green-700 dark:text-green-400 leading-relaxed">
          {t("perm.accessibilityOk")}
        </p>
        {:else}
        <p class="text-[0.67rem] text-muted-foreground leading-relaxed">
          {t("perm.accessibilityPending")}
        </p>
        {/if}

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-[0.7rem] h-7"
                  onclick={openAccessibilitySettings}>
            {t("perm.openAccessibilitySettings")}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                 class="w-3 h-3 ml-1 shrink-0">
              <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
              <polyline points="15 3 21 3 21 9"/>
              <line x1="10" y1="14" x2="21" y2="3"/>
            </svg>
          </Button>
        </div>

      </CardContent>
    </Card>
  </section>
  {/if}

  <!-- ── Screen Recording ────────────────────────────────────────────────── -->
  {#if isMac}
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("perm.screenRecording")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">🖥️</span>
            <span class="text-[0.8rem] font-semibold text-foreground">{t("perm.screenRecording")}</span>
          </div>
          <div class="flex items-center gap-2">
            <span class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[0.65rem] font-semibold
                         {statusClass(screenRecordingStatus)}">
              <span class="w-1.5 h-1.5 rounded-full {statusDot(screenRecordingStatus)}"></span>
              {statusLabel(screenRecordingStatus)}
            </span>
            <Button size="sm" variant="outline"
                    class="h-6 px-2 text-[0.65rem]"
                    onclick={refreshScreenRecording}>
              {t("common.retry")}
            </Button>
          </div>
        </div>

        <p class="text-[0.68rem] text-muted-foreground leading-relaxed">
          {t("perm.screenRecordingDesc")}
        </p>

        {#if screenRecordingStatus === "denied"}
        <div class="rounded-lg bg-red-50 dark:bg-red-900/10 border border-red-200 dark:border-red-800/30 px-3 py-2.5
                    text-[0.68rem] text-red-800 dark:text-red-300 leading-relaxed flex flex-col gap-1">
          <strong>{t("perm.howToGrant")}</strong>
          <ol class="flex flex-col gap-0.5 list-decimal list-inside">
            <li>{t("perm.screenRecordingStep1")}</li>
            <li>{t("perm.screenRecordingStep2")}</li>
            <li>{t("perm.screenRecordingStep3")}</li>
          </ol>
        </div>
        {:else if screenRecordingStatus === "granted"}
        <p class="text-[0.67rem] text-green-700 dark:text-green-400 leading-relaxed">
          {t("perm.screenRecordingOk")}
        </p>
        {/if}

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-[0.7rem] h-7"
                  onclick={openScreenRecordingSettings}>
            {t("perm.openScreenRecordingSettings")}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                 class="w-3 h-3 ml-1 shrink-0">
              <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
              <polyline points="15 3 21 3 21 9"/>
              <line x1="10" y1="14" x2="21" y2="3"/>
            </svg>
          </Button>
        </div>

      </CardContent>
    </Card>
  </section>
  {/if}

  <!-- ── Bluetooth ─────────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("perm.bluetooth")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">📶</span>
            <span class="text-[0.8rem] font-semibold text-foreground">{t("perm.bluetooth")}</span>
          </div>
          <span class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[0.65rem] font-semibold
                       {statusClass('not_required')}">
            <span class="w-1.5 h-1.5 rounded-full {statusDot('not_required')}"></span>
            {t("perm.systemManaged")}
          </span>
        </div>

        <p class="text-[0.68rem] text-muted-foreground leading-relaxed">
          {t("perm.bluetoothDesc")}
        </p>

        {#if isMac}
        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-[0.7rem] h-7"
                  onclick={openBluetoothSettings}>
            {t("perm.openBluetoothSettings")}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                 class="w-3 h-3 ml-1 shrink-0">
              <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
              <polyline points="15 3 21 3 21 9"/>
              <line x1="10" y1="14" x2="21" y2="3"/>
            </svg>
          </Button>
        </div>
        {/if}

      </CardContent>
    </Card>
  </section>

  <!-- ── Notifications ─────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("perm.notifications")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="px-4 py-3.5 flex flex-col gap-3">

        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2">
            <span class="text-base">🔔</span>
            <span class="text-[0.8rem] font-semibold text-foreground">{t("perm.notifications")}</span>
          </div>
          <span class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[0.65rem] font-semibold
                       {statusClass('not_required')}">
            <span class="w-1.5 h-1.5 rounded-full {statusDot('not_required')}"></span>
            {t("perm.systemManaged")}
          </span>
        </div>

        <p class="text-[0.68rem] text-muted-foreground leading-relaxed">
          {t("perm.notificationsDesc")}
        </p>

        <div class="flex items-center gap-2">
          <Button size="sm" variant="outline"
                  class="text-[0.7rem] h-7"
                  onclick={openNotificationsSettings}>
            {t("perm.openNotificationsSettings")}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                 class="w-3 h-3 ml-1 shrink-0">
              <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
              <polyline points="15 3 21 3 21 9"/>
              <line x1="10" y1="14" x2="21" y2="3"/>
            </svg>
          </Button>
        </div>

      </CardContent>
    </Card>
  </section>

  <!-- ── Platform permission matrix ────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("perm.matrix")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="p-0">
        <table class="w-full text-[0.68rem]">
          <thead>
            <tr class="divide-x divide-border dark:divide-white/[0.05]
                       bg-muted/40 dark:bg-white/[0.02] border-b border-border dark:border-white/[0.05]">
              <th class="px-3 py-2 text-left font-semibold text-muted-foreground">{t("perm.feature")}</th>
              <th class="px-3 py-2 text-center font-semibold text-muted-foreground">macOS</th>
              <th class="px-3 py-2 text-center font-semibold text-muted-foreground">Linux</th>
              <th class="px-3 py-2 text-center font-semibold text-muted-foreground">Windows</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border dark:divide-white/[0.04]">
            {#each [
              [t("perm.matrixBluetooth"),        "✅ " + t("perm.matrixNone"), "✅ " + t("perm.matrixNone"), "✅ " + t("perm.matrixNone")],
              [t("perm.matrixKeyboardMouse"),     "🔑 " + t("perm.matrixAccessibility"), "✅ libxtst", "✅ " + t("perm.matrixNone")],
              [t("perm.matrixActiveWindow"),      "✅ " + t("perm.matrixNone"), "✅ xdotool", "✅ " + t("perm.matrixNone")],
              [t("perm.matrixNotifications"),     "⚙️ " + t("perm.matrixOsPrompt"), "✅ " + t("perm.matrixNone"), "⚙️ " + t("perm.matrixOsPrompt")],
              [t("perm.matrixScreenRecording"),  "🔑 " + t("perm.matrixScreenRecordingReq"), "✅ " + t("perm.matrixNone"), "✅ " + t("perm.matrixNone")],
            ] as [feat, mac, linux, win]}
              <tr class="divide-x divide-border dark:divide-white/[0.04]">
                <td class="px-3 py-2 text-foreground/80">{feat}</td>
                <td class="px-3 py-2 text-center text-muted-foreground">{mac}</td>
                <td class="px-3 py-2 text-center text-muted-foreground">{linux}</td>
                <td class="px-3 py-2 text-center text-muted-foreground">{win}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <div class="px-4 py-2.5 border-t border-border dark:border-white/[0.05]
                    text-[0.62rem] text-muted-foreground/60 flex flex-wrap gap-x-3 gap-y-1">
          <span>✅ {t("perm.legendNone")}</span>
          <span>🔑 {t("perm.legendRequired")}</span>
          <span>⚙️ {t("perm.legendPrompt")}</span>
        </div>
      </CardContent>
    </Card>
  </section>

  <!-- ── "Why does this app need X?" explainer ─────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("perm.why")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="px-4 py-3.5">
        <div class="rounded-xl bg-muted/50 dark:bg-[#0f0f18] px-4 py-4
                    text-[0.68rem] text-muted-foreground leading-relaxed flex flex-col gap-2">
          <p>🔵 <strong class="text-foreground">{t("perm.whyBluetooth")}</strong> — {t("perm.whyBluetoothDesc")}</p>
          <p>⌨️ <strong class="text-foreground">{t("perm.whyAccessibility")}</strong> — {t("perm.whyAccessibilityDesc")}</p>
          <p>🔔 <strong class="text-foreground">{t("perm.whyNotifications")}</strong> — {t("perm.whyNotificationsDesc")}</p>
          <p>🖥️ <strong class="text-foreground">{t("perm.whyScreenRecording")}</strong> — {t("perm.whyScreenRecordingDesc")}</p>
          <p class="pt-1 text-[0.62rem]">{t("perm.privacyNote")}</p>
        </div>
      </CardContent>
    </Card>
  </section>

</div>
