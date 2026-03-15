<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Electrode guide — 3D interactive head with electrode positions and live signal quality.
-->
<script lang="ts">
  import { Canvas } from "@threlte/core";
  import { WebGLRenderer } from "three";
  import { Badge } from "$lib/components/ui/badge";
  import { t } from "$lib/i18n/index.svelte";
  import { getResolved } from "$lib/theme-store.svelte";
  import {
    type Electrode,
    type ElectrodeSystem,
    type BrainRegion,
    regionColors,
    regionLabels,
    regionDescriptions,
    electrodes as allElectrodes,
    getElectrodes,
  } from "$lib/data/electrodes";

  interface Props {
    /** Optional per-channel quality values [0–1] for TP9, AF7, AF8, TP10. */
    quality?: [number, number, number, number] | null;
    /** Optional per-channel quality labels ["good","fair","poor","no_signal"]. */
    qualityLabels?: string[] | null;
    /** Connected device kind for selecting the default tab. */
    device?: string;
  }

  let { quality = null, qualityLabels = null, device = "muse" }: Props = $props();

  /** Convert string quality labels to numeric values. */
  function labelToNum(label: string): number {
    switch (label) {
      case "good": return 0.9;
      case "fair": return 0.5;
      case "poor": return 0.2;
      default:     return 0;
    }
  }

  function labelToText(label: string): string {
    switch (label) {
      case "good": return "Good";
      case "fair": return "Fair";
      case "poor": return "Poor";
      default:     return "No Signal";
    }
  }

  let effectiveQuality = $derived.by((): [number, number, number, number] | null => {
    if (quality) return quality;
    if (qualityLabels && qualityLabels.length >= 4) {
      return [labelToNum(qualityLabels[0]), labelToNum(qualityLabels[1]),
              labelToNum(qualityLabels[2]), labelToNum(qualityLabels[3])];
    }
    return null;
  });

  let effectiveLabels = $derived.by((): string[] => {
    if (qualityLabels && qualityLabels.length >= 4) return qualityLabels;
    if (quality) return quality.map(q => q >= 0.7 ? "good" : q >= 0.4 ? "fair" : q > 0 ? "poor" : "no_signal");
    return ["no_signal", "no_signal", "no_signal", "no_signal"];
  });

  // ── Electrode system tabs ──────────────────────────────────────────────────
  type ActiveTab = "muse" | "mw75" | "hermes" | "ganglion" | "10-20" | "10-10" | "10-5";
  const TABS: { id: ActiveTab; label: string; count: string }[] = [
    { id: "muse",     label: "Muse",     count: "4"  },
    { id: "mw75",     label: "MW75",     count: "12" },
    { id: "hermes",   label: "Hermes",   count: "8"  },
    { id: "ganglion", label: "Ganglion", count: "4"  },
    { id: "10-20",    label: "10-20",    count: "21" },
    { id: "10-10",    label: "10-10",    count: "64" },
    { id: "10-5",     label: "10-5",     count: "345" },
  ];
  function defaultTab(d: string): ActiveTab {
    return d === "mw75" ? "mw75" : d === "hermes" ? "hermes"
      : d === "ganglion" ? "ganglion" : "muse";
  }
  let activeTab: ActiveTab = $state("muse" as ActiveTab);

  // Sync activeTab when device prop changes (also sets initial value)
  $effect(() => { activeTab = defaultTab(device); });

  // Device-specific electrode sets
  const museElectrodes = allElectrodes.filter(e => e.muse);
  const MW75_LABELS = ["FT7","T7","TP7","CP5","P7","C5","FT8","T8","TP8","CP6","P8","C6"];
  const mw75Electrodes = allElectrodes.filter(e => MW75_LABELS.includes(e.name));

  // System used for the 3D view (device tabs still need a valid system for raycasting)
  let system: ElectrodeSystem = $derived(
    (activeTab === "muse" || activeTab === "mw75" || activeTab === "hermes" || activeTab === "ganglion")
      ? "10-10" : activeTab as ElectrodeSystem
  );

  // Electrodes shown in the 3D view
  const electrodes3D = $derived(
    activeTab === "muse" ? museElectrodes
    : activeTab === "mw75" ? mw75Electrodes
    : activeTab === "hermes" ? [] // Hermes positions depend on montage
    : activeTab === "ganglion" ? [] // Ganglion has configurable positions
    : getElectrodes(activeTab as ElectrodeSystem)
  );

  let selectedElectrode: Electrode | null = $state(null);
  let Head3D: any = $state(null);
  let head3DRef: any = $state(null);
  let isDragging = $state(false);
  let pointerDownPos = $state({ x: 0, y: 0 });

  function createRenderer(canvas: HTMLCanvasElement) {
    return new WebGLRenderer({ canvas, antialias: true, alpha: true });
  }

  if (typeof window !== "undefined") {
    import("$lib/ElectrodeHead3D.svelte").then(m => { Head3D = m.default; });
  }

  let visible = $derived(electrodes3D);

  function onSelect(el: Electrode | null) { selectedElectrode = el; }

  function onCanvasPointerDown(e: PointerEvent) {
    isDragging = false;
    pointerDownPos = { x: e.clientX, y: e.clientY };
  }
  function onCanvasPointerMove(e: PointerEvent) {
    const dx = e.clientX - pointerDownPos.x;
    const dy = e.clientY - pointerDownPos.y;
    if (Math.abs(dx) > 4 || Math.abs(dy) > 4) isDragging = true;
  }
  function onCanvasClick(e: MouseEvent) {
    if (isDragging) return;
    if (!head3DRef?.hitTest) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const ndcX = ((e.clientX - rect.left) / rect.width) * 2 - 1;
    const ndcY = -(((e.clientY - rect.top) / rect.height) * 2 - 1);
    selectedElectrode = head3DRef.hitTest(ndcX, ndcY);
  }

  const museChannels = ["TP9", "AF7", "AF8", "TP10"];
  const musePositionLabels = ["Left ear", "Left forehead", "Right forehead", "Right ear"];

  function qualityColor(val: number): string {
    if (val >= 0.7) return "#22c55e";
    if (val >= 0.4) return "#f59e0b";
    if (val > 0) return "#ef4444";
    return "#94a3b8";
  }

  function qualityBg(val: number): string {
    if (val >= 0.7) return "bg-green-500/10 border-green-500/20";
    if (val >= 0.4) return "bg-amber-500/10 border-amber-500/20";
    if (val > 0) return "bg-red-500/10 border-red-500/20";
    return "bg-muted/30 border-border dark:border-white/[0.06]";
  }
