<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  TTS test widget — lets users type text and hear it via whichever engine is
  currently active (KittenTTS or NeuTTS).  Voice chips adapt accordingly.
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  // ── Types ──────────────────────────────────────────────────────────────────

  type TtsProgress = {
    phase: "step" | "ready" | "unloaded";
    step:  number;
    total: number;
    label: string;
  };

  interface NeuttsVoice { id: string; lang: string; flag: string; gender: string; }
  interface NeuttsConfig { enabled: boolean; voice_preset: string; }

  // ── State ──────────────────────────────────────────────────────────────────

  let inputText = $state("Calibration starting. Eyes open.");
  let speaking  = $state(false);
  let ready     = $state(false);
  let errorMsg  = $state("");

  let dlStep  = $state(0);
  let dlTotal = $state(3);
  let dlLabel = $state("");

  const dlPct = $derived(
    ready   ? 100 :
    dlStep  ? Math.max(4, Math.round((dlStep / Math.max(dlTotal, 1)) * 100)) :
    0
  );

  // ── Engine & voices ────────────────────────────────────────────────────────

  let isNeutts        = $state(false);
  let neuttsVoices    = $state<NeuttsVoice[]>([]);   // from tts_list_neutts_voices
  let kittenVoices    = $state<string[]>(["Jasper"]);
  let selectedVoice   = $state("");                  // "" = use saved default

  // Quick-pick phrases mirroring the real calibration announcements
  const SAMPLES: string[] = [
    "Calibration starting. 2 actions, 3 loops.",
    "Eyes Open",
    "Eyes Closed",
    "Mental Math",
    "Deep Breathing",
    "Break. Next: Eyes Open.",
    "Calibration complete. 3 loops recorded.",
    "Calibration cancelled.",
  ];

  // ── Event listener ─────────────────────────────────────────────────────────

  let unlistenTts:       UnlistenFn | null = null;
  let unlistenEngine:    UnlistenFn | null = null;

  /** Re-detect the active engine and refresh voice lists + ready state. */
  async function refreshEngine() {
    ready     = false;
    dlStep    = 0;
    dlLabel   = "";
    try {
      const cfg = await invoke<NeuttsConfig>("get_neutts_config");
      isNeutts = cfg.enabled;
      if (isNeutts) {
        selectedVoice = cfg.voice_preset || "";
        neuttsVoices  = await invoke<NeuttsVoice[]>("tts_list_neutts_voices");
      } else {
        const v = await invoke<string[]>("tts_list_voices");
        if (v.length) kittenVoices = v;
        const active = await invoke<string>("tts_get_voice");
        selectedVoice = active || (kittenVoices[0] ?? "Jasper");
      }
    } catch {}
  }

  onMount(async () => {
    await refreshEngine();

    unlistenEngine = await listen("tts-engine-changed", async () => {
      await refreshEngine();
    });

    unlistenTts = await listen<TtsProgress>("tts-progress", (ev) => {
      const p = ev.payload;
      if (p.phase === "ready") {
        ready  = true;
        dlStep = dlTotal;
        dlLabel = "";
        // Refresh KittenTTS voice list now that model is loaded
        if (!isNeutts) {
          invoke<string[]>("tts_list_voices")
            .then(v => { if (v.length) kittenVoices = v; })
            .catch(() => {});
        }
      } else if (p.phase === "unloaded") {
        ready   = false;
        dlStep  = 0;
        dlLabel = "";
      } else {
        dlStep  = p.step;
        dlTotal = p.total;
        dlLabel = p.label;
      }
    });
  });

  onDestroy(() => { unlistenTts?.(); unlistenEngine?.(); });

  // ── Voice helpers ──────────────────────────────────────────────────────────

  function pickVoice(v: string) {
    selectedVoice = v;
    if (!isNeutts) {
      // Persist active voice for KittenTTS globally
      invoke("tts_set_voice", { voice: v }).catch(() => {});
    }
    // For NeuTTS the voice is sent per-utterance in tts_speak; no global setter.
  }

  // ── Speak ──────────────────────────────────────────────────────────────────

  async function speak() {
    const text = inputText.trim();
    if (!text || speaking) return;
    speaking = true;
    errorMsg = "";
    try {
      // Kick off init if idle
      if (!ready) invoke("tts_init").catch(() => {});
      // Pass current voice selection; engine interprets it appropriately
      const voiceArg = selectedVoice || undefined;
      await invoke("tts_speak", { text, voice: voiceArg });
    } catch (e) {
      errorMsg = String(e);
    } finally {
      speaking = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); speak(); }
  }

  function pickSample(s: string) { inputText = s; speak(); }
