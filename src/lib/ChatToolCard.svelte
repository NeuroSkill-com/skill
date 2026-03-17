<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Expandable tool-use card shown inside assistant messages. -->
<script lang="ts">
  import { t }                from "$lib/i18n/index.svelte";
  import { detectToolDanger } from "$lib/chat-utils";
  import type { ToolUseEvent } from "$lib/chat-types";

  interface Props {
    tu: ToolUseEvent;
    onToggleExpand: () => void;
    onCancel: () => void;
  }

  let { tu, onToggleExpand, onCancel }: Props = $props();

  const icons: Record<string, string> = {
    date: "🕐", location: "📍", web_search: "🔍", web_fetch: "🌐",
    bash: "💻", read_file: "📄", write_file: "✏️", edit_file: "🔧", search_output: "🔎",
  };

  const icon       = $derived(icons[tu.tool] ?? "🔧");
  const bashCmd    = $derived(tu.tool === "bash" ? (tu.args?.command || tu.result?.command || "") : "");
  const hasNonEmptyArgs = $derived(tu.args && Object.keys(tu.args).length > 0);
  const hasDetails = $derived(!!(hasNonEmptyArgs || tu.result || tu.detail || bashCmd));
  const dangerKey  = $derived(detectToolDanger(tu));
  const isDangerous = $derived(!!dangerKey);

  const borderColor = $derived(
    tu.status === 'cancelled' ? 'border-amber-500/30'
    : tu.status === 'calling' && isDangerous ? 'border-red-500/40'
    : tu.status === 'calling'   ? 'border-primary/25'
    : tu.status === 'done'      ? 'border-emerald-500/25'
    :                             'border-red-500/25'
  );
  const bgColor = $derived(
    tu.status === 'cancelled' ? 'bg-amber-500/5'
    : tu.status === 'calling' && isDangerous ? 'bg-red-500/8'
    : tu.status === 'calling'   ? 'bg-primary/5'
    : tu.status === 'done'      ? 'bg-emerald-500/5'
    :                             'bg-red-500/5'
  );
  const statusTextColor = $derived(
    tu.status === 'cancelled' ? 'text-amber-600 dark:text-amber-400'
    : tu.status === 'calling' && isDangerous ? 'text-red-600 dark:text-red-400'
    : tu.status === 'calling'   ? 'text-primary'
    : tu.status === 'done'      ? 'text-emerald-700 dark:text-emerald-300'
    :                             'text-red-700 dark:text-red-300'
  );
</script>

