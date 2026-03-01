<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Science References — numbered list with DOIs and disclaimer -->

<script lang="ts">
  import HelpSection from "./HelpSection.svelte";
  import { t }       from "$lib/i18n/index.svelte";

  // ── Open-source projects used by Skill ──────────────────────────────────
  const repos: { name: string; url: string; desc: string; lang: string; role: string; registry?: "github" | "crates" }[] = [
    { name: "zuna-rs",       url: "https://github.com/eugenehp/zuna-rs",    desc: "GPU-accelerated EEG neural encoder (burn + wgpu)", lang: "Rust", role: "ZUNA embedding model — converts raw EEG epochs into 128-dim vectors" },
    { name: "muse-rs",       url: "https://github.com/eugenehp/muse-rs",    desc: "Rust driver for Muse EEG headbands (BLE protocol)", lang: "Rust", role: "BLE connection, EEG/PPG/IMU streaming, control packets" },
    { name: "openbci",       url: "https://crates.io/crates/openbci",        desc: "Rust driver for OpenBCI boards — Ganglion BLE, Cyton serial/WiFi, Galea UDP", lang: "Rust", role: "OpenBCI board connection, EEG streaming (Ganglion, Cyton, Cyton+Daisy, Galea)", registry: "crates" },
    { name: "fast-umap",     url: "https://github.com/eugenehp/fast-umap",  desc: "GPU-accelerated parametric UMAP (burn + CubeCL)",  lang: "Rust", role: "3D dimensionality reduction for embedding visualisation" },
    { name: "gpu-fft",       url: "https://github.com/eugenehp/gpu-fft",    desc: "WebGPU-based batched FFT (wgpu compute shaders)",  lang: "Rust", role: "Real-time EEG filtering (overlap-save) & band power analysis" },
    { name: "fast-hnsw",     url: "https://github.com/nataliyakosmyna/fast-hnsw", desc: "Hierarchical Navigable Small World graph for ANN search", lang: "Rust", role: "Nearest-neighbour index for EEG embeddings" },
    { name: "btleplug",      url: "https://github.com/eugenehp/btleplug",   desc: "Cross-platform BLE library (fork with macOS fixes)", lang: "Rust", role: "Bluetooth Low Energy adapter for Muse device communication" },
    { name: "cubek",         url: "https://github.com/eugenehp/cubek",      desc: "GPU matrix multiplication kernels (CubeCL / wgpu)", lang: "Rust", role: "Batched matmul for ZUNA encoder inference" },
    { name: "exg",           url: "https://github.com/eugenehp/exg",        desc: "EXG signal-processing toolkit (filters, features)",  lang: "Rust", role: "IIR/FIR filters, artifact detection, Hjorth parameters, entropy" },
    { name: "burn",          url: "https://github.com/tracel-ai/burn",      desc: "Deep learning framework with multi-backend support", lang: "Rust", role: "Neural network runtime (wgpu backend) for ZUNA & UMAP" },
    { name: "Tauri",         url: "https://github.com/tauri-apps/tauri",    desc: "Desktop app framework (Rust core + web frontend)",  lang: "Rust / TS", role: "Application shell, IPC, tray icon, auto-update" },
    { name: "Svelte",        url: "https://github.com/sveltejs/svelte",     desc: "Reactive UI compiler (Svelte 5 with runes)",       lang: "TypeScript", role: "All UI components, dashboard, charts, settings" },
    { name: "Threlte",       url: "https://github.com/threlte/threlte",     desc: "Svelte wrapper for Three.js (3D rendering)",       lang: "TypeScript", role: "3D electrode head, UMAP point cloud viewer" },
    { name: "shadcn-svelte", url: "https://github.com/huntabyte/shadcn-svelte", desc: "UI component library (Tailwind + Radix primitives)", lang: "TypeScript", role: "Cards, buttons, badges, tooltips, dialogs" },
    { name: "kittentts-rs",  url: "https://github.com/eugenehp/kittentts-rs",  desc: "Rust port of KittenTTS — ONNX-based TTS (English, tract-onnx, espeak-ng)", lang: "Rust", role: "Calibration voice guidance — announces each action phase, breaks, and completion via on-device English TTS" },
    { name: "rodio",         url: "https://github.com/RustAudio/rodio",         desc: "Pure-Rust audio playback library (CPAL backend)", lang: "Rust", role: "Plays synthesised TTS audio samples on the system default output device" },
  ];

  const refs: { doi: string; arxiv?: string; titleKey: string; authorsKey: string; journalKey: string; metricsKey: string; year: number }[] = [
    { doi: "10.3389/fnins.2017.00109",                    titleKey: "helpRef.title1",  authorsKey: "helpRef.authors1",  journalKey: "helpRef.journal1",  metricsKey: "helpRef.metrics1",  year: 2017 },
{ doi: "10.1093/acprof:oso/9780195050387.001.0001",    titleKey: "helpRef.title2",  authorsKey: "helpRef.authors2",  journalKey: "helpRef.journal2",  metricsKey: "helpRef.metrics2",  year: 2006 },
{ doi: "10.1109/TAU.1967.1161901",                    titleKey: "helpRef.title3",  authorsKey: "helpRef.authors3",  journalKey: "helpRef.journal3",  metricsKey: "helpRef.metrics3",  year: 1967 },
{ doi: "10.1093/acprof:oso/9780195178081.001.0001",    titleKey: "helpRef.title4",  authorsKey: "helpRef.authors4",  journalKey: "helpRef.journal4",  metricsKey: "helpRef.metrics4",  year: 2007 },
{ doi: "10.3389/fnhum.2017.00398",                    titleKey: "helpRef.title5",  authorsKey: "helpRef.authors5",  journalKey: "helpRef.journal5",  metricsKey: "helpRef.metrics5",  year: 2017 },
{ doi: "10.1016/s0165-0173(98)00056-3",               titleKey: "helpRef.title6",  authorsKey: "helpRef.authors6",  journalKey: "helpRef.journal6",  metricsKey: "helpRef.metrics6",  year: 1999 },
{ doi: "10.1016/j.biopsycho.2004.03.002",            titleKey: "helpRef.title7",  authorsKey: "helpRef.authors7",  journalKey: "helpRef.journal7",  metricsKey: "helpRef.metrics7",  year: 2004 },
{ doi: "10.1109/bibm52615.2021.9669778",             titleKey: "helpRef.title8",  authorsKey: "helpRef.authors8",  journalKey: "helpRef.journal8",  metricsKey: "helpRef.metrics8",  year: 2021 },
{ doi: "10.5664/jcsm.26814",                          titleKey: "helpRef.title9",  authorsKey: "helpRef.authors9",  journalKey: "helpRef.journal9",  metricsKey: "helpRef.metrics9",  year: 2007 },
{ doi: "10.1016/b978-1-4160-6645-3.00002-5",          titleKey: "helpRef.title10", authorsKey: "helpRef.authors10", journalKey: "helpRef.journal10", metricsKey: "helpRef.metrics10", year: 2011 },
{ doi: "10.3389/fnins.2018.00781",                    titleKey: "helpRef.title11", authorsKey: "helpRef.authors11", journalKey: "helpRef.journal11", metricsKey: "helpRef.metrics11", year: 2018 },
{ doi: "10.1016/j.biopsycho.2009.10.008",             titleKey: "helpRef.title12", authorsKey: "helpRef.authors12", journalKey: "helpRef.journal12", metricsKey: "helpRef.metrics12", year: 2010 },
{ doi: "10.1016/j.biopsycho.2016.09.008",             titleKey: "helpRef.title13", authorsKey: "helpRef.authors13", journalKey: "helpRef.journal13", metricsKey: "helpRef.metrics13", year: 2016 },
{ doi: "10.1016/0013-4694(91)90138-t",                titleKey: "helpRef.title14", authorsKey: "helpRef.authors14", journalKey: "helpRef.journal14", metricsKey: "helpRef.metrics14", year: 1991 },
{ doi: "10.1016/s1388-2457(99)00141-8",               titleKey: "helpRef.title15", authorsKey: "helpRef.authors15", journalKey: "helpRef.journal15", metricsKey: "helpRef.metrics15", year: 1999 },
{ doi: "10.1161/01.CIR.93.5.1043",                   titleKey: "helpRef.title16", authorsKey: "helpRef.authors16", journalKey: "helpRef.journal16", metricsKey: "helpRef.metrics16", year: 1996 },
{ doi: "10.1002/(sici)1097-0193(1999)8:4<194::aid-hbm4>3.0.co;2-c", titleKey: "helpRef.title17", authorsKey: "helpRef.authors17", journalKey: "helpRef.journal17", metricsKey: "helpRef.metrics17", year: 1999 },
{ doi: "10.1038/s41593-020-00744-x",                  titleKey: "helpRef.title18", authorsKey: "helpRef.authors18", journalKey: "helpRef.journal18", metricsKey: "helpRef.metrics18", year: 2020 },
{ doi: "10.1126/science.1128115",                     titleKey: "helpRef.title19", authorsKey: "helpRef.authors19", journalKey: "helpRef.journal19", metricsKey: "helpRef.metrics19", year: 2006 },
{ doi: "10.1016/j.neubiorev.2011.10.002",             titleKey: "helpRef.title20", authorsKey: "helpRef.authors20", journalKey: "helpRef.journal20", metricsKey: "helpRef.metrics20", year: 2012 },
{ doi: "10.7551/mitpress/9609.001.0001",              titleKey: "helpRef.title21", authorsKey: "helpRef.authors21", journalKey: "helpRef.journal21", metricsKey: "helpRef.metrics21", year: 2014 },
{ doi: "10.21105/joss.00861",                         titleKey: "helpRef.title22", authorsKey: "helpRef.authors22", journalKey: "helpRef.journal22", metricsKey: "helpRef.metrics22", year: 2018 },
{ doi: "10.1016/0013-4694(70)90143-4",                titleKey: "helpRef.title24", authorsKey: "helpRef.authors24", journalKey: "helpRef.journal24", metricsKey: "helpRef.metrics24", year: 1970 },
{ doi: "10.1103/PhysRevLett.88.174102",               titleKey: "helpRef.title25", authorsKey: "helpRef.authors25", journalKey: "helpRef.journal25", metricsKey: "helpRef.metrics25", year: 2002 },
{ doi: "10.1016/0167-2789(88)90081-4",                titleKey: "helpRef.title26", authorsKey: "helpRef.authors26", journalKey: "helpRef.journal26", metricsKey: "helpRef.metrics26", year: 1988 },
{ doi: "10.1063/1.166141",                            titleKey: "helpRef.title27", authorsKey: "helpRef.authors27", journalKey: "helpRef.journal27", metricsKey: "helpRef.metrics27", year: 1995 },
{ doi: "10.1152/ajpheart.2000.278.6.H2039",          titleKey: "helpRef.title28", authorsKey: "helpRef.authors28", journalKey: "helpRef.journal28", metricsKey: "helpRef.metrics28", year: 2000 },
{ doi: "10.1097/00000542-198009001-00012",            titleKey: "helpRef.title29", authorsKey: "helpRef.authors29", journalKey: "helpRef.journal29", metricsKey: "helpRef.metrics29", year: 1980 },
{ doi: "10.1088/0967-3334/28/3/R01",                  titleKey: "helpRef.title31", authorsKey: "helpRef.authors31", journalKey: "helpRef.journal31", metricsKey: "helpRef.metrics31", year: 2007 },
{ doi: "10.1088/0967-3334/37/4/610",                  titleKey: "helpRef.title32", authorsKey: "helpRef.authors32", journalKey: "helpRef.journal32", metricsKey: "helpRef.metrics32", year: 2016 },
{ doi: "10.1016/0301-0511(95)05116-3",                titleKey: "helpRef.title35", authorsKey: "helpRef.authors35", journalKey: "helpRef.journal35", metricsKey: "helpRef.metrics35", year: 1995 },
{ doi: "10.1016/j.biopsycho.2009.08.010",             titleKey: "helpRef.title36", authorsKey: "helpRef.authors36", journalKey: "helpRef.journal36", metricsKey: "helpRef.metrics36", year: 2010 },
{ doi: "10.1017/s0048577201393095",                   titleKey: "helpRef.title37", authorsKey: "helpRef.authors37", journalKey: "helpRef.journal37", metricsKey: "helpRef.metrics37", year: 2002 },
{ doi: "10.1016/j.neubiorev.2012.10.003",              titleKey: "helpRef.title38", authorsKey: "helpRef.authors38", journalKey: "helpRef.journal38", metricsKey: "helpRef.metrics38", year: 2014 },
{ doi: "10.1016/j.neubiorev.2015.09.018",              titleKey: "helpRef.title39", authorsKey: "helpRef.authors39", journalKey: "helpRef.journal39", metricsKey: "helpRef.metrics39", year: 2015 },
{ doi: "10.3390/s24010080",                            titleKey: "helpRef.title40", authorsKey: "helpRef.authors40", journalKey: "helpRef.journal40", metricsKey: "helpRef.metrics40", year: 2023 },
{ doi: "10.1038/s41598-024-66228-1",                   titleKey: "helpRef.title41", authorsKey: "helpRef.authors41", journalKey: "helpRef.journal41", metricsKey: "helpRef.metrics41", year: 2024 },
{ doi: "10.1371/journal.pone.0210145",                 titleKey: "helpRef.title42", authorsKey: "helpRef.authors42", journalKey: "helpRef.journal42", metricsKey: "helpRef.metrics42", year: 2019 },
{ doi: "10.1109/smc.2019.8913928",                    titleKey: "helpRef.title43", authorsKey: "helpRef.authors43", journalKey: "helpRef.journal43", metricsKey: "helpRef.metrics43", year: 2019 },
{ doi: "10.3390/s19235200",                            titleKey: "helpRef.title44", authorsKey: "helpRef.authors44", journalKey: "helpRef.journal44", metricsKey: "helpRef.metrics44", year: 2019 },
{ doi: "10.1038/s41598-018-31472-9",                   titleKey: "helpRef.title45", authorsKey: "helpRef.authors45", journalKey: "helpRef.journal45", metricsKey: "helpRef.metrics45", year: 2018 },
{ doi: "10.2514/6.2023-4656",                          titleKey: "helpRef.title46", authorsKey: "helpRef.authors46", journalKey: "helpRef.journal46", metricsKey: "helpRef.metrics46", year: 2023 },
{ doi: "10.1109/bsn63547.2024.10780518",              titleKey: "helpRef.title47", authorsKey: "helpRef.authors47", journalKey: "helpRef.journal47", metricsKey: "helpRef.metrics47", year: 2024 },
{ doi: "10.1145/3719160.3736623",                      titleKey: "helpRef.title48", authorsKey: "helpRef.authors48", journalKey: "helpRef.journal48", metricsKey: "helpRef.metrics48", year: 2025 },
{ doi: "", arxiv: "2506.08872",                         titleKey: "helpRef.title49", authorsKey: "helpRef.authors49", journalKey: "helpRef.journal49", metricsKey: "helpRef.metrics49", year: 2025 },
{ doi: "", arxiv: "2508.14442",                         titleKey: "helpRef.title50", authorsKey: "helpRef.authors50", journalKey: "helpRef.journal50", metricsKey: "helpRef.metrics50", year: 2025 },
{ doi: "10.1109/eusipco.2015.7362880",                titleKey: "helpRef.title52", authorsKey: "helpRef.authors52", journalKey: "helpRef.journal52", metricsKey: "helpRef.metrics52", year: 2015 },
{ doi: "10.3389/fnins.2021.789868",                    titleKey: "helpRef.title53", authorsKey: "helpRef.authors53", journalKey: "helpRef.journal53", metricsKey: "helpRef.metrics53", year: 2021 },
{ doi: "10.3389/fnhum.2017.00396",                    titleKey: "helpRef.title54", authorsKey: "helpRef.authors54", journalKey: "helpRef.journal54", metricsKey: "helpRef.metrics54", year: 2017 },
{ doi: "10.1145/2782758",                              titleKey: "helpRef.title55", authorsKey: "helpRef.authors55", journalKey: "helpRef.journal55", metricsKey: "helpRef.metrics55", year: 2015 },
{ doi: "10.3389/fnhum.2016.00416",                    titleKey: "helpRef.title56", authorsKey: "helpRef.authors56", journalKey: "helpRef.journal56", metricsKey: "helpRef.metrics56", year: 2016 },
{ doi: "10.1117/12.2597481",                           titleKey: "helpRef.title57", authorsKey: "helpRef.authors57", journalKey: "helpRef.journal57", metricsKey: "helpRef.metrics57", year: 2021 },
{ doi: "10.1117/12.2566398",                           titleKey: "helpRef.title58", authorsKey: "helpRef.authors58", journalKey: "helpRef.journal58", metricsKey: "helpRef.metrics58", year: 2020 },
{ doi: "10.1109/smc.2019.8914214",                    titleKey: "helpRef.title59", authorsKey: "helpRef.authors59", journalKey: "helpRef.journal59", metricsKey: "helpRef.metrics59", year: 2019 },
{ doi: "10.1109/bsn63547.2024.10780485",              titleKey: "helpRef.title60", authorsKey: "helpRef.authors60", journalKey: "helpRef.journal60", metricsKey: "helpRef.metrics60", year: 2024 },
{ doi: "10.3357/amhp.6584.2025",                      titleKey: "helpRef.title61", authorsKey: "helpRef.authors61", journalKey: "helpRef.journal61", metricsKey: "helpRef.metrics61", year: 2025 },
{ doi: "10.1109/embc.2019.8857177",                   titleKey: "helpRef.title62", authorsKey: "helpRef.authors62", journalKey: "helpRef.journal62", metricsKey: "helpRef.metrics62", year: 2019 },
{ doi: "", arxiv: "2407.08877",                         titleKey: "helpRef.title63", authorsKey: "helpRef.authors63", journalKey: "helpRef.journal63", metricsKey: "helpRef.metrics63", year: 2024 },
{ doi: "10.1007/s10194-009-0140-4",               titleKey: "helpRef.title69", authorsKey: "helpRef.authors69", journalKey: "helpRef.journal69", metricsKey: "helpRef.metrics69", year: 2009 },
{ doi: "10.1126/scitranslmed.3006294",            titleKey: "helpRef.title75", authorsKey: "helpRef.authors75", journalKey: "helpRef.journal75", metricsKey: "helpRef.metrics75", year: 2013 },
{ doi: "10.1186/1471-2202-5-42",                  titleKey: "helpRef.title76", authorsKey: "helpRef.authors76", journalKey: "helpRef.journal76", metricsKey: "helpRef.metrics76", year: 2004 },
  ];
