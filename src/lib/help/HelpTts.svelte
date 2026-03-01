<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Voice (TTS) — how it works, requirements, API, logging -->

<script lang="ts">
  import HelpSection   from "./HelpSection.svelte";
  import HelpItem      from "./HelpItem.svelte";
  import TtsTestWidget from "./TtsTestWidget.svelte";
  import { Separator } from "$lib/components/ui/separator";
  import { t }         from "$lib/i18n/index.svelte";

  const stackBadges: { label: string; url: string; cls: string }[] = [
    { label: "kittentts-rs",    url: "https://github.com/eugenehp/kittentts-rs",
      cls: "border-indigo-500/30 bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 hover:border-indigo-500/60" },
    { label: "espeak-ng",       url: "https://github.com/espeak-ng/espeak-ng",
      cls: "border-violet-500/30 bg-violet-500/10 text-violet-600 dark:text-violet-400 hover:border-violet-500/60" },
    { label: "rodio",           url: "https://github.com/RustAudio/rodio",
      cls: "border-sky-500/30 bg-sky-500/10 text-sky-600 dark:text-sky-400 hover:border-sky-500/60" },
    { label: "HuggingFace Hub", url: "https://huggingface.co/KittenML",
      cls: "border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400 hover:border-amber-500/60" },
  ];

  const pipelineSteps: { label: string; bg: string }[] = [
    { label: "Text",          bg: "bg-slate-500/10 text-slate-600 dark:text-slate-300" },
    { label: "→",             bg: "" },
    { label: "Preprocess",    bg: "bg-blue-500/10 text-blue-600 dark:text-blue-300" },
    { label: "→",             bg: "" },
    { label: "Chunk",         bg: "bg-blue-500/10 text-blue-600 dark:text-blue-300" },
    { label: "→",             bg: "" },
    { label: "espeak-ng IPA", bg: "bg-violet-500/10 text-violet-600 dark:text-violet-300" },
    { label: "→",             bg: "" },
    { label: "Tokenise",      bg: "bg-indigo-500/10 text-indigo-600 dark:text-indigo-300" },
    { label: "→",             bg: "" },
    { label: "ONNX Infer",    bg: "bg-indigo-500/10 text-indigo-600 dark:text-indigo-300" },
    { label: "→",             bg: "" },
    { label: "+1 s pad",      bg: "bg-amber-500/10 text-amber-600 dark:text-amber-300" },
    { label: "→",             bg: "" },
    { label: "rodio play",    bg: "bg-emerald-500/10 text-emerald-600 dark:text-emerald-300" },
  ];

  const apiExamples: { lang: string; code: string }[] = [
    { lang: "WebSocket (Python)", code:
`import asyncio, json, websockets

async def main():
    async with websockets.connect("ws://localhost:<port>") as ws:
        await ws.send(json.dumps({"command": "say", "text": "Eyes closed. Relax."}))
        print(await ws.recv())

asyncio.run(main())` },
    { lang: "HTTP (curl)", code:
`curl -X POST http://localhost:<port>/say \\
  -H 'Content-Type: application/json' \\
  -d '{"text":"Eyes closed. Relax."}'` },
    { lang: "websocat (CLI)", code:
`echo '{"command":"say","text":"Eyes closed."}' \\
  | websocat ws://localhost:<port>` },
    { lang: "Node.js", code:
`const ws = new WebSocket("ws://localhost:<port>");
ws.on("open", () => {
  ws.send(JSON.stringify({ command: "say", text: "Calibration complete." }));
});` },
  ];

  const infoKeys: [string, string][] = [
    ["helpTts.overviewTitle",     "helpTts.overviewBody"],
    ["helpTts.howItWorksTitle",   "helpTts.howItWorksBody"],
    ["helpTts.modelTitle",        "helpTts.modelBody"],
    ["helpTts.requirementsTitle", "helpTts.requirementsBody"],
    ["helpTts.calibrationTitle",  "helpTts.calibrationBody"],
    ["helpTts.apiTitle",          "helpTts.apiBody"],
    ["helpTts.loggingTitle",      "helpTts.loggingBody"],
  ] as const;
</script>

<div class="flex flex-col gap-6 pb-6">

  <!-- ── Open-source stack badge row ─────────────────────────────────────── -->
  <div class="flex flex-wrap gap-2">
    {#each stackBadges as badge}
      <a href={badge.url} target="_blank" rel="noopener noreferrer"
         class="flex items-center gap-1.5 rounded-full border px-2.5 py-1
                text-[0.6rem] font-semibold transition-colors {badge.cls}">
        <span>{badge.label}</span>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2" class="w-3 h-3 opacity-60">
          <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
          <polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/>
        </svg>
      </a>
    {/each}
  </div>

  <!-- ── Reference items ──────────────────────────────────────────────────── -->
  <HelpSection title={t("helpTts.overviewTitle")}>
    {#each infoKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Pipeline diagram ──────────────────────────────────────────────────── -->
  <HelpSection title="Pipeline">
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-muted/50 dark:bg-[#0f0f18] px-4 py-3">
      <div class="flex flex-wrap items-center gap-1.5 text-[0.66rem] font-mono">
        {#each pipelineSteps as step}
          {#if step.bg}
            <span class="rounded-md px-2 py-0.5 font-semibold {step.bg}">{step.label}</span>
          {:else}
            <span class="text-muted-foreground/50">{step.label}</span>
          {/if}
        {/each}
      </div>
    </div>
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── API examples ──────────────────────────────────────────────────────── -->
  <HelpSection title="API Examples">
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-muted/50 dark:bg-[#0f0f18] flex flex-col divide-y
                divide-border dark:divide-white/[0.05] overflow-hidden">
      {#each apiExamples as ex}
        <div class="px-4 py-3 flex flex-col gap-1.5">
          <span class="text-[0.56rem] font-semibold uppercase tracking-wider text-muted-foreground/60">
            {ex.lang}
          </span>
          <pre class="text-[0.68rem] font-mono text-foreground/80 whitespace-pre-wrap leading-relaxed overflow-x-auto">{ex.code}</pre>
        </div>
      {/each}
    </div>
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Live test widget ───────────────────────────────────────────────────── -->
  <HelpSection title={t("helpTts.testTitle")} description={t("helpTts.testBody")}>
    <TtsTestWidget />
  </HelpSection>

</div>
