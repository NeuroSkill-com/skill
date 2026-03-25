<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import {
  EMBEDDING_EPOCH_SECS,
  SCREENSHOT_INTERVAL_MAX_SECS,
  SCREENSHOT_INTERVAL_MIN_SECS,
  SCREENSHOT_INTERVAL_STEP_SECS,
} from "$lib/constants";
import { t } from "$lib/i18n/index.svelte";

interface ScreenshotCaptureConfig {
  enabled: boolean;
  interval_secs: number;
  image_size: number;
  quality: number;
  embed_backend: string;
  fastembed_model: string;
}

interface Props {
  config: ScreenshotCaptureConfig;
  saving: boolean;
  recommendedSize: number;
  onUpdate: (patch: Partial<ScreenshotCaptureConfig>, adoptRecommended?: boolean) => void;
  onSave: () => void | Promise<void>;
}

let { config, saving, recommendedSize, onUpdate, onSave }: Props = $props();
</script>

{#if config.enabled}
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-4 px-4 flex flex-col gap-4">
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <label for="ss-interval" class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.interval")}</label>
          <span class="text-[0.58rem] text-muted-foreground tabular-nums">
            {config.interval_secs}{t("screenshots.intervalUnit")}
            ({Math.round(config.interval_secs / EMBEDDING_EPOCH_SECS)}× {t("screenshots.intervalEpoch")})
          </span>
        </div>
        <input id="ss-interval" type="range"
               min={SCREENSHOT_INTERVAL_MIN_SECS} max={SCREENSHOT_INTERVAL_MAX_SECS}
               step={SCREENSHOT_INTERVAL_STEP_SECS}
               value={config.interval_secs}
               oninput={(e) => onUpdate({ interval_secs: Number((e.target as HTMLInputElement).value) })}
               class="w-full accent-violet-500 h-1.5" />
        <span class="text-[0.54rem] text-muted-foreground/60">{t("screenshots.intervalDesc")}</span>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <label for="ss-size" class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.imageSize")}</label>
          <span class="text-[0.58rem] text-muted-foreground tabular-nums">{config.image_size} {t("screenshots.imageSizeUnit")}</span>
        </div>
        <input id="ss-size" type="range" min="224" max="1536" step="32"
               value={config.image_size}
               oninput={(e) => onUpdate({ image_size: Number((e.target as HTMLInputElement).value) })}
               class="w-full accent-violet-500 h-1.5" />
        <span class="text-[0.54rem] text-muted-foreground/60">
          {t("screenshots.imageSizeDesc")}
          <span class="font-semibold"> {t("screenshots.imageSizeRecommended")} {recommendedSize}{t("screenshots.imageSizeUnit")}</span>
        </span>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <label for="ss-quality" class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.quality")}</label>
          <span class="text-[0.58rem] text-muted-foreground tabular-nums">{config.quality}</span>
        </div>
        <input id="ss-quality" type="range" min="10" max="100" step="5"
               value={config.quality}
               oninput={(e) => onUpdate({ quality: Number((e.target as HTMLInputElement).value) })}
               class="w-full accent-violet-500 h-1.5" />
        <span class="text-[0.54rem] text-muted-foreground/60">{t("screenshots.qualityDesc")}</span>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <div class="flex flex-col gap-1.5">
        <span class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.embeddingModel")}</span>
        <span class="text-[0.54rem] text-muted-foreground/60">{t("screenshots.embeddingModelDesc")}</span>

        <div class="flex flex-col gap-1">
          <select
            value={config.embed_backend}
            onchange={(e) => onUpdate({ embed_backend: (e.target as HTMLSelectElement).value }, true)}
            class="w-full rounded-lg border border-border dark:border-white/[0.08] bg-white dark:bg-[#14141e] px-3 py-2 text-[0.72rem] text-foreground focus:outline-none focus:ring-1 focus:ring-violet-500/50">
            <option value="fastembed">{t("screenshots.backendFastembed")}</option>
            <option value="mmproj">{t("screenshots.backendMmproj")}</option>
            <option value="llm-vlm">{t("screenshots.backendLlmVlm")}</option>
          </select>

          {#if config.embed_backend === "fastembed"}
            <select
              value={config.fastembed_model}
              onchange={(e) => onUpdate({ fastembed_model: (e.target as HTMLSelectElement).value }, true)}
              class="w-full rounded-lg border border-border dark:border-white/[0.08] bg-white dark:bg-[#14141e] px-3 py-2 text-[0.72rem] text-foreground focus:outline-none focus:ring-1 focus:ring-violet-500/50">
              <option value="clip-vit-b-32">{t("screenshots.modelClip")}</option>
              <option value="nomic-embed-vision-v1.5">{t("screenshots.modelNomic")}</option>
            </select>
          {/if}
        </div>
      </div>

      <div class="flex justify-end">
        <Button size="sm" onclick={onSave} disabled={saving} class="text-[0.65rem] h-7 px-4">
          {saving ? t("common.saving") : t("common.apply")}
        </Button>
      </div>
    </CardContent>
  </Card>
{/if}
