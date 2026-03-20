<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Updates tab — check for updates, auto-download, install, restart. -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { openUrl }            from "@tauri-apps/plugin-opener";
  import { relaunch }           from "@tauri-apps/plugin-process";
  import { Button }             from "$lib/components/ui/button";
  import { Card, CardContent }  from "$lib/components/ui/card";
  import { t }                  from "$lib/i18n/index.svelte";

  // ── Phase ─────────────────────────────────────────────────────────────────
  // Single state enum — avoids the boolean-soup that caused the previous bugs.
  type Phase =
    | "idle"        // nothing happening
    | "checking"    // calling check() / waiting for result
    | "downloading" // downloadAndInstall() in progress
    | "ready"       // installed, counting down to restart
    | "error";      // something went wrong (error string is always shown)

  // ── State ─────────────────────────────────────────────────────────────────
  let appVersion  = $state("…");
  let phase       = $state<Phase>("idle");
  let progress    = $state(0);        // 0–100 during download
  let error       = $state("");       // non-empty only on error phase
  let countdown   = $state(0);        // seconds until auto-relaunch
  let available   = $state<{ version: string; date?: string; body?: string } | null>(null);
  let lastCheckedUtc = $state(0);

  // Autostart
  let autostartEnabled  = $state(false);
  let autostartSaving   = $state(false);
  let autostartError    = $state("");

  // Update-check interval (backend-persisted)
  let checkIntervalSecs = $state(3600);
  let intervalSaving    = $state(false);

  // ── Interval options ──────────────────────────────────────────────────────
  const INTERVAL_OPTIONS: [number, string][] = [
    [900,   "updates.interval15m"],
    [1800,  "updates.interval30m"],
    [3600,  "updates.interval1h"],
    [14400, "updates.interval4h"],
    [86400, "updates.interval24h"],
    [0,     "updates.intervalOff"],
  ];

  const RELEASES_DOWNLOAD_URL = "https://github.com/NeuroSkill-com/skill/releases/latest";

  // ── Countdown timer ───────────────────────────────────────────────────────
  let countdownTimer: ReturnType<typeof setInterval> | null = null;

  function startCountdown(secs = 5) {
    countdown = secs;
    countdownTimer = setInterval(() => {
      countdown -= 1;
      if (countdown <= 0) {
        stopCountdown();
        relaunch();
      }
    }, 1000);
  }

  function stopCountdown() {
    if (countdownTimer) { clearInterval(countdownTimer); countdownTimer = null; }
    countdown = 0;
  }

  // ── Last-checked persistence ──────────────────────────────────────────────
  const LAST_KEY = "lastUpdateCheckUtc";

  function loadLastChecked() {
    try {
      const v = localStorage.getItem(LAST_KEY);
      if (v) lastCheckedUtc = Number(v) || 0;
    } catch (e) { console.warn("[updates] load last checked failed:", e); }
  }

  function saveLastChecked() {
    lastCheckedUtc = Math.floor(Date.now() / 1000);
    try { localStorage.setItem(LAST_KEY, String(lastCheckedUtc)); } catch (e) { console.warn("[updates] save last checked failed:", e); }
  }

  async function openOnlineDownload() {
    try {
      await openUrl(RELEASES_DOWNLOAD_URL);
    } catch (e) {
      const msg = t("updates.openDownloadPageFailed", { error: String(e) });
      error = error ? `${error}\n${msg}` : msg;
    }
  }

  // ── Core update logic ─────────────────────────────────────────────────────

  /** Download + install a known Update object.
   *  Sets phase to "downloading" → "ready" on success, "error" on failure.  */
  async function doInstall(update: Update) {
    phase    = "downloading";
    progress = 0;
    error    = "";

    let downloaded   = 0;
    let totalLength  = 0;

    try {
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            totalLength = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            progress = totalLength > 0
              ? Math.min(99, Math.round((downloaded / totalLength) * 100))
              : 0;
            break;
          case "Finished":
            progress = 100;
            break;
        }
      });

      // Install complete — begin countdown to auto-relaunch.
      phase = "ready";
      startCountdown(5);

    } catch (e) {
      phase = "error";
      error = `${String(e)}\n${t("updates.autoUpdateFailedOnline")}`;
      await openOnlineDownload();
    }
  }

  /** Check the update endpoint, store the result, and immediately download
   *  if an update is found.  Safe to call when phase is "idle" or "error".
   *
   *  @param hint  Metadata pre-fetched by the background Rust task.  When
   *               present the UI keeps showing the known version while we
   *               obtain a fresh Update object (which carries download
   *               capability).  If check() then returns null — e.g. because
   *               CDN edge nodes haven't propagated latest.json yet — we
   *               surface an error instead of silently going back to "idle".
   */
  async function checkAndDownload(
    hint?: { version: string; date?: string; body?: string },
  ) {
    if (phase === "checking" || phase === "downloading" || phase === "ready") return;

    stopCountdown();
    phase = "checking";
    error = "";
    // Preserve any hint metadata so the UI shows the version during the
    // network round-trip.  Only wipe available for a manual "Check Now".
    if (!hint) available = null;

    try {
      const update = await check();
      saveLastChecked();

      if (update) {
        available = {
          version: update.version,
          date:    update.date ?? undefined,
          body:    update.body ?? undefined,
        };
        // Immediately start downloading — no "Download Now" click needed.
        await doInstall(update);
      } else if (hint) {
        // The background task detected an update but check() now disagrees —
        // most likely a CDN propagation race (latest.json not yet on all
        // edges).  Surface an actionable error rather than silently dropping.
        phase = "error";
        error = `Update v${hint.version} was detected but could not be prepared (CDN may still be propagating). Click "Retry" in a moment.`;
      } else {
        available = null;
        phase = "idle";
      }

    } catch (e) {
      phase = "error";
      error = String(e);
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
  async function setCheckInterval(secs: number) {
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

    const [autoEnabled, intervalSecs] = await Promise.all([
      invoke<boolean>("get_autostart_enabled").catch(() => false),
      invoke<number>("get_update_check_interval").catch(() => 3600),
    ]);
    autostartEnabled  = autoEnabled;
    checkIntervalSecs = intervalSecs;

    unlisteners.push(
      // Background Rust task found an update — kick off download automatically.
      await listen<{ version: string; date?: string; body?: string }>(
        "update-available",
        (ev) => {
          if (phase === "checking" || phase === "downloading" || phase === "ready") return;
          saveLastChecked();
          // Pass the event payload as a hint so checkAndDownload() keeps the
          // version visible in the UI while it fetches a fresh Update object,
          // and surfaces an error if check() returns null (CDN race) instead
          // of silently reverting to "idle".
          checkAndDownload(ev.payload);
        },
      ),
      await listen("update-checked", () => {
        saveLastChecked();
      }),
    );
  });

  onDestroy(() => {
    unlisteners.forEach(u => u());
    stopCountdown();
  });
</script>

<section class="flex flex-col gap-4">

  <!-- ── Version hero ──────────────────────────────────────────────────────── -->
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
    {#if phase === "ready"}
      <span class="text-emerald-500 font-bold text-[0.72rem]">
        ✅ {t("updates.readyToRestart")}
      </span>
    {:else if phase === "downloading"}
      <span class="text-blue-500 font-semibold text-[0.65rem] tabular-nums">{progress}%</span>
    {:else if phase === "checking"}
      <svg class="w-4 h-4 text-muted-foreground animate-spin" viewBox="0 0 24 24"
           fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
      </svg>
    {/if}
  </div>

  <!-- ── Update status card ────────────────────────────────────────────────── -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <div class="flex flex-col gap-3 px-4 py-4">

        {#if phase === "ready"}
          <!-- ── Installed, counting down to restart ── -->
          <div class="flex items-center gap-3">
            <div class="w-10 h-10 rounded-full bg-emerald-500/10 flex items-center justify-center text-xl shrink-0">
              ✅
            </div>
            <div class="flex flex-col gap-0.5 flex-1">
              <span class="text-[0.78rem] font-semibold text-emerald-600 dark:text-emerald-400">
                {t("updates.installed", { version: available?.version ?? "" })}
              </span>
              <span class="text-[0.65rem] text-muted-foreground">
                {t("updates.restartingIn", { secs: countdown })}
              </span>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <Button size="sm" variant="outline"
                      class="text-[0.72rem] h-8 px-3"
                      onclick={() => { stopCountdown(); phase = "idle"; available = null; }}>
                {t("common.cancel")}
              </Button>
              <Button size="sm" class="text-[0.72rem] h-8 px-4"
                      onclick={() => { stopCountdown(); relaunch(); }}>
                {t("updates.restartNow")}
              </Button>
            </div>
          </div>

          <!-- Countdown progress bar -->
          <div class="h-1.5 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
            <div class="h-full rounded-full bg-emerald-500 transition-all duration-1000"
                 style="width:{Math.round(((5 - countdown) / 5) * 100)}%"></div>
          </div>

        {:else if phase === "downloading"}
          <!-- ── Downloading ── -->
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

        {:else}
          <!-- ── Idle / checking / error ── -->
          <div class="flex items-center gap-3">
            {#if phase === "error" && available}
              <!-- Update was found but download/install failed -->
              <div class="w-10 h-10 rounded-full bg-red-500/10 flex items-center justify-center text-xl shrink-0">
                ⚠
              </div>
              <div class="flex flex-col gap-0.5 flex-1">
                <span class="text-[0.78rem] font-semibold text-foreground">
                  v{available.version} {t("updates.available")}
                </span>
                <span class="text-[0.65rem] text-red-600 dark:text-red-400">
                  {t("updates.downloadFailed")}
                </span>
              </div>
            {:else}
              <div class="flex flex-col gap-0.5 flex-1">
                <span class="text-[0.78rem] font-semibold text-foreground">
                  {phase === "checking" ? t("updates.checking") : t("updates.upToDate")}
                </span>
                <span class="text-[0.6rem] text-muted-foreground/60">
                  {t("updates.lastChecked")}: {fmtLastChecked()}
                </span>
              </div>
            {/if}

            <!-- Check / Retry button -->
            <Button size="sm" variant="outline"
                    class="text-[0.72rem] h-8 px-4 gap-1.5 shrink-0"
                    disabled={phase === "checking"}
                    onclick={() => checkAndDownload()}>
              {#if phase === "checking"}
                <svg class="w-3 h-3 animate-spin" viewBox="0 0 24 24" fill="none"
                     stroke="currentColor" stroke-width="2">
                  <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
                </svg>
                {t("updates.checking")}
              {:else if phase === "error"}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3.5 h-3.5">
                  <polyline points="23 4 23 10 17 10"/>
                  <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
                </svg>
                {t("updates.retry")}
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3.5 h-3.5">
                  <polyline points="23 4 23 10 17 10"/>
                  <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
                </svg>
                {t("updates.checkNow")}
              {/if}
            </Button>
          </div>

          <!-- Error detail — always visible when phase === "error" -->
          {#if phase === "error" && error}
            <div class="rounded-lg border border-red-400/30 bg-red-50 dark:bg-[#1a0a0a] px-3 py-2">
              <span class="text-[0.65rem] text-red-600 dark:text-red-400 break-all">{error}</span>
            </div>
          {/if}

          {#if phase === "error" && available}
            <div class="flex justify-end">
              <Button size="sm" class="text-[0.72rem] h-8 px-4"
                      onclick={openOnlineDownload}>
                {t("updates.downloadNow")}
              </Button>
            </div>
          {/if}
        {/if}

      </div>
    </CardContent>
  </Card>

  <!-- ── Auto-check interval ───────────────────────────────────────────────── -->
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
              onclick={() => setCheckInterval(secs)}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                     transition-all cursor-pointer select-none
                     {checkIntervalSecs === secs
                       ? 'border-primary/50 bg-primary/10 text-primary'
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

  <!-- ── Launch at Login ───────────────────────────────────────────────────── -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-0 px-0">
      <button
        onclick={toggleAutostart}
        disabled={autostartSaving}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02] disabled:opacity-50">
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
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {autostartEnabled ? 'text-emerald-500' : 'text-muted-foreground/40'}">
          {autostartEnabled ? t("common.on") : t("common.off")}
        </span>
      </button>

      {#if autostartError}
        <div class="border-t border-border dark:border-white/[0.05] px-4 py-2">
          <span class="text-[0.6rem] text-red-600 dark:text-red-400 break-all">{autostartError}</span>
        </div>
      {/if}
    </CardContent>
  </Card>

  <!-- ── Release notes link ────────────────────────────────────────────────── -->
  <div class="text-center">
    <span class="text-[0.52rem] text-muted-foreground/40">
      {t("updates.footer")}
    </span>
  </div>

</section>
