<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Shared TTS / ASR engine chip picker (catalog-driven).
  Shows experimental / needs-bundle / downloadable badges; greys out unavailable.
-->
<script lang="ts">
import { t } from "$lib/i18n/index.svelte";
import {
  asrEngineLabelKey,
  ttsEngineLabelKey,
  type AsrEngineInfo,
  type TtsEngineInfo,
} from "$lib/llm/voice-catalog";

type Engine = TtsEngineInfo | AsrEngineInfo;

interface Props {
  kind: "tts" | "asr";
  engines: Engine[];
  selectedId: string;
  onSelect: (id: string) => void;
  /** Visual accent for the selected chip. */
  accent?: "violet" | "indigo";
}

let { kind, engines, selectedId, onSelect, accent = "violet" }: Props = $props();

const selectedClass =
  accent === "indigo"
    ? "border-indigo-500 bg-indigo-50 dark:bg-indigo-950/40 text-indigo-700 dark:text-indigo-300"
    : "border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400";

function labelFor(eng: Engine): string {
  const key = kind === "tts" ? ttsEngineLabelKey(eng.id) : asrEngineLabelKey(eng.id);
  const translated = t(key);
  return translated === key ? eng.label : translated;
}

function isAvailable(eng: Engine): boolean {
  return eng.available !== false;
}

function needsBundle(eng: Engine): boolean {
  return "needs_bundle" in eng && Boolean(eng.needs_bundle);
}

/** Bundle-only engines with no Hub download stay greyed until a local pack exists. */
function isSelectable(eng: Engine): boolean {
  if (!isAvailable(eng)) return false;
  if (needsBundle(eng) && !eng.downloadable) return false;
  return true;
}
</script>

<div class="flex items-center gap-1.5 flex-wrap">
  {#each engines as opt}
    {@const avail = isSelectable(opt)}
    {@const bundle = needsBundle(opt)}
    <button
      type="button"
      disabled={!avail}
      title={!isAvailable(opt)
        ? t("chat.voice.engineUnavailable")
        : bundle && !opt.downloadable
          ? t("chat.tts.bundleExportHint")
          : opt.experimental
            ? t("chat.voice.engineExperimental")
            : undefined}
      onclick={() => avail && onSelect(opt.id)}
      class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold transition-all
             inline-flex items-center gap-1.5
             {selectedId === opt.id
               ? selectedClass
               : 'border-border bg-muted text-muted-foreground hover:text-foreground'}
             {!avail ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'}"
    >
      <span>{labelFor(opt)}</span>
      {#if opt.experimental}
        <span class="text-[10px] font-bold uppercase tracking-wide opacity-70">exp</span>
      {/if}
      {#if bundle}
        <span class="text-[10px] font-bold uppercase tracking-wide opacity-70">bundle</span>
      {/if}
      {#if opt.downloadable && !bundle}
        <span class="text-[10px] font-bold uppercase tracking-wide opacity-50">dl</span>
      {/if}
    </button>
  {/each}
</div>
