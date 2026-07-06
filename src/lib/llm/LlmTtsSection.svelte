<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Voice output (TTS) engine selection — surfaced in the LLM/chat settings tab.

  The daemon speaks the voice-loop reply itself (headless-friendly). This picks the
  engine: KittenTTS / NeuTTS use the legacy backends; Qwen3-TTS and Orpheus route
  through the pluggable engine abstraction. Kyutai-TTS is experimental (CPU eager;
  first run downloads ~4 GB).

  `fetchTtsEngine` reads settings.json on mount; `saveTtsEngine` writes through on
  every change (with a localStorage cache for instant first paint).
-->
<script lang="ts">
import { onMount } from "svelte";
import { Card, CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { type TtsEngineConfig, fetchTtsEngine, loadTtsEngine, saveTtsEngine } from "$lib/llm/tts";
import { t } from "$lib/i18n/index.svelte";

let cfg = $state<TtsEngineConfig>(loadTtsEngine());

onMount(async () => {
  cfg = await fetchTtsEngine();
});

function update(patch: Partial<TtsEngineConfig>) {
  cfg = { ...cfg, ...patch };
  saveTtsEngine(cfg);
}

const engineOptions: { val: string; label: string; experimental?: boolean }[] = [
  { val: "kitten", label: "KittenTTS" },
  { val: "neutts", label: "NeuTTS" },
  { val: "qwen3-tts", label: "Qwen3-TTS" },
  { val: "orpheus", label: "Orpheus" },
  { val: "kyutai-tts", label: "Kyutai-TTS", experimental: true },
];

const MODELS_BY_ENGINE: Record<string, string[]> = {
  "qwen3-tts": ["Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice"],
};
const DEFAULT_MODEL_BY_ENGINE: Record<string, string> = {
  "qwen3-tts": "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
};
const DEFAULT_VOICE_BY_ENGINE: Record<string, string> = {
  "qwen3-tts": "vivian",
  orpheus: "tara",
};
/** Fallback when the daemon has not yet returned `voices`. */
const VOICES_BY_ENGINE: Record<string, string[]> = {
  "qwen3-tts": [
    "vivian",
    "serena",
    "uncle_fu",
    "dylan",
    "eric",
    "ryan",
    "aiden",
    "ono_anna",
    "sohee",
  ],
  orpheus: ["tara", "leah", "jess", "leo", "dan", "mia", "zac", "zoe"],
};

const hasModelPicker = $derived((MODELS_BY_ENGINE[cfg.engine] ?? []).length > 0);
const knownVoices = $derived(cfg.voices?.length ? cfg.voices : (VOICES_BY_ENGINE[cfg.engine] ?? []));
const knownModels = $derived(MODELS_BY_ENGINE[cfg.engine] ?? []);
const hasVoicePicker = $derived(knownVoices.length > 0);
const isKyutai = $derived(cfg.engine === "kyutai-tts");
const isOrpheus = $derived(cfg.engine === "orpheus");

function onEngineSelect(engine: string) {
  const models = MODELS_BY_ENGINE[engine] ?? [];
  const model = models.includes(cfg.model)
    ? cfg.model
    : (DEFAULT_MODEL_BY_ENGINE[engine] ?? "");
  const voices = cfg.voices?.length ? cfg.voices : (VOICES_BY_ENGINE[engine] ?? []);
  const defaultVoice = DEFAULT_VOICE_BY_ENGINE[engine] ?? "";
  const voice = voices.includes(cfg.voice)
    ? cfg.voice
    : (defaultVoice && voices.includes(defaultVoice) ? defaultVoice : "");
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
        <div class="flex items-center gap-1.5 flex-wrap">
          {#each engineOptions as opt}
            <button
              onclick={() => onEngineSelect(opt.val)}
              class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold transition-all cursor-pointer
                   {cfg.engine === opt.val
                     ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                     : 'border-border bg-muted text-muted-foreground hover:text-foreground'}"
            >
              {opt.label}
            </button>
          {/each}
        </div>
        {#if isKyutai}
          <p class="text-ui-base text-amber-600 dark:text-amber-400">{t("chat.tts.kyutaiExperimental")}</p>
        {/if}
        {#if isOrpheus}
          <p class="text-ui-base text-muted-foreground">{t("chat.tts.orpheusHint")}</p>
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
