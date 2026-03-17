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
  }

  let { segments, totalUsed, nCtx, isEstimate, onClose }: Props = $props();

  const sortedSegments = $derived(
    [...segments].filter(s => s.tokens > 0).sort((a, b) => b.tokens - a.tokens)
  );
  const segmentSum = $derived(sortedSegments.reduce((s, seg) => s + seg.tokens, 0));
  const freeTokens = $derived(Math.max(0, nCtx - totalUsed));

  /** Bar widths as % of nCtx, with a thin minimum so tiny slices stay visible. */
  const barWidths = $derived.by(() => {
    if (nCtx <= 0) return { segs: [] as number[], free: 100 };
    const raw = sortedSegments.map(s => (s.tokens / nCtx) * 100);
    const MIN = 0.6;
    // Give tiny segments a minimum, then scale the rest so total never exceeds 100
    const boosted = raw.map(v => Math.max(v, MIN));
    const usedPct = Math.min(boosted.reduce((a, b) => a + b, 0), 100);
    const freePct = Math.max(0, 100 - usedPct);
    return { segs: boosted, free: freePct };
  });

  const fmtPct = (n: number) => nCtx > 0 ? ((n / nCtx) * 100).toFixed(1) : "0.0";
  const fmtNum = (n: number) => n.toLocaleString();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="fixed inset-0 z-50" onclick={onClose}>
  <div
    class="absolute top-10 right-16 w-72 rounded-xl shadow-xl border border-border dark:border-white/10
           bg-white dark:bg-[#161622] p-3 text-xs select-none animate-in fade-in slide-in-from-top-1 duration-150"
    onclick={(e) => e.stopPropagation()}
  >
    <!-- Header -->
    <div class="flex items-center justify-between mb-2.5">
      <span class="font-semibold text-foreground text-[0.7rem]">{t("chat.ctx.title")}</span>
      {#if isEstimate}
        <span class="text-[0.55rem] text-muted-foreground/60 italic">{t("chat.ctx.estimated")}</span>
      {/if}
    </div>

    <!-- Stacked bar -->
    <div class="flex h-2.5 rounded-full overflow-hidden bg-muted-foreground/8 mb-3"
         title="{fmtNum(totalUsed)} / {fmtNum(nCtx)}">
      {#each sortedSegments as seg, i (seg.key)}
        <div
          class="h-full transition-all duration-200"
          style="width:{barWidths.segs[i]}%; background:{seg.color}; opacity:0.85;"
          title="{t(seg.labelKey)}: {fmtNum(seg.tokens)} ({fmtPct(seg.tokens)}%)"
        ></div>
      {/each}
    </div>

    <!-- Legend rows -->
    <div class="flex flex-col gap-1.5">
      {#each sortedSegments as seg (seg.key)}
        <div class="flex items-center gap-2">
          <span class="w-2.5 h-2.5 rounded-[3px] shrink-0" style="background:{seg.color};"></span>
          <span class="flex-1 text-muted-foreground truncate">{t(seg.labelKey)}</span>
          <span class="tabular-nums text-foreground font-medium">{fmtNum(seg.tokens)}</span>
          <span class="tabular-nums text-muted-foreground/60 w-10 text-right">{fmtPct(seg.tokens)}%</span>
        </div>
      {/each}

      <!-- Free -->
      <div class="flex items-center gap-2 opacity-50">
        <span class="w-2.5 h-2.5 rounded-[3px] shrink-0 bg-muted-foreground/15"></span>
        <span class="flex-1 text-muted-foreground truncate">{t("chat.ctx.free")}</span>
        <span class="tabular-nums text-foreground font-medium">{fmtNum(freeTokens)}</span>
        <span class="tabular-nums text-muted-foreground/60 w-10 text-right">{fmtPct(freeTokens)}%</span>
      </div>
    </div>

    <!-- Footer total -->
    <div class="mt-2.5 pt-2 border-t border-border dark:border-white/[0.06] flex justify-between">
      <span class="text-muted-foreground font-medium">{t("chat.ctx.total")}</span>
      <span class="tabular-nums font-semibold text-foreground">{fmtNum(totalUsed)} / {fmtNum(nCtx)}</span>
    </div>
  </div>
</div>
