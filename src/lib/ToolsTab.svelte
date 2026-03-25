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
import { invoke } from "@tauri-apps/api/core";
import { marked } from "marked";
import { onMount } from "svelte";
import { Card, CardContent } from "$lib/components/ui/card";
import { t } from "$lib/i18n/index.svelte";
import SkillsRefreshSection from "$lib/tools/SkillsRefreshSection.svelte";
import SuggestSkillCta from "$lib/tools/SuggestSkillCta.svelte";

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
interface ToolRetryConfig {
  max_retries: number;
  base_delay_ms: number;
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
  retry: ToolRetryConfig;
  web_cache?: {
    enabled: boolean;
    search_ttl_secs: number;
    fetch_ttl_secs: number;
    domain_ttl_overrides: Record<string, number>;
  };
}

interface LlmConfig {
  enabled: boolean;
  autostart: boolean;
  model_path: string | null;
  n_gpu_layers: number;
  ctx_size: number | null;
  parallel: number;
  api_key: string | null;
  tools: LlmToolsConfig;
  mmproj: string | null;
  mmproj_n_threads: number;
  no_mmproj_gpu: boolean;
  autoload_mmproj: boolean;
  verbose: boolean;
}

type LlmToolKey =
  | "date"
  | "location"
  | "web_search"
  | "web_fetch"
  | "bash"
  | "read_file"
  | "write_file"
  | "edit_file"
  | "skill_api";

// ── State ──────────────────────────────────────────────────────────────────

let config = $state<LlmConfig>({
  enabled: false,
  autostart: false,
  model_path: null,
  n_gpu_layers: 4294967295,
  ctx_size: null,
  parallel: 1,
  api_key: null,
  tools: {
    enabled: true,
    date: true,
    location: true,
    web_search: true,
    web_fetch: true,
    web_search_provider: { backend: "duckduckgo", brave_api_key: "", searxng_url: "" },
    bash: false,
    read_file: false,
    write_file: false,
    edit_file: false,
    skill_api: true,
    execution_mode: "parallel" as ToolExecutionMode,
    max_rounds: 15,
    max_calls_per_round: 4,
    context_compression: { level: "normal" as CompressionLevel, max_search_results: 0, max_result_chars: 0 },
    skills_refresh_interval_secs: 86400,
    retry: { max_retries: 2, base_delay_ms: 1000 },
  },
  mmproj: null,
  mmproj_n_threads: 4,
  no_mmproj_gpu: false,
  autoload_mmproj: true,
  verbose: false,
});

let configSaving = $state(false);

// ── Web cache state ────────────────────────────────────────────────────────
interface CacheEntryInfo {
  key: string;
  kind: string;
  domain: string;
  label: string;
  created_at: number;
  ttl_secs: number;
  bytes: number;
}
let cacheStats = $state<{ total_entries: number; expired_entries: number; total_bytes: number }>({
  total_entries: 0,
  expired_entries: 0,
  total_bytes: 0,
});
let cacheEntries = $state<CacheEntryInfo[]>([]);

function fmtBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
}

function fmtAge(unixSecs: number): string {
  const ago = Math.floor(Date.now() / 1000) - unixSecs;
  if (ago < 60) return `${ago}s ago`;
  if (ago < 3600) return `${Math.floor(ago / 60)}m ago`;
  if (ago < 86400) return `${Math.floor(ago / 3600)}h ago`;
  return `${Math.floor(ago / 86400)}d ago`;
}

async function refreshCache() {
  try {
    cacheStats = await invoke<typeof cacheStats>("web_cache_stats");
    cacheEntries = await invoke<CacheEntryInfo[]>("web_cache_list");
  } catch {
    /* cache not initialised yet */
  }
}

async function clearCache() {
  await invoke("web_cache_clear");
  await refreshCache();
}

async function removeDomain(domain: string) {
  await invoke("web_cache_remove_domain", { domain });
  await refreshCache();
}

async function removeEntry(key: string) {
  await invoke("web_cache_remove_entry", { key });
  await refreshCache();
}

let skillsRefreshInterval = $state(86400);
let skillsSyncOnLaunch = $state(false);
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

