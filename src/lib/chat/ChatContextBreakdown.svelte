<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Context breakdown popover — shows proportional usage of each context component. -->
<script lang="ts">
import { t } from "$lib/i18n/index.svelte";

export interface ContextSegment {
  key: string;
  labelKey: string;
  tokens: number;
  color: string;
}

interface Props {
  segments: ContextSegment[];
  totalUsed: number;
  nCtx: number;
  isEstimate: boolean;
  onClose: () => void;
  onViewFull?: () => void;
}

let { segments, totalUsed, nCtx, isEstimate, onClose, onViewFull }: Props = $props();

const sortedSegments = $derived([...segments].filter((s) => s.tokens > 0).sort((a, b) => b.tokens - a.tokens));

/** Sum of all segment tokens — this is the authoritative total for proportions. */
const segmentSum = $derived(sortedSegments.reduce((a, s) => a + s.tokens, 0));
const freeTokens = $derived(Math.max(0, nCtx - totalUsed));

/** Percentage of the total context window (nCtx). Used for bar widths + free row. */
const pctOfCtx = (n: number) => (nCtx > 0 ? (n / nCtx) * 100 : 0);

/** Percentage of used tokens. Used for legend rows so proportions sum to 100%. */
const pctOfUsed = (n: number) => (segmentSum > 0 ? (n / segmentSum) * 100 : 0);

/** Bar segment widths as % of nCtx, with a small floor so tiny slices stay visible. */
const barWidths = $derived.by(() => {
  if (nCtx <= 0) return [] as number[];
  const MIN = 0.5;
  const raw = sortedSegments.map((s) => pctOfCtx(s.tokens));
  return raw.map((v) => Math.max(v, MIN));
});

const fmtNum = (n: number) => n.toLocaleString();
const fmtPct1 = (v: number) => v.toFixed(1);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="fixed inset-0 z-50" onclick={onClose}>
  <div
    class="absolute top-10 right-16 w-72 rounded-xl shadow-xl border border-border dark:border-white/10
           bg-surface-3 p-3 text-xs select-none animate-in fade-in slide-in-from-top-1 duration-150"
    onclick={(e) => e.stopPropagation()}
  >
    <!-- Header -->
    <div class="flex items-center justify-between mb-2.5">
      <span class="font-semibold text-foreground text-ui-md">{t("chat.ctx.title")}</span>
      {#if isEstimate}
        <span class="text-ui-xs text-muted-foreground/60 italic">{t("chat.ctx.estimated")}</span>
      {/if}
    </div>

    <!-- Stacked bar (proportional to nCtx) -->
    <div class="flex h-2.5 rounded-full overflow-hidden bg-muted-foreground/8 mb-3"
         title="{fmtNum(totalUsed)} / {fmtNum(nCtx)}">
      {#each sortedSegments as seg, i (seg.key)}
        <div
          class="h-full transition-all duration-200"
          style="width:{barWidths[i]}%; background:{seg.color}; opacity:0.85;"
          title="{t(seg.labelKey)}: {fmtNum(seg.tokens)}"
        ></div>
      {/each}
    </div>

    <!-- Legend rows (percentages = share of used context) -->
    <div class="flex flex-col gap-1.5">
      {#each sortedSegments as seg (seg.key)}
        {@const share = pctOfUsed(seg.tokens)}
        <div class="flex items-center gap-2">
          <span class="w-2.5 h-2.5 rounded-[3px] shrink-0" style="background:{seg.color};"></span>
          <span class="flex-1 text-muted-foreground truncate">{t(seg.labelKey)}</span>
          <span class="tabular-nums text-foreground font-medium">{fmtNum(seg.tokens)}</span>
          <span class="tabular-nums text-muted-foreground/60 w-10 text-right">{fmtPct1(share)}%</span>
        </div>
      {/each}

      <!-- Free (percentage = share of nCtx) -->
      <div class="flex items-center gap-2 pt-1 border-t border-border/40 dark:border-white/[0.04]">
        <span class="w-2.5 h-2.5 rounded-[3px] shrink-0 bg-muted-foreground/15"></span>
        <span class="flex-1 text-muted-foreground/60 truncate">{t("chat.ctx.free")}</span>
        <span class="tabular-nums text-muted-foreground/60 font-medium">{fmtNum(freeTokens)}</span>
        <span class="tabular-nums text-muted-foreground/40 w-10 text-right">{fmtPct1(pctOfCtx(freeTokens))}%</span>
      </div>
    </div>

    <!-- Footer total -->
    <div class="mt-2.5 pt-2 border-t border-border dark:border-white/[0.06] flex justify-between">
      <span class="text-muted-foreground font-medium">{t("chat.ctx.total")}</span>
      <span class="tabular-nums font-semibold text-foreground">
        {fmtNum(totalUsed)} / {fmtNum(nCtx)}
        <span class="text-muted-foreground/50 font-normal ml-1">({fmtPct1(pctOfCtx(totalUsed))}%)</span>
      </span>
    </div>

    <!-- View full context button -->
    {#if onViewFull}
      <button
        class="mt-2.5 w-full flex items-center justify-center gap-1.5 py-1.5 rounded-lg
               text-ui-base font-medium
               text-muted-foreground hover:text-foreground
               bg-muted/30 hover:bg-muted/50 dark:bg-white/[0.02] dark:hover:bg-white/[0.04]
               border border-border/40 dark:border-white/[0.04]
               transition-colors"
        onclick={(e) => { e.stopPropagation(); onViewFull(); }}
      >
        <!-- Expand icon -->
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round" class="size-3">
          <path d="M15 3h6v6" /><path d="M9 21H3v-6" />
          <path d="M21 3l-7 7" /><path d="M3 21l7-7" />
        </svg>
        {t("chat.ctx.viewFull")}
      </button>
    {/if}
  </div>
</div>
