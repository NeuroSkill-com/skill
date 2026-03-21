<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Chat tools configuration panel — tool allow-list, execution mode, limits. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import type { ToolConfig, ToolExecutionMode, CompressionLevel } from "$lib/chat-types";
  import { TOOL_THINKING_LEVELS, type ToolThinkingLevel } from "$lib/chat-types";

  interface Props {
    toolConfig: ToolConfig;
    enabledToolCount: number;
    onUpdate: (patch: Partial<ToolConfig>) => void;
  }

  let { toolConfig, enabledToolCount, onUpdate }: Props = $props();
</script>

<div class="flex-1 min-h-0 overflow-y-auto
            bg-slate-50/60 dark:bg-[#111118] px-4 py-3 flex flex-col gap-3
            scrollbar-thin scrollbar-track-transparent scrollbar-thumb-border">

  <div class="flex flex-col gap-1.5">
    <div class="flex items-center justify-between gap-2">
      <span class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
        {t("chat.tools.label")}
      </span>
      <span class="text-[0.55rem] tabular-nums text-muted-foreground/40 select-none">
        {enabledToolCount}/9
      </span>
    </div>
    <div class="grid grid-cols-2 gap-1.5">
      {#each [
        { key: "date"       as const, icon: "🕐" },
        { key: "location"   as const, icon: "📍" },
        { key: "web_search" as const, icon: "🔍" },
        { key: "web_fetch"  as const, icon: "🌐" },
        { key: "bash"       as const, icon: "💻" },
        { key: "read_file"  as const, icon: "📄" },
        { key: "write_file" as const, icon: "✏️" },
        { key: "edit_file"  as const, icon: "🔧" },
        { key: "skill_api" as const, icon: "🧠" },
      ] as tool}
        <button
          onclick={() => onUpdate({ [tool.key]: !toolConfig[tool.key] })}
          class="flex items-center gap-2 px-2.5 py-1.5 rounded-lg border transition-all
                 cursor-pointer select-none text-left
                 {toolConfig[tool.key]
                   ? 'border-primary/40 bg-primary/8 text-foreground'
                   : 'border-border bg-background text-muted-foreground/50 hover:border-muted-foreground/30'}">
          <span class="text-sm shrink-0">{tool.icon}</span>
          <div class="flex flex-col gap-0 min-w-0">
            <span class="text-[0.63rem] font-medium truncate">
              {t(`chat.tools.${tool.key}`)}
            </span>
            <span class="text-[0.5rem] text-muted-foreground/50 truncate leading-tight">
              {t(`chat.tools.${tool.key}Desc`)}
            </span>
          </div>
          <div class="ml-auto shrink-0 w-3 h-3 rounded-full border-2 flex items-center justify-center
                      {toolConfig[tool.key]
                        ? 'border-primary bg-primary'
                        : 'border-muted-foreground/30 bg-transparent'}">
            {#if toolConfig[tool.key]}
              <svg viewBox="0 0 10 10" fill="none" stroke="white" stroke-width="2"
                   stroke-linecap="round" stroke-linejoin="round" class="w-2 h-2">
                <polyline points="2 5 4 7 8 3"/>
              </svg>
            {/if}
          </div>
        </button>
      {/each}
    </div>

    <!-- Tool execution mode toggle -->
    <div class="mt-1.5">
      <div class="flex items-center justify-between gap-2 mb-1">
        <span class="text-[0.53rem] text-muted-foreground/60">{t("chat.tools.executionMode")}</span>
      </div>
      <div class="flex rounded-md overflow-hidden border border-border text-[0.6rem] font-medium">
        {#each [
          { key: "parallel"   as ToolExecutionMode, labelKey: "chat.tools.parallel" },
          { key: "sequential" as ToolExecutionMode, labelKey: "chat.tools.sequential" },
        ] as mode}
          <button
            onclick={() => onUpdate({ execution_mode: mode.key })}
            class="flex-1 py-1 transition-colors cursor-pointer
                   {toolConfig.execution_mode === mode.key
                     ? 'bg-primary text-primary-foreground'
                     : 'bg-background text-muted-foreground hover:bg-muted'}">
            {t(mode.labelKey)}
          </button>
        {/each}
      </div>
    </div>

    <!-- Max rounds -->
    <div class="flex items-center justify-between gap-3 mt-1.5">
      <div class="flex flex-col gap-0">
        <span class="text-[0.6rem] font-semibold text-foreground">{t("llm.tools.maxRounds")}</span>
        <span class="text-[0.5rem] text-muted-foreground/60 leading-snug">{t("llm.tools.maxRoundsDesc")}</span>
      </div>
      <div class="flex items-center gap-0.5 shrink-0">
        {#each [1, 3, 5, 10] as val}
          <button
            onclick={() => onUpdate({ max_rounds: val })}
            class="rounded-md border px-1.5 py-0.5 text-[0.58rem] font-semibold transition-all cursor-pointer
                   {toolConfig.max_rounds === val
                     ? 'border-primary/50 bg-primary/10 text-primary'
                     : 'border-border/60 bg-background text-muted-foreground/50 hover:text-foreground'}">
            {val}
          </button>
        {/each}
      </div>
    </div>

    <!-- Max calls per round -->
    <div class="flex items-center justify-between gap-3 mt-0.5">
      <div class="flex flex-col gap-0">
        <span class="text-[0.6rem] font-semibold text-foreground">{t("llm.tools.maxCallsPerRound")}</span>
        <span class="text-[0.5rem] text-muted-foreground/60 leading-snug">{t("llm.tools.maxCallsPerRoundDesc")}</span>
      </div>
      <div class="flex items-center gap-0.5 shrink-0">
        {#each [1, 2, 4, 8] as val}
          <button
            onclick={() => onUpdate({ max_calls_per_round: val })}
            class="rounded-md border px-1.5 py-0.5 text-[0.58rem] font-semibold transition-all cursor-pointer
                   {toolConfig.max_calls_per_round === val
                     ? 'border-primary/50 bg-primary/10 text-primary'
                     : 'border-border/60 bg-background text-muted-foreground/50 hover:text-foreground'}">
            {val}
          </button>
        {/each}
      </div>
    </div>

    <!-- Context compression -->
    <div class="flex items-center justify-between gap-3 mt-0.5">
      <div class="flex flex-col gap-0">
        <span class="text-[0.6rem] font-semibold text-foreground">{t("llm.tools.contextCompression")}</span>
        <span class="text-[0.5rem] text-muted-foreground/60 leading-snug">{t("llm.tools.contextCompressionDesc")}</span>
      </div>
      <div class="flex rounded-md overflow-hidden border border-border text-[0.56rem] font-medium shrink-0">
        {#each [
          { key: "off"        as CompressionLevel, label: t("llm.tools.compressionOff") },
          { key: "normal"     as CompressionLevel, label: t("llm.tools.compressionNormal") },
          { key: "aggressive" as CompressionLevel, label: t("llm.tools.compressionAggressive") },
        ] as opt}
          <button
            onclick={() => onUpdate({ context_compression: { ...toolConfig.context_compression, level: opt.key } })}
            class="px-2 py-1 transition-colors cursor-pointer
                   {toolConfig.context_compression.level === opt.key
                     ? 'bg-primary text-primary-foreground'
                     : 'bg-background text-muted-foreground hover:bg-muted'}">
            {opt.label}
          </button>
        {/each}
      </div>
    </div>

    <!-- Tool thinking budget -->
    <div class="flex items-center justify-between gap-3 mt-0.5">
      <div class="flex flex-col gap-0">
        <span class="text-[0.6rem] font-semibold text-foreground">{t("chat.tools.thinkingBudget")}</span>
        <span class="text-[0.5rem] text-muted-foreground/60 leading-snug">{t("chat.tools.thinkingBudgetDesc")}</span>
      </div>
      <div class="flex rounded-md overflow-hidden border border-border text-[0.56rem] font-medium shrink-0">
        {#each TOOL_THINKING_LEVELS as lvl}
          {@const isActive = toolConfig.thinking_budget === lvl.budget}
          <button
            onclick={() => onUpdate({ thinking_budget: lvl.budget })}
            class="px-2 py-1 transition-colors cursor-pointer
                   {isActive
                     ? 'bg-primary text-primary-foreground'
                     : 'bg-background text-muted-foreground hover:bg-muted'}">
            {t(lvl.labelKey)}
          </button>
        {/each}
      </div>
    </div>
  </div>
</div>