let TOOL_ROWS = $derived<Array<{ key: LlmToolKey; label: string; desc: string; hint: string; warn?: boolean }>>([
  { key: "date", label: t("llm.tools.date"), desc: t("llm.tools.dateDesc"), hint: t("llm.tools.dateHint") },
  {
    key: "location",
    label: t("llm.tools.location"),
    desc: t("llm.tools.locationDesc"),
    hint: t("llm.tools.locationHint"),
  },
  {
    key: "web_search",
    label: t("llm.tools.webSearch"),
    desc: t("llm.tools.webSearchDesc"),
    hint: t("llm.tools.webSearchHint"),
  },
  {
    key: "web_fetch",
    label: t("llm.tools.webFetch"),
    desc: t("llm.tools.webFetchDesc"),
    hint: t("llm.tools.webFetchHint"),
  },
  { key: "bash", label: t("llm.tools.bash"), desc: t("llm.tools.bashDesc"), hint: t("llm.tools.bashHint"), warn: true },
  {
    key: "read_file",
    label: t("llm.tools.readFile"),
    desc: t("llm.tools.readFileDesc"),
    hint: t("llm.tools.readFileHint"),
  },
  {
    key: "write_file",
    label: t("llm.tools.writeFile"),
    desc: t("llm.tools.writeFileDesc"),
    hint: t("llm.tools.writeFileHint"),
    warn: true,
  },
  {
    key: "edit_file",
    label: t("llm.tools.editFile"),
    desc: t("llm.tools.editFileDesc"),
    hint: t("llm.tools.editFileHint"),
    warn: true,
  },
]);

let hoveredTool = $state<string | null>(null);
let hoveredSkill = $state<string | null>(null);

// ── Data loading ───────────────────────────────────────────────────────────

async function loadConfig() {
  try {
    config = await invoke<LlmConfig>("get_llm_config");
    skillsRefreshInterval = config.tools.skills_refresh_interval_secs ?? 86400;
  } catch (e) {}
}

async function saveConfig() {
  configSaving = true;
  try {
    await invoke("set_llm_config", { config });
  } finally {
    configSaving = false;
  }
}

async function loadSkillsMeta() {
  try {
    skillsRefreshInterval = await invoke<number>("get_skills_refresh_interval");
    skillsSyncOnLaunch = await invoke<boolean>("get_skills_sync_on_launch");
    skillsLastSync = await invoke<number | null>("get_skills_last_sync");
  } catch (e) {}
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
  } catch (e) {
  } finally {
    skillsSyncing = false;
  }
}

function formatLastSync(ts: number | null): string {
  if (ts == null || ts === 0) return t("llm.tools.skillsNeverSynced");
  return new Date(ts * 1000).toLocaleString();
}

async function loadSkills() {
  skillsLoading = true;
  try {
    skills = await invoke<SkillInfo[]>("list_skills");
  } catch {
    skills = [];
  } finally {
    skillsLoading = false;
  }
}

async function loadSkillsLicense() {
  try {
    skillsLicense = (await invoke<string | null>("get_skills_license")) ?? "";
  } catch {
    skillsLicense = "";
  }
}

async function toggleSkill(name: string, enabled: boolean) {
  // Update local state immediately for responsiveness.
  skills = skills.map((s) => (s.name === name ? { ...s, enabled } : s));
  const disabled = skills.filter((s) => !s.enabled).map((s) => s.name);
  await invoke("set_disabled_skills", { names: disabled });
}

async function setAllSkills(enabled: boolean) {
  skills = skills.map((s) => ({ ...s, enabled }));
  const disabled = enabled ? [] : skills.map((s) => s.name);
  await invoke("set_disabled_skills", { names: disabled });
}

// ── Lifecycle ──────────────────────────────────────────────────────────────

