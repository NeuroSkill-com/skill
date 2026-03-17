<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Chat settings panel — system prompt, EEG context, thinking level, generation params. -->
<script lang="ts">
  import { tick } from "svelte";
  import { t }    from "$lib/i18n/index.svelte";
  import {
    SYSTEM_PROMPT_DEFAULT, SYSTEM_PROMPT_PRESETS,
    THINKING_LEVELS,
    type ThinkingLevel,
    type BandSnapshot,
  } from "$lib/chat-types";

  interface Props {
    systemPrompt:  string;
    eegContext:    boolean;
    latestBands:   BandSnapshot | null;
    thinkingLevel: ThinkingLevel;
    temperature:   number;
    maxTokens:     number;
    topK:          number;
    topP:          number;
  }

  let {
    systemPrompt  = $bindable(),
    eegContext    = $bindable(),
    latestBands,
    thinkingLevel = $bindable(),
    temperature   = $bindable(),
    maxTokens     = $bindable(),
    topK          = $bindable(),
    topP          = $bindable(),
  }: Props = $props();

  let systemPromptEl = $state<HTMLTextAreaElement | null>(null);

  const isDefaultPrompt = $derived(systemPrompt.trim() === SYSTEM_PROMPT_DEFAULT.trim());

  function applyPreset(prompt: string) {
    systemPrompt = prompt;
    tick().then(() => systemPromptEl?.focus());
  }

  const eegActive = $derived(eegContext && latestBands !== null);
</script>

