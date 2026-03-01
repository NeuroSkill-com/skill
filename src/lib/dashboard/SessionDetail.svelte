<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Reusable session metrics + charts panel.
  Shows collapsible summary sections and time-series charts.
  Used in History, Compare, and Search views.

  Props:
    metrics:    SessionMetrics | null  (aggregated averages)
    timeseries: EpochRow[]     | null  (per-row data for charts)
    loading:    boolean               (show spinner)
    compact:    boolean               (hide charts, show only summary numbers)
-->
<script lang="ts" module>
  export interface SessionMetrics {
    n_epochs: number;
    rel_delta: number; rel_theta: number; rel_alpha: number; rel_beta: number; rel_gamma: number;
    relaxation: number; engagement: number; faa: number;
    tar: number; bar: number; dtr: number; tbr: number;
    pse: number; apf: number; sef95: number; spectral_centroid: number; bps: number; snr: number;
    coherence: number; mu_suppression: number; mood: number;
    hjorth_activity: number; hjorth_mobility: number; hjorth_complexity: number;
    permutation_entropy: number; higuchi_fd: number; dfa_exponent: number;
    sample_entropy: number; pac_theta_gamma: number; laterality_index: number;
    hr: number; rmssd: number; sdnn: number; pnn50: number; lf_hf_ratio: number;
    respiratory_rate: number; spo2_estimate: number; perfusion_index: number; stress_index: number;
    meditation: number; cognitive_load: number; drowsiness: number;
    blink_count: number; blink_rate: number;
    head_pitch: number; head_roll: number; stillness: number; nod_count: number; shake_count: number;
  }

  export interface EpochRow {
    t: number;
    rd: number; rt: number; ra: number; rb: number; rg: number;
    focus: number; relaxation: number; engagement: number; faa: number; // focus kept for compat
    tar: number; bar: number; dtr: number; tbr: number;
    pse: number; apf: number; sef95: number; sc: number; bps: number; snr: number;
    coherence: number; mu: number; mood: number;
    ha: number; hm: number; hc: number;
    pe: number; hfd: number; dfa: number; se: number; pac: number; lat: number;
    hr: number; rmssd: number; sdnn: number; pnn50: number; lf_hf: number;
    resp: number; spo2: number; perf: number; stress: number;
    blinks: number; blink_r: number;
    pitch: number; roll: number; still: number; nods: number; shakes: number;
    med: number; cog: number; drow: number;
    gpu: number; gpu_render: number; gpu_tiler: number;
  }

  export interface CsvMetricsResult {
    n_rows: number;
    summary: SessionMetrics;
    timeseries: EpochRow[];
  }
</script>

<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { TimeSeriesChart, MetricTooltip } from "$lib/dashboard";
  import { Spinner } from "$lib/components/ui/spinner";
  import {
    C_DELTA, C_THETA, C_ALPHA, C_BETA, C_GAMMA,
    C_RELAX, C_ENGAGE, C_MED, C_COG, C_DROW, C_MOOD,
    C_HR, C_HRV_G, C_HRV_B, C_HRV_A, C_BLINK,
    C_PITCH, C_ROLL, C_STILL, C_STRESS,
  } from "$lib/constants";

  interface Props {
    metrics?:    SessionMetrics | null;
    timeseries?: EpochRow[] | null;
    loading?:    boolean;
    compact?:    boolean;
  }

  let { metrics = null, timeseries = null, loading = false, compact = false }: Props = $props();

  // Section collapse state (local to each instance)
  let sectionOpen = $state<Record<string, boolean>>({});

  const ALL_SECTIONS = [
    "bands", "ratios", "complexity", "ppg", "events",
    "c_bands", "c_scores", "c_ratios", "c_faa", "c_spectral", "c_quality",
    "c_complexity", "c_hjorth", "c_mood", "c_composite",
    "c_hr", "c_hrv", "c_ppg", "c_artifacts", "c_pose", "c_gpu",
  ] as const;

  function isOpen(id: string) { return !!sectionOpen[id]; }
  function toggle(id: string) { sectionOpen[id] = !sectionOpen[id]; }
  function toggleAll() {
    const anyOpen = ALL_SECTIONS.some(s => sectionOpen[s]);
    for (const s of ALL_SECTIONS) sectionOpen[s] = !anyOpen;
  }

  let ts = $derived(timeseries && timeseries.length > 2 ? timeseries : null);
  let times = $derived(ts ? ts.map(r => r.t) : []);
</script>

