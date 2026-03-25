<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Button } from "$lib/components/ui/button";
import { t } from "$lib/i18n/index.svelte";

interface ReembedEstimate {
  total: number;
  stale: number;
  unembedded: number;
  per_image_ms: number;
  eta_secs: number;
}

interface ReembedProgress {
  done: number;
  total: number;
  elapsed_secs: number;
  eta_secs: number;
}

interface Props {
  modelChanged: boolean;
  staleCount: number;
  estimate: ReembedEstimate | null;
  reembedding: boolean;
  progress: ReembedProgress | null;
  onReembed: () => void | Promise<void>;
  onDismissModelChanged: () => void;
}

let { modelChanged, staleCount, estimate, reembedding, progress, onReembed, onDismissModelChanged }: Props = $props();

function fmtEta(secs: number): string {
  if (secs < 60) return `${Math.round(secs)}s`;
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return `${m}m ${s}s`;
}
</script>

{#if modelChanged && staleCount > 0}
  <div class="rounded-xl border border-amber-500/30 bg-amber-500/5 dark:bg-amber-500/10 px-4 py-3 flex flex-col gap-2">
    <div class="flex items-center gap-2">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 shrink-0 text-amber-500">
        <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
        <line x1="12" y1="9" x2="12" y2="13"/>
        <line x1="12" y1="17" x2="12.01" y2="17"/>
      </svg>
      <span class="text-[0.72rem] font-semibold text-amber-600 dark:text-amber-400">{t("screenshots.modelChanged")}</span>
    </div>
    <p class="text-[0.62rem] text-amber-600/80 dark:text-amber-400/80 leading-relaxed">{staleCount} {t("screenshots.modelChangedDesc")}</p>
    {#if estimate}
      <p class="text-[0.58rem] text-amber-600/60 dark:text-amber-400/60">{t("screenshots.estimate")} ~{fmtEta(estimate.eta_secs)}</p>
    {/if}
    <div class="flex gap-2 mt-1">
      <Button size="sm" onclick={onReembed} disabled={reembedding} class="text-[0.62rem] h-7 px-3">
        {reembedding ? t("screenshots.reembedding") : t("screenshots.reembedNowBtn")}
      </Button>
      <Button size="sm" variant="ghost" onclick={onDismissModelChanged} class="text-[0.62rem] h-7 px-3 text-muted-foreground">
        {t("common.dismiss")}
      </Button>
    </div>
  </div>
{/if}

<div class="flex flex-col gap-2">
  <div class="flex items-center justify-between">
    <div class="flex flex-col gap-0.5">
      <div class="flex items-center gap-2">
        <span class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.reembed")}</span>
        {#if estimate && estimate.stale > 0}
          <span class="rounded-full px-1.5 py-0 text-[0.55rem] font-semibold bg-amber-500/15 text-amber-600 dark:text-amber-400 border border-amber-500/25">{estimate.stale} {t("screenshots.stale")}</span>
        {/if}
        {#if estimate && estimate.unembedded > 0}
          <span class="rounded-full px-1.5 py-0 text-[0.55rem] font-semibold bg-violet-500/15 text-violet-600 dark:text-violet-400 border border-violet-500/25">{estimate.unembedded} {t("screenshots.unembedded")}</span>
        {/if}
      </div>
      <span class="text-[0.6rem] text-muted-foreground/60">
        {t("screenshots.reembedDesc")}
        {#if estimate && estimate.eta_secs > 0}
          — {t("screenshots.estimate")} ~{fmtEta(estimate.eta_secs)}
        {/if}
      </span>
    </div>
    <Button size="sm" variant="outline" onclick={onReembed} disabled={reembedding} class="text-[0.65rem] h-7 px-3 shrink-0">
      {reembedding ? t("screenshots.reembedding") : t("screenshots.reembedBtn")}
    </Button>
  </div>

  {#if progress !== null}
    {@const pct = progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0}
    <div class="flex flex-col gap-1">
      <div class="h-1.5 rounded-full bg-muted dark:bg-white/[0.06] overflow-hidden">
        <div class="h-full rounded-full bg-violet-500 transition-all duration-300" style="width: {pct}%"></div>
      </div>
      <span class="text-[0.58rem] text-muted-foreground/60 tabular-nums">
        {progress.done} / {progress.total} — {pct}%
        {#if progress.eta_secs > 0}
          — ETA {fmtEta(progress.eta_secs)}
        {/if}
      </span>
    </div>
  {/if}
</div>

{#if estimate}
  <div class="flex flex-col gap-1 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground/50">{t("screenshots.stats")}</span>
    <div class="grid grid-cols-2 gap-x-4 gap-y-1 text-[0.62rem]">
      <span class="text-muted-foreground">{t("screenshots.embeddedCount")}</span>
      <span class="text-foreground tabular-nums">{estimate.total}</span>
      <span class="text-muted-foreground">{t("screenshots.unembeddedCount")}</span>
      <span class="text-foreground tabular-nums">{estimate.unembedded}</span>
      <span class="text-muted-foreground">{t("screenshots.staleCount")}</span>
      <span class="text-foreground tabular-nums">{estimate.stale}</span>
    </div>
  </div>
{/if}
