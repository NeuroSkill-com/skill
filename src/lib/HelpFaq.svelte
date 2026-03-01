<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help & FAQ content — mounted by the settings page's "Help & FAQ" tab. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
</script>

<div class="prose-skill flex flex-col gap-6 pb-6">

  <!-- ── Tray icon states ────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <h2 class="help-h2">{t("helpOld.trayIconStates")}</h2>
    <p class="help-p">{t("helpOld.trayIconDesc")}</p>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] divide-y divide-border dark:divide-white/[0.05]
                overflow-hidden text-[0.78rem]">
      {#each [
        { dot:"bg-slate-400",               labelKey:"helpOld.greyDisconnected",  descKey:"helpOld.greyDesc" },
        { dot:"bg-yellow-400 animate-spin", labelKey:"helpOld.spinningScanning",  descKey:"helpOld.spinningDesc" },
        { dot:"bg-green-500",               labelKey:"helpOld.greenConnected",    descKey:"helpOld.greenDesc" },
        { dot:"bg-red-500",                 labelKey:"helpOld.redBtOff",          descKey:"helpOld.redDesc" },
      ] as row}
        <div class="flex items-start gap-3 px-4 py-3">
          <span class="mt-0.5 w-3 h-3 rounded-full shrink-0 {row.dot}"></span>
          <div class="flex flex-col gap-0.5">
            <span class="font-semibold text-foreground">{t(row.labelKey)}</span>
            <span class="text-muted-foreground leading-relaxed">{t(row.descKey)}</span>
          </div>
        </div>
      {/each}
    </div>
  </section>

  <!-- ── Bluetooth lifecycle ─────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <h2 class="help-h2">{t("helpOld.btLifecycle")}</h2>
    <p class="help-p">{t("helpOld.btLifecycleDesc")}</p>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] px-4 py-3 flex flex-col gap-3">
      <ol class="flex flex-col gap-2 list-none">
        {#each [
          ["helpOld.btStep1", "helpOld.btStep1Desc"],
          ["helpOld.btStep2", "helpOld.btStep2Desc"],
          ["helpOld.btStep3", "helpOld.btStep3Desc"],
          ["helpOld.btStep4", "helpOld.btStep4Desc"],
          ["helpOld.btStep5", "helpOld.btStep5Desc"],
        ] as [stepKey, descKey], i}
          <li class="flex gap-3 text-[0.78rem]">
            <span class="mt-0.5 flex-shrink-0 w-5 h-5 rounded-full bg-muted dark:bg-white/[0.07]
                         flex items-center justify-center text-[0.65rem] font-bold
                         text-muted-foreground">{i + 1}</span>
            <div class="flex flex-col gap-0.5">
              <span class="font-semibold text-foreground">{t(stepKey)}</span>
              <span class="text-muted-foreground leading-relaxed">{t(descKey)}</span>
            </div>
          </li>
        {/each}
      </ol>
    </div>
  </section>

  <!-- ── Examples ───────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <h2 class="help-h2">{t("helpOld.examples")}</h2>
    <div class="flex flex-col gap-3">

      {#each [
        {
          titleKey: "helpOld.example1Title",
          steps: [
            { color: "text-slate-400",  icon: "⬤", textKey: "helpOld.ex1Step1" },
            { color: "text-yellow-400", icon: "⟳", textKey: "helpOld.ex1Step2" },
            { color: "text-green-500",  icon: "⬤", textKey: "helpOld.ex1Step3" },
          ],
        },
        {
          titleKey: "helpOld.example2Title",
          steps: [
            { color: "text-green-500",  icon: "⬤", textKey: "helpOld.ex2Step1" },
            { color: "text-red-500",    icon: "⬤", textKey: "helpOld.ex2Step2" },
            { color: "text-slate-400",  icon: " ", textKey: "helpOld.ex2Step3", muted: true },
            { color: "text-yellow-400", icon: "⟳", textKey: "helpOld.ex2Step4" },
            { color: "text-green-500",  icon: "⬤", textKey: "helpOld.ex2Step5" },
          ],
        },
        {
          titleKey: "helpOld.example3Title",
          steps: [
            { color: "text-yellow-400", icon: "⟳", textKey: "helpOld.ex3Step1" },
            { color: "text-slate-400",  icon: " ", textKey: "helpOld.ex3Step2", muted: true },
            { color: "text-yellow-400", icon: "⟳", textKey: "helpOld.ex3Step3" },
            { color: "text-green-500",  icon: "⬤", textKey: "helpOld.ex3Step4" },
          ],
        },
      ] as ex}
        <div class="rounded-xl border border-border dark:border-white/[0.06]
                    bg-white dark:bg-[#14141e] px-4 py-3 flex flex-col gap-1.5">
          <p class="text-[0.72rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t(ex.titleKey)}
          </p>
          <div class="font-mono text-[0.72rem] text-foreground/80 flex flex-col gap-1 mt-1">
            {#each ex.steps as s}
              <div class="flex gap-2">
                <span class="{s.color}">{s.icon}</span>
                <span class="{s.muted ? 'text-muted-foreground' : ''}">{t(s.textKey)}</span>
              </div>
            {/each}
          </div>
        </div>
      {/each}

    </div>
  </section>

  <!-- ── WebSocket / LAN ─────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <h2 class="help-h2">{t("helpOld.wsTitle")}</h2>
    <p class="help-p">{t("helpOld.wsDesc")}</p>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-muted/50 dark:bg-[#0f0f18] px-4 py-3 flex flex-col gap-2">
      <p class="text-[0.72rem] font-semibold text-muted-foreground uppercase tracking-widest">
        {t("helpOld.discoverService")}
      </p>
      <pre class="text-[0.72rem] font-mono text-foreground/80 whitespace-pre-wrap"
># macOS
dns-sd -B _skill._tcp

# Linux
avahi-browse _skill._tcp</pre>
      <p class="text-[0.72rem] font-semibold text-muted-foreground uppercase tracking-widest mt-1">
        {t("helpOld.broadcastEvents")}
      </p>
      <pre class="text-[0.72rem] font-mono text-foreground/80 whitespace-pre-wrap"
>{`{
  "event":   "eeg-bands" | "muse-status" | "label-created",
  "payload": { … }
}`}</pre>
      <p class="text-[0.72rem] font-semibold text-muted-foreground uppercase tracking-widest mt-2">
        {t("helpOld.commands")}
      </p>
      <pre class="text-[0.72rem] font-mono text-foreground/80 whitespace-pre-wrap"
>{`status     — device state, scores, sleep summary
label      — attach a label to the current moment
search     — find nearest EEG embeddings
sessions   — list all recording sessions
compare    — full comparison: metrics, sleep, UMAP
sleep      — classify sleep stages for a time range
umap       — enqueue 3D UMAP projection (non-blocking)
umap_poll  — poll for UMAP result by job_id`}</pre>
    </div>
  </section>

  <!-- ── FAQ ────────────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <h2 class="help-h2">{t("helpOld.faq")}</h2>
    <div class="flex flex-col divide-y divide-border dark:divide-white/[0.05]
                rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] overflow-hidden">
      {#each [
        ["helpOld.faqQ1",  "helpOld.faqA1"],
        ["helpOld.faqQ2",  "helpOld.faqA2"],
        ["helpOld.faqQ3",  "helpOld.faqA3"],
        ["helpOld.faqQ4",  "helpOld.faqA4"],
        ["helpOld.faqQ5",  "helpOld.faqA5"],
        ["helpOld.faqQ6",  "helpOld.faqA6"],
        ["helpOld.faqQ7",  "helpOld.faqA7"],
        ["helpOld.faqQ8",  "helpOld.faqA8"],
        ["helpOld.faqQ9",  "helpOld.faqA9"],
        ["helpOld.faqQ10", "helpOld.faqA10"],
        ["helpOld.faqQ11", "helpOld.faqA11"],
        ["helpOld.faqQ12", "helpOld.faqA12"],
        ["helpOld.faqQ13", "helpOld.faqA13"],
        ["helpOld.faqQ14", "helpOld.faqA14"],
        ["helpOld.faqQ15", "helpOld.faqA15"],
        ["helpOld.faqQ16", "helpOld.faqA16"],
        ["helpOld.faqQ17", "helpOld.faqA17"],
        ["helpOld.faqQ18", "helpOld.faqA18"],
        ["helpOld.faqQ19", "helpOld.faqA19"],
      ] as [qKey, aKey]}
        <details class="group px-4 py-3 cursor-pointer">
          <summary class="flex items-center justify-between gap-3 text-[0.78rem]
                          font-semibold text-foreground list-none select-none">
            {t(qKey)}
            <svg class="w-4 h-4 shrink-0 text-muted-foreground transition-transform
                        group-open:rotate-180"
                 viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M6 9l6 6 6-6"/>
            </svg>
          </summary>
          <p class="mt-2 text-[0.75rem] text-muted-foreground leading-relaxed">
            {t(aKey)}
          </p>
        </details>
      {/each}
    </div>
  </section>

</div>

<style>
  :global(.help-h2) {
    font-size: 0.82rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: hsl(var(--muted-foreground));
    padding-left: 0.125rem;
  }

  :global(.help-p) {
    font-size: 0.78rem;
    line-height: 1.6;
    color: hsl(var(--foreground) / 0.75);
    padding: 0 0.125rem;
  }

  details > summary::-webkit-details-marker { display: none; }
</style>
