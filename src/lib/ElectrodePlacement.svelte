<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Electrode Placement Guide — top-down SVG head diagram with live signal
  quality feedback.  Shows the 4 Muse electrodes (TP9, AF7, AF8, TP10) on
  a stylised head outline with ears, nose, and 10-20 reference grid.

  Props:
    quality  – string[] of length 4 in electrode order [TP9, AF7, AF8, TP10]
    compact  – if true, shrinks for embedding inside onboarding cards
-->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  interface Props {
    quality?: string[];
    compact?: boolean;
  }
  let { quality = ["no_signal","no_signal","no_signal","no_signal"], compact = false }: Props = $props();

  // ── Electrode positions (SVG coords, viewBox 0 0 200 220) ──────────────
  // Top-down head: nose at top, ears at sides.
  const ELECTRODES = [
    { id: "TP9",  label: "TP9",  cx: 38,  cy: 148, side: "left"  },
    { id: "AF7",  label: "AF7",  cx: 62,  cy: 62,  side: "left"  },
    { id: "AF8",  label: "AF8",  cx: 138, cy: 62,  side: "right" },
    { id: "TP10", label: "TP10", cx: 162, cy: 148, side: "right" },
  ] as const;

  const QC_COLOR: Record<string, string> = {
    good:      "#22c55e",
    fair:      "#eab308",
    poor:      "#f97316",
    no_signal: "#94a3b8",
  };

  const qualityOf = (i: number) => quality[i] ?? "no_signal";
  const colorOf   = (i: number) => QC_COLOR[qualityOf(i)] ?? "#94a3b8";
  const isPulse   = (i: number) => qualityOf(i) === "poor" || qualityOf(i) === "no_signal";

  // Reference landmarks (dimmed)
  const REFS = [
    { label: "Cz",  cx: 100, cy: 110 },
    { label: "Fz",  cx: 100, cy: 72  },
    { label: "Pz",  cx: 100, cy: 150 },
    { label: "Fpz", cx: 100, cy: 42  },
  ];
</script>