<div class="rounded-xl border {borderColor} {bgColor} overflow-hidden text-[0.68rem]">
  <!-- Header row: clickable to expand -->
  <div class="flex items-center">
    <button
      onclick={() => { if (hasDetails) onToggleExpand(); }}
      class="flex-1 min-w-0 flex items-center gap-1.5 px-3 py-1.5 text-left
             transition-colors
             {hasDetails ? 'cursor-pointer hover:bg-black/5 dark:hover:bg-white/5' : 'cursor-default'}
             {statusTextColor}">
      <!-- Expand chevron -->
      {#if hasDetails}
        <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3 shrink-0 opacity-50
             transition-transform {tu.expanded ? 'rotate-90' : ''}">
          <path d="M6 3l5 5-5 5V3z"/>
        </svg>
      {/if}
      <span class="text-sm">{icon}</span>
      <span class="font-medium">{t(`chat.tools.${tu.tool}`)}</span>

      <!-- Danger badge -->
      {#if isDangerous && (tu.status === 'calling' || tu.status === 'cancelled')}
        <span class="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-md
                     text-[0.55rem] font-semibold shrink-0
                     bg-red-500/15 text-red-600 dark:text-red-400 border border-red-500/20">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"
               stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5 shrink-0">
            <path d="M7.15 2.43L1.41 12a1 1 0 0 0 .86 1.5h11.46a1 1 0 0 0 .86-1.5L8.85 2.43a1 1 0 0 0-1.7 0z"/>
            <line x1="8" y1="6" x2="8" y2="9"/><line x1="8" y1="11" x2="8.01" y2="11"/>
          </svg>
          {t("chat.tools.dangerWarning")}
        </span>
      {/if}

      <!-- Brief summary of args in header -->
      {#if hasNonEmptyArgs || bashCmd}
        <span class="text-[0.6rem] text-muted-foreground/60 truncate ml-1 flex-1 min-w-0 font-mono">
          {#if tu.tool === "bash" && bashCmd}
            {bashCmd.length > 60 ? bashCmd.slice(0, 60) + "…" : bashCmd}
          {:else if (tu.tool === "read_file" || tu.tool === "write_file" || tu.tool === "edit_file") && tu.args.path}
            {tu.args.path}
          {:else if tu.tool === "web_search" && tu.args.query}
            {tu.args.query}
          {:else if tu.tool === "web_fetch" && tu.args.url}
            {tu.args.url}
          {:else if tu.tool === "search_output" && tu.args.pattern}
            /{tu.args.pattern}/ in {tu.args.path?.split("/").pop() ?? tu.args.path}
          {:else if tu.tool === "search_output" && tu.args.path}
            {tu.args.path.split("/").pop() ?? tu.args.path}
          {/if}
        </span>
      {/if}

      <!-- Status indicator -->
      <span class="ml-auto shrink-0 flex items-center gap-1">
        {#if tu.status === "calling"}
          <span class="flex gap-0.5">
            {#each [0,1,2] as i}
              <span class="w-1 h-1 rounded-full bg-current animate-bounce"
                    style="animation-delay:{i*0.1}s"></span>
            {/each}
          </span>
        {:else if tu.status === "cancelled"}
          <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5"
               stroke-linecap="round" class="w-3 h-3">
            <circle cx="6" cy="6" r="4.5"/>
            <line x1="3.2" y1="8.8" x2="8.8" y2="3.2"/>
          </svg>
        {:else if tu.status === "done"}
          <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
               stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <polyline points="2 6 5 9 10 3"/>
          </svg>
        {:else}
          <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
               stroke-linecap="round" class="w-3 h-3">
            <line x1="3" y1="3" x2="9" y2="9"/><line x1="9" y1="3" x2="3" y2="9"/>
          </svg>
        {/if}
      </span>
    </button>

    <!-- Cancel button (only shown while calling) -->
    {#if tu.status === "calling"}
      <button
        onclick={(e) => { e.stopPropagation(); onCancel(); }}
        title={t("chat.tools.cancel")}
        class="shrink-0 flex items-center gap-1 px-2 py-1 mr-1.5
               rounded-lg text-[0.6rem] font-semibold transition-all cursor-pointer
               {isDangerous
                 ? 'bg-red-500/15 text-red-600 dark:text-red-400 hover:bg-red-500/25 border border-red-500/30'
                 : 'bg-muted text-muted-foreground/70 hover:bg-red-500/10 hover:text-red-600 dark:hover:text-red-400 border border-border'}">
        <svg viewBox="0 0 12 12" fill="currentColor" class="w-2.5 h-2.5">
          <rect x="2" y="2" width="8" height="8" rx="1"/>
        </svg>
        {t("chat.tools.cancel")}
      </button>
    {/if}
  </div>

  <!-- Danger detail banner -->
  {#if isDangerous && tu.status === 'calling' && dangerKey}
    <div class="flex items-center gap-2 mx-3 mb-1.5 px-2 py-1 rounded-lg
                bg-red-500/10 border border-red-500/20 text-red-600 dark:text-red-400">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
        <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
        <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
      </svg>
      <span class="text-[0.58rem] font-medium leading-snug">
        {t(dangerKey)}
      </span>
    </div>
  {/if}

  <!-- Expanded detail panel -->
  {#if tu.expanded && hasDetails}
    <div class="border-t border-current/10 px-3 py-2 flex flex-col gap-2
                text-[0.63rem] text-muted-foreground">
      <!-- Bash: show command prominently -->
      {#if tu.tool === "bash" && bashCmd}
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
            {t("chat.tools.commandLabel")}
          </span>
          <pre class="font-mono text-[0.65rem] leading-relaxed whitespace-pre-wrap break-all
                      bg-black/8 dark:bg-white/8 rounded-lg px-2.5 py-2 max-h-48 overflow-y-auto
                      text-foreground select-text">{bashCmd}</pre>
        </div>
      <!-- File tools: show path prominently -->
      {:else if (tu.tool === "read_file" || tu.tool === "write_file" || tu.tool === "edit_file") && tu.args?.path}
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
            {t("chat.tools.fileLabel")}
          </span>
          <pre class="font-mono text-[0.65rem] leading-relaxed whitespace-pre-wrap break-all
                      bg-black/8 dark:bg-white/8 rounded-lg px-2.5 py-2
                      text-foreground select-text">{tu.args.path}</pre>
          {#if tu.tool === "edit_file" && tu.args.old_text}
            <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50 mt-1">
              {t("chat.tools.editOldLabel")}
            </span>
            <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                        bg-red-500/5 border border-red-500/10 rounded-lg px-2 py-1.5 max-h-32 overflow-y-auto
                        text-foreground select-text">{tu.args.old_text}</pre>
          {/if}
          {#if tu.tool === "edit_file" && tu.args.new_text}
            <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50 mt-1">
              {t("chat.tools.editNewLabel")}
            </span>
            <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                        bg-emerald-500/5 border border-emerald-500/10 rounded-lg px-2 py-1.5 max-h-32 overflow-y-auto
                        text-foreground select-text">{tu.args.new_text}</pre>
          {/if}
          {#if tu.tool === "write_file" && tu.args.content}
            <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50 mt-1">
              {t("chat.tools.contentLabel")}
            </span>
            <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                        bg-black/5 dark:bg-white/5 rounded-lg px-2 py-1.5 max-h-48 overflow-y-auto
                        text-foreground select-text">{tu.args.content}</pre>
          {/if}
        </div>
      <!-- Web search: show query -->
      {:else if tu.tool === "web_search" && tu.args?.query}
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
            {t("chat.tools.queryLabel")}
          </span>
          <pre class="font-mono text-[0.65rem] leading-relaxed whitespace-pre-wrap break-all
                      bg-black/8 dark:bg-white/8 rounded-lg px-2.5 py-2
                      text-foreground select-text">{tu.args.query}</pre>
        </div>
      <!-- Web fetch: show URL -->
      {:else if tu.tool === "web_fetch" && tu.args?.url}
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
            URL
          </span>
          <pre class="font-mono text-[0.65rem] leading-relaxed whitespace-pre-wrap break-all
                      bg-black/8 dark:bg-white/8 rounded-lg px-2.5 py-2
                      text-foreground select-text">{tu.args.url}</pre>
        </div>
      <!-- Generic: show raw JSON args -->
      {:else if hasNonEmptyArgs}
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
            {t("chat.tools.argsLabel")}
          </span>
          <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                      bg-black/5 dark:bg-white/5 rounded-lg px-2 py-1.5 max-h-48 overflow-y-auto
                      select-text">{JSON.stringify(tu.args, null, 2)}</pre>
        </div>
      {/if}
      <!-- Result -->
      {#if tu.result}
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
            {t("chat.tools.resultLabel")}
          </span>
          <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                      bg-black/5 dark:bg-white/5 rounded-lg px-2 py-1.5 max-h-64 overflow-y-auto
                      select-text {tu.status === 'error' ? 'text-red-500' : ''}">{#if typeof tu.result === "string"}{tu.result}{:else}{JSON.stringify(tu.result, null, 2)}{/if}</pre>
        </div>
      {/if}

      <!-- Cancel button in expanded view too -->
      {#if tu.status === "calling"}
        <div class="flex justify-end pt-1">
          <button
            onclick={onCancel}
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[0.62rem]
                   font-semibold transition-all cursor-pointer
                   {isDangerous
                     ? 'bg-red-500 text-white hover:bg-red-600'
                     : 'bg-red-500/10 text-red-600 dark:text-red-400 hover:bg-red-500/20 border border-red-500/30'}">
            <svg viewBox="0 0 12 12" fill="currentColor" class="w-2.5 h-2.5">
              <rect x="2" y="2" width="8" height="8" rx="1"/>
            </svg>
            {t("chat.tools.cancel")}
          </button>
        </div>
      {/if}
    </div>
  {/if}
</div>
