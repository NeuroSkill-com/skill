<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { t } from "$lib/i18n/index.svelte";

interface PipelineMetrics {
  captures: number;
  capture_errors: number;
  drops: number;
  capture_us: number;
  ocr_us: number;
  resize_us: number;
  save_us: number;
  capture_total_us: number;
  embeds: number;
  embed_errors: number;
  vision_embed_us: number;
  text_embed_us: number;
  embed_total_us: number;
  queue_depth: number;
  last_capture_unix: number;
  last_embed_unix: number;
  backoff_multiplier: number;
}

interface Props {
  enabled: boolean;
  metrics: PipelineMetrics | null;
  captureHistory: number[];
  embedHistory: number[];
  queueHistory: number[];
  captureBreakdown: { capture: number; ocr: number; resize: number; save: number };
  embedBreakdown: { vision: number; text: number };
  onRefresh: () => void | Promise<void>;
}

let { enabled, metrics, captureHistory, embedHistory, queueHistory, captureBreakdown, embedBreakdown, onRefresh }: Props = $props();

function fmtUs(us: number): string {
  if (us < 1000) return `${us}µs`;
  if (us < 1_000_000) return `${(us / 1000).toFixed(1)}ms`;
  return `${(us / 1_000_000).toFixed(2)}s`;
}