onMount(async () => {
  await loadConfig();
  await refreshCache();
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

        <!-- Web cache -->
        {#if config.tools.web_search || config.tools.web_fetch}
          <div class="flex flex-col gap-2.5 rounded-xl border border-border/60 dark:border-white/[0.06]
                      bg-slate-50/60 dark:bg-[#111118] px-3 py-2.5">
            <div class="flex items-center justify-between gap-2">
              <div class="flex flex-col gap-0.5">
                <span class="text-[0.68rem] font-semibold text-foreground">{t("llm.tools.webCache")}</span>
                <span class="text-[0.58rem] text-muted-foreground leading-relaxed">{t("llm.tools.webCacheDesc")}</span>
              </div>
              <button role="switch" aria-checked={config.tools.web_cache?.enabled ?? true}
                aria-label={t("llm.tools.webCache")}
                onclick={async () => {
                  const wc = config.tools.web_cache ?? { enabled: true, search_ttl_secs: 1800, fetch_ttl_secs: 7200, domain_ttl_overrides: {} };
                  config = { ...config, tools: { ...config.tools, web_cache: { ...wc, enabled: !wc.enabled } } };
                  await saveConfig();
                }}
                class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                       border-transparent transition-colors duration-200
                       {(config.tools.web_cache?.enabled ?? true) ? 'bg-blue-500' : 'bg-muted dark:bg-white/10'}">
                <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                              transform transition-transform duration-200
                              {(config.tools.web_cache?.enabled ?? true) ? 'translate-x-4' : 'translate-x-0'}"></span>
              </button>
            </div>

            {#if config.tools.web_cache?.enabled ?? true}
              <!-- TTL controls -->
              <div class="flex gap-3">
                <div class="flex-1 flex flex-col gap-1">
                  <span class="text-[0.6rem] text-muted-foreground">{t("llm.tools.webCacheSearchTtl")}</span>
                  <div class="flex items-center gap-1">
                    {#each [
                      { secs: 300,  label: "5" },
                      { secs: 900,  label: "15" },
                      { secs: 1800, label: "30" },
                      { secs: 3600, label: "60" },
                    ] as opt}
                      <button
                        onclick={async () => {
                          const wc = config.tools.web_cache ?? { enabled: true, search_ttl_secs: 1800, fetch_ttl_secs: 7200, domain_ttl_overrides: {} };
                          config = { ...config, tools: { ...config.tools, web_cache: { ...wc, search_ttl_secs: opt.secs } } };
                          await saveConfig();
                        }}
                        class="rounded-md border px-1.5 py-0.5 text-[0.58rem] font-semibold transition-all cursor-pointer
                               {(config.tools.web_cache?.search_ttl_secs ?? 1800) === opt.secs
                                 ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                                 : 'border-border bg-background text-muted-foreground hover:text-foreground'}">
                        {opt.label}{t("llm.tools.webCacheTtlMin")}
                      </button>
                    {/each}
                  </div>
                </div>
                <div class="flex-1 flex flex-col gap-1">
                  <span class="text-[0.6rem] text-muted-foreground">{t("llm.tools.webCacheFetchTtl")}</span>
                  <div class="flex items-center gap-1">
                    {#each [
                      { secs: 1800,  label: "30" },
                      { secs: 3600,  label: "60" },
                      { secs: 7200,  label: "120" },
                      { secs: 14400, label: "240" },
                    ] as opt}
                      <button
                        onclick={async () => {
                          const wc = config.tools.web_cache ?? { enabled: true, search_ttl_secs: 1800, fetch_ttl_secs: 7200, domain_ttl_overrides: {} };
                          config = { ...config, tools: { ...config.tools, web_cache: { ...wc, fetch_ttl_secs: opt.secs } } };
                          await saveConfig();
                        }}
                        class="rounded-md border px-1.5 py-0.5 text-[0.58rem] font-semibold transition-all cursor-pointer
                               {(config.tools.web_cache?.fetch_ttl_secs ?? 7200) === opt.secs
                                 ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                                 : 'border-border bg-background text-muted-foreground hover:text-foreground'}">
                        {opt.label}{t("llm.tools.webCacheTtlMin")}
                      </button>
                    {/each}
                  </div>
                </div>
              </div>

              <!-- Stats + clear + entries list -->
              <div class="flex flex-col gap-2 pt-1">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-[0.58rem] text-muted-foreground">
                    {#if cacheStats.total_entries > 0}
                      {t("llm.tools.webCacheEntries").replace("{n}", String(cacheStats.total_entries))}
                      <span class="ml-1 opacity-60">
                        ({t("llm.tools.webCacheSize").replace("{size}", fmtBytes(cacheStats.total_bytes))})
                      </span>
                    {:else}
                      {t("llm.tools.webCacheEmpty")}
                    {/if}
                  </span>
                  <div class="flex items-center gap-1">
                    <button onclick={refreshCache}
                      class="rounded-md border border-border px-1.5 py-0.5 text-[0.54rem] font-semibold
                             text-muted-foreground hover:text-foreground transition-colors cursor-pointer bg-background">
                      ↻
                    </button>
                    {#if cacheStats.total_entries > 0}
                      <button onclick={clearCache}
                        class="rounded-md border border-red-500/30 bg-red-500/5 px-2 py-0.5 text-[0.54rem] font-semibold
                               text-red-600 dark:text-red-400 hover:bg-red-500/10 transition-colors cursor-pointer">
                        {t("llm.tools.webCacheClearAll")}
                      </button>
                    {/if}
                  </div>
                </div>

                <!-- Entry list (collapsible) -->
                {#if cacheEntries.length > 0}
                  <div class="flex flex-col gap-1 max-h-44 overflow-y-auto rounded-lg border border-border/40
                              dark:border-white/[0.04] bg-white dark:bg-[#0c0c14] p-1.5">
                    {#each cacheEntries as entry}
                      <div class="flex items-center justify-between gap-2 rounded-md px-2 py-1
                                  hover:bg-muted/50 dark:hover:bg-white/[0.03] group">
                        <div class="flex flex-col gap-0 min-w-0">
                          <div class="flex items-center gap-1.5">
                            <span class="text-[0.48rem] font-semibold rounded-full border px-1 py-0
                                         {entry.kind === 'search'
                                           ? 'border-blue-500/30 bg-blue-500/10 text-blue-600 dark:text-blue-400'
                                           : 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'}">
                              {entry.kind === 'search' ? t("llm.tools.webCacheSearch") : t("llm.tools.webCacheFetch")}
                            </span>
                            <span class="text-[0.56rem] text-foreground truncate">{entry.label}</span>
                          </div>
                          <div class="flex items-center gap-2 text-[0.48rem] text-muted-foreground/60">
                            <span>{entry.domain}</span>
                            <span>{fmtBytes(entry.bytes)}</span>
                            <span>{fmtAge(entry.created_at)}</span>
                          </div>
                        </div>
                        <div class="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                          <button onclick={() => removeDomain(entry.domain)}
                            title={t("llm.tools.webCacheRemoveDomain")}
                            class="rounded border border-border px-1 py-0.5 text-[0.48rem] text-muted-foreground
                                   hover:text-foreground hover:bg-muted transition-colors cursor-pointer bg-background">
                            {entry.domain} ✕
                          </button>
                          <button onclick={() => removeEntry(entry.key)}
                            title={t("llm.tools.webCacheRemoveEntry")}
                            class="rounded border border-red-500/30 px-1 py-0.5 text-[0.48rem]
                                   text-red-500 hover:bg-red-500/10 transition-colors cursor-pointer bg-background">
                            ✕
                          </button>
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
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
            {#each [1, 3, 5, 10, 15] as val}
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

        <!-- Network retry settings -->
        <div class="flex flex-col gap-3 pt-2">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.72rem] font-semibold text-foreground">{t("llm.tools.retrySection")}</span>
            <span class="text-[0.6rem] text-muted-foreground leading-relaxed">{t("llm.tools.retrySectionDesc")}</span>
          </div>

          <div class="flex gap-3">
            <!-- Max retries -->
            <div class="flex-1 flex flex-col gap-1">
              <label for="retry-max" class="text-[0.62rem] text-muted-foreground">
                {t("llm.tools.retryMaxRetries")}
              </label>
              <div class="flex items-center gap-1">
                {#each [0, 1, 2, 3] as val}
                  <button
                    onclick={async () => { config = { ...config, tools: { ...config.tools, retry: { ...config.tools.retry, max_retries: val } } }; await saveConfig(); }}
                    class="rounded-lg border px-2 py-1 text-[0.64rem] font-semibold transition-all cursor-pointer
                           {config.tools.retry.max_retries === val
                             ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                             : 'border-border bg-background text-muted-foreground hover:text-foreground'}">
                    {val}
                  </button>
                {/each}
              </div>
              <span class="text-[0.54rem] text-muted-foreground/60">{t("llm.tools.retryMaxRetriesDesc")}</span>
            </div>
            <!-- Base delay -->
            <div class="flex-1 flex flex-col gap-1">
              <label for="retry-delay" class="text-[0.62rem] text-muted-foreground">
                {t("llm.tools.retryBaseDelay")}
              </label>
              <div class="flex items-center gap-1">
                {#each [500, 1000, 2000, 3000] as val}
                  <button
                    onclick={async () => { config = { ...config, tools: { ...config.tools, retry: { ...config.tools.retry, base_delay_ms: val } } }; await saveConfig(); }}
                    class="rounded-lg border px-2 py-1 text-[0.64rem] font-semibold transition-all cursor-pointer
                           {config.tools.retry.base_delay_ms === val
                             ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                             : 'border-border bg-background text-muted-foreground hover:text-foreground'}">
                    {val}{t("llm.tools.retryMs")}
                  </button>
                {/each}
              </div>
              <span class="text-[0.54rem] text-muted-foreground/60">{t("llm.tools.retryBaseDelayDesc")}</span>
            </div>
          </div>
        </div>
      </div>

    </CardContent>
  </Card>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Suggest a skill CTA                                                         -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<SuggestSkillCta />

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
<SkillsRefreshSection
  skillsRefreshInterval={skillsRefreshInterval}
  skillsSyncOnLaunch={skillsSyncOnLaunch}
  skillsSyncing={skillsSyncing}
  skillsLastSync={skillsLastSync}
  {formatLastSync}
  onSetSkillsInterval={setSkillsInterval}
  onToggleSyncOnLaunch={async () => {
    skillsSyncOnLaunch = !skillsSyncOnLaunch;
    await invoke("set_skills_sync_on_launch", { enabled: skillsSyncOnLaunch });
  }}
  onSyncNow={syncNow}
/>

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