</script>

<div class="flex flex-col items-center gap-3">

  <!-- ── System tabs ─────────────────────────────────────────────────────── -->
  <div class="flex items-center gap-1 self-start w-full max-w-[480px]">
    {#each TABS as tab}
      <button
        onclick={() => { activeTab = tab.id; selectedElectrode = null; }}
        class="flex items-center gap-1 rounded-md px-2.5 py-1 text-[0.62rem] font-semibold
               transition-all border
               {activeTab === tab.id
                 ? 'bg-foreground text-background border-transparent'
                 : 'text-muted-foreground border-border dark:border-white/[0.07] hover:text-foreground hover:border-foreground/30'}"
      >
        {tab.label}
        <span class="text-[0.5rem] opacity-60 tabular-nums">{tab.count}</span>
      </button>
    {/each}
    {#if activeTab !== "muse"}
      <span class="ml-auto text-[0.52rem] text-muted-foreground/50">
        {getElectrodes(activeTab as ElectrodeSystem).length} electrodes
      </span>
    {/if}
  </div>

  <!-- Signal quality cards — shown only for Muse tab -->
  {#if activeTab === "muse"}
  <div class="grid grid-cols-4 gap-2 w-full max-w-[480px]">
    {#each museChannels as name, idx}
      {@const q = effectiveQuality ? effectiveQuality[idx] : 0}
      {@const label = effectiveLabels[idx]}
      {@const el = visible.find(e => e.name === name)}
      <button
        class="flex flex-col items-center gap-0.5 p-2 rounded-lg border transition-all cursor-pointer
               {qualityBg(q)}
               hover:ring-1 hover:ring-indigo-500/30"
        onclick={() => { if (el) selectedElectrode = el; }}
      >
        <!-- Quality indicator dot -->
        <div class="relative">
          <span class="w-3 h-3 rounded-full block" style="background:{qualityColor(q)}">
          </span>
          {#if q === 0 || label === "no_signal"}
            <span class="absolute inset-0 w-3 h-3 rounded-full animate-ping" style="background:{qualityColor(q)}; opacity:0.3"></span>
          {/if}
        </div>
        <!-- Channel name -->
        <span class="text-[0.7rem] font-bold font-mono" style="color:{qualityColor(q)}">{name}</span>
        <!-- Quality label -->
        <span class="text-[0.45rem] font-semibold uppercase tracking-wider" style="color:{qualityColor(q)}; opacity:0.8">
          {labelToText(label)}
        </span>
        <!-- Position -->
        <span class="text-[0.4rem] text-muted-foreground/50">{musePositionLabels[idx]}</span>
      </button>
    {/each}
  </div>
  {:else}
  <!-- Compact quality strip for non-Muse tabs -->
  <div class="flex items-center gap-2 w-full max-w-[480px] rounded-lg border border-border dark:border-white/[0.07]
              bg-muted/20 px-3 py-1.5">
    <span class="text-[0.52rem] font-semibold text-muted-foreground/60 uppercase tracking-wider shrink-0">Muse signal</span>
    {#each museChannels as name, idx}
      {@const q = effectiveQuality ? effectiveQuality[idx] : 0}
      {@const label = effectiveLabels[idx]}
      <div class="flex items-center gap-1">
        <span class="w-2 h-2 rounded-full shrink-0" style="background:{qualityColor(q)}"></span>
        <span class="text-[0.55rem] font-mono font-bold" style="color:{qualityColor(q)}">{name}</span>
      </div>
    {/each}
  </div>
  {/if}

  <!-- 3D viewer — centered -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="relative rounded-xl border border-border dark:border-white/[0.06]
           bg-white dark:bg-[#14141e] overflow-hidden cursor-grab active:cursor-grabbing
           w-full max-w-[480px]"
    style="aspect-ratio: 4 / 3;"
    onpointerdown={onCanvasPointerDown}
    onpointermove={onCanvasPointerMove}
    onclick={onCanvasClick}
  >
    {#if Head3D}
      <Canvas {createRenderer}>
        <Head3D bind:this={head3DRef} {system}
                electrodesOverride={activeTab === "muse" ? museElectrodes : activeTab === "mw75" ? mw75Electrodes : activeTab === "hermes" ? [] : null}
                {onSelect} selectedName={selectedElectrode?.name} />
      </Canvas>
    {:else}
      <div class="absolute inset-0 flex items-center justify-center text-muted-foreground text-[0.7rem]">
        Loading 3D view…
      </div>
    {/if}

    <!-- Region legend -->
    <div class="absolute bottom-2 left-2 right-2 flex flex-wrap justify-center gap-x-2 gap-y-0.5
                text-[0.45rem] text-muted-foreground/50 bg-background/70 backdrop-blur-sm
                rounded-md px-2 py-1 border border-border/40 pointer-events-none">
      {#each Object.entries(regionColors) as [region, color]}
        {#if region !== "reference"}
          <span class="flex items-center gap-0.5">
            <span class="w-1.5 h-1.5 rounded-full" style="background:{color}"></span>
            {regionLabels[region as BrainRegion]}
          </span>
        {/if}
      {/each}
    </div>

    <!-- Hint -->
    <div class="absolute top-2 left-2 text-[0.45rem] text-muted-foreground/40
                bg-background/60 backdrop-blur-sm rounded-md px-1.5 py-0.5 border border-border/30 pointer-events-none">
      Drag to rotate · Click electrode
    </div>
  </div>

  <!-- Electrode detail (when selected) -->
  {#if selectedElectrode}
    {@const el = selectedElectrode}
    <div class="w-full max-w-[480px] rounded-lg border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] p-3">
      <div class="flex items-center gap-1.5 mb-2 flex-wrap">
        <span class="w-2.5 h-2.5 rounded-full shrink-0" style="background:{regionColors[el.region]}"></span>
        <span class="text-[0.85rem] font-bold font-mono">{el.name}</span>
        {#if el.muse}
          <Badge variant="outline" class="text-[0.44rem] px-1 py-0 rounded-full bg-indigo-500/10 text-indigo-500 border-indigo-500/20">Muse</Badge>
        {/if}
        <Badge variant="outline" class="text-[0.44rem] px-1 py-0 rounded-full">{regionLabels[el.region]}</Badge>
        {#if el.muse && effectiveQuality}
          {@const chIdx = museChannels.indexOf(el.name)}
          {#if chIdx >= 0}
            {@const q = effectiveQuality[chIdx]}
            <span class="text-[0.55rem] font-bold tabular-nums ml-1" style="color:{qualityColor(q)}">
              Signal: {labelToText(effectiveLabels[chIdx])}
            </span>
          {/if}
        {/if}
        <button
          class="ml-auto text-[0.52rem] text-muted-foreground hover:text-foreground cursor-pointer"
          onclick={() => selectedElectrode = null}
        >✕</button>
      </div>
      <div class="space-y-1 text-[0.62rem] leading-relaxed">
        <div>
          <span class="text-muted-foreground/50 font-semibold text-[0.48rem] uppercase tracking-wider">Position</span>
          <p class="text-foreground/80">{el.lobe} — {el.function}</p>
        </div>
        <div>
          <span class="text-muted-foreground/50 font-semibold text-[0.48rem] uppercase tracking-wider">Signals</span>
          <p class="text-foreground/80">{el.signals}</p>
        </div>
        {#if el.museRole}
          <div>
            <span class="text-muted-foreground/50 font-semibold text-[0.48rem] uppercase tracking-wider">Muse Role</span>
            <p class="text-indigo-400 text-[0.58rem]">{el.museRole}</p>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
