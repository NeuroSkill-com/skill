<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Voice input (ASR + VAD) defaults — settings.asr surfaced in the LLM/chat tab.

  Engine chips come from the daemon catalog (`/v1/asr/engines`) so every
  rlx-models ASR backend Skill wires (Whisper, Qwen3-ASR, Voxtral, FunASR,
  Nemotron) is selectable; first use auto-downloads Hub weights.
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
import VoiceEnginePicker from "$lib/llm/VoiceEnginePicker.svelte";
import { ASR_ENGINE_FALLBACK_LIST, type AsrEngineInfo, fetchAsrEngines } from "$lib/llm/voice-catalog";

let defaults = $state<AsrDefaults>(loadAsrDefaults());
let engines = $state<AsrEngineInfo[]>([...ASR_ENGINE_FALLBACK_LIST]);

onMount(async () => {
  const [nextDefaults, nextEngines] = await Promise.all([fetchAsrDefaults(), fetchAsrEngines()]);
  defaults = nextDefaults;
  engines = nextEngines;
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

const activeMeta = $derived(engines.find((e) => e.id === defaults.engine));
const knownModels = $derived(activeMeta?.models ?? []);
let customModel = $derived(!knownModels.includes(defaults.model));
const CUSTOM_SENTINEL = "__custom__";

function onEngineSelect(engine: string) {
  const meta = engines.find((e) => e.id === engine);
  const models = meta?.models ?? [];
  const model = models.includes(defaults.model) ? defaults.model : (meta?.default_model ?? defaults.model);
  update({ engine, model });
}

function onModelSelect(value: string) {
  if (value === CUSTOM_SENTINEL) {
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

      <ToggleRow
        checked={defaults.enabled}
        label={t("chat.voice.enabled")}
        description={t("chat.voice.enabledDesc")}
        ontoggle={() => update({ enabled: !defaults.enabled })}
        showBadge={false}
      />

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

      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-ui-lg font-semibold text-foreground">{t("chat.voice.engineLabel")}</span>
          <p class="text-ui-base text-muted-foreground">{t("chat.voice.engineDesc")}</p>
        </div>
        <VoiceEnginePicker
          kind="asr"
          {engines}
          selectedId={defaults.engine}
          onSelect={onEngineSelect}
        />
        {#if activeMeta?.experimental}
          <p class="text-ui-base text-amber-600 dark:text-amber-400">{t("chat.voice.engineExperimental")}</p>
        {/if}
      </div>

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