<div class="min-h-0 max-h-[40vh] overflow-y-auto border-b border-border dark:border-white/[0.06]
            bg-slate-50/60 dark:bg-[#111118] px-4 py-3 flex flex-col gap-3
            scrollbar-thin scrollbar-track-transparent scrollbar-thumb-border">

  <!-- ── System prompt ─────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-1.5">
    <div class="flex items-baseline justify-between gap-2">
      <label for="chat-system-prompt"
             class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
        {t("chat.systemPrompt")}
      </label>
      <div class="flex items-center gap-2">
        <span class="text-[0.55rem] tabular-nums text-muted-foreground/40 select-none">
          {t("chat.systemPrompt.chars", { n: systemPrompt.length })}
        </span>
        {#if !isDefaultPrompt}
          <button
            onclick={() => applyPreset(SYSTEM_PROMPT_DEFAULT)}
            title={t("chat.systemPrompt.reset")}
            class="flex items-center gap-0.5 text-[0.58rem] text-muted-foreground/50
                   hover:text-violet-600 dark:hover:text-violet-400 transition-colors
                   cursor-pointer select-none">
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                 stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"
                 class="w-2.5 h-2.5">
              <path d="M13.5 8A5.5 5.5 0 1 1 8 2.5"/>
              <polyline points="13.5 2.5 13.5 6 10 6"/>
            </svg>
            {t("chat.systemPrompt.reset")}
          </button>
        {/if}
      </div>
    </div>

    <textarea
      id="chat-system-prompt"
      bind:this={systemPromptEl}
      bind:value={systemPrompt}
      rows="4"
      spellcheck="true"
      class="w-full rounded-lg border border-border bg-background text-[0.73rem]
             text-foreground px-2.5 py-1.5 resize-y leading-relaxed
             focus:outline-none focus:ring-1 focus:ring-violet-500/50
             min-h-[4.5rem] max-h-48 transition-shadow"
      style="field-sizing: content"
    ></textarea>

    <!-- Preset persona chips -->
    <div class="flex flex-col gap-1">
      <span class="text-[0.55rem] font-semibold uppercase tracking-widest
                   text-muted-foreground/50 select-none">
        {t("chat.systemPrompt.presets")}
      </span>
      <div class="flex flex-wrap gap-1">
        {#each SYSTEM_PROMPT_PRESETS as preset}
          {@const isActive = systemPrompt.trim() === preset.prompt.trim()}
          <button
            onclick={() => applyPreset(preset.prompt)}
            title={preset.prompt}
            class="flex items-center gap-1 px-2 py-0.5 rounded-md text-[0.63rem]
                   border transition-all cursor-pointer select-none
                   {isActive
                     ? 'border-violet-500/50 bg-violet-500/10 text-violet-700 dark:text-violet-300'
                     : 'border-border bg-background text-muted-foreground/70 hover:border-violet-400/40 hover:bg-violet-500/8 hover:text-foreground'}">
            <span aria-hidden="true">{preset.icon}</span>
            {t(`chat.systemPrompt.preset.${preset.key}`)}
          </button>
        {/each}
      </div>
    </div>
  </div>

  <!-- EEG context injection toggle -->
  <div class="flex items-center justify-between gap-2">
    <div class="flex items-center gap-1.5">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6"
           stroke-linecap="round" stroke-linejoin="round"
           class="w-3 h-3 shrink-0 {latestBands ? 'text-cyan-500' : 'text-muted-foreground/30'}">
        <path d="M2 10 Q4 6 6 10 Q8 14 10 10 Q12 6 14 10 Q16 14 18 10"/>
      </svg>
      <span class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
        {t("chat.eeg.contextLabel")}
      </span>
    </div>
    <button
      onclick={() => eegContext = !eegContext}
      disabled={!latestBands}
      title={latestBands ? (eegContext ? t("chat.eeg.on") : t("chat.eeg.off")) : t("chat.eeg.noSignal")}
      class="relative inline-flex h-4 w-7 shrink-0 cursor-pointer items-center
             rounded-full transition-colors disabled:opacity-30 disabled:cursor-not-allowed
             {eegContext && latestBands ? 'bg-cyan-500' : 'bg-muted-foreground/20'}">
      <span class="inline-block h-3 w-3 rounded-full bg-white shadow
                   transition-transform
                   {eegContext && latestBands ? 'translate-x-3.5' : 'translate-x-0.5'}">
      </span>
    </button>
  </div>

  <!-- Live EEG stats preview -->
  {#if eegActive && latestBands}
    {@const b = latestBands}
    {@const n = b.channels.length || 1}
    {@const rA = b.channels.reduce((s,c)=>s+c.rel_alpha,0)/n}
    {@const rB = b.channels.reduce((s,c)=>s+c.rel_beta,0)/n}
    {@const rT = b.channels.reduce((s,c)=>s+c.rel_theta,0)/n}
    <div class="grid grid-cols-4 gap-1.5 rounded-lg border border-cyan-500/20
                 bg-cyan-500/5 px-2 py-1.5">
      {#each [
        { label: "SNR",   val: (b.snr ?? 0).toFixed(1) + "dB" },
        { label: "Mood",  val: (b.mood ?? 0).toFixed(0) + "/100" },
        { label: "α",     val: (rA*100).toFixed(0) + "%" },
        { label: "β/α",   val: (b.bar ?? 0).toFixed(2) },
        { label: "θ/α",   val: (b.tar ?? 0).toFixed(2) },
        { label: "FAA",   val: (b.faa>=0?"+":"")+b.faa.toFixed(2) },
        ...(b.hr != null ? [{ label: "HR", val: b.hr.toFixed(0)+"bpm" }] : []),
        ...(b.meditation != null ? [{ label: "Med", val: b.meditation.toFixed(0)+"/100" }] : []),
      ] as s}
        <div class="flex flex-col items-center gap-0">
          <span class="text-[0.48rem] text-cyan-600/60 dark:text-cyan-400/60 font-medium uppercase">
            {s.label}
          </span>
          <span class="text-[0.6rem] font-semibold tabular-nums text-cyan-700 dark:text-cyan-300">
            {s.val}
          </span>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Thinking level -->
  <div class="flex flex-col gap-1">
    <span class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
      {t("chat.thinkDepth")}
    </span>
    <div class="flex rounded-lg overflow-hidden border border-border text-[0.65rem] font-medium">
      {#each THINKING_LEVELS as lvl}
        <button
          onclick={() => thinkingLevel = lvl.key}
          class="flex-1 py-1 transition-colors cursor-pointer
                 {thinkingLevel === lvl.key
                   ? 'bg-violet-600 text-white'
                   : 'bg-background text-muted-foreground hover:bg-muted'}">
          {t(lvl.labelKey)}
        </button>
      {/each}
    </div>
    <p class="text-[0.58rem] text-muted-foreground/60">
      {t(`chat.think.${thinkingLevel}Desc`)}
    </p>
  </div>

  <!-- Sliders row -->
  <div class="grid grid-cols-2 gap-3">
    {#each [
      { labelKey: "chat.param.temperature", min: 0,  max: 2,    step: 0.05, value: temperature, set: (v: number) => temperature = v },
      { labelKey: "chat.param.maxTokens",  min: 64, max: 8192, step: 64,  value: maxTokens,   set: (v: number) => maxTokens   = v },
      { labelKey: "chat.param.topK",       min: 1,  max: 200,  step: 1,   value: topK,        set: (v: number) => topK        = v },
      { labelKey: "chat.param.topP",       min: 0,  max: 1,    step: 0.05, value: topP,       set: (v: number) => topP        = v },
    ] as s}
      <div class="flex flex-col gap-0.5">
        <div class="flex items-baseline justify-between">
          <span class="text-[0.6rem] text-muted-foreground">{t(s.labelKey)}</span>
          <span class="text-[0.62rem] font-mono text-foreground tabular-nums">{s.value}</span>
        </div>
        <input type="range" min={s.min} max={s.max} step={s.step} value={s.value}
          oninput={(e) => s.set(+(e.target as HTMLInputElement).value)}
          class="w-full accent-violet-500 h-1 cursor-pointer" />
      </div>
    {/each}
  </div>
</div>
