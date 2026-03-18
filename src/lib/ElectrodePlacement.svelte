<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Electrode Placement Guide — top-down SVG head diagram with live signal
  quality feedback.  Supports multiple device presets: Muse (4 electrodes),
  Ganglion (4 electrodes), and MW75 Neuro (12 electrodes around ears).

  Props:
    quality  – string[] of quality labels in electrode order
    compact  – if true, shrinks for embedding inside onboarding cards
    device   – "muse" | "ganglion" | "mw75" | "hermes" | "emotiv" | "idun" | "unknown"
-->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  interface ElectrodePos {
    id: string;
    label: string;
    cx: number;
    cy: number;
    side: "left" | "right" | "center";
  }

  interface Props {
    quality?: string[];
    compact?: boolean;
    device?: string;
  }
  let { quality = [], compact = false, device = "muse" }: Props = $props();

  // ── Device presets (SVG coords, viewBox 0 0 200 220) ───────────────────
  const MUSE_ELECTRODES: ElectrodePos[] = [
    { id: "TP9",  label: "TP9",  cx: 38,  cy: 148, side: "left"  },
    { id: "AF7",  label: "AF7",  cx: 62,  cy: 62,  side: "left"  },
    { id: "AF8",  label: "AF8",  cx: 138, cy: 62,  side: "right" },
    { id: "TP10", label: "TP10", cx: 162, cy: 148, side: "right" },
  ];

  const GANGLION_ELECTRODES: ElectrodePos[] = [
    { id: "Ch1", label: "Ch1", cx: 62,  cy: 80,  side: "left"   },
    { id: "Ch2", label: "Ch2", cx: 138, cy: 80,  side: "right"  },
    { id: "Ch3", label: "Ch3", cx: 62,  cy: 140, side: "left"   },
    { id: "Ch4", label: "Ch4", cx: 138, cy: 140, side: "right"  },
  ];

  // MW75: 6 electrodes equidistantly around each ear cup.
  // Left ear (cx ≈ 30–38): FT7 (front-top), T7 (front), TP7 (front-bottom),
  //   CP5 (back-bottom), P7 (back), C5 (back-top)
  // Right ear (cx ≈ 162–170): FT8, T8, TP8, CP6, P8, C6
  const MW75_ELECTRODES: ElectrodePos[] = [
    // Left ear cup — clockwise from front-top
    { id: "FT7", label: "FT7", cx: 42,  cy: 92,  side: "left"  },
    { id: "T7",  label: "T7",  cx: 30,  cy: 112, side: "left"  },
    { id: "TP7", label: "TP7", cx: 32,  cy: 136, side: "left"  },
    { id: "CP5", label: "CP5", cx: 48,  cy: 154, side: "left"  },
    { id: "P7",  label: "P7",  cx: 58,  cy: 138, side: "left"  },
    { id: "C5",  label: "C5",  cx: 54,  cy: 110, side: "left"  },
    // Right ear cup — clockwise from front-top
    { id: "FT8", label: "FT8", cx: 158, cy: 92,  side: "right" },
    { id: "T8",  label: "T8",  cx: 170, cy: 112, side: "right" },
    { id: "TP8", label: "TP8", cx: 168, cy: 136, side: "right" },
    { id: "CP6", label: "CP6", cx: 152, cy: 154, side: "right" },
    { id: "P8",  label: "P8",  cx: 142, cy: 138, side: "right" },
    { id: "C6",  label: "C6",  cx: 146, cy: 110, side: "right" },
  ];

  // Hermes V1: 8 channels, positions depend on user's montage.
  // Default placement assumes a standard research headband layout.
  const HERMES_ELECTRODES: ElectrodePos[] = [
    { id: "Fp1", label: "Fp1", cx: 62,  cy: 62,  side: "left"   },
    { id: "Fp2", label: "Fp2", cx: 138, cy: 62,  side: "right"  },
    { id: "AF3", label: "AF3", cx: 50,  cy: 90,  side: "left"   },
    { id: "AF4", label: "AF4", cx: 150, cy: 90,  side: "right"  },
    { id: "F3",  label: "F3",  cx: 50,  cy: 120, side: "left"   },
    { id: "F4",  label: "F4",  cx: 150, cy: 120, side: "right"  },
    { id: "FC1", label: "FC1", cx: 62,  cy: 148, side: "left"   },
    { id: "FC2", label: "FC2", cx: 138, cy: 148, side: "right"  },
  ];

  // Emotiv EPOC X / EPOC+: first 12 of 14 electrodes (pipeline-capped at EEG_CHANNELS).
  const EMOTIV_ELECTRODES: ElectrodePos[] = [
    { id: "AF3", label: "AF3", cx: 68,  cy: 58,  side: "left"   },
    { id: "F7",  label: "F7",  cx: 36,  cy: 80,  side: "left"   },
    { id: "F3",  label: "F3",  cx: 62,  cy: 86,  side: "left"   },
    { id: "FC5", label: "FC5", cx: 38,  cy: 108, side: "left"   },
    { id: "T7",  label: "T7",  cx: 24,  cy: 130, side: "left"   },
    { id: "P7",  label: "P7",  cx: 44,  cy: 162, side: "left"   },
    { id: "O1",  label: "O1",  cx: 76,  cy: 190, side: "left"   },
    { id: "O2",  label: "O2",  cx: 124, cy: 190, side: "right"  },
    { id: "P8",  label: "P8",  cx: 156, cy: 162, side: "right"  },
    { id: "T8",  label: "T8",  cx: 176, cy: 130, side: "right"  },
    { id: "FC6", label: "FC6", cx: 162, cy: 108, side: "right"  },
    { id: "F4",  label: "F4",  cx: 138, cy: 86,  side: "right"  },
  ];

  // IDUN Guardian: single in-ear bipolar channel.
  const IDUN_ELECTRODES: ElectrodePos[] = [
    { id: "EEG", label: "EEG", cx: 100, cy: 130, side: "center" },
  ];

  const ELECTRODES = $derived(
    device === "mw75" ? MW75_ELECTRODES
    : device === "hermes" ? HERMES_ELECTRODES
    : device === "emotiv" ? EMOTIV_ELECTRODES
    : device === "idun" ? IDUN_ELECTRODES
    : device === "ganglion" ? GANGLION_ELECTRODES
    : MUSE_ELECTRODES
  );

  // Ensure quality array matches electrode count
  const safeQuality = $derived(
    Array.from({ length: ELECTRODES.length }, (_, i) => quality[i] ?? "no_signal")
  );

  const QC_COLOR: Record<string, string> = {
    good:      "#22c55e",
    fair:      "#eab308",
    poor:      "#f97316",
    no_signal: "#94a3b8",
  };

  const qualityOf = (i: number) => safeQuality[i] ?? "no_signal";
  const colorOf   = (i: number) => QC_COLOR[qualityOf(i)] ?? "#94a3b8";
  const isPulse   = (i: number) => qualityOf(i) === "poor" || qualityOf(i) === "no_signal";

  // Reference landmarks (dimmed)
  const REFS = [
    { label: "Cz",  cx: 100, cy: 110 },
    { label: "Fz",  cx: 100, cy: 72  },
    { label: "Pz",  cx: 100, cy: 150 },
    { label: "Fpz", cx: 100, cy: 42  },
  ];

  // MW75: show ear cup outlines instead of individual reference markers
  const isMw75 = $derived(device === "mw75");

  const deviceLabel = $derived(
    device === "mw75" ? "MW75 Neuro" : device === "hermes" ? "Hermes V1"
    : device === "emotiv" ? "Emotiv EPOC" : device === "idun" ? "IDUN Guardian"
    : device === "ganglion" ? "Ganglion" : "Muse"
  );
