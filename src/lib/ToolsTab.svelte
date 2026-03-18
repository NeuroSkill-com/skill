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
  import { marked }                   from "marked";
  import { Card, CardContent }        from "$lib/components/ui/card";
  import { t }                        from "$lib/i18n/index.svelte";

  /** Render inline markdown (bold, italic, code, links) — no block elements. */
  function inlineMd(src: string): string {
    return marked.parseInline(src, { gfm: true }) as string;
  }

  // ── Types ──────────────────────────────────────────────────────────────────

  type ToolExecutionMode = "sequential" | "parallel";
  type SearchBackend = "duckduckgo" | "brave" | "searxng";
  type CompressionLevel = "off" | "normal" | "aggressive";
  interface WebSearchProvider {
    backend: SearchBackend;
    brave_api_key: string;
    searxng_url: string;
  }
  interface ToolContextCompression {
    level: CompressionLevel;
    max_search_results: number;
    max_result_chars: number;
  }
  interface LlmToolsConfig {
    enabled: boolean;
    date: boolean;
    location: boolean;
    web_search: boolean;
    web_fetch: boolean;
    web_search_provider: WebSearchProvider;
    bash: boolean;
    read_file: boolean;
    write_file: boolean;
    edit_file: boolean;
    skill_api: boolean;
    execution_mode: ToolExecutionMode;
    max_rounds: number;
    max_calls_per_round: number;
    context_compression: ToolContextCompression;
    skills_refresh_interval_secs: number;
  }

  interface LlmConfig {
    enabled: boolean; autostart: boolean; model_path: string | null; n_gpu_layers: number;
    ctx_size: number | null; parallel: number; api_key: string | null;
    tools: LlmToolsConfig;
    mmproj: string | null; mmproj_n_threads: number; no_mmproj_gpu: boolean;
    autoload_mmproj: boolean; verbose: boolean;
  }

  type LlmToolKey = "date" | "location" | "web_search" | "web_fetch" | "bash" | "read_file" | "write_file" | "edit_file" | "skill_api";

  // ── State ──────────────────────────────────────────────────────────────────

  let config  = $state<LlmConfig>({
    enabled: false, autostart: false, model_path: null, n_gpu_layers: 4294967295,
    ctx_size: null, parallel: 1, api_key: null,
    tools: { enabled: true, date: true, location: true, web_search: true, web_fetch: true, web_search_provider: { backend: "duckduckgo", brave_api_key: "", searxng_url: "" }, bash: false, read_file: false, write_file: false, edit_file: false, skill_api: true, execution_mode: "parallel" as ToolExecutionMode, max_rounds: 10, max_calls_per_round: 4, context_compression: { level: "normal" as CompressionLevel, max_search_results: 0, max_result_chars: 0 }, skills_refresh_interval_secs: 86400 },
    mmproj: null, mmproj_n_threads: 4, no_mmproj_gpu: false, autoload_mmproj: true,
    verbose: false,
  });

  let configSaving = $state(false);
  let skillsRefreshInterval = $state(86400);
  let skillsLastSync = $state<number | null>(null);
  let skillsSyncing = $state(false);

  interface SkillInfo {
    name: string;
    description: string;
    source: string;
    enabled: boolean;
  }
  let skills = $state<SkillInfo[]>([]);
  let skillsLoading = $state(false);
  let skillsLicense = $state("");
  let skillsLicenseOpen = $state(false);

  let TOOL_ROWS = $derived<Array<{ key: LlmToolKey; label: string; desc: string; hint: string; warn?: boolean }>>(
    [
      { key: "date",       label: t("llm.tools.date"),      desc: t("llm.tools.dateDesc"),      hint: t("llm.tools.dateHint") },
      { key: "location",   label: t("llm.tools.location"),  desc: t("llm.tools.locationDesc"),  hint: t("llm.tools.locationHint") },
      { key: "web_search", label: t("llm.tools.webSearch"), desc: t("llm.tools.webSearchDesc"), hint: t("llm.tools.webSearchHint") },
      { key: "web_fetch",  label: t("llm.tools.webFetch"),  desc: t("llm.tools.webFetchDesc"),  hint: t("llm.tools.webFetchHint") },
      { key: "bash",       label: t("llm.tools.bash"),      desc: t("llm.tools.bashDesc"),      hint: t("llm.tools.bashHint"),     warn: true },
      { key: "read_file",  label: t("llm.tools.readFile"),  desc: t("llm.tools.readFileDesc"),  hint: t("llm.tools.readFileHint") },
      { key: "write_file", label: t("llm.tools.writeFile"), desc: t("llm.tools.writeFileDesc"), hint: t("llm.tools.writeFileHint"), warn: true },
      { key: "edit_file",  label: t("llm.tools.editFile"),  desc: t("llm.tools.editFileDesc"),  hint: t("llm.tools.editFileHint"),  warn: true },
    ]
  );

  let hoveredTool = $state<string | null>(null);
  let hoveredSkill = $state<string | null>(null);

  // ── Data loading ───────────────────────────────────────────────────────────

  async function loadConfig() {
    try {
      config = await invoke<LlmConfig>("get_llm_config");
      skillsRefreshInterval = config.tools.skills_refresh_interval_secs ?? 86400;
    } catch {}
  }

  async function saveConfig() {
    configSaving = true;
    try { await invoke("set_llm_config", { config }); }
    finally { configSaving = false; }
  }

  async function loadSkillsMeta() {
    try {
      skillsRefreshInterval = await invoke<number>("get_skills_refresh_interval");
      skillsLastSync = await invoke<number | null>("get_skills_last_sync");
    } catch {}
  }

  async function setSkillsInterval(secs: number) {
    skillsRefreshInterval = secs;
    config = { ...config, tools: { ...config.tools, skills_refresh_interval_secs: secs } };
    await invoke("set_skills_refresh_interval", { secs });
    await saveConfig();
  }

  async function syncNow() {
    skillsSyncing = true;
    try {
      await invoke("sync_skills_now");
      await loadSkillsMeta();
      await loadSkills();
    } catch {}
    finally { skillsSyncing = false; }
  }

  function formatLastSync(ts: number | null): string {
    if (ts == null || ts === 0) return t("llm.tools.skillsNeverSynced");
    return new Date(ts * 1000).toLocaleString();
  }

  async function loadSkills() {
    skillsLoading = true;
    try { skills = await invoke<SkillInfo[]>("list_skills"); }
    catch { skills = []; }
    finally { skillsLoading = false; }
  }

  async function loadSkillsLicense() {
    try { skillsLicense = await invoke<string | null>("get_skills_license") ?? ""; }
    catch { skillsLicense = ""; }
  }

  async function toggleSkill(name: string, enabled: boolean) {
    // Update local state immediately for responsiveness.
    skills = skills.map(s => s.name === name ? { ...s, enabled } : s);
    const disabled = skills.filter(s => !s.enabled).map(s => s.name);
    await invoke("set_disabled_skills", { names: disabled });
  }

  async function setAllSkills(enabled: boolean) {
    skills = skills.map(s => ({ ...s, enabled }));
    const disabled = enabled ? [] : skills.map(s => s.name);
    await invoke("set_disabled_skills", { names: disabled });
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  onMount(async () => {
    await loadConfig();
    await loadSkillsMeta();
    await loadSkills();
    await loadSkillsLicense();
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
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="relative rounded-xl border
                      {tool.warn && config.tools[tool.key]
                        ? 'border-amber-500/40 bg-amber-50/40 dark:bg-amber-950/15'
                        : 'border-border/60 dark:border-white/[0.06] bg-slate-50/60 dark:bg-[#111118]'}"
               onmouseenter={() => hoveredTool = tool.key}
               onmouseleave={() => hoveredTool = null}>
            <div class="flex items-center justify-between gap-4 px-3 py-2.5">
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

            <!-- Hover overlay -->
            {#if hoveredTool === tool.key}
              <div class="px-3 pb-2.5 flex flex-col gap-1.5 animate-in fade-in duration-150">
                <div class="border-t {tool.warn ? 'border-amber-500/20' : 'border-border/40 dark:border-white/[0.04]'}"></div>
                <p class="text-[0.58rem] leading-relaxed text-muted-foreground/80">{tool.hint}</p>
                {#if tool.warn}
                  <p class="text-[0.54rem] leading-relaxed text-amber-600/80 dark:text-amber-400/70 italic">
                    {t("llm.tools.advancedHint")}
                  </p>
                {/if}
              </div>
            {/if}
          </div>
        {/each}

        <!-- Search provider -->
        {#if config.tools.web_search}
          <div class="flex flex-col gap-2.5 rounded-xl border border-border/60 dark:border-white/[0.06]
                      bg-slate-50/60 dark:bg-[#111118] px-3 py-2.5">
            <div class="flex flex-col gap-1">
              <span class="text-[0.68rem] font-semibold text-foreground">{t("llm.tools.searchProvider")}</span>
              <span class="text-[0.6rem] text-muted-foreground leading-relaxed">{t("llm.tools.searchProviderDesc")}</span>
            </div>

            <!-- Backend selector -->
            <div class="flex rounded-lg overflow-hidden border border-border text-[0.66rem] font-medium">
              {#each [
                { key: "duckduckgo" as SearchBackend, label: "DuckDuckGo" },
                { key: "brave"     as SearchBackend, label: "Brave" },
                { key: "searxng"   as SearchBackend, label: "SearXNG" },
              ] as opt}
                <button
                  onclick={async () => {
                    config = { ...config, tools: { ...config.tools, web_search_provider: { ...config.tools.web_search_provider, backend: opt.key } } };
                    await saveConfig();
                  }}
                  class="flex-1 py-1.5 transition-colors cursor-pointer
                         {config.tools.web_search_provider.backend === opt.key
                           ? 'bg-primary text-primary-foreground'
                           : 'bg-background text-muted-foreground hover:bg-muted'}">
                  {opt.label}
                </button>
              {/each}
            </div>

            <!-- Brave API key -->
            {#if config.tools.web_search_provider.backend === "brave"}
              <div class="flex flex-col gap-1">
                <label for="brave-api-key" class="text-[0.64rem] font-semibold text-foreground">
                  {t("llm.tools.braveApiKey")}
                </label>
                <span class="text-[0.58rem] text-muted-foreground leading-relaxed">{t("llm.tools.braveApiKeyDesc")}</span>
                <input id="brave-api-key" type="password" autocomplete="off" placeholder="BSA..."
                  value={config.tools.web_search_provider.brave_api_key ?? ""}
                  oninput={(e: Event) => {
                    const val = (e.target as HTMLInputElement).value;
                    config = { ...config, tools: { ...config.tools, web_search_provider: { ...config.tools.web_search_provider, brave_api_key: val } } };
                  }}
                  onchange={async () => { await saveConfig(); }}
                  class="mt-0.5 w-full rounded-lg border border-border/60 dark:border-white/[0.08]
                         bg-white dark:bg-[#0c0c14] px-2.5 py-1.5 text-[0.7rem] text-foreground
                         placeholder:text-muted-foreground/50 outline-none focus:ring-1 focus:ring-blue-500/50" />
              </div>
            {/if}

            <!-- SearXNG URL -->
            {#if config.tools.web_search_provider.backend === "searxng"}
              <div class="flex flex-col gap-1">
                <label for="searxng-url" class="text-[0.64rem] font-semibold text-foreground">
                  {t("llm.tools.searxngUrl")}
                </label>
                <span class="text-[0.58rem] text-muted-foreground leading-relaxed">{t("llm.tools.searxngUrlDesc")}</span>
                <input id="searxng-url" type="text" placeholder="https://search.example.com"
                  value={config.tools.web_search_provider.searxng_url ?? ""}
                  oninput={(e: Event) => {
                    const val = (e.target as HTMLInputElement).value;
                    config = { ...config, tools: { ...config.tools, web_search_provider: { ...config.tools.web_search_provider, searxng_url: val } } };
                  }}
                  onchange={async () => { await saveConfig(); }}
                  class="mt-0.5 w-full rounded-lg border border-border/60 dark:border-white/[0.08]
                         bg-white dark:bg-[#0c0c14] px-2.5 py-1.5 text-[0.7rem] text-foreground
                         placeholder:text-muted-foreground/50 outline-none focus:ring-1 focus:ring-blue-500/50" />
              </div>
            {/if}
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

        <!-- Context compression -->
        <div class="flex flex-col gap-2.5 pt-1">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.72rem] font-semibold text-foreground">{t("llm.tools.contextCompression")}</span>
            <span class="text-[0.6rem] text-muted-foreground leading-relaxed">{t("llm.tools.contextCompressionDesc")}</span>
          </div>
          <div class="flex rounded-lg overflow-hidden border border-border text-[0.66rem] font-medium">
            {#each [
              { key: "off"        as CompressionLevel, label: t("llm.tools.compressionOff") },
              { key: "normal"     as CompressionLevel, label: t("llm.tools.compressionNormal") },
              { key: "aggressive" as CompressionLevel, label: t("llm.tools.compressionAggressive") },
            ] as opt}
              <button
                onclick={async () => {
                  config = { ...config, tools: { ...config.tools, context_compression: { ...config.tools.context_compression, level: opt.key } } };
                  await saveConfig();
                }}
                class="flex-1 py-1.5 transition-colors cursor-pointer
                       {config.tools.context_compression.level === opt.key
                         ? 'bg-primary text-primary-foreground'
                         : 'bg-background text-muted-foreground hover:bg-muted'}">
                {opt.label}
              </button>
            {/each}
          </div>

          <!-- Custom overrides (shown when not "off") -->
          {#if config.tools.context_compression.level !== "off"}
            <div class="flex gap-3">
              <!-- Max search results -->
              <div class="flex-1 flex flex-col gap-1">
                <label for="comp-max-results" class="text-[0.62rem] text-muted-foreground">
                  {t("llm.tools.maxSearchResults")}
                </label>
                <input id="comp-max-results" type="number" min="0" max="20" step="1"
                  value={config.tools.context_compression.max_search_results}
                  oninput={(e: Event) => {
                    const val = parseInt((e.target as HTMLInputElement).value) || 0;
                    config = { ...config, tools: { ...config.tools, context_compression: { ...config.tools.context_compression, max_search_results: Math.max(0, Math.min(20, val)) } } };
                  }}
                  onchange={async () => { await saveConfig(); }}
                  class="w-full rounded-lg border border-border/60 dark:border-white/[0.08]
                         bg-white dark:bg-[#0c0c14] px-2.5 py-1.5 text-[0.7rem] text-foreground
                         placeholder:text-muted-foreground/50 outline-none focus:ring-1 focus:ring-blue-500/50" />
                <span class="text-[0.54rem] text-muted-foreground/60">{t("llm.tools.zeroAutoLabel")}</span>
              </div>
              <!-- Max result chars -->
              <div class="flex-1 flex flex-col gap-1">
                <label for="comp-max-chars" class="text-[0.62rem] text-muted-foreground">
                  {t("llm.tools.maxResultChars")}
                </label>
                <input id="comp-max-chars" type="number" min="0" max="32000" step="500"
                  value={config.tools.context_compression.max_result_chars}
                  oninput={(e: Event) => {
                    const val = parseInt((e.target as HTMLInputElement).value) || 0;
                    config = { ...config, tools: { ...config.tools, context_compression: { ...config.tools.context_compression, max_result_chars: Math.max(0, Math.min(32000, val)) } } };
                  }}
                  onchange={async () => { await saveConfig(); }}
                  class="w-full rounded-lg border border-border/60 dark:border-white/[0.08]
                         bg-white dark:bg-[#0c0c14] px-2.5 py-1.5 text-[0.7rem] text-foreground
                         placeholder:text-muted-foreground/50 outline-none focus:ring-1 focus:ring-blue-500/50" />
                <span class="text-[0.54rem] text-muted-foreground/60">{t("llm.tools.zeroAutoLabel")}</span>
              </div>
            </div>
          {/if}
        </div>
      </div>

    </CardContent>
  </Card>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Suggest a skill CTA                                                         -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section>
  <a href="https://discord.gg/Rcvb8Cx4cZ" target="_blank" rel="noopener noreferrer"
     class="flex items-center gap-3 rounded-xl border border-primary/30 bg-primary/10
            hover:bg-primary/15 transition-colors px-4 py-3 group cursor-pointer no-underline">
    <svg class="w-5 h-5 text-primary-foreground/90 bg-primary rounded-md p-0.5 shrink-0" viewBox="0 0 24 24" fill="currentColor">
      <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028 14.09 14.09 0 0 0 1.226-1.994.076.076 0 0 0-.041-.106 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.095 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.095 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z"/>
    </svg>
    <div class="flex flex-col gap-0.5">
      <span class="text-[0.72rem] font-semibold text-primary group-hover:text-primary/90">
        {t("llm.tools.suggestSkill")}
      </span>
      <span class="text-[0.6rem] text-primary/60 leading-relaxed">
        {t("llm.tools.suggestSkillDesc")}
      </span>
    </div>
    <svg class="w-4 h-4 text-primary/40 ml-auto shrink-0" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
      <polyline points="15 3 21 3 21 9"/>
      <line x1="10" y1="14" x2="21" y2="3"/>
    </svg>
  </a>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Agent Skills                                                                -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.tools.skillsSection")}
    </span>
    <span class="text-[0.52rem] text-muted-foreground/50">
      {skills.filter(s => s.enabled).length}/{skills.length}
    </span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col py-0 px-0">

      <!-- Description + license toggle + bulk actions -->
      <div class="flex flex-col gap-1 px-4 pt-3.5 pb-2">
        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-1">
            <p class="text-[0.65rem] text-muted-foreground leading-relaxed">
              {t("llm.tools.skillsSectionDesc")}
            </p>
            {#if skillsLicense}
              <button
                onclick={() => skillsLicenseOpen = !skillsLicenseOpen}
                class="flex items-center gap-1 text-[0.58rem] font-semibold text-primary
                       hover:text-primary/80 transition-colors cursor-pointer self-start">
                <svg class="w-3 h-3 transition-transform {skillsLicenseOpen ? 'rotate-90' : ''}"
                     viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                     stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="9 18 15 12 9 6"/>
                </svg>
                <span class="text-primary font-bold">AI100</span> {t("llm.tools.skillsLicenseLabel")}
              </button>
            {/if}
          </div>
        {#if skills.length > 0}
          <div class="flex items-center gap-1 shrink-0">
            <button onclick={() => setAllSkills(true)}
              class="rounded-md border border-border px-2 py-0.5 text-[0.56rem] font-semibold
                     text-muted-foreground hover:text-foreground transition-colors cursor-pointer bg-background">
              {t("llm.tools.skillsEnableAll")}
            </button>
            <button onclick={() => setAllSkills(false)}
              class="rounded-md border border-border px-2 py-0.5 text-[0.56rem] font-semibold
                     text-muted-foreground hover:text-foreground transition-colors cursor-pointer bg-background">
              {t("llm.tools.skillsDisableAll")}
            </button>
          </div>
        {/if}
        </div>

        <!-- Collapsible license -->
        {#if skillsLicenseOpen && skillsLicense}
          <div class="mt-1 rounded-lg border border-primary/20 bg-primary/[0.04] px-3 py-2.5
                      max-h-48 overflow-y-auto">
            <pre class="text-[0.54rem] leading-relaxed text-muted-foreground whitespace-pre-wrap font-sans">{@html skillsLicense.replace(/AI100/g, '<span class="text-primary font-semibold">AI100</span>')}</pre>
          </div>
        {/if}
      </div>

      <!-- Skills list -->
      <div class="flex flex-col gap-2 px-4 pb-3">
        {#if skillsLoading}
          <p class="text-[0.62rem] text-muted-foreground py-2">{t("llm.tools.skillsLoading")}</p>
        {:else if skills.length === 0}
          <p class="text-[0.62rem] text-muted-foreground py-2">{t("llm.tools.skillsNone")}</p>
        {:else}
          {#each skills as skill}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="relative rounded-xl border
                        border-border/60 dark:border-white/[0.06]
                        {skill.enabled
                          ? 'bg-slate-50/60 dark:bg-[#111118]'
                          : 'bg-slate-50/30 dark:bg-[#111118]/50 opacity-60'}"
                 onmouseenter={() => hoveredSkill = skill.name}
                 onmouseleave={() => hoveredSkill = null}>
              <div class="flex items-center justify-between gap-3 px-3 py-2.5">
                <div class="flex flex-col gap-0.5 min-w-0">
                  <div class="flex items-center gap-1.5">
                    <span class="text-[0.72rem] font-semibold text-foreground truncate">{skill.name}</span>
                    <span class="text-[0.48rem] font-medium rounded-full border px-1.5 py-0
                                 border-border/50 text-muted-foreground/60 shrink-0">
                      {skill.source}
                    </span>
                  </div>
                  <span class="text-[0.6rem] text-muted-foreground leading-relaxed skill-desc
                               {hoveredSkill === skill.name ? '' : 'line-clamp-2'}">{@html inlineMd(skill.description)}</span>
                </div>
                <button role="switch" aria-checked={skill.enabled} aria-label={skill.name}
                  onclick={() => toggleSkill(skill.name, !skill.enabled)}
                  class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                         border-transparent transition-colors duration-200 mt-0.5
                         {skill.enabled ? 'bg-blue-500' : 'bg-muted dark:bg-white/10'}">
                  <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                                transform transition-transform duration-200
                                {skill.enabled ? 'translate-x-4' : 'translate-x-0'}"></span>
                </button>
              </div>
            </div>
          {/each}
        {/if}
      </div>

    </CardContent>
  </Card>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Skills auto-refresh                                                         -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.tools.skillsRefresh")}
    </span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col py-0 px-0">

      <!-- Description -->
      <div class="px-4 pt-3.5 pb-2">
        <p class="text-[0.65rem] text-muted-foreground leading-relaxed">
          {t("llm.tools.skillsRefreshDesc")}
        </p>
      </div>

      <!-- Interval selector -->
      <div class="flex flex-col gap-3 px-4 pb-3">
        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.72rem] font-semibold text-foreground">{t("llm.tools.skillsRefresh")}</span>
          </div>
          <div class="flex items-center gap-1">
            {#each [
              { secs: 0,      label: t("llm.tools.skillsRefreshOff") },
              { secs: 43200,  label: t("llm.tools.skillsRefresh12h") },
              { secs: 86400,  label: t("llm.tools.skillsRefresh24h") },
              { secs: 604800, label: t("llm.tools.skillsRefresh7d") },
            ] as opt}
              <button
                onclick={() => setSkillsInterval(opt.secs)}
                class="rounded-lg border px-2 py-1 text-[0.64rem] font-semibold transition-all cursor-pointer
                       {skillsRefreshInterval === opt.secs
                         ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                         : 'border-border bg-background text-muted-foreground hover:text-foreground'}">
                {opt.label}
              </button>
            {/each}
          </div>
        </div>

        <!-- Last sync + manual sync button -->
        <div class="flex items-center justify-between gap-4 pt-1">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.62rem] text-muted-foreground">
              {t("llm.tools.skillsLastSync")}: {formatLastSync(skillsLastSync)}
            </span>
          </div>
          <button
            onclick={syncNow}
            disabled={skillsSyncing}
            class="rounded-lg border border-border px-3 py-1.5 text-[0.64rem] font-semibold
                   transition-all cursor-pointer bg-background text-foreground
                   hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed">
            {skillsSyncing ? t("llm.tools.skillsSyncing") : t("llm.tools.skillsSyncNow")}
          </button>
        </div>
      </div>

    </CardContent>
  </Card>
</section>

<style>
  :global(.skill-desc code) {
    font-size: 0.58rem;
    padding: 0.05rem 0.3rem;
    border-radius: 0.25rem;
    background: var(--color-muted, oklch(0.96 0 0));
  }
  :global(.skill-desc a) {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  :global(.skill-desc strong) {
    font-weight: 600;
    color: var(--color-foreground);
  }
</style>
