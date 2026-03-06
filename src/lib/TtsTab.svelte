<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Settings tab — Voice (TTS) -->
<script lang="ts">
  import { onMount, onDestroy }       from "svelte";
  import { fade }                     from "svelte/transition";
  import { invoke }                   from "@tauri-apps/api/core";
  import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
  import { Card, CardContent }        from "$lib/components/ui/card";
  import TtsTestWidget                from "$lib/help/TtsTestWidget.svelte";
  import { t }                        from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────

  interface NeuttsConfig {
    enabled:       boolean;
    backbone_repo: string;
    gguf_file:     string;
    voice_preset:  string;   // "jo"|"dave"|"greta"|"juliette"|"mateo"|""
    ref_wav_path:  string;
    ref_text:      string;
  }

  interface LogConfig {
    embedder: boolean; bluetooth: boolean; websocket: boolean;
    csv: boolean; filter: boolean; bands: boolean; tts: boolean; history: boolean;
  }

  type TtsProgress = { phase: "step" | "ready" | "unloaded" | "error"; step: number; total: number; label: string };

  // ── Backbone model registry ────────────────────────────────────────────────

  interface BackboneModel {
    repo: string; name: string; language: string;
    size_mb: number; recommended: boolean; pros: string; cons: string;
  }

  const BACKBONE_MODELS: BackboneModel[] = [
    // English
    { repo: "neuphonic/neutts-nano-q4-gguf",         name: "Nano Q4",        language: "en-us", size_mb: 135,  recommended: true,  pros: "Fast · small · low RAM",                 cons: "Slightly lower quality than Q8"      },
    { repo: "neuphonic/neutts-nano-q8-gguf",         name: "Nano Q8",        language: "en-us", size_mb: 230,  recommended: false, pros: "Better quality than Q4",                 cons: "2× larger; ~500 MB RAM"              },
    { repo: "neuphonic/neutts-nano",                 name: "Nano fp16",      language: "en-us", size_mb: 430,  recommended: false, pros: "Reference Nano quality",                 cons: "Slowest; needs FP16 llama.cpp"        },
    { repo: "neuphonic/neutts-air-q4-gguf",          name: "Air Q4",         language: "en-us", size_mb: 430,  recommended: false, pros: "Richer prosody · voice cloning",         cons: "3× heavier; ~900 MB RAM"             },
    { repo: "neuphonic/neutts-air-q8-gguf",          name: "Air Q8",         language: "en-us", size_mb: 820,  recommended: false, pros: "Near-lossless for 0.7B model",           cons: "~820 MB; ~1.5 GB RAM"                },
    { repo: "neuphonic/neutts-air",                  name: "Air fp16",       language: "en-us", size_mb: 1450, recommended: false, pros: "Highest English quality",                cons: "Very large; slow on CPU"             },
    // German
    { repo: "neuphonic/neutts-nano-german-q4-gguf",  name: "Nano German Q4", language: "de",    size_mb: 135,  recommended: true,  pros: "Fast German TTS",                        cons: "Q4 quantisation"                     },
    { repo: "neuphonic/neutts-nano-german-q8-gguf",  name: "Nano German Q8", language: "de",    size_mb: 230,  recommended: false, pros: "Better German quality",                  cons: "2× larger"                           },
    { repo: "neuphonic/neutts-nano-german",          name: "Nano German fp16",language: "de",   size_mb: 430,  recommended: false, pros: "Reference German quality",               cons: "Largest; needs FP16"                 },
    // French
    { repo: "neuphonic/neutts-nano-french-q4-gguf",  name: "Nano French Q4", language: "fr-fr", size_mb: 135,  recommended: true,  pros: "Fast French TTS",                        cons: "Q4 quantisation"                     },
    { repo: "neuphonic/neutts-nano-french-q8-gguf",  name: "Nano French Q8", language: "fr-fr", size_mb: 230,  recommended: false, pros: "Better French quality",                  cons: "2× larger"                           },
    { repo: "neuphonic/neutts-nano-french",          name: "Nano French fp16",language: "fr-fr", size_mb: 430, recommended: false, pros: "Reference French quality",               cons: "Largest; needs FP16"                 },
    // Spanish
    { repo: "neuphonic/neutts-nano-spanish-q4-gguf", name: "Nano Spanish Q4",language: "es",    size_mb: 135,  recommended: true,  pros: "Fast Spanish TTS",                       cons: "Q4 quantisation"                     },
    { repo: "neuphonic/neutts-nano-spanish-q8-gguf", name: "Nano Spanish Q8",language: "es",    size_mb: 230,  recommended: false, pros: "Better Spanish quality",                 cons: "2× larger"                           },
    { repo: "neuphonic/neutts-nano-spanish",         name: "Nano Spanish fp16",language: "es",  size_mb: 430,  recommended: false, pros: "Reference Spanish quality",              cons: "Largest; needs FP16"                 },
  ];

  // ── Preset voices (bundled in neutts-rs/samples/) ─────────────────────────

  interface PresetVoice {
    id: string; labelKey: string; lang: string; flag: string; gender: "♀" | "♂";
  }

  const PRESET_VOICES: PresetVoice[] = [
    { id: "jo",       labelKey: "ttsTab.voiceJo",       lang: "en-us", flag: "🇺🇸", gender: "♀" },
    { id: "dave",     labelKey: "ttsTab.voiceDave",     lang: "en-us", flag: "🇺🇸", gender: "♂" },
    { id: "greta",    labelKey: "ttsTab.voiceGreta",    lang: "de",    flag: "🇩🇪", gender: "♀" },
    { id: "juliette", labelKey: "ttsTab.voiceJuliette", lang: "fr-fr", flag: "🇫🇷", gender: "♀" },
    { id: "mateo",    labelKey: "ttsTab.voiceMateo",    lang: "es",    flag: "🇪🇸", gender: "♂" },
  ];

  // ── State ──────────────────────────────────────────────────────────────────

  // Engine status
  type EnginePhase = "idle" | "loading" | "ready" | "unloaded" | "error";
  let enginePhase = $state<EnginePhase>("idle");
  let loadLabel   = $state("");
  let loadStep    = $state(0);
  let loadTotal   = $state(3);
  let errorMsg    = $state("");

  const loadPct = $derived(
    enginePhase === "ready"   ? 100 :
    enginePhase === "loading" ? Math.max(4, Math.round((loadStep / Math.max(loadTotal, 1)) * 100)) :
    0
  );

  // Configs
  let neuttsConfig = $state<NeuttsConfig>({
    enabled:      false,
    backbone_repo:"neuphonic/neutts-nano-q4-gguf",
    gguf_file:    "",
    voice_preset: "jo",
    ref_wav_path: "",
    ref_text:     "",
  });
  let kittenVoices  = $state<string[]>(["Jasper"]);
  let kittenVoice   = $state("Jasper");
  let ttsPreload    = $state(true);
  let logConfig     = $state<LogConfig>({
    embedder: true, bluetooth: true, websocket: false,
    csv: false, filter: false, bands: false, tts: false, history: false,
  });

  // NeuTTS config dirty / save state
  let neuttsDirty = $state(false);
  let neuttsSaved = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Derived helpers ────────────────────────────────────────────────────────

  const activeBackend = $derived(neuttsConfig.enabled ? "neutts" : "kitten");

  function selectedModel(): BackboneModel | undefined {
    return BACKBONE_MODELS.find(m => m.repo === neuttsConfig.backbone_repo);
  }

  // ── Event listener ─────────────────────────────────────────────────────────

  let unlistenTts: UnlistenFn | null = null;

  onMount(async () => {
    try { logConfig    = await invoke<LogConfig>("get_log_config"); }    catch {}
    try { neuttsConfig = await invoke<NeuttsConfig>("get_neutts_config"); } catch {}
    try { ttsPreload   = await invoke<boolean>("get_tts_preload"); } catch {}
    try {
      const voices = await invoke<string[]>("tts_list_voices");
      if (voices.length) kittenVoices = voices;
    } catch {}
    try { kittenVoice = await invoke<string>("tts_get_voice"); } catch {}

    unlistenTts = await listen<TtsProgress>("tts-progress", (ev) => {
      const p = ev.payload;
      if (p.phase === "ready") {
        enginePhase = "ready";
        loadStep    = loadTotal;
        loadLabel   = "";
        errorMsg    = "";
      } else if (p.phase === "unloaded") {
        enginePhase = "unloaded";
        loadStep    = 0;
        loadLabel   = "";
        errorMsg    = "";
      } else if (p.phase === "error") {
        enginePhase = "error";
        loadStep    = 0;
        loadLabel   = "";
        errorMsg    = p.label;
      } else {
        enginePhase = "loading";
        loadStep    = p.step;
        loadTotal   = p.total;
        loadLabel   = p.label;
        errorMsg    = "";
      }
    });

    // Pre-warm immediately
    invoke("tts_init").catch(() => {});
  });

  onDestroy(() => {
    unlistenTts?.();
    if (saveTimer !== null) clearTimeout(saveTimer);
  });

  // ── Engine lifecycle ───────────────────────────────────────────────────────

  function preload() {
    enginePhase = "loading";
    loadStep    = 0;
    invoke("tts_init").catch(() => {});
  }

  async function unload() {
    await invoke("tts_unload").catch(() => {});
  }

  // ── Backend switch ─────────────────────────────────────────────────────────

  async function switchBackend(toNeutts: boolean) {
    if (neuttsConfig.enabled === toNeutts) return;
    neuttsConfig = { ...neuttsConfig, enabled: toNeutts };
    neuttsDirty  = true;
    enginePhase  = "idle";
    loadStep     = 0;
    await saveNeutts();
  }

  // ── KittenTTS voice ────────────────────────────────────────────────────────

  async function setKittenVoice(v: string) {
    kittenVoice = v;
    try { await invoke("tts_set_voice", { voice: v }); } catch {}
  }

  // ── NeuTTS save ────────────────────────────────────────────────────────────

  async function saveNeutts() {
    try {
      await invoke("set_neutts_config", { config: neuttsConfig });
      neuttsDirty = false;
      neuttsSaved = true;
      if (saveTimer !== null) clearTimeout(saveTimer);
      saveTimer = setTimeout(() => { neuttsSaved = false; }, 2000);
      // Notify other components that the active engine may have changed.
      await emit("tts-engine-changed", { enabled: neuttsConfig.enabled });
      // Always re-warm so the new active engine loads immediately.
      enginePhase = "loading";
      loadStep    = 0;
      loadLabel   = "";
      invoke("tts_init").catch(() => {});
    } catch (e) {
      console.error("[NeuTTS] set_neutts_config failed", e);
    }
  }

  async function pickRefWav() {
    try {
      const path = await invoke<string | null>("pick_ref_wav_file");
      if (path) { neuttsConfig = { ...neuttsConfig, ref_wav_path: path }; neuttsDirty = true; }
    } catch {}
  }

  // ── Log toggle ─────────────────────────────────────────────────────────────

  async function toggleTtsLog() {
    logConfig = { ...logConfig, tts: !logConfig.tts };
    try { await invoke("set_log_config", { config: logConfig }); } catch {}
  }
