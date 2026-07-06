<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Voice input (ASR + VAD) defaults — settings.asr surfaced in the LLM/chat tab.

  These are the per-session fall-backs the chat window uses for the voice loop:
  trigger (continuous / push-to-talk), routing (voice loop / transcribe-only),
  the speech-recognition engine + model, and the language hint. The daemon
  (settings.json) is the source of truth: `fetchAsrDefaults` reads it on mount and
  `saveAsrDefaults` writes through on every change (with a localStorage cache for
  instant first paint).
-->
<script lang="ts">
import { onMount } from "svelte";
import {
  type AsrDefaults,
  type AsrRouting,
  type AsrTrigger,
  fetchAsrDefaults,
  loadAsrDefaults,
  saveAsrDefaults,
} from "$lib/chat/asr";
import { Card, CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { ToggleRow } from "$lib/components/ui/toggle-row";
import { t } from "$lib/i18n/index.svelte";

// Seed from the localStorage cache for instant first paint, then reconcile
// against the authoritative daemon defaults on mount.
let defaults = $state<AsrDefaults>(loadAsrDefaults());

onMount(async () => {
  defaults = await fetchAsrDefaults();
});

function update(patch: Partial<AsrDefaults>) {
  defaults = { ...defaults, ...patch };
  saveAsrDefaults(defaults);
}

const triggerOptions: { val: AsrTrigger; labelKey: string }[] = [
  { val: "continuous", labelKey: "chat.voice.triggerContinuous" },
  { val: "push_to_talk", labelKey: "chat.voice.triggerPtt" },
];

const routingOptions: { val: AsrRouting; labelKey: string }[] = [
  { val: "voice_loop", labelKey: "chat.voice.routingLoop" },
  { val: "transcribe_only", labelKey: "chat.voice.routingTranscribe" },
];

// ASR engine backends. Structured as a list so more engines slot in over time.
const engineOptions: { val: string; label: string }[] = [
  { val: "whisper", label: "Whisper" },
  { val: "qwen3-asr", label: "Qwen3-ASR" },
  { val: "voxtral", label: "Voxtral" },
];

// Known model repos per engine for the dropdown; a free-text field allows any repo.
const MODELS_BY_ENGINE: Record<string, string[]> = {
  whisper: [
    "openai/whisper-tiny.en",
    "openai/whisper-base.en",
    "openai/whisper-small.en",
    "openai/whisper-small",
    "openai/whisper-medium",
    "openai/whisper-large-v3",
  ],
  "qwen3-asr": ["Qwen/Qwen3-ASR-0.6B", "Qwen/Qwen3-ASR-1.7B"],
  voxtral: ["mistralai/Voxtral-Mini-3B-2507"],
};
const DEFAULT_MODEL_BY_ENGINE: Record<string, string> = {
  whisper: "openai/whisper-base.en",
  "qwen3-asr": "Qwen/Qwen3-ASR-0.6B",
  voxtral: "mistralai/Voxtral-Mini-3B-2507",
};

const knownModels = $derived(MODELS_BY_ENGINE[defaults.engine] ?? []);
// `true` when the current model isn't one of the known repos — surface the
// free-text field so the custom value stays editable.
let customModel = $derived(!knownModels.includes(defaults.model));
const CUSTOM_SENTINEL = "__custom__";

function onEngineSelect(engine: string) {
  // Reset the model to the engine's default unless the current one is valid for it.
  const models = MODELS_BY_ENGINE[engine] ?? [];
  const model = models.includes(defaults.model)
    ? defaults.model
    : (DEFAULT_MODEL_BY_ENGINE[engine] ?? defaults.model);
  update({ engine, model });
}

function onModelSelect(value: string) {
  if (value === CUSTOM_SENTINEL) {
    // Switching to "custom" keeps whatever is there but lets the user type a repo.
    update({ model: defaults.model });
    return;
  }
  update({ model: value });
}
</script>

<section class="flex flex-col gap-2">
  <SectionHeader>{t("chat.voice.section")}</SectionHeader>

  <Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- Enable voice controls -->
      <ToggleRow
        checked={defaults.enabled}
        label={t("chat.voice.enabled")}
        description={t("chat.voice.enabledDesc")}
        ontoggle={() => update({ enabled: !defaults.enabled })}
        showBadge={false}
      />

      <!-- Default trigger -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-ui-lg font-semibold text-foreground">{t("chat.voice.triggerLabel")}</span>
          <p class="text-ui-base text-muted-foreground">{t("chat.voice.triggerDesc")}</p>
        </div>
        <div class="flex items-center gap-1.5 flex-wrap">
          {#each triggerOptions as opt}
            <button
              onclick={() => update({ default_trigger: opt.val })}
              class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold transition-all cursor-pointer
                   {defaults.default_trigger === opt.val
                     ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                     : 'border-border bg-muted text-muted-foreground hover:text-foreground'}"
            >
              {t(opt.labelKey)}
            </button>
          {/each}
        </div>
      </div>

      <!-- Default routing -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-ui-lg font-semibold text-foreground">{t("chat.voice.routingLabel")}</span>
          <p class="text-ui-base text-muted-foreground">{t("chat.voice.routingDesc")}</p>
        </div>
        <div class="flex items-center gap-1.5 flex-wrap">
          {#each routingOptions as opt}
            <button
              onclick={() => update({ default_routing: opt.val })}
              class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold transition-all cursor-pointer
                   {defaults.default_routing === opt.val
                     ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                     : 'border-border bg-muted text-muted-foreground hover:text-foreground'}"
            >
              {t(opt.labelKey)}
            </button>
          {/each}
        </div>
      </div>

      <!-- ASR engine -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-ui-lg font-semibold text-foreground">{t("chat.voice.engineLabel")}</span>
          <p class="text-ui-base text-muted-foreground">{t("chat.voice.engineDesc")}</p>
        </div>
        <div class="flex items-center gap-1.5 flex-wrap">
          {#each engineOptions as opt}
            <button
              onclick={() => onEngineSelect(opt.val)}
              class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold transition-all cursor-pointer
                   {defaults.engine === opt.val
                     ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                     : 'border-border bg-muted text-muted-foreground hover:text-foreground'}"
            >
              {opt.label}
            </button>
          {/each}
        </div>
      </div>

      <!-- ASR model -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-ui-lg font-semibold text-foreground">{t("chat.voice.modelLabel")}</span>
          <p class="text-ui-base text-muted-foreground">{t("chat.voice.modelDesc")}</p>
        </div>
        <select
          aria-label={t("chat.voice.modelLabel")}
          value={customModel ? CUSTOM_SENTINEL : defaults.model}
          onchange={(e) => onModelSelect((e.currentTarget as HTMLSelectElement).value)}
          class="w-full rounded-lg border border-border dark:border-white/[0.08]
                 bg-muted dark:bg-surface-2 px-2.5 py-1.5 text-ui-base
                 text-foreground focus:outline-none focus:ring-1 focus:ring-violet-500/50"
        >
          {#each knownModels as m}
            <option value={m}>{m}</option>
          {/each}
          <option value={CUSTOM_SENTINEL}>{t("chat.voice.modelCustom")}</option>
        </select>
        {#if customModel}
          <input
            type="text"
            value={defaults.model}
            oninput={(e) => update({ model: (e.currentTarget as HTMLInputElement).value.trim() })}
            spellcheck="false"
            autocapitalize="off"
            placeholder="openai/whisper-base.en"
            class="w-full rounded-lg border border-border bg-background px-2.5 py-1.5 text-ui-base text-foreground
                   focus:outline-none focus:ring-1 focus:ring-violet-500/50 focus:border-violet-500/30"
          />
        {/if}
      </div>

      <!-- Language hint -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-ui-lg font-semibold text-foreground">{t("chat.voice.languageLabel")}</span>
          <p class="text-ui-base text-muted-foreground">{t("chat.voice.languageDesc")}</p>
        </div>
        <input
          type="text"
          value={defaults.language}
          oninput={(e) => update({ language: (e.currentTarget as HTMLInputElement).value.trim() })}
          spellcheck="false"
          autocapitalize="off"
          placeholder="en"
          class="w-24 rounded-lg border border-border bg-background px-2.5 py-1.5 text-ui-base text-foreground
                 focus:outline-none focus:ring-1 focus:ring-violet-500/50 focus:border-violet-500/30"
        />
      </div>

    </CardContent>
  </Card>
</section>
