<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Voice output (TTS) engine selection — surfaced in the LLM/chat settings tab.

  Engine chips come from the daemon catalog (`/v1/tts/engines` → skill-tts /
  rlx-tts-bench) so every supported backend is selectable; first use downloads
  Hub weights when the engine is marked downloadable.
-->
<script lang="ts">
import { onMount } from "svelte";
import { Card, CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { t } from "$lib/i18n/index.svelte";
import { fetchTtsEngine, loadTtsEngine, saveTtsEngine, type TtsEngineConfig } from "$lib/llm/tts";
import VoiceEnginePicker from "$lib/llm/VoiceEnginePicker.svelte";
import {
  fetchTtsEngines,
  TTS_ENGINE_FALLBACK_LIST,
  type TtsEngineInfo,
} from "$lib/llm/voice-catalog";

let cfg = $state<TtsEngineConfig>(loadTtsEngine());
let engines = $state<TtsEngineInfo[]>([...TTS_ENGINE_FALLBACK_LIST]);

onMount(async () => {
  const [nextCfg, nextEngines] = await Promise.all([fetchTtsEngine(), fetchTtsEngines()]);
  cfg = nextCfg;
  engines = nextEngines;
});

function update(patch: Partial<TtsEngineConfig>) {
  cfg = { ...cfg, ...patch };
  saveTtsEngine(cfg);
}

const activeMeta = $derived(engines.find((e) => e.id === cfg.engine));
const knownModels = $derived(activeMeta?.models ?? []);
const hasModelPicker = $derived(knownModels.length > 0);
const knownVoices = $derived(
  cfg.voices?.length ? cfg.voices : (activeMeta?.voices ?? []),
);
const hasVoicePicker = $derived(knownVoices.length > 0);
const isKyutai = $derived(cfg.engine === "kyutai-tts");
const isOrpheus = $derived(cfg.engine === "orpheus");
const needsBundleExport = $derived(Boolean(activeMeta?.needs_bundle));

function onEngineSelect(engine: string) {
  const meta = engines.find((e) => e.id === engine);
  const models = meta?.models ?? [];
  const model = models.includes(cfg.model) ? cfg.model : (meta?.default_model ?? "");
  const voices = meta?.voices?.length ? meta.voices : (cfg.voices ?? []);
  const defaultVoice = meta?.default_voice ?? "";
  const voice = voices.includes(cfg.voice)
    ? cfg.voice
    : defaultVoice && voices.includes(defaultVoice)
      ? defaultVoice
      : "";
  update({ engine, model, voice, voices: voices.length ? voices : undefined });
}
</script>

<section class="flex flex-col gap-2">
  <SectionHeader>{t("chat.tts.section")}</SectionHeader>

  <Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- Engine -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-ui-lg font-semibold text-foreground">{t("chat.tts.engineLabel")}</span>
          <p class="text-ui-base text-muted-foreground">{t("chat.tts.engineDesc")}</p>
        </div>
        <VoiceEnginePicker
          kind="tts"
          {engines}
          selectedId={cfg.engine}
          onSelect={onEngineSelect}
        />
        {#if isKyutai}
          <p class="text-ui-base text-amber-600 dark:text-amber-400">{t("chat.tts.kyutaiExperimental")}</p>
        {/if}
        {#if isOrpheus}
          <p class="text-ui-base text-muted-foreground">{t("chat.tts.orpheusHint")}</p>
        {/if}
        {#if needsBundleExport}
          <p class="text-ui-base text-amber-600 dark:text-amber-400">{t("chat.tts.bundleExportHint")}</p>
        {/if}
        {#if activeMeta?.downloadable === false && !needsBundleExport}
          <p class="text-ui-base text-muted-foreground">{t("chat.tts.manualWeightsHint")}</p>
        {/if}
      </div>

      {#if hasModelPicker}
        <!-- Model -->
        <div class="flex flex-col gap-2 px-4 py-3.5">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-lg font-semibold text-foreground">{t("chat.tts.modelLabel")}</span>
            <p class="text-ui-base text-muted-foreground">{t("chat.tts.modelDesc")}</p>
          </div>
          <select
            aria-label={t("chat.tts.modelLabel")}
            value={cfg.model}
            onchange={(e) => update({ model: (e.currentTarget as HTMLSelectElement).value })}
            class="w-full rounded-lg border border-border dark:border-white/[0.08]
                   bg-muted dark:bg-surface-2 px-2.5 py-1.5 text-ui-base
                   text-foreground focus:outline-none focus:ring-1 focus:ring-violet-500/50"
          >
            {#each knownModels as m}
              <option value={m}>{m}</option>
            {/each}
          </select>
        </div>
      {/if}

      {#if hasVoicePicker}
        <!-- Voice -->
        <div class="flex flex-col gap-2 px-4 py-3.5">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-lg font-semibold text-foreground">{t("chat.tts.voiceLabel")}</span>
            <p class="text-ui-base text-muted-foreground">{t("chat.tts.voiceDesc")}</p>
          </div>
          <select
            aria-label={t("chat.tts.voiceLabel")}
            value={cfg.voice}
            onchange={(e) => update({ voice: (e.currentTarget as HTMLSelectElement).value })}
            class="w-full rounded-lg border border-border dark:border-white/[0.08]
                   bg-muted dark:bg-surface-2 px-2.5 py-1.5 text-ui-base
                   text-foreground focus:outline-none focus:ring-1 focus:ring-violet-500/50"
          >
            <option value="">{t("chat.tts.voiceDefault")}</option>
            {#each knownVoices as v}
              <option value={v}>{v}</option>
            {/each}
          </select>
        </div>
      {/if}

    </CardContent>
  </Card>
</section>