</script>

<!-- ═══════════════════════════════════════════════════════════════════════════ -->
<div class="flex flex-col gap-5 px-4 py-4 pb-8">

  <!-- ── 1. Backend selector ───────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("ttsTab.backendSection")}
    </span>

    <div class="grid grid-cols-2 gap-2">
      <!-- KittenTTS card -->
      <button
        onclick={() => switchBackend(false)}
        class="relative flex flex-col gap-1 rounded-xl border px-3.5 py-3 text-left transition-all
               {activeBackend === 'kitten'
                 ? 'border-indigo-500 bg-indigo-50 dark:bg-indigo-950/40 shadow-sm'
                 : 'border-border dark:border-white/[0.07] bg-white dark:bg-[#14141e] hover:border-indigo-300 dark:hover:border-indigo-700'}">
        {#if activeBackend === "kitten"}
          <span class="absolute top-2 right-2.5 w-1.5 h-1.5 rounded-full bg-indigo-500"></span>
        {/if}
        <span class="text-[0.75rem] font-semibold text-foreground">{t("ttsTab.backendKitten")}</span>
        <span class="text-[0.57rem] text-muted-foreground/70 leading-relaxed">{t("ttsTab.backendKittenTag")}</span>
        <span class="text-[0.6rem] text-muted-foreground/50 leading-relaxed mt-0.5">{t("ttsTab.backendKittenDesc")}</span>
      </button>

      <!-- NeuTTS card -->
      <button
        onclick={() => switchBackend(true)}
        class="relative flex flex-col gap-1 rounded-xl border px-3.5 py-3 text-left transition-all
               {activeBackend === 'neutts'
                 ? 'border-indigo-500 bg-indigo-50 dark:bg-indigo-950/40 shadow-sm'
                 : 'border-border dark:border-white/[0.07] bg-white dark:bg-[#14141e] hover:border-indigo-300 dark:hover:border-indigo-700'}">
        {#if activeBackend === "neutts"}
          <span class="absolute top-2 right-2.5 w-1.5 h-1.5 rounded-full bg-indigo-500"></span>
        {/if}
        <span class="text-[0.75rem] font-semibold text-foreground">{t("ttsTab.backendNeutts")}</span>
        <span class="text-[0.57rem] text-muted-foreground/70 leading-relaxed">{t("ttsTab.backendNeuttsTag")}</span>
        <span class="text-[0.6rem] text-muted-foreground/50 leading-relaxed mt-0.5">{t("ttsTab.backendNeuttsDesc")}</span>
      </button>
    </div>
  </section>

  <!-- ── 2. Engine status ───────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("ttsTab.statusSection")}
    </span>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col gap-3 px-4 py-3.5">

        <!-- Status row -->
        <div class="flex items-center gap-2.5">
          {#if enginePhase === "ready"}
            <span class="w-2 h-2 rounded-full bg-emerald-500 shrink-0"></span>
            <span class="text-[0.76rem] font-semibold text-emerald-600 dark:text-emerald-400">
              {t("ttsTab.statusReady")}
            </span>
          {:else if enginePhase === "loading"}
            <span class="w-2 h-2 rounded-full bg-amber-500 shrink-0 animate-pulse"></span>
            <span class="text-[0.76rem] font-semibold text-amber-600 dark:text-amber-400 animate-pulse">
              {t("ttsTab.statusLoading")}
            </span>
          {:else if enginePhase === "error"}
            <span class="w-2 h-2 rounded-full bg-rose-500 shrink-0"></span>
            <span class="text-[0.76rem] font-semibold text-rose-600 dark:text-rose-400">
              {t("ttsTab.statusError")}
            </span>
          {:else if enginePhase === "unloaded"}
            <span class="w-2 h-2 rounded-full bg-muted-foreground/20 shrink-0"></span>
            <span class="text-[0.76rem] font-semibold text-muted-foreground">
              {t("ttsTab.statusUnloaded")}
            </span>
          {:else}
            <span class="w-2 h-2 rounded-full bg-muted-foreground/30 shrink-0"></span>
            <span class="text-[0.76rem] font-semibold text-muted-foreground">
              {t("ttsTab.statusIdle")}
            </span>
          {/if}

          <!-- Preload / Unload buttons -->
          <div class="ml-auto flex items-center gap-1.5">
            <button
              onclick={preload}
              disabled={enginePhase === "loading"}
              class="rounded-lg border border-border dark:border-white/[0.08]
                     bg-muted dark:bg-[#1a1a28] px-2.5 py-1 text-[0.62rem] font-semibold
                     text-muted-foreground hover:text-foreground transition-colors
                     disabled:opacity-40 disabled:cursor-not-allowed">
              {enginePhase === "error" ? t("ttsTab.retryButton") : t("ttsTab.preloadButton")}
            </button>
            <button
              onclick={unload}
              disabled={enginePhase !== "ready"}
              class="rounded-lg border border-border dark:border-white/[0.08]
                     bg-muted dark:bg-[#1a1a28] px-2.5 py-1 text-[0.62rem] font-semibold
                     text-muted-foreground hover:text-rose-600 dark:hover:text-rose-400 transition-colors
                     disabled:opacity-40 disabled:cursor-not-allowed">
              {t("ttsTab.unloadButton")}
            </button>
          </div>
        </div>

        <!-- Progress bar -->
        {#if enginePhase === "loading"}
          <div class="flex flex-col gap-1">
            <div class="flex items-center justify-between">
              <span class="text-[0.6rem] text-muted-foreground truncate max-w-[80%]" title={loadLabel}>
                {loadLabel || "Connecting…"}
              </span>
              {#if loadTotal > 0}
                <span class="text-[0.56rem] tabular-nums text-muted-foreground/60 shrink-0 ml-2">
                  {loadStep}/{loadTotal}
                </span>
              {/if}
            </div>
            <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
              <div class="h-full rounded-full bg-indigo-500 transition-all duration-500 ease-out"
                   style="width: {loadPct}%"></div>
            </div>
          </div>
        {/if}

        <!-- Error message -->
        {#if enginePhase === "error" && errorMsg}
          <div class="rounded-lg border border-rose-200 dark:border-rose-800/60
                      bg-rose-50 dark:bg-rose-950/30 px-3 py-2">
            <p class="text-[0.62rem] font-semibold text-rose-700 dark:text-rose-400 mb-0.5">
              {t("ttsTab.errorTitle")}
            </p>
            <p class="text-[0.58rem] text-rose-600/80 dark:text-rose-400/70 font-mono break-all leading-relaxed">
              {errorMsg}
            </p>
          </div>
        {/if}

        <!-- espeak-ng note -->
        <p class="text-[0.58rem] text-muted-foreground/50 leading-relaxed">
          {t("ttsTab.requirements")} · {t("ttsTab.requirementsDesc")}
        </p>

      </CardContent>
    </Card>
  </section>

  <!-- ── 3a. KittenTTS config ───────────────────────────────────────────────── -->
  {#if activeBackend === "kitten"}
    <section class="flex flex-col gap-2">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("ttsTab.kittenConfigSection")}
      </span>

      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="flex flex-col gap-3 px-4 py-3.5">

          <!-- Voice selector -->
          <div class="flex flex-col gap-1.5">
            <span class="text-[0.66rem] font-semibold text-foreground">
              {t("ttsTab.kittenVoiceLabel")}
            </span>
            <div class="flex flex-wrap gap-1.5">
              {#each kittenVoices as v}
                <button
                  onclick={() => setKittenVoice(v)}
                  class="rounded-lg border px-3 py-1.5 text-[0.66rem] font-medium transition-colors
                         {kittenVoice === v
                           ? 'border-indigo-500 bg-indigo-50 dark:bg-indigo-950/40 text-indigo-700 dark:text-indigo-300'
                           : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground'}">
                  {v}
                </button>
              {/each}
            </div>
          </div>

          <!-- Model info -->
          <p class="text-[0.6rem] text-muted-foreground/60 font-mono">
            {t("ttsTab.kittenModelInfo")}
          </p>

        </CardContent>
      </Card>
    </section>
  {/if}

  <!-- ── 3b. NeuTTS config ──────────────────────────────────────────────────── -->
  {#if activeBackend === "neutts"}
    <section class="flex flex-col gap-2">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("ttsTab.neuttsConfigSection")}
      </span>

      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="py-0 px-0 flex flex-col divide-y divide-border dark:divide-white/[0.05]">

          <!-- ── Backbone model ─────────────────────────────────────────────── -->
          <div class="px-4 py-3.5 flex flex-col gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-[0.68rem] font-semibold text-foreground">
                {t("ttsTab.neuttsModelLabel")}
              </span>
              <span class="text-[0.58rem] text-muted-foreground/60">
                {t("ttsTab.neuttsModelDesc")}
              </span>
            </div>

            <select
              bind:value={neuttsConfig.backbone_repo}
              onchange={() => { neuttsDirty = true; }}
              class="w-full rounded-lg border border-border dark:border-white/[0.08]
                     bg-muted dark:bg-[#1a1a28] px-2.5 py-1.5 text-[0.68rem]
                     text-foreground focus:outline-none focus:ring-1 focus:ring-indigo-500">
              {#each BACKBONE_MODELS as m}
                <option value={m.repo}>
                  {m.name} · {m.language} · ~{m.size_mb} MB{m.recommended ? " ★" : ""}
                </option>
              {/each}
            </select>

            {#if selectedModel()}
              {@const m = selectedModel()!}
              <div class="rounded-lg border border-border dark:border-white/[0.06]
                          bg-muted/40 dark:bg-white/[0.02] px-3 py-2 flex flex-col gap-0.5">
                <span class="text-[0.58rem] text-emerald-600 dark:text-emerald-400">✓ {m.pros}</span>
                <span class="text-[0.58rem] text-muted-foreground/50">✗ {m.cons}</span>
              </div>
            {/if}
          </div>

          <!-- ── Voice ──────────────────────────────────────────────────────── -->
          <div class="px-4 py-3.5 flex flex-col gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-[0.68rem] font-semibold text-foreground">
                {t("ttsTab.neuttsVoiceSection")}
              </span>
              <span class="text-[0.58rem] text-muted-foreground/60">
                {t("ttsTab.neuttsVoiceDesc")}
              </span>
            </div>

            <!-- Preset voice grid -->
            <div class="flex flex-col gap-1">
              <span class="text-[0.6rem] font-medium text-muted-foreground/70 uppercase tracking-wider">
                {t("ttsTab.neuttsPresetLabel")}
              </span>
              <div class="grid grid-cols-3 gap-1.5">
                {#each PRESET_VOICES as pv}
                  <button
                    onclick={() => { neuttsConfig = { ...neuttsConfig, voice_preset: pv.id }; neuttsDirty = true; }}
                    class="flex flex-col items-center gap-0.5 rounded-xl border py-2.5 px-1.5 transition-all text-center
                           {neuttsConfig.voice_preset === pv.id
                             ? 'border-indigo-500 bg-indigo-50 dark:bg-indigo-950/40'
                             : 'border-border dark:border-white/[0.07] bg-muted/40 dark:bg-[#1a1a28]/60 hover:border-indigo-300 dark:hover:border-indigo-700'}">
                    <span class="text-base leading-none">{pv.flag}</span>
                    <span class="text-[0.68rem] font-semibold leading-tight mt-1
                                 {neuttsConfig.voice_preset === pv.id
                                   ? 'text-indigo-700 dark:text-indigo-300'
                                   : 'text-foreground'}">
                      {t(pv.labelKey)}
                    </span>
                    <span class="text-[0.52rem] text-muted-foreground/50 leading-none mt-0.5">
                      {pv.gender} {pv.lang}
                    </span>
                  </button>
                {/each}

                <!-- Custom option -->
                <button
                  onclick={() => { neuttsConfig = { ...neuttsConfig, voice_preset: "" }; neuttsDirty = true; }}
                  class="flex flex-col items-center gap-0.5 rounded-xl border py-2.5 px-1.5 transition-all text-center
                         {neuttsConfig.voice_preset === ''
                           ? 'border-indigo-500 bg-indigo-50 dark:bg-indigo-950/40'
                           : 'border-dashed border-border dark:border-white/[0.07] bg-transparent hover:border-indigo-300 dark:hover:border-indigo-700'}">
                  <span class="text-base leading-none">📁</span>
                  <span class="text-[0.68rem] font-semibold leading-tight mt-1
                               {neuttsConfig.voice_preset === ''
                                 ? 'text-indigo-700 dark:text-indigo-300'
                                 : 'text-muted-foreground'}">
                    {t("ttsTab.voiceCustom")}
                  </span>
                  <span class="text-[0.52rem] text-muted-foreground/40 leading-none mt-0.5">WAV</span>
                </button>
              </div>
            </div>

            <!-- Custom WAV fields (expanded when Custom is selected) -->
            {#if neuttsConfig.voice_preset === ""}
              <div class="flex flex-col gap-2 pt-1">

                <!-- WAV file picker -->
                <div class="flex flex-col gap-1">
                  <span class="text-[0.6rem] font-medium text-muted-foreground/70 uppercase tracking-wider">
                    {t("ttsTab.neuttsRefWavLabel")}
                  </span>
                  <div class="flex items-center gap-2">
                    <span class="flex-1 truncate rounded-lg border border-border dark:border-white/[0.08]
                                 bg-muted dark:bg-[#1a1a28] px-2.5 py-1.5 text-[0.62rem]
                                 text-muted-foreground font-mono" title={neuttsConfig.ref_wav_path}>
                      {neuttsConfig.ref_wav_path || t("ttsTab.neuttsRefWavNone")}
                    </span>
                    <button
                      onclick={pickRefWav}
                      class="shrink-0 rounded-lg border border-border dark:border-white/[0.08]
                             bg-muted dark:bg-[#1a1a28] px-2.5 py-1.5 text-[0.62rem] font-semibold
                             text-muted-foreground hover:text-foreground transition-colors">
                      {t("ttsTab.neuttsRefWavBrowse")}
                    </button>
                  </div>
                </div>

                <!-- Transcript -->
                <div class="flex flex-col gap-1">
                  <span class="text-[0.6rem] font-medium text-muted-foreground/70 uppercase tracking-wider">
                    {t("ttsTab.neuttsRefTextLabel")}
                  </span>
                  <textarea
                    bind:value={neuttsConfig.ref_text}
                    oninput={() => { neuttsDirty = true; }}
                    rows={2}
                    placeholder={t("ttsTab.neuttsRefTextPlaceholder")}
                    class="w-full rounded-lg border border-border dark:border-white/[0.08]
                           bg-muted dark:bg-[#1a1a28] px-2.5 py-1.5 text-[0.68rem]
                           text-foreground placeholder:text-muted-foreground/40 resize-none
                           focus:outline-none focus:ring-1 focus:ring-indigo-500">
                  </textarea>
                </div>

              </div>
            {/if}
          </div>

          <!-- ── Save button ────────────────────────────────────────────────── -->
          <div class="px-4 py-3 flex justify-end">
            <button
              onclick={neuttsDirty ? saveNeutts : undefined}
              class="relative overflow-hidden rounded-lg px-3.5 py-1.5 text-[0.66rem] font-semibold
                     transition-all duration-300
                     {neuttsSaved
                       ? 'bg-emerald-500 text-white border border-emerald-500'
                       : neuttsDirty
                         ? 'bg-indigo-500 hover:bg-indigo-600 text-white border border-indigo-500'
                         : 'bg-muted dark:bg-[#1a1a28] text-muted-foreground border border-border dark:border-white/[0.08] opacity-40 cursor-not-allowed'}">
              {#key neuttsSaved}
                <span transition:fade={{ duration: 160 }}>
                  {neuttsSaved ? t("ttsTab.neuttsSaved") : t("ttsTab.neuttsSaveButton")}
                </span>
              {/key}
            </button>
          </div>

        </CardContent>
      </Card>
    </section>
  {/if}

  <!-- ── 4. Test voice ──────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("ttsTab.testSection")}
    </span>
    <p class="text-[0.64rem] text-muted-foreground/70 -mt-1">
      {t("ttsTab.testDesc")}
    </p>
    <TtsTestWidget />
  </section>

  <!-- ── 5. API snippets ────────────────────────────────────────────────────── -->
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

  <!-- ── 6. Startup behaviour ─────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("ttsTab.startupSection")}
    </span>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="py-0 px-0">
        <button
          onclick={() => {
            ttsPreload = !ttsPreload;
            invoke("set_tts_preload", { preload: ttsPreload }).catch(() => {});
          }}
          class="w-full flex items-center gap-3 px-4 py-3.5 text-left transition-colors
                 hover:bg-slate-50 dark:hover:bg-white/[0.02]">
          <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                      {ttsPreload ? 'bg-indigo-500' : 'bg-muted dark:bg-white/[0.08]'}">
            <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                        {ttsPreload ? 'translate-x-4' : 'translate-x-0.5'}"></div>
          </div>
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
              {t("ttsTab.preloadOnStartup")}
            </span>
            <span class="text-[0.58rem] text-muted-foreground leading-tight">
              {t("ttsTab.preloadOnStartupDesc")}
            </span>
          </div>
        </button>
      </CardContent>
    </Card>
  </section>

  <!-- ── 7. Debug logging ───────────────────────────────────────────────────── -->
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
