<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { t } from "$lib/i18n/index.svelte";

interface Props {
  enabled: boolean;
  isMac: boolean;
  ocrEngine: string;
  useGpu: boolean;
  sharedTextModel: string;
  saving: boolean;
  onSetOcrEngine: (engine: string) => void;
  onSave: () => void | Promise<void>;
}

let { enabled, isMac, ocrEngine, useGpu, sharedTextModel, saving, onSetOcrEngine, onSave }: Props = $props();
</script>

{#if enabled}
  <div class="flex items-center gap-2 px-0.5 pt-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("screenshots.ocrTitle")}
    </span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="px-4 py-3.5 flex flex-col gap-3">
      <div class="flex items-start gap-3">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"
             stroke-linecap="round" stroke-linejoin="round"
             class="w-5 h-5 shrink-0 text-violet-500/70 mt-0.5">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="16" y1="13" x2="8" y2="13"/>
          <line x1="16" y1="17" x2="8" y2="17"/>
          <polyline points="10 9 9 9 8 9"/>
        </svg>
        <div class="flex flex-col gap-1 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.ocrEngine")}</span>
          <p class="text-[0.6rem] text-muted-foreground leading-relaxed">{t("screenshots.ocrDesc")}</p>
        </div>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <div class="flex flex-col gap-1.5">
        <span class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.ocrEngineSelect")}</span>
        <select
          value={ocrEngine}
          onchange={(e) => onSetOcrEngine((e.target as HTMLSelectElement).value)}
          class="w-full rounded-lg border border-border dark:border-white/[0.08]
                 bg-white dark:bg-[#14141e] px-3 py-2
                 text-[0.72rem] text-foreground
                 focus:outline-none focus:ring-1 focus:ring-violet-500/50">
          {#if isMac}
            <option value="apple-vision">{t("screenshots.ocrEngineAppleVision")}</option>
          {/if}
          <option value="ocrs">{t("screenshots.ocrEngineOcrs")}</option>
        </select>
        {#if isMac && ocrEngine !== "apple-vision"}
          <span class="text-[0.5rem] text-amber-600 dark:text-amber-400">{t("screenshots.ocrAppleVisionHint")}</span>
        {/if}
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <div class="flex justify-end">
        <Button size="sm" onclick={onSave} disabled={saving} class="text-[0.65rem] h-7 px-4">
          {saving ? t("common.saving") : t("common.apply")}
        </Button>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <div class="flex flex-col gap-2">
        <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground/50">
          {t("screenshots.ocrActiveModels")}
        </span>
        <div class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-[0.62rem]">
          <span class="text-muted-foreground">{t("screenshots.ocrDetModel")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">text-detection.rten</span>
          <span class="text-muted-foreground">{t("screenshots.ocrRecModel")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">text-recognition.rten</span>
          <span class="text-muted-foreground">{t("screenshots.ocrTextEmbed")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">
            {sharedTextModel || "—"}
            <span class="text-muted-foreground/50 font-sans ml-1">(Embeddings)</span>
          </span>
          <span class="text-muted-foreground">{t("screenshots.ocrIndex")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">screenshots_ocr.hnsw</span>
          <span class="text-muted-foreground">{t("screenshots.ocrInference")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">{useGpu ? "GPU" : "CPU"}</span>
        </div>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <div class="flex items-center gap-2 px-1 py-1">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             class="w-3.5 h-3.5 text-violet-500/50 shrink-0">
          <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
        </svg>
        <span class="text-[0.58rem] text-muted-foreground leading-relaxed">{t("screenshots.ocrSearchHint")}</span>
      </div>
    </CardContent>
  </Card>
{/if}