</script>

<div class="electrode-placement flex flex-col items-center gap-2 {compact ? '' : 'py-2'}"
     role="img" aria-label={t("electrode.title")}>

  {#if !compact}
    <h3 class="text-[0.72rem] font-bold tracking-tight">{t("electrode.title")}</h3>
    <span class="text-[0.58rem] text-muted-foreground">{deviceLabel} — {ELECTRODES.length} electrodes</span>
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

    {#if isMw75}
      <!-- MW75: ear cup outlines -->
      <ellipse cx="44" cy="122" rx="22" ry="38" fill="none" stroke="currentColor"
        stroke-width="0.8" stroke-dasharray="3,3" opacity="0.12" />
      <ellipse cx="156" cy="122" rx="22" ry="38" fill="none" stroke="currentColor"
        stroke-width="0.8" stroke-dasharray="3,3" opacity="0.12" />
    {/if}

    <!-- ── Nose indicator ────────────────────────────────────────────────── -->
    <path d="M 93 36 L 100 22 L 107 36" fill="none" stroke="currentColor"
      stroke-width="1.2" stroke-linejoin="round" opacity="0.18" />
    <text x="100" y="16" text-anchor="middle" font-size="7" fill="currentColor"
      opacity="0.35" font-weight="600">Front</text>

    <!-- ── Reference landmarks ───────────────────────────────────────────── -->
    {#each REFS as r}
      <circle cx={r.cx} cy={r.cy} r="2.5" fill="currentColor" opacity="0.08" />
      <text x={r.cx} y={r.cy - 5} text-anchor="middle" font-size="6"
        fill="currentColor" opacity="0.2" font-weight="500">{r.label}</text>
    {/each}

    <!-- ── Electrodes ────────────────────────────────────────────────────── -->
    {#each ELECTRODES as el, i}
      <circle cx={el.cx} cy={el.cy} r={isMw75 ? 5 : 7} fill={colorOf(i)} opacity="0.85">
        {#if isPulse(i)}
          <animate attributeName="opacity" values="0.85;0.35;0.85"
            dur="1.6s" repeatCount="indefinite" />
        {/if}
      </circle>
      <!-- Label — positioned to avoid overlap for MW75's denser layout -->
      <text
        x={el.cx + (el.side === "left" ? (isMw75 ? -9 : -12) : (isMw75 ? 9 : 12))}
        y={el.cy + 3}
        text-anchor={el.side === "left" ? "end" : "start"}
        font-size={isMw75 ? "5.5" : "7"}
        fill={colorOf(i)}
        font-weight="700"
        opacity="0.9"
      >{el.label}</text>
    {/each}
  </svg>

  {#if !compact}
    <div class="grid {isMw75 ? 'grid-cols-3' : 'grid-cols-2'} gap-x-3 gap-y-1 text-[0.6rem]">
      {#each ELECTRODES as el, i}
        <div class="flex items-center gap-1.5">
          <span class="inline-block w-2 h-2 rounded-full" style="background:{colorOf(i)}"></span>
          <span class="font-mono font-semibold text-muted-foreground">{el.label}</span>
          <span class="text-muted-foreground/60 capitalize">{qualityOf(i).replace("_"," ")}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>
