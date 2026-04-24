<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- UMAP projection settings — repulsion, epochs, timeout -->
<script lang="ts">
import { onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { CardContent } from "$lib/components/ui/card";
import { ChipGroup } from "$lib/components/ui/chip-group";
import { SectionHeader } from "$lib/components/ui/section-header";
import { SettingsCard } from "$lib/components/ui/settings-card";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { t } from "$lib/i18n/index.svelte";

// ── Types ──────────────────────────────────────────────────────────────────
interface UmapConfig {
  repulsion_strength: number;
  neg_sample_rate: number;
  timeout_secs: number;
  n_epochs: number;
  n_neighbors: number;
  cooldown_ms: number;
  backend: string;
}

// ── State ──────────────────────────────────────────────────────────────────
let cfg = $state<UmapConfig>({
  repulsion_strength: 3.0,
  neg_sample_rate: 15,
  timeout_secs: 120,
  n_epochs: 500,
  n_neighbors: 15,
  cooldown_ms: 0,
  backend: "auto",
});
let availableBackends = $state<string[]>([]);
let saving = $state(false);
let dirty = $state(false);
let loaded = $state(false);

// ── Persistence ────────────────────────────────────────────────────────────
async function save() {
  saving = true;
  try {
    await daemonInvoke("set_umap_config", { config: cfg });
    dirty = false;
  } finally {
    saving = false;
  }
}

function markDirty() {
  dirty = true;
}

async function resetDefaults() {
  cfg = {
    repulsion_strength: 3.0,
    neg_sample_rate: 15,
    timeout_secs: 120,
    n_epochs: 500,
    n_neighbors: 15,
    cooldown_ms: 0,
    backend: "auto",
  };
  dirty = true;
  await save();
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
onMount(async () => {
  const [config, backends] = await Promise.all([
    daemonInvoke<UmapConfig>("get_umap_config"),
    daemonInvoke<{ available: string[] }>("get_umap_backends"),
  ]);
  cfg = config;
  availableBackends = backends.available;
  loaded = true;
});

// ── Presets ────────────────────────────────────────────────────────────────
const REPULSION_PRESETS: [string, number][] = [
  ["0.5 — subtle", 0.5],
  ["1.0 — standard", 1.0],
  ["2.0 — strong", 2.0],
  ["3.0 — aggressive", 3.0],
  ["5.0 — extreme", 5.0],
  ["8.0 — maximum", 8.0],
];

const NEG_SAMPLE_PRESETS: [string, number][] = [
  ["3", 3],
  ["5", 5],
  ["10", 10],
  ["15", 15],
  ["25", 25],
  ["30", 30],
];

const EPOCH_PRESETS: [string, number][] = [
  ["100", 100],
  ["200", 200],
  ["500", 500],
  ["800", 800],
  ["1500", 1500],
];

const NEIGHBOR_PRESETS: [string, number][] = [
  ["5", 5],
  ["10", 10],
  ["15", 15],
  ["25", 25],
  ["50", 50],
];

const TIMEOUT_PRESETS: [string, number][] = [
  ["30 s", 30],
  ["60 s", 60],
  ["120 s", 120],
  ["300 s", 300],
  ["600 s", 600],
];
</script>

{#if !loaded}
  <div class="flex items-center justify-center py-12">
    <span class="text-ui-md text-muted-foreground">{t("common.loading")}</span>
  </div>
{:else}

<!-- ── Compute Backend ─────────────────────────────────────────────────────── -->
{#if availableBackends.length > 0}
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <SectionHeader>{t("umapSettings.backend")}</SectionHeader>
    {#if saving}
      <span class="text-ui-xs text-muted-foreground">{t("common.saving")}</span>
    {/if}
    {#if dirty}
      <Button size="sm" variant="default" class="ml-auto text-ui-sm h-6 px-3" onclick={save}>
        {t("umapSettings.apply")}
      </Button>
    {/if}
  </div>

  <SettingsCard>
    <CardContent class="flex flex-col gap-2.5 px-4 py-3.5">
      <p class="text-ui-base text-muted-foreground leading-relaxed">
        {t("umapSettings.backendDesc")}
      </p>
      <ChipGroup
        items={[
          ["Auto", "auto"] as [string, string],
          ...availableBackends.map(b => [b.toUpperCase(), b] as [string, string]),
        ]}
        selected={
          [["Auto", "auto"] as [string, string],
           ...availableBackends.map(b => [b.toUpperCase(), b] as [string, string]),
          ].find(p => p[1] === cfg.backend) ?? ["Auto", "auto"]
        }
        onselect={(p) => { cfg.backend = p[1]; markDirty(); }}
        labelFn={(p) => p[0]}
      />
    </CardContent>
  </SettingsCard>
</section>
{/if}

<!-- ── Repulsion Strength ──────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <SectionHeader>{t("umapSettings.repulsion")}</SectionHeader>
    {#if saving}
      <span class="text-ui-xs text-muted-foreground">{t("common.saving")}</span>
    {/if}
    {#if dirty}
      <Button size="sm" variant="default" class="ml-auto text-ui-sm h-6 px-3" onclick={save}>
        {t("umapSettings.apply")}
      </Button>
    {/if}
  </div>

  <SettingsCard>
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- Repulsion strength -->
      <div class="flex flex-col gap-2.5 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-ui-lg font-semibold text-foreground">{t("umapSettings.repulsionStrength")}</span>
          <span class="text-ui-base text-muted-foreground tabular-nums">{cfg.repulsion_strength.toFixed(1)}</span>
        </div>
        <p class="text-ui-base text-muted-foreground leading-relaxed -mt-0.5">
          {t("umapSettings.repulsionDesc")}
        </p>
        <ChipGroup
          items={REPULSION_PRESETS}
          selected={REPULSION_PRESETS.find(p => p[1] === cfg.repulsion_strength) ?? REPULSION_PRESETS[0]}
          onselect={(p) => { cfg.repulsion_strength = p[1]; markDirty(); }}
          labelFn={(p) => p[0]}
        />
        <input type="range" min="0.1" max="10.0" step="0.1"
               bind:value={cfg.repulsion_strength}
               oninput={markDirty}
               class="umap-range umap-range-orange w-full h-1.5" />
      </div>

      <!-- Negative sample rate -->
      <div class="flex flex-col gap-2.5 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-ui-lg font-semibold text-foreground">{t("umapSettings.negSampleRate")}</span>
          <span class="text-ui-base text-muted-foreground tabular-nums">{cfg.neg_sample_rate}</span>
        </div>
        <p class="text-ui-base text-muted-foreground leading-relaxed -mt-0.5">
          {t("umapSettings.negSampleRateDesc")}
        </p>
        <ChipGroup
          items={NEG_SAMPLE_PRESETS}
          selected={NEG_SAMPLE_PRESETS.find(p => p[1] === cfg.neg_sample_rate) ?? NEG_SAMPLE_PRESETS[0]}
          onselect={(p) => { cfg.neg_sample_rate = p[1]; markDirty(); }}
          labelFn={(p) => p[0]}
        />
      </div>
    </CardContent>
  </SettingsCard>
</section>

<!-- ── Graph & Optimisation ────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <SectionHeader>{t("umapSettings.graphOptimisation")}</SectionHeader>

  <SettingsCard>
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- Neighbours -->
      <div class="flex flex-col gap-2.5 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-ui-lg font-semibold text-foreground">{t("umapSettings.nNeighbors")}</span>
          <span class="text-ui-base text-muted-foreground tabular-nums">{cfg.n_neighbors}</span>
        </div>
        <p class="text-ui-base text-muted-foreground leading-relaxed -mt-0.5">
          {t("umapSettings.nNeighborsDesc")}
        </p>
        <ChipGroup
          items={NEIGHBOR_PRESETS}
          selected={NEIGHBOR_PRESETS.find(p => p[1] === cfg.n_neighbors) ?? NEIGHBOR_PRESETS[0]}
          onselect={(p) => { cfg.n_neighbors = p[1]; markDirty(); }}
          labelFn={(p) => p[0]}
        />
      </div>

      <!-- Epochs -->
      <div class="flex flex-col gap-2.5 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-ui-lg font-semibold text-foreground">{t("umapSettings.nEpochs")}</span>
          <span class="text-ui-base text-muted-foreground tabular-nums">{cfg.n_epochs}</span>
        </div>
        <p class="text-ui-base text-muted-foreground leading-relaxed -mt-0.5">
          {t("umapSettings.nEpochsDesc")}
        </p>
        <ChipGroup
          items={EPOCH_PRESETS}
          selected={EPOCH_PRESETS.find(p => p[1] === cfg.n_epochs) ?? EPOCH_PRESETS[0]}
          onselect={(p) => { cfg.n_epochs = p[1]; markDirty(); }}
          labelFn={(p) => p[0]}
        />
      </div>
    </CardContent>
  </SettingsCard>
</section>

<!-- ── Timeout ──────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <SectionHeader>{t("umapSettings.safetyPerformance")}</SectionHeader>

  <SettingsCard>
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- Timeout -->
      <div class="flex flex-col gap-2.5 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-ui-lg font-semibold text-foreground">{t("umapSettings.timeout")}</span>
          <span class="text-ui-base text-muted-foreground tabular-nums">{cfg.timeout_secs}s</span>
        </div>
        <p class="text-ui-base text-muted-foreground leading-relaxed -mt-0.5">
          {t("umapSettings.timeoutDesc")}
        </p>
        <ChipGroup
          items={TIMEOUT_PRESETS}
          selected={TIMEOUT_PRESETS.find(p => p[1] === cfg.timeout_secs) ?? TIMEOUT_PRESETS[0]}
          onselect={(p) => { cfg.timeout_secs = p[1]; markDirty(); }}
          labelFn={(p) => p[0]}
        />
      </div>

      <!-- GPU Cooldown -->
      <div class="flex flex-col gap-2.5 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-ui-lg font-semibold text-foreground">{t("umapSettings.cooldownMs")}</span>
          <span class="text-ui-base text-muted-foreground tabular-nums">
            {cfg.cooldown_ms === 0 ? "0 ms" : cfg.cooldown_ms < 1000 ? `${cfg.cooldown_ms} ms` : `${(cfg.cooldown_ms / 1000).toFixed(1)}s`}
          </span>
        </div>
        <p class="text-ui-base text-muted-foreground leading-relaxed -mt-0.5">
          {t("umapSettings.cooldownMsDesc")}
        </p>
        <input type="range" min="0" max="10000" step="500"
               bind:value={cfg.cooldown_ms}
               oninput={markDirty}
               class="umap-range umap-range-rose w-full h-1.5" />
        <div class="flex justify-between text-ui-xs text-muted-foreground/60 -mt-0.5">
          <span>0s</span><span>5s</span><span>10s</span>
        </div>
      </div>

      <!-- Pipeline summary -->
      <div class="flex items-center gap-2 flex-wrap px-4 py-3 bg-surface-3">
        <SectionHeader class="shrink-0">{t("umapSettings.pipeline")}</SectionHeader>
        <Badge variant="outline"
          class="text-ui-xs py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
          repulsion {cfg.repulsion_strength.toFixed(1)}
        </Badge>
        <Badge variant="outline"
          class="text-ui-xs py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
          neg×{cfg.neg_sample_rate}
        </Badge>
        <Badge variant="outline"
          class="text-ui-xs py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
          k={cfg.n_neighbors}
        </Badge>
        <Badge variant="outline"
          class="text-ui-xs py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
          {cfg.n_epochs} epochs
        </Badge>
        <Badge variant="outline"
          class="text-ui-xs py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
          {cfg.timeout_secs}s timeout
        </Badge>
        <Badge variant="outline"
          class="text-ui-xs py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
          {cfg.cooldown_ms}ms cooldown
        </Badge>
        <Badge variant="outline"
          class="text-ui-xs py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
          {cfg.backend === "auto" ? "auto" : cfg.backend.toUpperCase()}
        </Badge>
        <span class="ml-auto text-ui-xs text-muted-foreground/60 shrink-0">fast-umap 1.6.0</span>
      </div>
    </CardContent>
  </SettingsCard>
</section>

<!-- ── Reset ───────────────────────────────────────────────────────────────── -->
<section class="flex items-center gap-3 px-0.5">
  <Button size="sm" variant="outline"
          class="text-ui-base h-7 px-3 text-muted-foreground hover:text-foreground"
          onclick={resetDefaults}>
    {t("umapSettings.resetDefaults")}
  </Button>
  {#if dirty}
    <Button size="sm" variant="default" class="text-ui-base h-7 px-4" onclick={save}>
      {saving ? t("common.saving") : t("umapSettings.apply")}
    </Button>
  {/if}
</section>

{/if}

<style>
  /* ── Range slider — light / dark ────────────────────────────────────── */
  .umap-range {
    -webkit-appearance: none;
    appearance: none;
    border-radius: 9999px;
    cursor: pointer;
  }
  /* Track */
  .umap-range::-webkit-slider-runnable-track {
    height: 6px;
    border-radius: 9999px;
  }
  .umap-range::-moz-range-track {
    height: 6px;
    border-radius: 9999px;
    border: none;
  }
  /* Light track */
  .umap-range::-webkit-slider-runnable-track { background: #e2e8f0; }
  .umap-range::-moz-range-track              { background: #e2e8f0; }
  :global(.dark) .umap-range::-webkit-slider-runnable-track { background: rgba(255,255,255,0.08); }
  :global(.dark) .umap-range::-moz-range-track              { background: rgba(255,255,255,0.08); }

  /* Thumb */
  .umap-range::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px; height: 16px;
    border-radius: 9999px;
    margin-top: -5px;
    border: 2px solid white;
    box-shadow: 0 1px 3px rgba(0,0,0,0.18);
  }
  .umap-range::-moz-range-thumb {
    width: 16px; height: 16px;
    border-radius: 9999px;
    border: 2px solid white;
    box-shadow: 0 1px 3px rgba(0,0,0,0.18);
  }
  :global(.dark) .umap-range::-webkit-slider-thumb { border-color: #1a1a28; box-shadow: 0 1px 4px rgba(0,0,0,0.5); }
  :global(.dark) .umap-range::-moz-range-thumb     { border-color: #1a1a28; box-shadow: 0 1px 4px rgba(0,0,0,0.5); }

  /* Orange thumb */
  .umap-range-orange::-webkit-slider-thumb { background: var(--primary); }
  .umap-range-orange::-moz-range-thumb     { background: var(--primary); }
  :global(.dark) .umap-range-orange::-webkit-slider-thumb { background: var(--primary); }
  :global(.dark) .umap-range-orange::-moz-range-thumb     { background: var(--primary); }

  /* Rose thumb */
  .umap-range-rose::-webkit-slider-thumb { background: var(--primary); }
  .umap-range-rose::-moz-range-thumb     { background: var(--primary); }
  :global(.dark) .umap-range-rose::-webkit-slider-thumb { background: var(--primary); }
  :global(.dark) .umap-range-rose::-moz-range-thumb     { background: var(--primary); }

  /* Focus ring */
  .umap-range:focus { outline: none; }
  .umap-range:focus::-webkit-slider-thumb { box-shadow: 0 0 0 3px color-mix(in oklab, var(--ring) 35%, transparent); }
  .umap-range:focus::-moz-range-thumb     { box-shadow: 0 0 0 3px color-mix(in oklab, var(--ring) 35%, transparent); }
</style>
