<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Settings tab — Voice (TTS) -->
<script lang="ts">
  import { onMount, onDestroy }        from "svelte";
  import { invoke }                    from "@tauri-apps/api/core";
  import { listen, type UnlistenFn }  from "@tauri-apps/api/event";
  import { Card, CardContent }         from "$lib/components/ui/card";
  import { Separator }                 from "$lib/components/ui/separator";
  import TtsTestWidget                 from "$lib/help/TtsTestWidget.svelte";
  import { t }                         from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────
  interface LogConfig {
    embedder:  boolean;
    bluetooth: boolean;
    websocket: boolean;
    csv:       boolean;
    filter:    boolean;
    bands:     boolean;
    tts:       boolean;
    history:   boolean;
  }

  type TtsProgress = { phase: "step" | "ready"; step: number; total: number; label: string };

  // ── State ──────────────────────────────────────────────────────────────────
  let ttsReady   = $state(false);
  let ttsLabel   = $state("");
  let ttsStep    = $state(0);
  let ttsTotal   = $state(4);
  let logConfig  = $state<LogConfig>({
    embedder: true, bluetooth: true, websocket: false,
    csv: false, filter: false, bands: false, tts: false, history: false,
  });

  const dlPct = $derived(
    ttsReady ? 100 :
    ttsStep === 0 ? 0 :
    Math.round((ttsStep - 1) / ttsTotal * 100)
  );

  // ── Event listener ─────────────────────────────────────────────────────────
  let unlistenTts: UnlistenFn | null = null;

  onMount(async () => {
    // Load log config
    try { logConfig = await invoke<LogConfig>("get_log_config"); } catch {}

    // Progress listener must be set up before calling tts_init
    unlistenTts = await listen<TtsProgress>("tts-progress", (ev) => {
      const p = ev.payload;
      if (p.phase === "ready") {
        ttsReady = true;
        ttsStep  = ttsTotal;
        ttsLabel = "";
      } else {
        ttsReady = false;
        ttsStep  = p.step;
        ttsTotal = p.total;
        ttsLabel = p.label;
      }
    });

    // Pre-warm engine so status shows immediately
    invoke("tts_init").catch(() => {});
  });

  onDestroy(() => { unlistenTts?.(); });

  // ── Log toggle ─────────────────────────────────────────────────────────────
  async function toggleTtsLog() {
    logConfig = { ...logConfig, tts: !logConfig.tts };
    try { await invoke("set_log_config", { config: logConfig }); } catch {}
  }

  // ── Manual init ────────────────────────────────────────────────────────────
  function initEngine() {
    ttsReady = false;
    ttsStep  = 0;
    invoke("tts_init").catch(() => {});
  }
</script>

