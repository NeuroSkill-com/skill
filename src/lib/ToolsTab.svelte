<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Tools Settings Tab
  ──────────────────
  • Per-tool toggles (date, location, web search, web fetch, bash, read/write/edit file)
  • SearXNG URL configuration
  • Execution mode, max rounds, max calls per round
-->
<script lang="ts">
  import { onMount }                  from "svelte";
  import { invoke }                   from "@tauri-apps/api/core";
  import { Card, CardContent }        from "$lib/components/ui/card";
  import { t }                        from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────

  type ToolExecutionMode = "sequential" | "parallel";
  interface LlmToolsConfig {
    enabled: boolean;
    date: boolean;
    location: boolean;
    web_search: boolean;
    web_fetch: boolean;
    searxng_url: string;
    bash: boolean;
    read_file: boolean;
    write_file: boolean;
    edit_file: boolean;
    execution_mode: ToolExecutionMode;
    max_rounds: number;
    max_calls_per_round: number;
  }

  interface LlmConfig {
    enabled: boolean; autostart: boolean; model_path: string | null; n_gpu_layers: number;
    ctx_size: number | null; parallel: number; api_key: string | null;
    tools: LlmToolsConfig;
    mmproj: string | null; mmproj_n_threads: number; no_mmproj_gpu: boolean;
    autoload_mmproj: boolean; verbose: boolean;
  }

  type LlmToolKey = "date" | "location" | "web_search" | "web_fetch" | "bash" | "read_file" | "write_file" | "edit_file";

  // ── State ──────────────────────────────────────────────────────────────────

  let config  = $state<LlmConfig>({
    enabled: false, autostart: false, model_path: null, n_gpu_layers: 4294967295,
    ctx_size: null, parallel: 1, api_key: null,
    tools: { enabled: true, date: true, location: true, web_search: true, web_fetch: true, searxng_url: "", bash: false, read_file: false, write_file: false, edit_file: false, execution_mode: "parallel" as ToolExecutionMode, max_rounds: 10, max_calls_per_round: 4 },
    mmproj: null, mmproj_n_threads: 4, no_mmproj_gpu: false, autoload_mmproj: true,
    verbose: false,
  });

  let configSaving = $state(false);

  let TOOL_ROWS = $derived<Array<{ key: LlmToolKey; label: string; desc: string; warn?: boolean }>>(
    [
      { key: "date",       label: t("llm.tools.date"),      desc: t("llm.tools.dateDesc") },
      { key: "location",   label: t("llm.tools.location"),  desc: t("llm.tools.locationDesc") },
      { key: "web_search", label: t("llm.tools.webSearch"), desc: t("llm.tools.webSearchDesc") },
      { key: "web_fetch",  label: t("llm.tools.webFetch"),  desc: t("llm.tools.webFetchDesc") },
      { key: "bash",       label: t("llm.tools.bash"),      desc: t("llm.tools.bashDesc"),      warn: true },
      { key: "read_file",  label: t("llm.tools.readFile"),  desc: t("llm.tools.readFileDesc") },
      { key: "write_file", label: t("llm.tools.writeFile"), desc: t("llm.tools.writeFileDesc"), warn: true },
      { key: "edit_file",  label: t("llm.tools.editFile"),  desc: t("llm.tools.editFileDesc"),  warn: true },
    ]
  );

  // ── Data loading ───────────────────────────────────────────────────────────

  async function loadConfig() {
    try {
      config = await invoke<LlmConfig>("get_llm_config");
    } catch {}
  }

  async function saveConfig() {
    configSaving = true;
    try { await invoke("set_llm_config", { config }); }
    finally { configSaving = false; }
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  onMount(async () => {
    await loadConfig();
  });
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Chat tools                                                                  -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.tools.section")}
    </span>
    <span class="text-[0.52rem] text-muted-foreground/50">{config.tools.enabled ? TOOL_ROWS.filter(r => config.tools[r.key]).length + '/' + TOOL_ROWS.length : 'off'}</span>
    {#if configSaving}<span class="text-[0.56rem] text-muted-foreground ml-auto">saving…</span>{/if}
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col py-0 px-0">

      <!-- Description + master toggle -->
      <div class="flex items-center justify-between gap-4 px-4 pt-3.5 pb-2">
        <p class="text-[0.65rem] text-muted-foreground leading-relaxed">
          {t("llm.tools.sectionDesc")}
        </p>
        <button role="switch" aria-checked={config.tools.enabled} aria-label={t("llm.tools.enableAll")}
          onclick={async () => {
            config = { ...config, tools: { ...config.tools, enabled: !config.tools.enabled } };
            await saveConfig();
          }}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                 border-transparent transition-colors duration-200
                 {config.tools.enabled ? 'bg-blue-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                        transform transition-transform duration-200
                        {config.tools.enabled ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

      <!-- Tool toggles -->
      <div class="flex flex-col gap-2 px-4 pb-3 {config.tools.enabled ? '' : 'opacity-40 pointer-events-none'}">
        {#each TOOL_ROWS as tool}
          <div class="flex items-center justify-between gap-4 rounded-xl border
                      {tool.warn && config.tools[tool.key]
                        ? 'border-amber-500/40 bg-amber-50/40 dark:bg-amber-950/15'
                        : 'border-border/60 dark:border-white/[0.06] bg-slate-50/60 dark:bg-[#111118]'}
                      px-3 py-2.5">
            <div class="flex flex-col gap-0.5">
              <div class="flex items-center gap-1.5">
                <span class="text-[0.74rem] font-semibold text-foreground">{tool.label}</span>
                {#if tool.warn}
                  <span class="text-[0.5rem] font-semibold rounded-full border px-1.5 py-0
                               border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400">
                    {t("llm.tools.advanced")}
                  </span>
                {/if}
              </div>
              <span class="text-[0.62rem] text-muted-foreground leading-relaxed">{tool.desc}</span>
            </div>
            <button role="switch" aria-checked={config.tools[tool.key]} aria-label={tool.label}
              onclick={async () => {
                config = { ...config, tools: { ...config.tools, [tool.key]: !config.tools[tool.key] } };
                await saveConfig();
              }}
              class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                     border-transparent transition-colors duration-200
                     {config.tools[tool.key]
                       ? (tool.warn ? 'bg-amber-500' : 'bg-blue-500')
                       : 'bg-muted dark:bg-white/10'}">
              <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                            transform transition-transform duration-200
                            {config.tools[tool.key] ? 'translate-x-4' : 'translate-x-0'}"></span>
            </button>
          </div>
        {/each}

        <!-- SearXNG URL -->
        {#if config.tools.web_search}
          <div class="flex flex-col gap-1 rounded-xl border border-border/60 dark:border-white/[0.06]
                      bg-slate-50/60 dark:bg-[#111118] px-3 py-2.5">
            <label for="searxng-url" class="text-[0.68rem] font-semibold text-foreground">
              {t("llm.tools.searxngUrl")}
            </label>
            <span class="text-[0.6rem] text-muted-foreground leading-relaxed">{t("llm.tools.searxngUrlDesc")}</span>
            <input id="searxng-url" type="text" placeholder="https://search.example.com"
              value={config.tools.searxng_url ?? ""}
              oninput={async (e: Event) => {
                const val = (e.target as HTMLInputElement).value;
                config = { ...config, tools: { ...config.tools, searxng_url: val } };
              }}
              onchange={async () => { await saveConfig(); }}
              class="mt-1 w-full rounded-lg border border-border/60 dark:border-white/[0.08]
                     bg-white dark:bg-[#0c0c14] px-2.5 py-1.5 text-[0.7rem] text-foreground
                     placeholder:text-muted-foreground/50 outline-none focus:ring-1 focus:ring-blue-500/50" />
          </div>
        {/if}
      </div>

      <!-- Execution mode + limits -->
      <div class="flex flex-col gap-3 px-4 py-3 border-t border-border/40 dark:border-white/[0.04]
                  bg-slate-50 dark:bg-[#111118] {config.tools.enabled ? '' : 'opacity-40 pointer-events-none'}">
        <!-- Execution mode -->
        <div class="flex flex-col gap-1.5">
          <span class="text-[0.65rem] text-muted-foreground">{t("llm.tools.executionMode")}</span>
          <div class="flex rounded-lg overflow-hidden border border-border text-[0.68rem] font-medium">
            {#each [
              { key: "parallel"   as ToolExecutionMode, label: t("llm.tools.parallel") },
              { key: "sequential" as ToolExecutionMode, label: t("llm.tools.sequential") },
            ] as mode}
              <button
                onclick={async () => {
                  config = { ...config, tools: { ...config.tools, execution_mode: mode.key } };
                  await saveConfig();
                }}
                class="flex-1 py-1.5 transition-colors cursor-pointer
                       {config.tools.execution_mode === mode.key
                         ? 'bg-primary text-primary-foreground'
                         : 'bg-background text-muted-foreground hover:bg-muted'}">
                {mode.label}
              </button>
            {/each}
          </div>
        </div>

        <!-- Max rounds (tool hops) -->
        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.72rem] font-semibold text-foreground">{t("llm.tools.maxRounds")}</span>
            <span class="text-[0.6rem] text-muted-foreground leading-relaxed">{t("llm.tools.maxRoundsDesc")}</span>
          </div>
          <div class="flex items-center gap-1">
            {#each [1, 3, 5, 10] as val}
              <button
                onclick={async () => { config = { ...config, tools: { ...config.tools, max_rounds: val } }; await saveConfig(); }}
                class="rounded-lg border px-2 py-1 text-[0.64rem] font-semibold transition-all cursor-pointer
                       {config.tools.max_rounds === val
                         ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                         : 'border-border bg-background text-muted-foreground hover:text-foreground'}">
                {val}
              </button>
            {/each}
          </div>
        </div>

        <!-- Max calls per round -->
        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.72rem] font-semibold text-foreground">{t("llm.tools.maxCallsPerRound")}</span>
            <span class="text-[0.6rem] text-muted-foreground leading-relaxed">{t("llm.tools.maxCallsPerRoundDesc")}</span>
          </div>
          <div class="flex items-center gap-1">
            {#each [1, 2, 4, 8] as val}
              <button
                onclick={async () => { config = { ...config, tools: { ...config.tools, max_calls_per_round: val } }; await saveConfig(); }}
                class="rounded-lg border px-2 py-1 text-[0.64rem] font-semibold transition-all cursor-pointer
                       {config.tools.max_calls_per_round === val
                         ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                         : 'border-border bg-background text-muted-foreground hover:text-foreground'}">
                {val}
              </button>
            {/each}
          </div>
        </div>
      </div>

    </CardContent>
  </Card>
</section>