function fmtMs(ms: number): string {
  if (ms < 1) return `${(ms * 1000).toFixed(0)}µs`;
  if (ms < 1000) return `${ms.toFixed(1)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function sparklinePath(data: number[], w: number, h: number, pad = 2): string {
  if (data.length < 2) return "";
  const maxV = Math.max(...data, 1);
  const usableH = h - pad * 2;
  return data
    .map((v, i) => {
      const x = (i / (data.length - 1)) * w;
      const y = pad + usableH - (v / maxV) * usableH;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

function areaPath(data: number[], w: number, h: number, pad = 2): string {
  if (data.length < 2) return "";
  const line = sparklinePath(data, w, h, pad);
  return `0,${h} ${line} ${w},${h}`;
}
</script>

{#if enabled && metrics && metrics.captures > 0}
  {@const CW = 280}
  {@const CH = 56}
  <div class="flex items-center gap-2 px-0.5 pt-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("screenshots.perfTitle")}
    </span>
    <button onclick={onRefresh}
            class="text-[0.48rem] text-muted-foreground/40 hover:text-muted-foreground transition-colors">
      ↻
    </button>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="px-4 py-3.5 flex flex-col gap-4">
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center gap-2">
          <span class="w-1.5 h-1.5 rounded-full bg-green-500 shrink-0"></span>
          <span class="text-[0.66rem] font-semibold text-foreground">{t("screenshots.perfCapture")}</span>
          <span class="ml-auto text-[0.54rem] text-muted-foreground tabular-nums">
            {metrics.captures} {t("screenshots.perfTotal")}
            {#if metrics.capture_errors > 0}
              · <span class="text-red-500">{metrics.capture_errors} {t("screenshots.perfErrors")}</span>
            {/if}
          </span>
        </div>

        <div class="rounded-lg bg-muted/30 dark:bg-white/[0.02] border border-border/50 dark:border-white/[0.04] overflow-hidden">
          <svg viewBox="0 0 {CW} {CH}" class="w-full h-14" preserveAspectRatio="none">
            {#if captureHistory.length >= 2}
              <polygon points={areaPath(captureHistory, CW, CH)}
                       fill="currentColor" class="text-emerald-500/15 dark:text-emerald-400/10" />
              <polyline points={sparklinePath(captureHistory, CW, CH)}
                        fill="none" stroke="currentColor" stroke-width="1.5"
                        class="text-emerald-500 dark:text-emerald-400" />
            {/if}
          </svg>
        </div>

        {#if (captureBreakdown.capture + captureBreakdown.ocr + captureBreakdown.resize + captureBreakdown.save) > 0}
          {@const capTotal = captureBreakdown.capture + captureBreakdown.ocr + captureBreakdown.resize + captureBreakdown.save}
          <div class="flex h-2 rounded-full overflow-hidden bg-muted/40 dark:bg-white/[0.04]">
            <div class="bg-blue-500" style="width:{(captureBreakdown.capture / capTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfWindowCapture')}: {fmtMs(captureBreakdown.capture)}"></div>
            <div class="bg-violet-500" style="width:{(captureBreakdown.ocr / capTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfOcr')}: {fmtMs(captureBreakdown.ocr)}"></div>
            <div class="bg-amber-500" style="width:{(captureBreakdown.resize / capTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfResize')}: {fmtMs(captureBreakdown.resize)}"></div>
            <div class="bg-emerald-500" style="width:{(captureBreakdown.save / capTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfSave')}: {fmtMs(captureBreakdown.save)}"></div>
          </div>
          <div class="flex flex-wrap gap-x-3 gap-y-0.5 text-[0.5rem] text-muted-foreground">
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-blue-500 shrink-0"></span>{t("screenshots.perfWindowCapture")} {fmtMs(captureBreakdown.capture)}</span>
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-violet-500 shrink-0"></span>{t("screenshots.perfOcr")} {fmtMs(captureBreakdown.ocr)}</span>
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-amber-500 shrink-0"></span>{t("screenshots.perfResize")} {fmtMs(captureBreakdown.resize)}</span>
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-emerald-500 shrink-0"></span>{t("screenshots.perfSave")} {fmtMs(captureBreakdown.save)}</span>
            <span class="font-semibold text-foreground/70">{t("screenshots.perfIterTotal")} {fmtUs(metrics.capture_total_us)}</span>
          </div>
        {/if}
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <div class="flex flex-col gap-1.5">
        <div class="flex items-center gap-2">
          <span class="w-1.5 h-1.5 rounded-full shrink-0 {metrics.queue_depth > 2 ? 'bg-amber-500' : 'bg-green-500'}"></span>
          <span class="text-[0.66rem] font-semibold text-foreground">{t("screenshots.perfEmbed")}</span>
          <span class="ml-auto text-[0.54rem] text-muted-foreground tabular-nums">{metrics.embeds} {t("screenshots.perfTotal")}</span>
        </div>

        <div class="rounded-lg bg-muted/30 dark:bg-white/[0.02] border border-border/50 dark:border-white/[0.04] overflow-hidden">
          <svg viewBox="0 0 {CW} {CH}" class="w-full h-14" preserveAspectRatio="none">
            {#if embedHistory.length >= 2}
              <polygon points={areaPath(embedHistory, CW, CH)}
                       fill="currentColor" class="text-blue-500/15 dark:text-blue-400/10" />
              <polyline points={sparklinePath(embedHistory, CW, CH)}
                        fill="none" stroke="currentColor" stroke-width="1.5"
                        class="text-blue-500 dark:text-blue-400" />
            {/if}
          </svg>
        </div>

        {#if (embedBreakdown.vision + embedBreakdown.text) > 0}
          {@const embTotal = embedBreakdown.vision + embedBreakdown.text}
          <div class="flex h-2 rounded-full overflow-hidden bg-muted/40 dark:bg-white/[0.04]">
            <div class="bg-blue-500" style="width:{(embedBreakdown.vision / embTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfVisionEmbed')}: {fmtMs(embedBreakdown.vision)}"></div>
            <div class="bg-violet-500" style="width:{(embedBreakdown.text / embTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfTextEmbed')}: {fmtMs(embedBreakdown.text)}"></div>
          </div>
          <div class="flex flex-wrap gap-x-3 gap-y-0.5 text-[0.5rem] text-muted-foreground">
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-blue-500 shrink-0"></span>{t("screenshots.perfVisionEmbed")} {fmtMs(embedBreakdown.vision)}</span>
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-violet-500 shrink-0"></span>{t("screenshots.perfTextEmbed")} {fmtMs(embedBreakdown.text)}</span>
            <span class="font-semibold text-foreground/70">{t("screenshots.perfIterTotal")} {fmtUs(metrics.embed_total_us)}</span>
          </div>
        {/if}
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <div class="flex flex-col gap-1.5">
        <div class="flex items-center gap-2">
          <span class="text-[0.62rem] font-semibold text-foreground">{t("screenshots.perfQueue")}</span>
          <span class="text-[0.58rem] tabular-nums font-semibold {metrics.queue_depth > 2 ? 'text-amber-500' : 'text-foreground'}">
            {metrics.queue_depth}/4
          </span>
          <span class="text-[0.54rem] text-muted-foreground ml-2">{t("screenshots.perfDrops")}</span>
          <span class="text-[0.58rem] tabular-nums font-semibold {metrics.drops > 0 ? 'text-red-500' : 'text-foreground'}">
            {metrics.drops}
          </span>
        </div>

        <div class="rounded-lg bg-muted/30 dark:bg-white/[0.02] border border-border/50 dark:border-white/[0.04] overflow-hidden">
          <svg viewBox="0 0 {CW} 32" class="w-full h-8" preserveAspectRatio="none">
            <line x1="0" y1="4" x2={CW} y2="4" stroke="currentColor" stroke-width="0.5"
                  stroke-dasharray="4 3" class="text-red-500/30" />
            {#if queueHistory.length >= 2}
              {@const qPoints = queueHistory.map((v, i) => {
                const maxQ = 4;
                const x = (i / (queueHistory.length - 1)) * CW;
                const y = 30 - (Math.min(v, maxQ) / maxQ) * 26;
                return `${x.toFixed(1)},${y.toFixed(1)}`;
              }).join(" ")}
              <polygon points="0,32 {qPoints} {CW},32"
                       fill="currentColor" class="text-amber-500/15 dark:text-amber-400/10" />
              <polyline points={qPoints}
                        fill="none" stroke="currentColor" stroke-width="1.5"
                        class="text-amber-500 dark:text-amber-400" />
            {/if}
          </svg>
        </div>

        {#if metrics.backoff_multiplier > 1}
          <div class="flex items-center gap-1.5">
            <span class="text-muted-foreground">{t("screenshots.perfBackoff")}</span>
            <span class="tabular-nums font-semibold text-amber-500">{metrics.backoff_multiplier}×</span>
          </div>
        {/if}
        {#if metrics.drops > 0}
          <span class="text-[0.5rem] text-red-500/70">{t("screenshots.perfDropsHint")}</span>
        {/if}
      </div>
    </CardContent>
  </Card>
{/if}