<div class="flex flex-col gap-6 px-4 py-4 pb-8">

  <!-- ── Engine status ──────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("ttsTab.engineSection")}
    </span>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col gap-3 px-4 py-3.5">

        <!-- Status row -->
        <div class="flex items-center gap-2.5">
          {#if ttsReady}
            <span class="w-2 h-2 rounded-full bg-emerald-500 shrink-0"></span>
            <span class="text-[0.76rem] font-semibold text-emerald-600 dark:text-emerald-400">
              {t("ttsTab.statusReady")}
            </span>
          {:else if ttsStep > 0}
            <span class="w-2 h-2 rounded-full bg-amber-500 shrink-0 animate-pulse"></span>
            <span class="text-[0.76rem] font-semibold text-amber-600 dark:text-amber-400 animate-pulse">
              {t("ttsTab.statusLoading")}
            </span>
          {:else}
            <span class="w-2 h-2 rounded-full bg-muted-foreground/30 shrink-0"></span>
            <span class="text-[0.76rem] font-semibold text-muted-foreground">
              {t("ttsTab.statusIdle")}
            </span>
          {/if}

          <button
            onclick={initEngine}
            disabled={ttsStep > 0 && !ttsReady}
            class="ml-auto rounded-lg border border-border dark:border-white/[0.08]
                   bg-muted dark:bg-[#1a1a28] px-2.5 py-1 text-[0.62rem] font-semibold
                   text-muted-foreground hover:text-foreground transition-colors
                   disabled:opacity-40 disabled:cursor-not-allowed">
            {t("ttsTab.initButton")}
          </button>
        </div>

        <!-- Progress bar (visible while loading) -->
        {#if ttsStep > 0 && !ttsReady}
          <div class="flex flex-col gap-1">
            <div class="flex items-center justify-between">
              <span class="text-[0.6rem] text-muted-foreground truncate max-w-[80%]" title={ttsLabel}>
                {ttsLabel || "Connecting…"}
              </span>
              <span class="text-[0.56rem] tabular-nums text-muted-foreground/60 shrink-0 ml-2">
                {ttsStep}/{ttsTotal}
              </span>
            </div>
            <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
              <div class="h-full rounded-full bg-indigo-500 transition-all duration-700 ease-out"
                   style="width: {dlPct}%"></div>
            </div>
          </div>
        {/if}

        <!-- Model info -->
        <p class="text-[0.62rem] text-muted-foreground/70 leading-relaxed">
          {t("ttsTab.modelInfo")}
        </p>
        <p class="text-[0.6rem] text-muted-foreground/50">
          {t("ttsTab.requirements")} · {t("ttsTab.requirementsDesc")}
        </p>

      </CardContent>
    </Card>
  </section>

  <!-- ── Test voice ──────────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("ttsTab.testSection")}
    </span>
    <p class="text-[0.64rem] text-muted-foreground/70 -mt-1">
      {t("ttsTab.testDesc")}
    </p>
    <TtsTestWidget />
  </section>

  <!-- ── API snippets ────────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("ttsTab.apiSection")}
    </span>
    <p class="text-[0.64rem] text-muted-foreground/70 -mt-1">
      {t("ttsTab.apiDesc")}
    </p>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-muted/50 dark:bg-[#0f0f18] flex flex-col divide-y
                divide-border dark:divide-white/[0.05] overflow-hidden">
      {#each [
        ["WebSocket", `{"command":"say","text":"Eyes closed. Relax."}`],
        ["HTTP (curl)", `curl -X POST http://localhost:<port>/say \\
  -H 'Content-Type: application/json' \\
  -d '{"text":"Eyes closed. Relax."}'`],
        ["websocat (CLI)", `echo '{"command":"say","text":"Eyes closed."}' | websocat ws://localhost:<port>`],
      ] as [label, code]}
        <div class="px-3 py-2.5 flex flex-col gap-1">
          <span class="text-[0.54rem] font-semibold uppercase tracking-wider text-muted-foreground/60">
            {label}
          </span>
          <pre class="text-[0.66rem] font-mono text-foreground/80 whitespace-pre-wrap leading-relaxed">{code}</pre>
        </div>
      {/each}
    </div>
  </section>

  <!-- ── Debug logging ───────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("ttsTab.loggingSection")}
    </span>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="py-0 px-0">
        <button
          onclick={toggleTtsLog}
          class="w-full flex items-center gap-3 px-4 py-3.5 text-left transition-colors
                 hover:bg-slate-50 dark:hover:bg-white/[0.02]">
          <!-- Toggle pill -->
          <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                      {logConfig.tts ? 'bg-emerald-500' : 'bg-muted dark:bg-white/[0.08]'}">
            <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                        {logConfig.tts ? 'translate-x-4' : 'translate-x-0.5'}"></div>
          </div>
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
              {t("ttsTab.loggingLabel")}
            </span>
            <span class="text-[0.58rem] text-muted-foreground leading-tight">
              {t("ttsTab.loggingDesc")}
            </span>
          </div>
        </button>
      </CardContent>
    </Card>
  </section>

</div>
