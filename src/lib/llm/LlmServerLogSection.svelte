<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { tick } from "svelte";
import { t } from "$lib/i18n/index.svelte";

interface LlmLogEntry {
  ts: number;
  level: string;
  message: string;
}

interface Props {
  logs: LlmLogEntry[];
  onClear: () => void;
}

let { logs, onClear }: Props = $props();

let logEl = $state<HTMLElement | null>(null);
let logAutoScroll = $state(true);
let logFilter = $state<"all" | "info" | "warn" | "error">("all");
let logSearch = $state("");

const filteredLogs = $derived.by(() => {
  let filtered = logs;
  if (logFilter !== "all") filtered = filtered.filter((e) => e.level === logFilter);
  if (logSearch.trim()) {
    const q = logSearch.trim().toLowerCase();
    filtered = filtered.filter((e) => e.message.toLowerCase().includes(q));
  }
  return filtered;
});

$effect(() => {
  logs.length;
  if (!logAutoScroll) return;
  tick().then(() => {
    if (logEl) logEl.scrollTop = logEl.scrollHeight;
  });
});

function handleLogScroll() {
  if (!logEl) return;
  logAutoScroll = logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 40;
}

function toggleAutoScroll() {
  logAutoScroll = !logAutoScroll;
  if (!logAutoScroll) return;
  tick().then(() => {
    if (logEl) logEl.scrollTop = logEl.scrollHeight;
  });
}
</script>

<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5 flex-wrap">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      Server log
    </span>
    <span class="flex items-center gap-1 text-[0.52rem] text-muted-foreground/50">
      <span class="w-1 h-1 rounded-full {logs.length > 0 ? 'bg-emerald-500 animate-pulse' : 'bg-slate-400'}"></span>
      {filteredLogs.length}{filteredLogs.length !== logs.length ? `/${logs.length}` : ""}
    </span>

    <div class="flex rounded-md overflow-hidden border border-border/50 text-[0.5rem] font-medium ml-1">
      {#each [
        { key: "all" as const, label: t("chat.logFilter.all"), color: "" },
        { key: "info" as const, label: t("chat.logFilter.info"), color: "text-emerald-500" },
        { key: "warn" as const, label: t("chat.logFilter.warn"), color: "text-amber-500" },
        { key: "error" as const, label: t("chat.logFilter.error"), color: "text-red-500" },
      ] as tab}
        <button onclick={() => (logFilter = tab.key)}
          class="px-2 py-0.5 transition-colors cursor-pointer
                 {logFilter === tab.key
                   ? `bg-foreground/10 ${tab.color || 'text-foreground'} font-bold`
                   : 'bg-transparent text-muted-foreground/50 hover:text-muted-foreground'}">
          {tab.label}
        </button>
      {/each}
    </div>

    <input
      type="text"
      bind:value={logSearch}
      placeholder={t("chat.logFilter.search")}
      class="ml-auto h-5 w-28 text-[0.52rem] px-2 rounded-md border border-border/50
             bg-transparent text-muted-foreground placeholder:text-muted-foreground/30
             focus:outline-none focus:border-violet-500/50" />

    <button onclick={toggleAutoScroll}
      class="text-[0.52rem] cursor-pointer select-none transition-colors
             {logAutoScroll ? 'text-emerald-600 dark:text-emerald-400' : 'text-muted-foreground/50 hover:text-foreground'}">
      auto-scroll {logAutoScroll ? "on" : "off"}
    </button>
    <button onclick={onClear}
      class="text-[0.52rem] text-muted-foreground/50 hover:text-muted-foreground cursor-pointer select-none">
      clear
    </button>
  </div>

  <div bind:this={logEl} onscroll={handleLogScroll}
    class="h-[26vh] min-h-[180px] max-h-[340px] overflow-y-auto rounded-xl border
           border-border dark:border-white/[0.06] bg-black/90 text-[0.62rem]
           font-mono leading-relaxed px-3 py-2 space-y-0.5">

    {#if filteredLogs.length === 0}
      <p class="text-zinc-500 italic">No logs yet…</p>
    {:else}
      {#each filteredLogs as entry (entry.ts + entry.message)}
        <div class="flex gap-2 whitespace-pre-wrap break-words">
          <span class="text-zinc-500 shrink-0">{new Date(entry.ts * 1000).toLocaleTimeString()}</span>
          <span class="shrink-0 font-semibold
            {entry.level === 'error' ? 'text-red-400'
            : entry.level === 'warn' ? 'text-amber-300'
            : 'text-emerald-300'}">[{entry.level}]</span>
          <span class="text-zinc-200">{entry.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</section>