{#if loading}
  <div class="flex items-center gap-2 py-2">
    <Spinner size="w-3.5 h-3.5" class="text-muted-foreground/50" />
    <span class="text-[0.6rem] text-muted-foreground/50">{t("history.metrics")}…</span>
  </div>

{:else if metrics && metrics.n_epochs > 0}
  {@const m = metrics}
  {@const hasPpg = m.hr > 0}
  {@const hasEvents = m.blink_count > 0 || m.stillness > 0}
  {@const hasComposite = ts ? ts.some(r => r.med > 0) : false}
  {@const hasArtifacts = ts ? ts.some(r => r.blink_r > 0) : false}
  {@const hasPose = ts ? ts.some(r => r.still > 0 || r.pitch !== 0 || r.roll !== 0) : false}
  {@const hasGpu = ts ? ts.some(r => r.gpu > 0) : false}

  <div class="flex flex-col gap-0">
    <!-- Title + Toggle All -->
    <div class="flex items-center gap-2 mb-1">
      <span class="text-[0.5rem] font-semibold tracking-widest uppercase text-muted-foreground/60">
        {t("history.metrics")} ({m.n_epochs})
      </span>
      {#if !compact}
        <button class="text-[0.48rem] text-muted-foreground/40 hover:text-muted-foreground
                       transition-colors px-1.5 py-0.5 rounded border border-transparent
                       hover:border-border dark:hover:border-white/[0.08]"
                onclick={(e: MouseEvent) => { e.stopPropagation(); toggleAll(); }}>
          {ALL_SECTIONS.some(s => sectionOpen[s]) ? t("sd.collapseAll") : t("sd.expandAll")}
        </button>
      {/if}
    </div>

    <!-- Core Scores (always visible) -->
    <div class="grid grid-cols-3 sm:grid-cols-6 gap-x-3 gap-y-1.5 mb-1.5">
      {#each [
        { l: t("sd.relax"),      v: m.relaxation.toFixed(0), c: C_RELAX, tip: t("tip.relaxation") },
        { l: t("sd.engage"),     v: m.engagement.toFixed(0), c: C_ENGAGE, tip: t("tip.engagement") },
        { l: t("sd.meditation"), v: m.meditation.toFixed(0), c: C_MED, tip: t("tip.meditation") },
        { l: t("sd.cogLoad"),    v: m.cognitive_load.toFixed(0), c: C_COG, tip: t("tip.cognitiveLoad") },
        { l: t("sd.drowsiness"), v: m.drowsiness.toFixed(0), c: C_DROW, tip: t("tip.drowsiness") },
        { l: t("sd.mood"),       v: m.mood.toFixed(0),       c: C_MOOD, tip: t("tip.mood") },
      ] as item}
        <MetricTooltip text={item.tip}>
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.42rem] text-muted-foreground/60 uppercase tracking-wider">{item.l}</span>
            <div class="flex items-end gap-0.5">
              <span class="text-[0.72rem] font-bold tabular-nums" style="color:{item.c}">{item.v}</span>
              <span class="text-[0.42rem] text-muted-foreground/30 pb-0.5">{t("sd.outOf100")}</span>
            </div>
          </div>
        </MetricTooltip>
      {/each}
    </div>

    {#if !compact}
      <!-- ═══ Collapsible section header snippet ═══ -->
      {#snippet secHead(id: string, label: string)}
        <button class="flex items-center gap-1.5 w-full py-1 group"
                onclick={(e: MouseEvent) => { e.stopPropagation(); toggle(id); }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
               stroke-linecap="round" stroke-linejoin="round"
               class="w-2.5 h-2.5 text-muted-foreground/30 group-hover:text-muted-foreground/60
                      transition-transform duration-150 shrink-0
                      {isOpen(id) ? 'rotate-90' : ''}">
            <path d="M9 18l6-6-6-6"/>
          </svg>
          <span class="text-[0.44rem] text-muted-foreground/40 group-hover:text-muted-foreground/60
                       uppercase tracking-wider font-semibold transition-colors">{label}</span>
        </button>
      {/snippet}

      <!-- Band Powers -->
      {@render secHead("bands", t("sd.secBands"))}
      {#if isOpen("bands")}
        <div class="grid grid-cols-5 gap-x-2 gap-y-1 pl-4 pb-1.5">
          {#each [
            { l: t("sd.delta"),  v: (m.rel_delta * 100).toFixed(1) + "%", c: C_DELTA, tip: t("tip.delta") },
            { l: t("sd.theta"),  v: (m.rel_theta * 100).toFixed(1) + "%", c: C_THETA, tip: t("tip.theta") },
            { l: t("sd.alpha"),  v: (m.rel_alpha * 100).toFixed(1) + "%", c: C_ALPHA, tip: t("tip.alpha") },
            { l: t("sd.beta"),   v: (m.rel_beta  * 100).toFixed(1) + "%", c: C_BETA, tip: t("tip.beta") },
            { l: t("sd.gamma"),  v: (m.rel_gamma * 100).toFixed(1) + "%", c: C_GAMMA, tip: t("tip.gamma") },
          ] as item}
            <MetricTooltip text={item.tip}>
              <div class="flex flex-col items-center gap-0">
                <span class="text-[0.42rem] text-muted-foreground/50">{item.l}</span>
                <span class="text-[0.62rem] font-bold tabular-nums" style="color:{item.c}">{item.v}</span>
              </div>
            </MetricTooltip>
          {/each}
        </div>
      {/if}

      <!-- Ratios & Spectral -->
      {@render secHead("ratios", t("sd.secRatios"))}
      {#if isOpen("ratios")}
        <div class="grid grid-cols-3 sm:grid-cols-4 gap-x-3 gap-y-1 pl-4 pb-1.5">
          {#each [
            { l: t("sd.faa"),       v: (m.faa >= 0 ? "+" : "") + m.faa.toFixed(3), tip: t("tip.faa") },
            { l: t("sd.tar"),       v: m.tar.toFixed(2), tip: t("tip.tar") },
            { l: t("sd.bar"),       v: m.bar.toFixed(2), tip: t("tip.bar") },
            { l: t("sd.dtr"),       v: m.dtr.toFixed(2), tip: t("tip.dtr") },
            { l: t("sd.tbr"),       v: m.tbr.toFixed(2), tip: t("tip.tbr") },
            { l: t("sd.pse"),       v: m.pse.toFixed(3), tip: t("tip.pse") },
            { l: t("sd.bps"),       v: m.bps.toFixed(2), tip: t("tip.bps") },
            { l: t("sd.apf"),       v: m.apf.toFixed(1) + " Hz", tip: t("tip.apf") },
            { l: t("sd.sef95"),     v: m.sef95.toFixed(1) + " Hz", tip: t("tip.sef95") },
            { l: t("sd.specCen"),   v: m.spectral_centroid.toFixed(1) + " Hz", tip: t("tip.spectralCentroid") },
            { l: t("sd.snr"),       v: m.snr.toFixed(1) + " dB", tip: t("tip.snr") },
            { l: t("sd.coherence"), v: m.coherence.toFixed(3), tip: t("tip.coherence") },
            { l: t("sd.muSupp"),    v: m.mu_suppression.toFixed(3), tip: t("tip.muSuppression") },
            { l: t("sd.laterality"),v: m.laterality_index.toFixed(3), tip: t("tip.lateralityIndex") },
            { l: t("sd.pac"),       v: m.pac_theta_gamma.toFixed(3), tip: t("tip.pacThetaGamma") },
          ] as item}
            <MetricTooltip text={item.tip}>
              <div class="flex items-center justify-between">
                <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">{item.l}</span>
                <span class="text-[0.58rem] font-bold tabular-nums">{item.v}</span>
              </div>
            </MetricTooltip>
          {/each}
        </div>
      {/if}

      <!-- Complexity -->
      {@render secHead("complexity", t("sd.secComplexity"))}
      {#if isOpen("complexity")}
        <div class="grid grid-cols-3 sm:grid-cols-4 gap-x-3 gap-y-1 pl-4 pb-1.5">
          {#each [
            { l: t("sd.hjorthAct"),  v: m.hjorth_activity.toFixed(3), tip: t("tip.hjorthActivity") },
            { l: t("sd.hjorthMob"),  v: m.hjorth_mobility.toFixed(3), tip: t("tip.hjorthMobility") },
            { l: t("sd.hjorthCmpl"), v: m.hjorth_complexity.toFixed(3), tip: t("tip.hjorthComplexity") },
            { l: t("sd.permEnt"),    v: m.permutation_entropy.toFixed(3), tip: t("tip.permEntropy") },
            { l: t("sd.higuchiFd"),  v: m.higuchi_fd.toFixed(3), tip: t("tip.higuchiFd") },
            { l: t("sd.dfaExp"),     v: m.dfa_exponent.toFixed(3), tip: t("tip.dfaExponent") },
            { l: t("sd.sampEnt"),    v: m.sample_entropy.toFixed(3), tip: t("tip.sampleEntropy") },
          ] as item}
            <MetricTooltip text={item.tip}>
              <div class="flex items-center justify-between">
                <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">{item.l}</span>
                <span class="text-[0.58rem] font-bold tabular-nums">{item.v}</span>
              </div>
            </MetricTooltip>
          {/each}
        </div>
      {/if}

      <!-- PPG / HRV -->
      {#if hasPpg}
        {@render secHead("ppg", t("sd.secPpg"))}
        {#if isOpen("ppg")}
          <div class="grid grid-cols-3 sm:grid-cols-4 gap-x-3 gap-y-1 pl-4 pb-1.5">
            {#each [
              { l: t("sd.hr"),       v: m.hr.toFixed(0) + " bpm", tip: t("tip.hr") },
              { l: t("sd.rmssd"),   v: m.rmssd.toFixed(1) + " ms", tip: t("tip.rmssd") },
              { l: t("sd.sdnn"),    v: m.sdnn.toFixed(1) + " ms", tip: t("tip.sdnn") },
              { l: t("sd.pnn50"),   v: m.pnn50.toFixed(1) + "%", tip: t("tip.pnn50") },
              { l: t("sd.lfhf"),    v: m.lf_hf_ratio.toFixed(2), tip: t("tip.lfHfRatio") },
              { l: t("sd.respRate"),v: m.respiratory_rate.toFixed(1) + " bpm", tip: t("tip.respiratoryRate") },
              { l: t("sd.spo2"),   v: m.spo2_estimate.toFixed(1) + "%", tip: t("tip.spo2") },
              { l: t("sd.perfIdx"),v: m.perfusion_index.toFixed(2) + "%", tip: t("tip.perfusionIndex") },
              { l: t("sd.stress"), v: m.stress_index.toFixed(0), tip: t("tip.stressIndex") },
            ] as item}
              <MetricTooltip text={item.tip}>
                <div class="flex items-center justify-between">
                  <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">{item.l}</span>
                  <span class="text-[0.58rem] font-bold tabular-nums">{item.v}</span>
                </div>
              </MetricTooltip>
            {/each}
          </div>
        {/if}
      {/if}

      <!-- Events & Pose -->
      {#if hasEvents}
        {@render secHead("events", t("sd.secEvents"))}
        {#if isOpen("events")}
          <div class="grid grid-cols-3 sm:grid-cols-4 gap-x-3 gap-y-1 pl-4 pb-1.5">
            {#each [
              { l: t("sd.blinks"),    v: m.blink_count.toFixed(0), tip: t("tip.blinks") },
              { l: t("sd.blinkRate"), v: m.blink_rate.toFixed(1) + "/min", tip: t("tip.blinkRate") },
              { l: t("sd.pitch"),     v: m.head_pitch.toFixed(1) + "°", tip: t("tip.pitch") },
              { l: t("sd.roll"),      v: m.head_roll.toFixed(1) + "°", tip: t("tip.roll") },
              { l: t("sd.stillness"), v: m.stillness.toFixed(0), tip: t("tip.stillness") },
              { l: t("sd.nods"),      v: m.nod_count.toFixed(0), tip: t("tip.nods") },
              { l: t("sd.shakes"),    v: m.shake_count.toFixed(0), tip: t("tip.shakes") },
            ] as item}
              <MetricTooltip text={item.tip}>
                <div class="flex items-center justify-between">
                  <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">{item.l}</span>
                  <span class="text-[0.58rem] font-bold tabular-nums">{item.v}</span>
                </div>
              </MetricTooltip>
            {/each}
          </div>
        {/if}
      {/if}

      <!-- ═══ Charts ═══ -->
      {#if ts}
        {@render secHead("c_bands", t("sd.chartBands"))}
        {#if isOpen("c_bands")}
          <div class="pl-4 pb-1.5"><TimeSeriesChart height={90} yMin={0} yMax={1} timestamps={times} series={[
            { key: "delta", label: "δ", color: C_DELTA, data: ts.map(r => r.rd) },
            { key: "theta", label: "θ", color: C_THETA, data: ts.map(r => r.rt) },
            { key: "alpha", label: "α", color: C_ALPHA, data: ts.map(r => r.ra) },
            { key: "beta",  label: "β", color: C_BETA, data: ts.map(r => r.rb) },
            { key: "gamma", label: "γ", color: C_GAMMA, data: ts.map(r => r.rg) },
          ]} /></div>
        {/if}

        {@render secHead("c_scores", t("sd.chartScores"))}
        {#if isOpen("c_scores")}
          <div class="pl-4 pb-1.5"><TimeSeriesChart height={90} yMin={0} yMax={100} timestamps={times} series={[
            { key: "relax", label: "Relax", color: C_RELAX, data: ts.map(r => r.relaxation) },
            { key: "engage", label: "Engage", color: C_ENGAGE, data: ts.map(r => r.engagement) },
          ]} /></div>
        {/if}

        {@render secHead("c_ratios", t("sd.chartRatios"))}
        {#if isOpen("c_ratios")}
          <div class="pl-4 pb-1.5"><TimeSeriesChart height={80} timestamps={times} series={[
            { key: "tar", label: "TAR θ/α", color: C_THETA, data: ts.map(r => r.tar) },
            { key: "bar", label: "BAR β/α", color: C_BETA, data: ts.map(r => r.bar) },
            { key: "dtr", label: "DTR δ/θ", color: C_DELTA, data: ts.map(r => r.dtr) },
            { key: "tbr", label: "TBR θ/β", color: C_BLINK, data: ts.map(r => r.tbr) },
          ]} /></div>
        {/if}

        {@render secHead("c_faa", t("sd.chartFaa"))}
        {#if isOpen("c_faa")}
          <div class="pl-4 pb-1.5"><TimeSeriesChart height={70} timestamps={times} series={[
            { key: "faa", label: "FAA", color: C_MED, data: ts.map(r => r.faa) },
          ]} /></div>
        {/if}

        {@render secHead("c_spectral", t("sd.chartSpectral"))}
        {#if isOpen("c_spectral")}
          <div class="pl-4 pb-1.5"><TimeSeriesChart height={80} timestamps={times} series={[
            { key: "apf", label: "APF Hz", color: C_ALPHA, data: ts.map(r => r.apf) },
            { key: "sef95", label: "SEF95 Hz", color: C_RELAX, data: ts.map(r => r.sef95) },
            { key: "sc", label: "Centroid", color: C_BETA, data: ts.map(r => r.sc) },
          ]} /></div>
        {/if}

        {@render secHead("c_quality", t("sd.chartQuality"))}
        {#if isOpen("c_quality")}
          <div class="pl-4 pb-1.5"><TimeSeriesChart height={70} timestamps={times} series={[
            { key: "snr", label: "SNR dB", color: C_COG, data: ts.map(r => r.snr) },
            { key: "coh", label: "Coherence", color: C_DELTA, data: ts.map(r => r.coherence) },
            { key: "mu", label: "Mu Supp.", color: C_STRESS, data: ts.map(r => r.mu) },
          ]} /></div>
        {/if}

        {@render secHead("c_complexity", t("sd.chartComplexity"))}
        {#if isOpen("c_complexity")}
          <div class="pl-4 pb-1.5"><TimeSeriesChart height={80} timestamps={times} series={[
            { key: "pe", label: "Perm Ent", color: C_MED, data: ts.map(r => r.pe) },
            { key: "hfd", label: "Higuchi FD", color: C_ALPHA, data: ts.map(r => r.hfd) },
            { key: "dfa", label: "DFA", color: C_RELAX, data: ts.map(r => r.dfa) },
            { key: "se", label: "Samp Ent", color: C_BETA, data: ts.map(r => r.se) },
          ]} /></div>
        {/if}

        {@render secHead("c_hjorth", t("sd.chartHjorth"))}
        {#if isOpen("c_hjorth")}
          <div class="pl-4 pb-1.5"><TimeSeriesChart height={70} timestamps={times} series={[
            { key: "ha", label: "Activity", color: C_DROW, data: ts.map(r => r.ha) },
            { key: "hm", label: "Mobility", color: C_ALPHA, data: ts.map(r => r.hm) },
            { key: "hc", label: "Complexity", color: C_THETA, data: ts.map(r => r.hc) },
          ]} /></div>
        {/if}

        {@render secHead("c_mood", t("sd.chartMood"))}
        {#if isOpen("c_mood")}
          <div class="pl-4 pb-1.5"><TimeSeriesChart height={70} timestamps={times} series={[
            { key: "mood", label: "Mood", color: C_MOOD, data: ts.map(r => r.mood) },
            { key: "lat", label: "Laterality", color: C_DELTA, data: ts.map(r => r.lat) },
            { key: "pac", label: "PAC θ-γ", color: C_BLINK, data: ts.map(r => r.pac) },
          ]} /></div>
        {/if}

        {#if hasComposite}
          {@render secHead("c_composite", t("sd.chartComposite"))}
          {#if isOpen("c_composite")}
            <div class="pl-4 pb-1.5"><TimeSeriesChart height={90} yMin={0} yMax={100} timestamps={times} series={[
              { key: "med", label: "Meditation", color: C_MED, data: ts.map(r => r.med) },
              { key: "cog", label: "Cog. Load", color: C_COG, data: ts.map(r => r.cog) },
              { key: "drow", label: "Drowsiness", color: C_DROW, data: ts.map(r => r.drow) },
            ]} /></div>
          {/if}
        {/if}

        {#if ts.some(r => r.hr > 0)}
          {@render secHead("c_hr", t("sd.chartHr"))}
          {#if isOpen("c_hr")}
            <div class="pl-4 pb-1.5"><TimeSeriesChart height={80} timestamps={times} series={[
              { key: "hr", label: "HR bpm", color: C_HR, data: ts.map(r => r.hr) },
            ]} /></div>
          {/if}

          {@render secHead("c_hrv", t("sd.chartHrv"))}
          {#if isOpen("c_hrv")}
            <div class="pl-4 pb-1.5"><TimeSeriesChart height={80} timestamps={times} series={[
              { key: "rmssd", label: "RMSSD ms", color: C_HRV_G, data: ts.map(r => r.rmssd) },
              { key: "sdnn", label: "SDNN ms", color: C_HRV_B, data: ts.map(r => r.sdnn) },
              { key: "pnn50", label: "pNN50 %", color: C_HRV_A, data: ts.map(r => r.pnn50) },
            ]} /></div>
          {/if}

          {@render secHead("c_ppg", t("sd.chartPpg"))}
          {#if isOpen("c_ppg")}
            <div class="pl-4 pb-1.5"><TimeSeriesChart height={70} timestamps={times} series={[
              { key: "lfhf", label: "LF/HF", color: C_DELTA, data: ts.map(r => r.lf_hf) },
              { key: "resp", label: "Resp bpm", color: C_PITCH, data: ts.map(r => r.resp) },
              { key: "spo2", label: "SpO₂ %", color: C_HR, data: ts.map(r => r.spo2) },
              { key: "perf", label: "Perf Idx", color: C_STILL, data: ts.map(r => r.perf) },
              { key: "stress", label: "Stress", color: C_STRESS, data: ts.map(r => r.stress) },
            ]} /></div>
          {/if}
        {/if}

        {#if hasArtifacts}
          {@render secHead("c_artifacts", t("sd.chartArtifacts"))}
          {#if isOpen("c_artifacts")}
            <div class="pl-4 pb-1.5"><TimeSeriesChart height={70} timestamps={times} series={[
              { key: "blink_r", label: "Blinks/min", color: C_BLINK, data: ts.map(r => r.blink_r) },
            ]} /></div>
          {/if}
        {/if}

        {#if hasPose}
          {@render secHead("c_pose", t("sd.chartPose"))}
          {#if isOpen("c_pose")}
            <div class="pl-4 pb-1.5"><TimeSeriesChart height={80} timestamps={times} series={[
              { key: "pitch", label: "Pitch °", color: C_PITCH, data: ts.map(r => r.pitch) },
              { key: "roll", label: "Roll °", color: C_ROLL, data: ts.map(r => r.roll) },
              { key: "still", label: "Stillness", color: C_STILL, data: ts.map(r => r.still) },
            ]} /></div>
          {/if}
        {/if}

        {#if hasGpu}
          {@render secHead("c_gpu", "GPU Load")}
          {#if isOpen("c_gpu")}
            <div class="pl-4 pb-1.5"><TimeSeriesChart height={80} yMin={0} yMax={1} timestamps={times} series={[
              { key: "gpu", label: "Overall", color: "#6366f1", data: ts.map(r => r.gpu) },
              { key: "gpu_render", label: "Render", color: "#22c55e", data: ts.map(r => r.gpu_render) },
              { key: "gpu_tiler", label: "Tiler", color: "#f59e0b", data: ts.map(r => r.gpu_tiler) },
            ]} /></div>
          {/if}
        {/if}
      {/if}
    {/if}
  </div>

{:else if !loading}
  <div class="flex items-center gap-1.5 py-1">
    <span class="text-[0.55rem] text-muted-foreground/40 italic">📊 {t("history.noMetrics")}</span>
  </div>
{/if}
