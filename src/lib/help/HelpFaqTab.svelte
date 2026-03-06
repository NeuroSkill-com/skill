<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: FAQ -->

<script lang="ts">
  import HelpSection from "./HelpSection.svelte";
  import { t } from "$lib/i18n/index.svelte";

  const faqKeys = [
    ["helpFaq.q1",  "helpFaq.a1"],
    ["helpFaq.q2",  "helpFaq.a2"],
    ["helpFaq.q3",  "helpFaq.a3"],
    ["helpFaq.q4",  "helpFaq.a4"],
    ["helpFaq.q5",  "helpFaq.a5"],
    ["helpFaq.q6",  "helpFaq.a6"],
    ["helpFaq.q7",  "helpFaq.a7"],
    ["helpFaq.q8",  "helpFaq.a8"],
    ["helpFaq.q9",  "helpFaq.a9"],
    ["helpFaq.q10", "helpFaq.a10"],
    ["helpFaq.q11", "helpFaq.a11"],
    ["helpFaq.q12", "helpFaq.a12"],
    ["helpFaq.q13", "helpFaq.a13"],
    ["helpFaq.q14", "helpFaq.a14"],
    ["helpFaq.q15", "helpFaq.a15"],
    ["helpFaq.q16", "helpFaq.a16"],
    ["helpFaq.q17", "helpFaq.a17"],
    ["helpFaq.q18", "helpFaq.a18"],
    ["helpFaq.q19", "helpFaq.a19"],
    ["helpFaq.q20", "helpFaq.a20"],
    ["helpFaq.q21", "helpFaq.a21"],
    ["helpFaq.q22", "helpFaq.a22"],
    ["helpFaq.q23", "helpFaq.a23"],
    ["helpFaq.q24", "helpFaq.a24"],
    ["helpFaq.q25", "helpFaq.a25"],
    ["helpFaq.q26", "helpFaq.a26"],
    ["helpFaq.q27", "helpFaq.a27"],
    ["helpFaq.q28", "helpFaq.a28"],
    ["helpFaq.q29", "helpFaq.a29"],
    ["helpFaq.q30", "helpFaq.a30"],
    ["helpFaq.q31", "helpFaq.a31"],
    ["helpFaq.q32", "helpFaq.a32"],
    ["helpFaq.q33", "helpFaq.a33"],
    ["helpFaq.q34", "helpFaq.a34"],
    ["helpFaq.q35", "helpFaq.a35"],
    ["helpFaq.q36", "helpFaq.a36"],
    // ── OpenBCI ──────────────────────────────────────────────────────────────
    ["helpFaq.q37", "helpFaq.a37"],
    ["helpFaq.q38", "helpFaq.a38"],
    ["helpFaq.q39", "helpFaq.a39"],
    ["helpFaq.q40", "helpFaq.a40"],
    ["helpFaq.q41", "helpFaq.a41"],
    ["helpFaq.q42", "helpFaq.a42"],
    ["helpFaq.q43", "helpFaq.a43"],
    ["helpFaq.q44", "helpFaq.a44"],
    ["helpFaq.q45", "helpFaq.a45"],
    ["helpFaq.q46", "helpFaq.a46"],
    ["helpFaq.q47", "helpFaq.a47"],
    // ── Activity Tracking ─────────────────────────────────────────────────
    ["helpFaq.q48", "helpFaq.a48"],
    ["helpFaq.q49", "helpFaq.a49"],
    ["helpFaq.q50", "helpFaq.a50"],
    // ── Permissions ───────────────────────────────────────────────────────
    ["helpFaq.q51", "helpFaq.a51"],
    ["helpFaq.q52", "helpFaq.a52"],
    ["helpFaq.q53", "helpFaq.a53"],
    ["helpFaq.q54", "helpFaq.a54"],
    ["helpFaq.q55", "helpFaq.a55"],
  ] as const;
</script>

<div class="flex flex-col gap-6 pb-6">

  <HelpSection title={t("helpFaq.title")}>
    <div class="flex flex-col divide-y divide-border dark:divide-white/[0.05]
                rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] overflow-hidden">
      {#each faqKeys as [qKey, aKey]}
        <details id={qKey} class="group px-4 py-3 cursor-pointer scroll-mt-4">
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
  </HelpSection>

</div>

<style>
  details > summary::-webkit-details-marker { display: none; }
</style>
