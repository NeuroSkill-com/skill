<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  TTS test widget — lets users type any English text and hear it spoken
  via the kittentts engine directly from the help window.
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";

  // ── Progress event shape (matches tts.rs TtsProgressEvent) ────────────────
  type TtsProgress = {
    phase: "step" | "ready";
    step:  number;  // 1-based; 0 when ready
    total: number;  // 0 when ready
    label: string;  // "" when ready
  };

  // ── State ──────────────────────────────────────────────────────────────────
  let inputText     = $state("Calibration starting. Eyes open.");
  let speaking      = $state(false);
  let ready         = $state(false);      // true once the engine is initialised
  let initCalled    = $state(false);      // true once we've kicked off tts_init
  let errorMsg      = $state("");
  let voices        = $state<string[]>(["Jasper"]);   // updated after engine ready
  let selectedVoice = $state("Jasper");

  // Download-step progress (only meaningful while !ready && initCalled)
  let dlStep  = $state(0);   // current step (1-based, 0 = not started)
  let dlTotal = $state(4);   // always 4
  let dlLabel = $state("");  // file name or "Loading ONNX session"

  // pct: 0 → 100 shown on the progress bar
  const dlPct = $derived(
    ready ? 100 :
    dlStep === 0 ? 0 :
    Math.round((dlStep - 1) / dlTotal * 100)
  );

  // Quick-pick phrases that mirror the real calibration announcements
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
  let unlistenTts: UnlistenFn | null = null;

  onMount(async () => {
    unlistenTts = await listen<TtsProgress>("tts-progress", (ev) => {
      const p = ev.payload;
      if (p.phase === "ready") {
        ready  = true;
        dlStep = dlTotal;   // fill bar to 100 %
        // Fetch actual voice list now that the engine is loaded
        invoke<string[]>("tts_list_voices")
          .then(v => { if (v.length > 0) voices = v; })
          .catch(() => {});
      } else {
        dlStep  = p.step;
        dlTotal = p.total;
        dlLabel = p.label;
      }
    });
  });

  onDestroy(() => { unlistenTts?.(); });

  // ── Helpers ────────────────────────────────────────────────────────────────
  async function ensureInit(): Promise<boolean> {
    if (ready) return true;
    if (initCalled) return false;   // already in flight
    initCalled = true;
    try {
      await invoke("tts_init");     // events arrive via the listener above
      return ready;                  // ready may have been set by the listener
    } catch (e) {
      errorMsg = String(e);
      return false;
    }
  }

  async function speak() {
    const text = inputText.trim();
    if (!text || speaking) return;
    speaking = true;
    errorMsg = "";
    try {
      const ok = await ensureInit();
      if (!ok) return;
      await invoke("tts_speak", { text, voice: selectedVoice });
    } catch (e) {
      errorMsg = String(e);
    } finally {
      speaking = false;
    }
  }

  function pickVoice(v: string) {
    selectedVoice = v;
    invoke("tts_set_voice", { voice: v }).catch(() => {});
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
    <!-- Speaker icon -->
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

    <!-- Status badge (right-aligned) -->
    {#if ready}
      <span class="ml-auto flex items-center gap-1 text-[0.58rem] font-semibold
                   text-emerald-600 dark:text-emerald-400">
        <span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span>
        Engine ready
      </span>
    {:else if initCalled}
      <span class="ml-auto text-[0.58rem] font-semibold uppercase tracking-wider
                   text-amber-600 dark:text-amber-400 animate-pulse">
        Preparing…
      </span>
    {:else}
      <span class="ml-auto text-[0.56rem] text-muted-foreground/50">
        English only · espeak-ng required
      </span>
    {/if}
  </div>

  <!-- ── Download progress bar (visible only while initialising) ───────────── -->
  {#if initCalled && !ready}
    <div class="flex flex-col gap-1">
      <div class="flex items-center justify-between">
        <span class="text-[0.58rem] text-muted-foreground truncate max-w-[80%]" title={dlLabel}>
          {dlLabel || "Connecting…"}
        </span>
        <span class="text-[0.56rem] tabular-nums text-muted-foreground/60 shrink-0 ml-2">
          {dlStep}/{dlTotal}
        </span>
      </div>
      <!-- Bar track -->
      <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
        <div class="h-full rounded-full bg-indigo-500 transition-all duration-700 ease-out"
             style="width: {dlPct}%"></div>
      </div>
    </div>
  {/if}

  <!-- ── Voice picker (shown once engine is ready and >1 voice available) ── -->
  {#if ready && voices.length > 1}
    <div class="flex flex-wrap items-center gap-1.5">
      <span class="text-[0.54rem] font-semibold uppercase tracking-wider
                   text-muted-foreground/60 self-center shrink-0 mr-0.5">
        Voice:
      </span>
      {#each voices as v}
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

  <!-- ── Quick-sample chips ─────────────────────────────────────────────── -->
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

  <!-- ── Error / idle hint ──────────────────────────────────────────────── -->
  {#if errorMsg}
    <p class="text-[0.62rem] text-red-500 dark:text-red-400 leading-relaxed">
      ⚠ {errorMsg}
    </p>
  {:else if !initCalled}
    <p class="text-[0.6rem] text-muted-foreground/50 leading-relaxed">
      Requires <code class="font-mono bg-muted px-1 rounded">espeak-ng</code> on
      PATH. First run downloads the KittenTTS model (~30 MB) and caches it locally.
      Press Enter or click Speak.
    </p>
  {/if}

</div>
