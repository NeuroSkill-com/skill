<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Updates tab — check for updates, auto-update toggle, download + install. -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { check }              from "@tauri-apps/plugin-updater";
  import { relaunch }           from "@tauri-apps/plugin-process";
  import { Button }             from "$lib/components/ui/button";
  import { Card, CardContent }  from "$lib/components/ui/card";
  import { t }                  from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────
  interface BackgroundUpdate {
    version: string;
    date?:   string;
    body?:   string;
  }

  // ── State ──────────────────────────────────────────────────────────────────
  let appVersion     = $state("…");
  let checking       = $state(false);
  let available      = $state<{ version: string; date?: string; body?: string } | null>(null);
  let downloading    = $state(false);
  let progress       = $state(0);       // 0–100
  let ready          = $state(false);
  let error          = $state("");
  let lastCheckedUtc = $state(0);

  // Autostart
  let autostartEnabled  = $state(false);
  let autostartSaving   = $state(false);
  let autostartError    = $state("");

  // Update-check interval (backend-persisted)
  let checkIntervalSecs  = $state(3600);
  let intervalSaving     = $state(false);

  // ── Interval options ───────────────────────────────────────────────────────
  const INTERVAL_OPTIONS: [number, string][] = [
    [900,   "updates.interval15m"],
    [1800,  "updates.interval30m"],
    [3600,  "updates.interval1h"],
    [14400, "updates.interval4h"],
    [86400, "updates.interval24h"],
    [0,     "updates.intervalOff"],
  ];

  // ── Last-checked persistence (localStorage) ───────────────────────────────
  const LAST_KEY = "lastUpdateCheckUtc";

  function loadLastChecked() {
    try {
      const l = localStorage.getItem(LAST_KEY);
      if (l) lastCheckedUtc = Number(l) || 0;
    } catch {}
  }

  function saveLastChecked() {
    lastCheckedUtc = Math.floor(Date.now() / 1000);
    try { localStorage.setItem(LAST_KEY, String(lastCheckedUtc)); } catch {}
  }

  // ── Check for updates ─────────────────────────────────────────────────────
  async function checkForUpdate() {
    error     = "";
    available = null;
    checking  = true;
    try {
      const update = await check();
      saveLastChecked();
      if (update) {
        available = {
          version: update.version,
          date:    update.date ?? undefined,
          body:    update.body ?? undefined,
        };
        downloading = true;
        progress    = 0;
        let downloaded = 0;
        let contentLength = 0;
        await update.downloadAndInstall((event) => {
          switch (event.event) {
            case "Started":
              contentLength = (event.data as any)?.contentLength ?? 0;
              break;
            case "Progress":
              downloaded += (event.data as any)?.chunkLength ?? 0;
              progress = contentLength > 0
                ? Math.min(100, Math.round((downloaded / contentLength) * 100))
                : 0;
              break;
            case "Finished":
              progress = 100;
              break;
          }
        });
        downloading = false;
        ready = true;
      } else {
        available = null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      checking    = false;
      downloading = false;
    }
  }

  function fmtLastChecked(): string {
    if (!lastCheckedUtc) return t("common.never");
    const d = new Date(lastCheckedUtc * 1000);
    return d.toLocaleDateString(undefined, {
      month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
    });
  }

  // ── Autostart ─────────────────────────────────────────────────────────────
  async function toggleAutostart() {
    autostartError  = "";
    autostartSaving = true;
    try {
      await invoke("set_autostart_enabled", { enabled: !autostartEnabled });
      autostartEnabled = !autostartEnabled;
    } catch (e) {
      autostartError = String(e);
    } finally {
      autostartSaving = false;
    }
  }

  // ── Update-check interval ─────────────────────────────────────────────────
  async function setInterval(secs: number) {
    intervalSaving    = true;
    checkIntervalSecs = secs;
    try {
      await invoke("set_update_check_interval", { secs });
    } finally {
      intervalSaving = false;
    }
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────────
  let unlisteners: UnlistenFn[] = [];

  onMount(async () => {
    loadLastChecked();
    appVersion = await invoke<string>("get_app_version");

    // Read persisted settings from backend
    const [autoEnabled, intervalSecs] = await Promise.all([
      invoke<boolean>("get_autostart_enabled").catch(() => false),
      invoke<number>("get_update_check_interval").catch(() => 3600),
    ]);
    autostartEnabled  = autoEnabled;
    checkIntervalSecs = intervalSecs;

    // Listen for background update-available events emitted by the Rust task
    unlisteners.push(
      await listen<BackgroundUpdate>("update-available", (ev) => {
        if (!available && !downloading && !ready) {
          available = ev.payload;
          saveLastChecked();
        }
      }),
      await listen("update-checked", () => {
        saveLastChecked();
      }),
    );
  });

  onDestroy(() => unlisteners.forEach(u => u()));
</script>

<section class="flex flex-col gap-4">

  <!-- ── Version hero ───────────────────────────────────────────────────────── -->
  <div class="rounded-2xl border border-border dark:border-white/[0.06]
              bg-gradient-to-r from-sky-500/10 via-blue-500/10 to-indigo-500/10
              dark:from-sky-500/15 dark:via-blue-500/15 dark:to-indigo-500/15
              px-5 py-4 flex items-center gap-4">
    <div class="flex items-center justify-center w-11 h-11 rounded-xl
                bg-gradient-to-br from-sky-500 to-blue-600
                shadow-lg shadow-blue-500/25 dark:shadow-blue-500/40 shrink-0">
      <span class="text-xl leading-none">⬆</span>
    </div>
    <div class="flex flex-col gap-0.5">
      <span class="text-[0.82rem] font-bold">{t("updates.title")}</span>
      <span class="text-[0.55rem] text-muted-foreground/70">
        {t("updates.currentVersion", { version: appVersion })}
      </span>
    </div>
    <span class="flex-1"></span>
    {#if ready}
      <span class="text-emerald-500 font-bold text-[0.72rem]">✅ {t("updates.readyToRestart")}</span>
    {:else if downloading}
      <span class="text-blue-500 font-semibold text-[0.65rem] tabular-nums">{progress}%</span>
    {:else if checking}
      <svg class="w-4 h-4 text-muted-foreground animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
      </svg>
    {/if}
  </div>

  <!-- ── Update status card ─────────────────────────────────────────────────── -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <div class="flex flex-col gap-3 px-4 py-4">
        {#if ready}
          <!-- Update installed, ready to relaunch -->
          <div class="flex items-center gap-3">
            <div class="w-10 h-10 rounded-full bg-emerald-500/10 flex items-center justify-center text-xl shrink-0">
              ✅
            </div>
            <div class="flex flex-col gap-0.5 flex-1">
              <span class="text-[0.78rem] font-semibold text-emerald-600 dark:text-emerald-400">
                {t("updates.installed", { version: available?.version ?? "" })}
              </span>
              <span class="text-[0.65rem] text-muted-foreground">
                {t("updates.restartToApply")}
              </span>
            </div>
            <Button size="sm" class="text-[0.72rem] h-8 px-4" onclick={() => relaunch()}>
              {t("updates.restartNow")}
            </Button>
          </div>

        {:else if downloading}
          <!-- Downloading -->
          <div class="flex flex-col gap-2.5">
            <div class="flex items-center gap-2">
              <span class="text-[0.78rem] font-semibold text-foreground">
                {t("updates.downloading", { version: available?.version ?? "" })}
              </span>
              <span class="ml-auto text-[0.72rem] font-bold text-blue-500 tabular-nums">
                {progress}%
              </span>
            </div>
            <div class="h-2 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
              <div class="h-full rounded-full bg-blue-500 transition-all duration-300"
                   style="width:{progress}%"></div>
            </div>
            {#if available?.body}
              <p class="text-[0.6rem] text-muted-foreground/70 line-clamp-3">{available.body}</p>
            {/if}
          </div>

        {:else if available && !checking}
          <!-- Update available (download starts immediately on click) -->
          <div class="flex items-center gap-3">
            <div class="w-10 h-10 rounded-full bg-blue-500/10 flex items-center justify-center text-xl shrink-0">
              ⬆
            </div>
            <div class="flex flex-col gap-0.5 flex-1">
              <span class="text-[0.78rem] font-semibold text-blue-600 dark:text-blue-400">
                v{available.version} {t("updates.available")}
              </span>
              {#if available.body}
                <span class="text-[0.65rem] text-muted-foreground line-clamp-2">{available.body}</span>
              {/if}
            </div>
            <Button size="sm" class="text-[0.72rem] h-8 px-4" onclick={checkForUpdate}>
              {t("updates.downloadNow")}
            </Button>
          </div>

        {:else}
          <!-- Idle — check button -->
          <div class="flex items-center gap-3">
            <div class="flex flex-col gap-0.5 flex-1">
              <span class="text-[0.78rem] font-semibold text-foreground">
                {checking ? t("updates.checking") : t("updates.upToDate")}
              </span>
              <span class="text-[0.6rem] text-muted-foreground/60">
                {t("updates.lastChecked")}: {fmtLastChecked()}
              </span>
            </div>
            <Button size="sm" variant="outline"
                    class="text-[0.72rem] h-8 px-4 gap-1.5"
                    disabled={checking}
                    onclick={checkForUpdate}>
              {#if checking}
                <svg class="w-3 h-3 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
                </svg>
                {t("updates.checking")}
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3.5 h-3.5">
                  <polyline points="23 4 23 10 17 10"/>
                  <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
                </svg>
                {t("updates.checkNow")}
              {/if}
            </Button>
          </div>

          {#if error}
            <div class="rounded-lg border border-red-400/30 bg-red-50 dark:bg-[#1a0a0a] px-3 py-2">
              <span class="text-[0.65rem] text-red-600 dark:text-red-400 break-all">{error}</span>
            </div>
          {/if}
        {/if}
      </div>

    </CardContent>
  </Card>

  <!-- ── Auto-check interval ────────────────────────────────────────────────── -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <div class="flex flex-col gap-3 px-4 py-4">
        <div class="flex items-center gap-2">
          <div class="flex flex-col gap-0.5 flex-1">
            <span class="text-[0.78rem] font-semibold text-foreground">
              {t("updates.checkInterval")}
            </span>
            <span class="text-[0.6rem] text-muted-foreground/60">
              {t("updates.checkIntervalDesc")}
            </span>
          </div>
          {#if intervalSaving}
            <svg class="w-3.5 h-3.5 text-muted-foreground animate-spin shrink-0" viewBox="0 0 24 24"
                 fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
            </svg>
          {/if}
        </div>

        <div class="flex items-center gap-1.5 flex-wrap">
          {#each INTERVAL_OPTIONS as [secs, labelKey]}
            <button
              onclick={() => setInterval(secs)}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                     transition-all cursor-pointer select-none
                     {checkIntervalSecs === secs
                       ? 'border-blue-500/50 bg-blue-500/10 dark:bg-blue-500/15 text-blue-600 dark:text-blue-400'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              {t(labelKey)}
            </button>
          {/each}
        </div>

        {#if checkIntervalSecs === 0}
          <p class="text-[0.6rem] text-amber-600 dark:text-amber-400 leading-relaxed">
            {t("updates.intervalOffWarning")}
          </p>
        {/if}
      </div>

    </CardContent>
  </Card>

  <!-- ── Launch at Login ────────────────────────────────────────────────────── -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-0 px-0">
      <button
        onclick={toggleAutostart}
        disabled={autostartSaving}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02] disabled:opacity-50">
        <!-- Toggle pill -->
        <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                    {autostartEnabled ? 'bg-emerald-500' : 'bg-muted dark:bg-white/[0.08]'}">
          {#if autostartSaving}
            <div class="absolute inset-0 flex items-center justify-center">
              <svg class="w-2.5 h-2.5 text-white/80 animate-spin" viewBox="0 0 24 24"
                   fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
              </svg>
            </div>
          {:else}
            <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                        {autostartEnabled ? 'translate-x-4' : 'translate-x-0.5'}"></div>
          {/if}
        </div>
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
            {t("updates.autostart")}
          </span>
          <span class="text-[0.58rem] text-muted-foreground leading-tight">
            {t("updates.autostartDesc")}
          </span>
        </div>
        {#if autostartEnabled}
          <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase text-emerald-500 shrink-0">
            {t("common.on")}
          </span>
        {:else}
          <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase text-muted-foreground/40 shrink-0">
            {t("common.off")}
          </span>
        {/if}
      </button>

      {#if autostartError}
        <div class="border-t border-border dark:border-white/[0.05] px-4 py-2">
          <span class="text-[0.6rem] text-red-600 dark:text-red-400 break-all">{autostartError}</span>
        </div>
      {/if}
    </CardContent>
  </Card>

  <!-- ── Release notes link ─────────────────────────────────────────────────── -->
  <div class="text-center">
    <span class="text-[0.52rem] text-muted-foreground/40">
      {t("updates.footer")}
    </span>
  </div>

</section>