</script>

<div class="flex flex-col gap-6 pb-6">

  <!-- ── Disclaimer ───────────────────────────────────────────────────────── -->
  <div class="rounded-xl border-2 border-amber-500/50 dark:border-amber-400/30
              bg-amber-50 dark:bg-amber-950/20 px-4 py-3.5 flex flex-col gap-2">
    <div class="flex items-center gap-2">
      <span class="text-base">⚠️</span>
      <span class="text-[0.78rem] font-bold uppercase tracking-widest text-amber-700 dark:text-amber-400">
        {t("disclaimer.title")}
      </span>
    </div>
    <p class="text-[0.72rem] text-amber-900/80 dark:text-amber-200/70 leading-relaxed">
      {t("disclaimer.body")}
    </p>
    <p class="text-[0.65rem] font-semibold text-amber-700/70 dark:text-amber-400/50 uppercase tracking-wider">
      {t("disclaimer.nonCommercial")}
    </p>
  </div>

  <!-- ── Numbered reference list ──────────────────────────────────────────── -->
  <HelpSection title={t("helpRef.sectionTitle")} description={t("helpRef.sectionDesc")}>
    <div class="flex flex-col divide-y divide-border dark:divide-white/[0.05]
                rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] overflow-hidden">
      {#each refs as ref, i}
        <div class="px-4 py-3 flex gap-3">
          <!-- Number -->
          <span class="text-[0.72rem] font-bold text-muted-foreground/50 tabular-nums leading-snug shrink-0 w-6 text-right">
            [{i + 1}]
          </span>

          <div class="flex flex-col gap-1 min-w-0">
            <!-- Title -->
            <p class="text-[0.78rem] font-semibold text-foreground leading-snug">{t(ref.titleKey)}</p>
            <!-- Authors -->
            <p class="text-[0.72rem] text-muted-foreground leading-relaxed">{t(ref.authorsKey)}</p>
            <!-- Journal + year -->
            <p class="text-[0.72rem] text-muted-foreground/70 italic">{t(ref.journalKey)}, {ref.year}</p>
            <!-- Metrics tag -->
            <span class="inline-flex items-center rounded-md bg-blue-50 dark:bg-blue-500/10
                         px-2 py-0.5 text-[0.62rem] font-semibold text-blue-600 dark:text-blue-400
                         w-fit mt-0.5">
              ↳ {t(ref.metricsKey)}
            </span>
            <!-- DOI / arXiv -->
            {#if ref.doi}
              <a href="https://doi.org/{ref.doi}"
                 target="_blank" rel="noopener noreferrer"
                 class="text-[0.65rem] text-blue-500 dark:text-blue-400 hover:underline w-fit font-mono">
                doi:{ref.doi}
              </a>
            {:else if ref.arxiv}
              <a href="https://arxiv.org/abs/{ref.arxiv}"
                 target="_blank" rel="noopener noreferrer"
                 class="text-[0.65rem] text-blue-500 dark:text-blue-400 hover:underline w-fit font-mono">
                arXiv:{ref.arxiv}
              </a>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  </HelpSection>

  <!-- ── Open-Source Projects ─────────────────────────────────────────────── -->
  <HelpSection title={t("helpRef.ossTitle")} description={t("helpRef.ossDesc")}>
    <div class="flex flex-col divide-y divide-border dark:divide-white/[0.05]
                rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] overflow-hidden">
      {#each repos as repo}
        <div class="px-4 py-3 flex gap-3 items-start">
          <!-- GitHub or crates.io icon -->
          {#if repo.registry === "crates"}
            <!-- crates.io package icon -->
            <svg class="w-5 h-5 shrink-0 mt-0.5 text-muted-foreground/60" viewBox="0 0 512 512" fill="currentColor">
              <path d="M239.1 6.3l-208 78c-18.7 7-31.1 25-31.1 45v272c0 18.3 10.7 35.4 27.6 43.5l208 104c16.3 8.1 36.2 8.1 52.5 0l208-104c16.8-8.4 27.6-25.5 27.6-43.5V129.3c0-20-12.4-37.9-31.1-45l-208-78C262 2.2 250 2.2 239.1 6.3zM256 68.4l192 72v1.1l-192 78-192-78v-1.1l192-72zm32 356V275.5l160-65v133.5l-160 80z"/>
            </svg>
          {:else}
            <!-- GitHub icon -->
            <svg class="w-5 h-5 shrink-0 mt-0.5 text-muted-foreground/60" viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/>
            </svg>
          {/if}

          <div class="flex flex-col gap-1 min-w-0">
            <!-- Name + language badge -->
            <div class="flex items-center gap-2">
              <a href={repo.url}
                 target="_blank" rel="noopener noreferrer"
                 class="text-[0.82rem] font-bold text-blue-600 dark:text-blue-400 hover:underline">
                {repo.name}
              </a>
              <span class="inline-flex items-center rounded-md
                           bg-neutral-100 dark:bg-white/[0.06]
                           px-1.5 py-0.5 text-[0.58rem] font-semibold
                           text-muted-foreground">
                {repo.lang}
              </span>
            </div>
            <!-- Description -->
            <p class="text-[0.72rem] text-muted-foreground leading-relaxed">{repo.desc}</p>
            <!-- Role in Skill -->
            <span class="inline-flex items-center rounded-md bg-violet-50 dark:bg-violet-500/10
                         px-2 py-0.5 text-[0.62rem] font-semibold text-violet-600 dark:text-violet-400
                         w-fit mt-0.5">
              ↳ {repo.role}
            </span>
          </div>
        </div>
      {/each}
    </div>
  </HelpSection>

</div>