</script>

<div class="rounded-xl border border-indigo-500/20 bg-indigo-50/40 dark:bg-indigo-500/5
            flex flex-col gap-3 p-4">

  <!-- ── Header ───────────────────────────────────────────────────────────── -->
  <div class="flex items-center gap-2">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
         stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
         class="w-4 h-4 shrink-0 text-indigo-500">
      <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
      <path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>
      <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
    </svg>
    <span class="text-[0.75rem] font-semibold text-indigo-700 dark:text-indigo-300">
      TTS Test
    </span>

    <!-- Engine badge -->
    <span class="rounded-full border px-2 py-0.5 text-[0.52rem] font-semibold uppercase tracking-wider
                 {isNeutts
                   ? 'border-violet-400/40 bg-violet-100/40 dark:bg-violet-900/20 text-violet-600 dark:text-violet-300'
                   : 'border-indigo-400/40 bg-indigo-100/40 dark:bg-indigo-900/20 text-indigo-600 dark:text-indigo-300'}">
      {isNeutts ? "NeuTTS" : "KittenTTS"}
    </span>

    <!-- Status badge -->
    {#if ready}
      <span class="ml-auto flex items-center gap-1 text-[0.58rem] font-semibold
                   text-emerald-600 dark:text-emerald-400">
        <span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span>
        Ready
      </span>
    {:else if dlStep > 0}
      <span class="ml-auto text-[0.58rem] font-semibold uppercase tracking-wider
                   text-amber-600 dark:text-amber-400 animate-pulse">
        Loading…
      </span>
    {:else}
      <span class="ml-auto text-[0.56rem] text-muted-foreground/50">
        Speak to auto-load
      </span>
    {/if}
  </div>

  <!-- ── Progress bar ─────────────────────────────────────────────────────── -->
  {#if dlStep > 0 && !ready}
    <div class="flex flex-col gap-1">
      <div class="flex items-center justify-between">
        <span class="text-[0.58rem] text-muted-foreground truncate max-w-[80%]" title={dlLabel}>
          {dlLabel || "Connecting…"}
        </span>
        {#if dlTotal > 0}
          <span class="text-[0.56rem] tabular-nums text-muted-foreground/60 shrink-0 ml-2">
            {dlStep}/{dlTotal}
          </span>
        {/if}
      </div>
      <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
        <div class="h-full rounded-full bg-indigo-500 transition-all duration-700 ease-out"
             style="width: {dlPct}%"></div>
      </div>
    </div>
  {/if}

  <!-- ── Voice chips ───────────────────────────────────────────────────────── -->
  {#if isNeutts && neuttsVoices.length > 0}
    <!-- NeuTTS: preset voice cards -->
    <div class="flex flex-wrap items-center gap-1.5">
      <span class="text-[0.54rem] font-semibold uppercase tracking-wider
                   text-muted-foreground/60 self-center shrink-0 mr-0.5">
        Voice:
      </span>
      {#each neuttsVoices as pv}
        <button
          onclick={() => pickVoice(pv.id)}
          class="flex items-center gap-1 rounded-full border px-2.5 py-0.5
                 text-[0.6rem] font-semibold transition-colors
                 {selectedVoice === pv.id
                   ? 'border-violet-500 bg-violet-500 text-white'
                   : 'border-border dark:border-white/[0.07] bg-white dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:border-violet-500/40'}">
          <span class="text-[0.7rem] leading-none">{pv.flag}</span>
          {pv.id}
          <span class="text-[0.52rem] opacity-60">{pv.gender}</span>
        </button>
      {/each}
    </div>

  {:else if !isNeutts && kittenVoices.length > 1}
    <!-- KittenTTS: voice name chips -->
    <div class="flex flex-wrap items-center gap-1.5">
      <span class="text-[0.54rem] font-semibold uppercase tracking-wider
                   text-muted-foreground/60 self-center shrink-0 mr-0.5">
        Voice:
      </span>
      {#each kittenVoices as v}
        <button
          onclick={() => pickVoice(v)}
          class="rounded-full border px-2.5 py-0.5 text-[0.6rem] font-semibold transition-colors
                 {selectedVoice === v
                   ? 'border-indigo-500 bg-indigo-500 text-white'
                   : 'border-border dark:border-white/[0.07] bg-white dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:border-indigo-500/40'}">
          {v}
        </button>
      {/each}
    </div>
  {/if}

  <!-- ── Text input + Speak button ────────────────────────────────────────── -->
  <div class="flex gap-2">
    <input
      type="text"
      bind:value={inputText}
      onkeydown={onKeydown}
      placeholder="Type anything to speak…"
      disabled={speaking}
      class="flex-1 rounded-lg border border-border dark:border-white/[0.08]
             bg-white dark:bg-[#1a1a28] px-3 py-2 text-[0.72rem] text-foreground
             placeholder:text-muted-foreground/50
             focus:outline-none focus:ring-1 focus:ring-indigo-500/50
             disabled:opacity-50 disabled:cursor-not-allowed"
    />
    <button
      onclick={speak}
      disabled={speaking || !inputText.trim()}
      class="flex items-center gap-1.5 rounded-lg px-3 py-2
             text-[0.68rem] font-semibold transition-all shrink-0
             disabled:cursor-not-allowed
             {speaking
               ? 'bg-muted dark:bg-white/[0.06] text-muted-foreground/50'
               : 'bg-indigo-500 text-white hover:bg-indigo-600 disabled:opacity-40'}"
    >
      {#if speaking}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
             class="w-3.5 h-3.5">
          <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
          <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
        </svg>
        Speaking…
      {:else}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
             class="w-3.5 h-3.5">
          <polygon points="5 3 19 12 5 21 5 3"/>
        </svg>
        Speak
      {/if}
    </button>
  </div>

  <!-- ── Quick-sample chips ────────────────────────────────────────────────── -->
  <div class="flex flex-wrap gap-1.5">
    <span class="text-[0.54rem] font-semibold uppercase tracking-wider
                 text-muted-foreground/60 self-center shrink-0 mr-0.5">
      Try:
    </span>
    {#each SAMPLES as s}
      <button
        onclick={() => pickSample(s)}
        disabled={speaking}
        class="rounded-full border border-border dark:border-white/[0.07]
               bg-white dark:bg-[#1a1a28]
               px-2 py-0.5 text-[0.58rem] text-muted-foreground
               hover:text-foreground hover:border-indigo-500/40
               disabled:opacity-40 disabled:cursor-not-allowed
               transition-colors truncate max-w-[14rem]"
        title={s}
      >{s}</button>
    {/each}
  </div>

  <!-- ── Error / hint ──────────────────────────────────────────────────────── -->
  {#if errorMsg}
    <p class="text-[0.62rem] text-red-500 dark:text-red-400 leading-relaxed">⚠ {errorMsg}</p>
  {:else if dlStep === 0 && !ready}
    <p class="text-[0.6rem] text-muted-foreground/50 leading-relaxed">
      {#if isNeutts}
        First run downloads the NeuTTS backbone model and caches it locally.
        Requires <code class="font-mono bg-muted px-1 rounded">espeak-ng</code> on PATH.
      {:else}
        First run downloads the KittenTTS model (~30 MB) and caches it locally.
        Requires <code class="font-mono bg-muted px-1 rounded">espeak-ng</code> on PATH.
      {/if}
    </p>
  {/if}

</div>
