<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { ToggleRow } from "$lib/components/ui/toggle-row";
import { fmtGB } from "$lib/format";
import { t } from "$lib/i18n/index.svelte";

interface Props {
  enabled: boolean;
  autostart: boolean;
  verbose: boolean;
  hasActive: boolean;
  activeModel: string;
  serverStatus: "stopped" | "loading" | "running";
  serverBusy?: boolean;
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
  serverBusy = false,
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
    <SectionHeader>{t("llm.section.server")}</SectionHeader>
    <span class="w-1.5 h-1.5 rounded-full {hasActive && enabled ? 'bg-emerald-500' : 'bg-slate-400'}"></span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
      <ToggleRow
        checked={enabled}
        label={t("llm.enabled")}
        description={t("llm.enabledDesc")}
        ontoggle={onToggleEnabled}
       
        showBadge={false}
      />

      <ToggleRow
        checked={autostart}
        label={t("llm.autostart")}
        description={t("llm.autostartDesc")}
        ontoggle={onToggleAutostart}
       
        showBadge={false}
      />

      <ToggleRow
        checked={verbose}
        label={t("llm.verbose")}
        description={t("llm.verboseDesc")}
        ontoggle={onToggleVerbose}
       
        showBadge={false}
      />

      <div class="flex items-center justify-between gap-4 px-4 py-3">
        <div class="flex items-center gap-2">
          <span class="w-2 h-2 rounded-full shrink-0
            {serverStatus === 'running'  ? 'bg-emerald-500'
            : serverStatus === 'loading' ? 'bg-amber-500 animate-pulse'
            :                             'bg-slate-400/50'}"></span>
          <span class="text-ui-lg font-semibold text-foreground">
            {serverStatus === "running" ? (activeFamilyName ?? "Running") : serverStatus === "loading" ? "Loading…" : "Stopped"}
          </span>
          {#if serverStatus === "running" && activeQuant && activeSizeGb !== null}
            <span class="text-ui-sm text-muted-foreground/60 font-mono">
              {activeQuant} · {fmtGB(activeSizeGb)}
            </span>
          {/if}
        </div>
        <div class="flex items-center gap-1.5">
          {#if serverStatus === "stopped"}
            <Button size="sm"
              class="h-6 text-ui-sm px-2.5 bg-violet-600 hover:bg-violet-700 text-white
                     disabled:opacity-40 disabled:cursor-not-allowed"
              onclick={onStart} disabled={!hasActive || serverBusy}>
              {serverBusy ? "Starting…" : "Start"}
            </Button>
          {:else}
            <Button size="sm" variant="outline"
              class="h-6 text-ui-sm px-2 text-red-500 border-red-500/30 hover:bg-red-500/10
                     disabled:opacity-40 disabled:cursor-not-allowed"
              onclick={onStop} disabled={serverBusy}>
              {serverBusy ? "Stopping…" : serverStatus === "loading" ? "Cancel" : "Stop"}
            </Button>
          {/if}
          <Button size="sm" variant="outline"
            class="h-6 text-ui-sm px-2.5 border-violet-500/40 text-primary
                   dark:text-violet-600 dark:text-violet-400 hover:bg-violet-500/10"
            onclick={onOpenChat}>
            Chat…
          </Button>
        </div>
      </div>

      {#if startError}
        <div class="mx-4 mb-2 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20
                    text-ui-base text-red-600 dark:text-red-400 leading-snug">
          {startError}
        </div>
      {/if}

      {#if serverStatus === "stopped" && activeModel && !hasActive}
        <div class="mx-4 mb-2 px-3 py-2 rounded-lg bg-amber-500/10 border border-amber-500/20
                    text-ui-base text-amber-700 dark:text-amber-400 leading-snug">
          <strong>{activeModel}</strong> is not downloaded yet.
          Find it in Models below and click Download.
        </div>
      {/if}

      <div class="flex flex-col gap-0.5 px-4 py-3 bg-surface-3">
        <SectionHeader>{t("llm.endpoint")}</SectionHeader>
        <div class="flex flex-wrap gap-1">
          {#each ["/v1/chat/completions","/v1/completions","/v1/embeddings","/v1/models","/health"] as ep}
            <code class="text-ui-sm font-mono text-muted-foreground
                          bg-muted dark:bg-white/5 rounded px-1.5 py-0.5">{ep}</code>
          {/each}
        </div>
        <span class="text-ui-sm text-muted-foreground/60 mt-0.5">
          http://localhost:{wsPort} · {t("llm.endpointHint")}
        </span>
      </div>
    </CardContent>
  </Card>
</section>