<div class="electrode-placement flex flex-col items-center gap-2 {compact ? '' : 'py-2'}"
     role="img" aria-label={t("electrode.title")}>

  {#if !compact}
    <h3 class="text-[0.72rem] font-bold tracking-tight">{t("electrode.title")}</h3>
  {/if}

  <svg
    viewBox="0 0 200 220"
    class="{compact ? 'w-[160px] h-[176px]' : 'w-[220px] h-[242px]'}"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <!-- ── Head outline ──────────────────────────────────────────────────── -->
    <ellipse cx="100" cy="115" rx="68" ry="80"
      fill="none" stroke="currentColor" stroke-width="1.5" opacity="0.18" />

    <!-- ── Ears ──────────────────────────────────────────────────────────── -->
    <path d="M 30 100 Q 18 115, 30 132" fill="none" stroke="currentColor"
      stroke-width="1.2" opacity="0.15" />
    <path d="M 170 100 Q 182 115, 170 132" fill="none" stroke="currentColor"
      stroke-width="1.2" opacity="0.15" />

    <!-- ── Nose indicator ────────────────────────────────────────────────── -->
    <path d="M 93 36 L 100 22 L 107 36" fill="none" stroke="currentColor"
      stroke-width="1.2" stroke-linejoin="round" opacity="0.18" />
    <text x="100" y="16" text-anchor="middle" font-size="7" fill="currentColor"
      opacity="0.3" font-weight="600">{t("electrode.front")}</text>

    <!-- ── Back label ────────────────────────────────────────────────────── -->
    <text x="100" y="210" text-anchor="middle" font-size="7" fill="currentColor"
      opacity="0.3" font-weight="600">{t("electrode.back")}</text>

    <!-- ── Midline cross ─────────────────────────────────────────────────── -->
    <line x1="100" y1="38" x2="100" y2="192" stroke="currentColor"
      stroke-width="0.5" opacity="0.07" stroke-dasharray="3,3" />
    <line x1="34" y1="115" x2="166" y2="115" stroke="currentColor"
      stroke-width="0.5" opacity="0.07" stroke-dasharray="3,3" />

    <!-- ── 10-20 reference landmarks ─────────────────────────────────────── -->
    {#each REFS as ref}
      <circle cx={ref.cx} cy={ref.cy} r="2.5" fill="currentColor" opacity="0.08" />
      <text x={ref.cx} y={ref.cy - 5} text-anchor="middle" font-size="5.5"
        fill="currentColor" opacity="0.18" font-weight="500">{ref.label}</text>
    {/each}

    <!-- ── Electrode markers ─────────────────────────────────────────────── -->
    {#each ELECTRODES as el, i}
      <!-- Glow ring (animated for poor/no_signal) -->
      <circle cx={el.cx} cy={el.cy} r="14" fill="{colorOf(i)}10" stroke="none">
        {#if isPulse(i)}
          <animate attributeName="r" values="14;19;14" dur="1.5s" repeatCount="indefinite" />
          <animate attributeName="opacity" values="0.5;0.15;0.5" dur="1.5s" repeatCount="indefinite" />
        {/if}
      </circle>

      <!-- Outer ring -->
      <circle cx={el.cx} cy={el.cy} r="11" fill="{colorOf(i)}18"
        stroke={colorOf(i)} stroke-width="2" />

      <!-- Inner dot -->
      <circle cx={el.cx} cy={el.cy} r="5" fill={colorOf(i)}>
        {#if isPulse(i)}
          <animate attributeName="opacity" values="1;0.4;1" dur="1.2s" repeatCount="indefinite" />
        {/if}
      </circle>

      <!-- Label -->
      {@const labelY = el.cy > 110 ? el.cy + 22 : el.cy - 18}
      <text x={el.cx} y={labelY} text-anchor="middle"
        font-size="8" font-weight="700" fill={colorOf(i)}>
        {el.label}
      </text>

      <!-- Quality text -->
      {@const qLabelY = el.cy > 110 ? el.cy + 31 : el.cy - 10}
      <text x={el.cx} y={qLabelY} text-anchor="middle"
        font-size="5.5" font-weight="500" fill={colorOf(i)} opacity="0.8">
        {t(`electrode.quality.${qualityOf(i)}`)}
      </text>
    {/each}

    <!-- ── Left / Right labels ───────────────────────────────────────────── -->
    <text x="12" y="118" text-anchor="middle" font-size="6" fill="currentColor"
      opacity="0.22" font-weight="600" transform="rotate(-90,12,118)">
      {t("electrode.left")}
    </text>
    <text x="188" y="118" text-anchor="middle" font-size="6" fill="currentColor"
      opacity="0.22" font-weight="600" transform="rotate(90,188,118)">
      {t("electrode.right")}
    </text>
  </svg>

  <!-- ── Legend ─────────────────────────────────────────────────────────── -->
  <div class="flex items-center gap-3 {compact ? 'gap-2' : 'gap-3'}">
    {#each ["good", "fair", "poor", "no_signal"] as q}
      <div class="flex items-center gap-1">
        <div class="w-2 h-2 rounded-full shrink-0" style="background:{QC_COLOR[q]}" aria-hidden="true"></div>
        <span class="text-[{compact ? '0.45' : '0.52'}rem] text-muted-foreground font-medium">
          {t(`electrode.quality.${q}`)}
        </span>
      </div>
    {/each}
  </div>

  <!-- ── Placement tips (only in full mode) ────────────────────────────── -->
  {#if !compact}
    <div class="w-full max-w-[340px] flex flex-col gap-1.5 mt-1">
      <p class="text-[0.5rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("electrode.tipsTitle")}
      </p>
      {#each [1,2,3,4] as n}
        <div class="flex items-start gap-2">
          <span class="w-4 h-4 rounded-full bg-muted dark:bg-white/[0.06] flex items-center justify-center
                       text-[0.5rem] font-bold text-muted-foreground shrink-0 mt-0.5">{n}</span>
          <p class="text-[0.6rem] text-muted-foreground leading-relaxed">{t(`electrode.tip${n}`)}</p>
        </div>
      {/each}
    </div>
  {/if}
</div>
