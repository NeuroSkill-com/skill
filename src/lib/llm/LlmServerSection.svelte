<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { fmtGB } from "$lib/format";
import { t } from "$lib/i18n/index.svelte";

interface Props {
  enabled: boolean;
  autostart: boolean;
  verbose: boolean;
  hasActive: boolean;
  activeModel: string;
  serverStatus: "stopped" | "loading" | "running";
  activeFamilyName: string | null;
  activeQuant: string | null;
  activeSizeGb: number | null;
  wsPort: number;
  startError: string;
  onToggleEnabled: () => void | Promise<void>;
  onToggleAutostart: () => void | Promise<void>;
  onToggleVerbose: () => void | Promise<void>;
  onStart: () => void | Promise<void>;
  onStop: () => void | Promise<void>;
  onOpenChat: () => void | Promise<void>;
}

let {
  enabled,
  autostart,
  verbose,
  hasActive,
  activeModel,
  serverStatus,
  activeFamilyName,
  activeQuant,
  activeSizeGb,
  wsPort,
  startError,
  onToggleEnabled,
  onToggleAutostart,
  onToggleVerbose,
  onStart,
  onStop,
  onOpenChat,
}: Props = $props();
</script>

<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.section.server")}
    </span>
    <span class="w-1.5 h-1.5 rounded-full {hasActive && enabled ? 'bg-emerald-500' : 'bg-slate-400'}"></span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
      <div class="flex items-center justify-between gap-4 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.enabled")}</span>
          <span class="text-[0.65rem] text-muted-foreground leading-relaxed">{t("llm.enabledDesc")}</span>
        </div>
        <button role="switch" aria-checked={enabled} aria-label={t("llm.enabled")}
          onclick={onToggleEnabled}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                 border-transparent transition-colors duration-200
                 {enabled ? 'bg-emerald-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                        transform transition-transform duration-200
                        {enabled ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

      <div class="flex items-center justify-between gap-4 px-4 py-3">
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.autostart")}</span>
          <span class="text-[0.65rem] text-muted-foreground leading-relaxed">{t("llm.autostartDesc")}</span>
        </div>
        <button role="switch" aria-checked={autostart} aria-label={t("llm.autostart")}
          onclick={onToggleAutostart}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                 border-transparent transition-colors duration-200
                 {autostart ? 'bg-emerald-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                        transform transition-transform duration-200
                        {autostart ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

      <div class="flex items-center justify-between gap-4 px-4 py-3">
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.verbose")}</span>
          <span class="text-[0.65rem] text-muted-foreground leading-relaxed">{t("llm.verboseDesc")}</span>
        </div>
        <button role="switch" aria-checked={verbose} aria-label={t("llm.verbose")}
          onclick={onToggleVerbose}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                 border-transparent transition-colors duration-200
                 {verbose ? 'bg-emerald-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                        transform transition-transform duration-200
                        {verbose ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

      <div class="flex items-center justify-between gap-4 px-4 py-3">
        <div class="flex items-center gap-2">
          <span class="w-2 h-2 rounded-full shrink-0
            {serverStatus === 'running'  ? 'bg-emerald-500'
            : serverStatus === 'loading' ? 'bg-amber-500 animate-pulse'
            :                             'bg-slate-400/50'}"></span>
          <span class="text-[0.78rem] font-semibold text-foreground">
            {serverStatus === "running" ? (activeFamilyName ?? "Running") : serverStatus === "loading" ? "Loading…" : "Stopped"}
          </span>
          {#if serverStatus === "running" && activeQuant && activeSizeGb !== null}
            <span class="text-[0.62rem] text-muted-foreground/60 font-mono">
              {activeQuant} · {fmtGB(activeSizeGb)}
            </span>
          {/if}
        </div>
        <div class="flex items-center gap-1.5">
          {#if serverStatus === "stopped"}
            <Button size="sm"
              class="h-6 text-[0.62rem] px-2.5 bg-violet-600 hover:bg-violet-700 text-white
                     disabled:opacity-40 disabled:cursor-not-allowed"
              onclick={onStart} disabled={!hasActive}>
              Start
            </Button>
          {:else}
            <Button size="sm" variant="outline"
              class="h-6 text-[0.62rem] px-2 text-red-500 border-red-500/30 hover:bg-red-500/10"
              onclick={onStop}>
              {serverStatus === "loading" ? "Cancel" : "Stop"}
            </Button>
          {/if}
          <Button size="sm" variant="outline"
            class="h-6 text-[0.62rem] px-2.5 border-violet-500/40 text-violet-700
                   dark:text-violet-400 hover:bg-violet-500/10"
            onclick={onOpenChat}>
            Chat…
          </Button>
        </div>
      </div>

      {#if startError}
        <div class="mx-4 mb-2 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20
                    text-[0.68rem] text-red-600 dark:text-red-400 leading-snug">
          {startError}
        </div>
      {/if}

      {#if serverStatus === "stopped" && activeModel && !hasActive}
        <div class="mx-4 mb-2 px-3 py-2 rounded-lg bg-amber-500/10 border border-amber-500/20
                    text-[0.68rem] text-amber-700 dark:text-amber-400 leading-snug">
          <strong>{activeModel}</strong> is not downloaded yet.
          Find it in Models below and click Download.
        </div>
      {/if}

      <div class="flex flex-col gap-0.5 px-4 py-3 bg-slate-50 dark:bg-[#111118]">
        <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
          {t("llm.endpoint")}
        </span>
        <div class="flex flex-wrap gap-1">
          {#each ["/v1/chat/completions","/v1/completions","/v1/embeddings","/v1/models","/health"] as ep}
            <code class="text-[0.6rem] font-mono text-muted-foreground
                          bg-muted dark:bg-white/5 rounded px-1.5 py-0.5">{ep}</code>
          {/each}
        </div>
        <span class="text-[0.58rem] text-muted-foreground/60 mt-0.5">
          http://localhost:{wsPort} · {t("llm.endpointHint")}
        </span>
      </div>
    </CardContent>
  </Card>
</section>
