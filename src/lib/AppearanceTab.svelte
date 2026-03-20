<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Appearance tab — Font Size · Theme · Chart Color Scheme -->
<script lang="ts">
  import { Card, CardContent } from "$lib/components/ui/card";
  import { t }                 from "$lib/i18n/index.svelte";
  import { getFontSize, setFontSize, FONT_SIZE_PRESETS } from "$lib/stores/font-size.svelte";
  import {
    getTheme, setTheme, getHighContrast, toggleHighContrast,
    getAccentId, setAccent, ACCENT_PRESETS,
  } from "$lib/stores/theme.svelte";
  import type { ThemeMode } from "$lib/stores/theme.svelte";
  import { getChartScheme, setChartScheme, CHART_SCHEMES, type ChartScheme } from "$lib/stores/chart-colors.svelte";
  import { EEG_CH } from "$lib/constants";

  const THEME_OPTIONS: { value: ThemeMode; icon: string; labelKey: string }[] = [
    { value: "system", icon: "💻", labelKey: "appearance.themeSystem" },
    { value: "light",  icon: "☀️", labelKey: "appearance.themeLight" },
    { value: "dark",   icon: "🌙", labelKey: "appearance.themeDark" },
  ];
</script>

<div class="flex flex-col gap-5">

<!-- ── Font Size ──────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("settings.fontSize")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 px-4 py-3.5">
      <p class="text-[0.68rem] text-muted-foreground leading-relaxed">
        {t("settings.fontSizeDesc")}
      </p>
      <div class="flex items-center gap-1.5 flex-wrap">
        {#each FONT_SIZE_PRESETS as preset}
          <button
            onclick={() => setFontSize(preset.value)}
            class="rounded-lg border px-3 py-1.5 text-[0.68rem] font-semibold
                   transition-all cursor-pointer select-none
                   {getFontSize() === preset.value
                     ? 'border-primary/50 bg-primary/10 text-primary'
                     : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
            {preset.label} · {preset.value}%
          </button>
        {/each}
      </div>
    </CardContent>
  </Card>
</section>

<!-- ── Theme ──────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("appearance.theme")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- Theme mode -->
      <div class="flex flex-col gap-2.5 px-4 py-3.5">
        <span class="text-[0.78rem] font-semibold text-foreground">{t("appearance.colorMode")}</span>
        <div class="flex gap-2">
          {#each THEME_OPTIONS as opt}
            <button
              onclick={() => setTheme(opt.value)}
              class="flex flex-col items-center gap-1 rounded-xl border px-3 py-2.5 flex-1
                     transition-all cursor-pointer select-none
                     {getTheme() === opt.value
                       ? 'border-primary/50 bg-primary/10'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              <span class="text-[1rem]">{opt.icon}</span>
              <span class="text-[0.7rem] font-semibold leading-tight
                           {getTheme() === opt.value ? 'text-primary' : 'text-foreground'}">
                {t(opt.labelKey)}
              </span>
              {#if getTheme() === opt.value}
                <span class="text-[0.52rem] font-bold tracking-widest uppercase text-primary mt-0.5">{t("common.active")}</span>
              {/if}
            </button>
          {/each}
        </div>
      </div>

      <!-- High contrast toggle -->
      <div class="flex items-center gap-3 px-4 py-3.5">
        <button
          onclick={() => toggleHighContrast()}
          class="flex items-center gap-3 text-left transition-colors w-full">
          <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                      {getHighContrast() ? 'bg-emerald-500' : 'bg-muted dark:bg-white/[0.08]'}">
            <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                        {getHighContrast() ? 'translate-x-4' : 'translate-x-0.5'}"></div>
          </div>
          <div class="flex flex-col gap-0.5 min-w-0">
            <span class="text-[0.72rem] font-semibold text-foreground leading-tight">{t("appearance.highContrast")}</span>
            <span class="text-[0.58rem] text-muted-foreground leading-tight">{t("appearance.highContrastDesc")}</span>
          </div>
        </button>
      </div>

    </CardContent>
  </Card>
</section>

<!-- ── Accent colour ──────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("appearance.accentColor")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="px-4 py-4 flex flex-col gap-3">
      <p class="text-[0.68rem] text-muted-foreground leading-relaxed">
        {t("appearance.accentColorDesc")}
      </p>

      <!-- Swatch grid -->
      <div class="flex flex-wrap gap-2.5">
        {#each ACCENT_PRESETS as preset}
          {@const active = getAccentId() === preset.id}
          <button
            onclick={() => setAccent(preset.id)}
            title={preset.label}
            class="group relative flex flex-col items-center gap-1.5
                   cursor-pointer select-none focus:outline-none">

            <!-- Colour circle -->
            <span class="relative flex items-center justify-center
                          w-8 h-8 rounded-full transition-transform duration-150
                          group-hover:scale-110
                          {active ? 'ring-2 ring-offset-2 ring-offset-background' : ''}"
                  style="background-color: {preset.swatch};
                         {active ? `ring-color: ${preset.swatch};` : ''}">
              {#if active}
                <!-- Checkmark -->
                <svg viewBox="0 0 12 12" fill="none" stroke="white" stroke-width="2.2"
                     stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                  <polyline points="1.5 6.5 4.5 9.5 10.5 2.5"/>
                </svg>
              {/if}
            </span>

            <!-- Label -->
            <span class="text-[0.55rem] font-medium leading-none
                         {active ? 'text-foreground' : 'text-muted-foreground/60'}">
              {preset.label}
            </span>
          </button>
        {/each}
      </div>
    </CardContent>
  </Card>
</section>

<!-- ── Chart Color Scheme ─────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("appearance.chartColors")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 px-4 py-3.5">
      <p class="text-[0.68rem] text-muted-foreground leading-relaxed">
        {t("appearance.chartColorsDesc")}
      </p>

      <div class="flex flex-col gap-2">
        {#each CHART_SCHEMES as scheme (scheme.id)}
          <button
            onclick={() => setChartScheme(scheme.id)}
            class="flex items-center gap-3 rounded-xl border px-3 py-3
                   transition-all cursor-pointer select-none
                   {getChartScheme() === scheme.id
                     ? 'border-primary/50 bg-primary/10'
                     : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">

            <!-- Color swatches -->
            <div class="flex items-center gap-1 shrink-0">
              {#each scheme.channels as color, i}
                <div class="flex flex-col items-center gap-0.5">
                  <div class="w-5 h-5 rounded-full border border-black/10 dark:border-white/10"
                       style="background-color: {color}"></div>
                  <span class="text-[0.42rem] text-muted-foreground/60 font-mono leading-none">{EEG_CH[i]}</span>
                </div>
              {/each}

              <!-- Band color dots -->
              <div class="flex items-center gap-0.5 ml-1.5 pl-1.5 border-l border-border dark:border-white/10">
                {#each [scheme.delta, scheme.theta, scheme.alpha, scheme.beta, scheme.gamma] as color, i}
                  <div class="flex flex-col items-center gap-0.5">
                    <div class="w-3 h-3 rounded-full border border-black/10 dark:border-white/10"
                         style="background-color: {color}"></div>
                    <span class="text-[0.38rem] text-muted-foreground/50 font-mono leading-none">
                      {["δ","θ","α","β","γ"][i]}
                    </span>
                  </div>
                {/each}
              </div>
            </div>

            <!-- Label and description -->
            <div class="flex flex-col gap-0.5 min-w-0 flex-1">
              <span class="text-[0.72rem] font-semibold leading-tight
                           {getChartScheme() === scheme.id ? 'text-primary' : 'text-foreground'}">
                {t(scheme.labelKey)}
              </span>
              <span class="text-[0.58rem] text-muted-foreground leading-tight">
                {t(scheme.descKey)}
              </span>
            </div>

            <!-- Active indicator -->
            {#if getChartScheme() === scheme.id}
              <span class="text-[0.52rem] font-bold tracking-widest uppercase text-primary shrink-0">
                {t("common.active")}
              </span>
            {/if}
          </button>
        {/each}
      </div>
    </CardContent>
  </Card>
</section>

</div>
