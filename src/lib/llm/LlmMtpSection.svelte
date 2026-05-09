<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Card, CardContent } from "$lib/components/ui/card";
import { t } from "$lib/i18n/index.svelte";

interface Props {
  mtpDraftCount: number;
  configSaving: boolean;
  quant: string;
  onSetMtpDraftCount: (val: number) => void | Promise<void>;
}

let { mtpDraftCount, configSaving, quant, onSetMtpDraftCount }: Props = $props();

let showSection = $state(false);

/** Recommended default based on quantisation: Q8 → 3, Q4 → 1, else 1. */
const recommendedDraft = $derived(quant.toUpperCase().startsWith("Q8") ? 3 : 1);

const draftOptions: [number, string][] = [
  [0, "Off"],
  [1, "1"],
  [2, "2"],
  [3, "3"],
];
</script>

<section class="flex flex-col gap-2">
  <button
    onclick={() => (showSection = !showSection)}
    class="flex items-center gap-2 px-0.5 cursor-pointer select-none group"
  >
    <span
      class="text-ui-xs font-semibold tracking-widest uppercase text-muted-foreground group-hover:text-foreground transition-colors"
    >
      {t("llm.section.mtp")}
    </span>
    <svg
      viewBox="0 0 16 16"
      fill="currentColor"
      class="w-2.5 h-2.5 text-muted-foreground/50 transition-transform {showSection
        ? 'rotate-180'
        : ''}"
    >
      <path d="M3 6l5 5 5-5H3z" />
    </svg>
    {#if configSaving}<span class="text-ui-xs text-muted-foreground">saving…</span>{/if}
  </button>

  {#if showSection}
    <Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
        <div class="flex flex-col gap-2 px-4 py-3.5">
          <div class="flex items-baseline justify-between">
            <span class="text-ui-lg font-semibold text-foreground">{t("llm.mtp.draftTokens")}</span>
            <span class="text-ui-base text-muted-foreground tabular-nums">
              {mtpDraftCount === 0 ? "Off" : mtpDraftCount}
            </span>
          </div>
          <p class="text-ui-base text-muted-foreground -mt-1">{t("llm.mtp.draftTokensDesc")}</p>
          <div class="flex items-center gap-1.5 flex-wrap">
            {#each draftOptions as [val, label]}
              <button
                onclick={() => onSetMtpDraftCount(val)}
                class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold transition-all cursor-pointer
                     {mtpDraftCount === val
                       ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                       : 'border-border bg-muted text-muted-foreground hover:text-foreground'}"
              >
                {label}{val > 0 && val === recommendedDraft ? " ★" : ""}
              </button>
            {/each}
          </div>
        </div>
      </CardContent>
    </Card>
  {/if}
</section>
