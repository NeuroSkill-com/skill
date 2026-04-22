<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Card, CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { t } from "$lib/i18n/index.svelte";

type ToolExecutionMode = "sequential" | "parallel";
type SearchBackend = "duckduckgo" | "brave" | "searxng";
type CompressionLevel = "off" | "normal" | "aggressive";

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

interface ToolRow {
  key: LlmToolKey;
  label: string;
  desc: string;
  hint: string;
  warn?: boolean;
}

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
interface LlmConfigView {
  tools: LlmToolsConfig;
}

interface CacheEntryInfo {
  key: string;
  kind: string;
  domain: string;
  label: string;
  created_at: number;
  ttl_secs: number;
  bytes: number;
}

interface Props {
  config: LlmConfigView;
  configSaving: boolean;
  toolRows: ToolRow[];
  cacheStats: { total_entries: number; expired_entries: number; total_bytes: number };
  cacheEntries: CacheEntryInfo[];
  onConfigChange: (next: LlmConfigView) => void | Promise<void>;
  onRefreshCache: () => void | Promise<void>;
  onClearCache: () => void | Promise<void>;
  onRemoveDomain: (domain: string) => void | Promise<void>;
  onRemoveEntry: (key: string) => void | Promise<void>;
}

let {
  config,
  configSaving,
  toolRows,
  cacheStats,
  cacheEntries,
  onConfigChange,
  onRefreshCache,
  onClearCache,
  onRemoveDomain,
  onRemoveEntry,
}: Props = $props();

let hoveredTool = $state<string | null>(null);

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
</script>

<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <SectionHeader>{t("llm.tools.section")}</SectionHeader>
    <span class="text-ui-xs text-muted-foreground/50">{config.tools.enabled ? toolRows.filter((r) => config.tools[r.key]).length + "/" + toolRows.length : "off"}</span>
    {#if configSaving}<span class="text-ui-xs text-muted-foreground ml-auto">saving…</span>{/if}
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col py-0 px-0">
      <div class="flex items-center justify-between gap-4 px-4 pt-3.5 pb-2">
        <p class="text-ui-base text-muted-foreground leading-relaxed">{t("llm.tools.sectionDesc")}</p>
        <button role="switch" aria-checked={config.tools.enabled} aria-label={t("llm.tools.enableAll")}
          onclick={async () => await onConfigChange({ ...config, tools: { ...config.tools, enabled: !config.tools.enabled } })}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 {config.tools.enabled ? 'bg-violet-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md transform transition-transform duration-200 {config.tools.enabled ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

      <div class="flex flex-col gap-2 px-4 pb-3 {config.tools.enabled ? '' : 'opacity-40 pointer-events-none'}">
        {#each toolRows as tool}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="relative rounded-xl border {tool.warn && config.tools[tool.key] ? 'border-amber-500/40 bg-amber-50/40 dark:bg-amber-950/15' : 'border-border/60 dark:border-white/[0.06] bg-surface-3'}"
               onmouseenter={() => (hoveredTool = tool.key)}
               onmouseleave={() => (hoveredTool = null)}>
            <div class="flex items-center justify-between gap-4 px-3 py-2.5">
              <div class="flex flex-col gap-0.5">
                <div class="flex items-center gap-1.5">
                  <span class="text-ui-md font-semibold text-foreground">{tool.label}</span>
                  {#if tool.warn}
                    <span class="text-ui-2xs font-semibold rounded-full border px-1.5 py-0 border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400">{t("llm.tools.advanced")}</span>
                  {/if}
                </div>
                <span class="text-ui-sm text-muted-foreground leading-relaxed">{tool.desc}</span>
              </div>
              <button role="switch" aria-checked={config.tools[tool.key]} aria-label={tool.label}
                onclick={async () => await onConfigChange({ ...config, tools: { ...config.tools, [tool.key]: !config.tools[tool.key] } })}
                class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 {config.tools[tool.key] ? (tool.warn ? 'bg-amber-500' : 'bg-violet-500') : 'bg-muted dark:bg-white/10'}">
                <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md transform transition-transform duration-200 {config.tools[tool.key] ? 'translate-x-4' : 'translate-x-0'}"></span>
              </button>
            </div>
            {#if hoveredTool === tool.key}
              <div class="px-3 pb-2.5 flex flex-col gap-1.5 animate-in fade-in duration-150">
                <div class="border-t {tool.warn ? 'border-amber-500/20' : 'border-border/40 dark:border-white/[0.04]'}"></div>
                <p class="text-ui-sm leading-relaxed text-muted-foreground/80">{tool.hint}</p>
                {#if tool.warn}<p class="text-ui-xs leading-relaxed text-amber-600/80 dark:text-amber-400/70 italic">{t("llm.tools.advancedHint")}</p>{/if}
              </div>
            {/if}
          </div>
        {/each}

        {#if config.tools.web_search}
          <div class="flex flex-col gap-2.5 rounded-xl border border-border/60 dark:border-white/[0.06] bg-surface-3 px-3 py-2.5">
            <div class="flex flex-col gap-1">
              <span class="text-ui-base font-semibold text-foreground">{t("llm.tools.searchProvider")}</span>
              <span class="text-ui-sm text-muted-foreground leading-relaxed">{t("llm.tools.searchProviderDesc")}</span>
            </div>

            <div class="flex rounded-lg overflow-hidden border border-border text-ui-base font-medium">
              {#each [
                { key: "duckduckgo" as SearchBackend, label: "DuckDuckGo" },
                { key: "brave" as SearchBackend, label: "Brave" },
                { key: "searxng" as SearchBackend, label: "SearXNG" },
              ] as opt}
                <button onclick={async () => await onConfigChange({ ...config, tools: { ...config.tools, web_search_provider: { ...config.tools.web_search_provider, backend: opt.key } } })}
                  class="flex-1 py-1.5 transition-colors cursor-pointer {config.tools.web_search_provider.backend === opt.key ? 'bg-violet-600 text-white' : 'bg-background text-muted-foreground hover:bg-muted'}">{opt.label}</button>
              {/each}
            </div>

            {#if config.tools.web_search_provider.backend === "brave"}
              <div class="flex flex-col gap-1">
                <label for="brave-api-key" class="text-ui-base font-semibold text-foreground">{t("llm.tools.braveApiKey")}</label>
                <span class="text-ui-sm text-muted-foreground leading-relaxed">{t("llm.tools.braveApiKeyDesc")}</span>
                <input id="brave-api-key" type="password" autocomplete="off" placeholder="BSA..."
                  value={config.tools.web_search_provider.brave_api_key ?? ""}
                  oninput={(e: Event) => {
                    const val = (e.target as HTMLInputElement).value;
                    onConfigChange({ ...config, tools: { ...config.tools, web_search_provider: { ...config.tools.web_search_provider, brave_api_key: val } } });
                  }}
                  class="mt-0.5 w-full rounded-lg border border-border/60 dark:border-white/[0.08] bg-surface-3 px-2.5 py-1.5 text-ui-md text-foreground placeholder:text-muted-foreground/50 outline-none focus:ring-1 focus:ring-blue-500/50" />
              </div>
            {/if}

            {#if config.tools.web_search_provider.backend === "searxng"}
              <div class="flex flex-col gap-1">
                <label for="searxng-url" class="text-ui-base font-semibold text-foreground">{t("llm.tools.searxngUrl")}</label>
                <span class="text-ui-sm text-muted-foreground leading-relaxed">{t("llm.tools.searxngUrlDesc")}</span>
                <input id="searxng-url" type="text" placeholder="https://search.example.com"
                  value={config.tools.web_search_provider.searxng_url ?? ""}
                  oninput={(e: Event) => {
                    const val = (e.target as HTMLInputElement).value;
                    onConfigChange({ ...config, tools: { ...config.tools, web_search_provider: { ...config.tools.web_search_provider, searxng_url: val } } });
                  }}
                  class="mt-0.5 w-full rounded-lg border border-border/60 dark:border-white/[0.08] bg-surface-3 px-2.5 py-1.5 text-ui-md text-foreground placeholder:text-muted-foreground/50 outline-none focus:ring-1 focus:ring-blue-500/50" />
              </div>
            {/if}
          </div>
        {/if}

        {#if config.tools.web_search || config.tools.web_fetch}
          <div class="flex flex-col gap-2.5 rounded-xl border border-border/60 dark:border-white/[0.06] bg-surface-3 px-3 py-2.5">
            <div class="flex items-center justify-between gap-2">
              <div class="flex flex-col gap-0.5">
                <span class="text-ui-base font-semibold text-foreground">{t("llm.tools.webCache")}</span>
                <span class="text-ui-sm text-muted-foreground leading-relaxed">{t("llm.tools.webCacheDesc")}</span>
              </div>
              <button role="switch" aria-checked={config.tools.web_cache?.enabled ?? true} aria-label={t("llm.tools.webCache")}
                onclick={async () => {
                  const wc = config.tools.web_cache ?? { enabled: true, search_ttl_secs: 1800, fetch_ttl_secs: 7200, domain_ttl_overrides: {} };
                  await onConfigChange({ ...config, tools: { ...config.tools, web_cache: { ...wc, enabled: !wc.enabled } } });
                }}
                class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 {(config.tools.web_cache?.enabled ?? true) ? 'bg-violet-500' : 'bg-muted dark:bg-white/10'}">
                <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md transform transition-transform duration-200 {(config.tools.web_cache?.enabled ?? true) ? 'translate-x-4' : 'translate-x-0'}"></span>
              </button>
            </div>

            {#if config.tools.web_cache?.enabled ?? true}
              <div class="flex gap-3">
                <div class="flex-1 flex flex-col gap-1">
                  <span class="text-ui-sm text-muted-foreground">{t("llm.tools.webCacheSearchTtl")}</span>
                  <div class="flex items-center gap-1">
                    {#each [{ secs: 300, label: "5" }, { secs: 900, label: "15" }, { secs: 1800, label: "30" }, { secs: 3600, label: "60" }] as opt}
                      <button onclick={async () => {
                        const wc = config.tools.web_cache ?? { enabled: true, search_ttl_secs: 1800, fetch_ttl_secs: 7200, domain_ttl_overrides: {} };
                        await onConfigChange({ ...config, tools: { ...config.tools, web_cache: { ...wc, search_ttl_secs: opt.secs } } });
                      }}
                        class="rounded-md border px-1.5 py-0.5 text-ui-sm font-semibold transition-all cursor-pointer {(config.tools.web_cache?.search_ttl_secs ?? 1800) === opt.secs ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400' : 'border-border bg-background text-muted-foreground hover:text-foreground'}">{opt.label}{t("llm.tools.webCacheTtlMin")}</button>
                    {/each}
                  </div>
                </div>
                <div class="flex-1 flex flex-col gap-1">
                  <span class="text-ui-sm text-muted-foreground">{t("llm.tools.webCacheFetchTtl")}</span>
                  <div class="flex items-center gap-1">
                    {#each [{ secs: 1800, label: "30" }, { secs: 3600, label: "60" }, { secs: 7200, label: "120" }, { secs: 14400, label: "240" }] as opt}
                      <button onclick={async () => {
                        const wc = config.tools.web_cache ?? { enabled: true, search_ttl_secs: 1800, fetch_ttl_secs: 7200, domain_ttl_overrides: {} };
                        await onConfigChange({ ...config, tools: { ...config.tools, web_cache: { ...wc, fetch_ttl_secs: opt.secs } } });
                      }}
                        class="rounded-md border px-1.5 py-0.5 text-ui-sm font-semibold transition-all cursor-pointer {(config.tools.web_cache?.fetch_ttl_secs ?? 7200) === opt.secs ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400' : 'border-border bg-background text-muted-foreground hover:text-foreground'}">{opt.label}{t("llm.tools.webCacheTtlMin")}</button>
                    {/each}
                  </div>
                </div>
              </div>

              <div class="flex flex-col gap-2 pt-1">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-ui-sm text-muted-foreground">
                    {#if cacheStats.total_entries > 0}
                      {t("llm.tools.webCacheEntries").replace("{n}", String(cacheStats.total_entries))}
                      <span class="ml-1 opacity-60">({t("llm.tools.webCacheSize").replace("{size}", fmtBytes(cacheStats.total_bytes))})</span>
                    {:else}
                      {t("llm.tools.webCacheEmpty")}
                    {/if}
                  </span>
                  <div class="flex items-center gap-1">
                    <button onclick={onRefreshCache} aria-label="Refresh cache" class="rounded-md border border-border px-1.5 py-0.5 text-ui-xs font-semibold text-muted-foreground hover:text-foreground transition-colors cursor-pointer bg-background">↻</button>
                    {#if cacheStats.total_entries > 0}
                      <button onclick={onClearCache} class="rounded-md border border-red-500/30 bg-red-500/5 px-2 py-0.5 text-ui-xs font-semibold text-red-600 dark:text-red-400 hover:bg-red-500/10 transition-colors cursor-pointer">{t("llm.tools.webCacheClearAll")}</button>
                    {/if}
                  </div>
                </div>

                {#if cacheEntries.length > 0}
                  <div class="flex flex-col gap-1 max-h-44 overflow-y-auto rounded-lg border border-border/40 dark:border-white/[0.04] bg-surface-3 p-1.5">
                    {#each cacheEntries as entry}
                      <div class="flex items-center justify-between gap-2 rounded-md px-2 py-1 hover:bg-muted/50 dark:hover:bg-white/[0.02] group">
                        <div class="flex flex-col gap-0 min-w-0">
                          <div class="flex items-center gap-1.5">
                            <span class="text-ui-2xs font-semibold rounded-full border px-1 py-0 {entry.kind === 'search' ? 'border-blue-500/30 bg-blue-500/10 text-blue-600 dark:text-blue-400' : 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'}">
                              {entry.kind === "search" ? t("llm.tools.webCacheSearch") : t("llm.tools.webCacheFetch")}
                            </span>
                            <span class="text-ui-xs text-foreground truncate">{entry.label}</span>
                          </div>
                          <div class="flex items-center gap-2 text-ui-2xs text-muted-foreground/60">
                            <span>{entry.domain}</span><span>{fmtBytes(entry.bytes)}</span><span>{fmtAge(entry.created_at)}</span>
                          </div>
                        </div>
                        <div class="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                          <button onclick={() => onRemoveDomain(entry.domain)} title={t("llm.tools.webCacheRemoveDomain")} class="rounded border border-border px-1 py-0.5 text-ui-2xs text-muted-foreground hover:text-foreground hover:bg-muted transition-colors cursor-pointer bg-background">{entry.domain} ✕</button>
                          <button onclick={() => onRemoveEntry(entry.key)} title={t("llm.tools.webCacheRemoveEntry")} class="rounded border border-red-500/30 px-1 py-0.5 text-ui-2xs text-red-500 hover:bg-red-500/10 transition-colors cursor-pointer bg-background">✕</button>
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

      <div class="flex flex-col gap-3 px-4 py-3 border-t border-border/40 dark:border-white/[0.04] bg-surface-3 {config.tools.enabled ? '' : 'opacity-40 pointer-events-none'}">
        <div class="flex flex-col gap-1.5">
          <span class="text-ui-base text-muted-foreground">{t("llm.tools.executionMode")}</span>
          <div class="flex rounded-lg overflow-hidden border border-border text-ui-base font-medium">
            {#each [{ key: "parallel" as ToolExecutionMode, label: t("llm.tools.parallel") }, { key: "sequential" as ToolExecutionMode, label: t("llm.tools.sequential") }] as mode}
              <button onclick={async () => await onConfigChange({ ...config, tools: { ...config.tools, execution_mode: mode.key } })}
                class="flex-1 py-1.5 transition-colors cursor-pointer {config.tools.execution_mode === mode.key ? 'bg-violet-600 text-white' : 'bg-background text-muted-foreground hover:bg-muted'}">{mode.label}</button>
            {/each}
          </div>
        </div>

        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-md font-semibold text-foreground">{t("llm.tools.maxRounds")}</span>
            <span class="text-ui-sm text-muted-foreground leading-relaxed">{t("llm.tools.maxRoundsDesc")}</span>
          </div>
          <div class="flex items-center gap-1">
            {#each [1, 3, 5, 10, 15] as val}
              <button onclick={async () => await onConfigChange({ ...config, tools: { ...config.tools, max_rounds: val } })}
                class="rounded-lg border px-2 py-1 text-ui-base font-semibold transition-all cursor-pointer {config.tools.max_rounds === val ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400' : 'border-border bg-background text-muted-foreground hover:text-foreground'}">{val}</button>
            {/each}
          </div>
        </div>

        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-md font-semibold text-foreground">{t("llm.tools.maxCallsPerRound")}</span>
            <span class="text-ui-sm text-muted-foreground leading-relaxed">{t("llm.tools.maxCallsPerRoundDesc")}</span>
          </div>
          <div class="flex items-center gap-1">
            {#each [1, 2, 4, 8] as val}
              <button onclick={async () => await onConfigChange({ ...config, tools: { ...config.tools, max_calls_per_round: val } })}
                class="rounded-lg border px-2 py-1 text-ui-base font-semibold transition-all cursor-pointer {config.tools.max_calls_per_round === val ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400' : 'border-border bg-background text-muted-foreground hover:text-foreground'}">{val}</button>
            {/each}
          </div>
        </div>

        <div class="flex flex-col gap-2.5 pt-1">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-md font-semibold text-foreground">{t("llm.tools.contextCompression")}</span>
            <span class="text-ui-sm text-muted-foreground leading-relaxed">{t("llm.tools.contextCompressionDesc")}</span>
          </div>
          <div class="flex rounded-lg overflow-hidden border border-border text-ui-base font-medium">
            {#each [
              { key: "off" as CompressionLevel, label: t("llm.tools.compressionOff") },
              { key: "normal" as CompressionLevel, label: t("llm.tools.compressionNormal") },
              { key: "aggressive" as CompressionLevel, label: t("llm.tools.compressionAggressive") },
            ] as opt}
              <button onclick={async () => await onConfigChange({ ...config, tools: { ...config.tools, context_compression: { ...config.tools.context_compression, level: opt.key } } })}
                class="flex-1 py-1.5 transition-colors cursor-pointer {config.tools.context_compression.level === opt.key ? 'bg-violet-600 text-white' : 'bg-background text-muted-foreground hover:bg-muted'}">{opt.label}</button>
            {/each}
          </div>

          {#if config.tools.context_compression.level !== "off"}
            <div class="flex gap-3">
              <div class="flex-1 flex flex-col gap-1">
                <label for="comp-max-results" class="text-ui-sm text-muted-foreground">{t("llm.tools.maxSearchResults")}</label>
                <input id="comp-max-results" type="number" min="0" max="20" step="1"
                  value={config.tools.context_compression.max_search_results}
                  oninput={async (e: Event) => {
                    const val = parseInt((e.target as HTMLInputElement).value) || 0;
                    await onConfigChange({
                      ...config,
                      tools: {
                        ...config.tools,
                        context_compression: {
                          ...config.tools.context_compression,
                          max_search_results: Math.max(0, Math.min(20, val)),
                        },
                      },
                    });
                  }}
                  class="w-full rounded-lg border border-border/60 dark:border-white/[0.08] bg-surface-3 px-2.5 py-1.5 text-ui-md text-foreground placeholder:text-muted-foreground/50 outline-none focus:ring-1 focus:ring-blue-500/50" />
                <span class="text-ui-xs text-muted-foreground/60">{t("llm.tools.zeroAutoLabel")}</span>
              </div>
              <div class="flex-1 flex flex-col gap-1">
                <label for="comp-max-chars" class="text-ui-sm text-muted-foreground">{t("llm.tools.maxResultChars")}</label>
                <input id="comp-max-chars" type="number" min="0" max="32000" step="500"
                  value={config.tools.context_compression.max_result_chars}
                  oninput={async (e: Event) => {
                    const val = parseInt((e.target as HTMLInputElement).value) || 0;
                    await onConfigChange({
                      ...config,
                      tools: {
                        ...config.tools,
                        context_compression: {
                          ...config.tools.context_compression,
                          max_result_chars: Math.max(0, Math.min(32000, val)),
                        },
                      },
                    });
                  }}
                  class="w-full rounded-lg border border-border/60 dark:border-white/[0.08] bg-surface-3 px-2.5 py-1.5 text-ui-md text-foreground placeholder:text-muted-foreground/50 outline-none focus:ring-1 focus:ring-blue-500/50" />
                <span class="text-ui-xs text-muted-foreground/60">{t("llm.tools.zeroAutoLabel")}</span>
              </div>
            </div>
          {/if}
        </div>

        <div class="flex flex-col gap-3 pt-2">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-md font-semibold text-foreground">{t("llm.tools.retrySection")}</span>
            <span class="text-ui-sm text-muted-foreground leading-relaxed">{t("llm.tools.retrySectionDesc")}</span>
          </div>

          <div class="flex gap-3">
            <div class="flex-1 flex flex-col gap-1">
              <label for="retry-max" class="text-ui-sm text-muted-foreground">{t("llm.tools.retryMaxRetries")}</label>
              <div class="flex items-center gap-1">
                {#each [0, 1, 2, 3] as val}
                  <button onclick={async () => await onConfigChange({ ...config, tools: { ...config.tools, retry: { ...config.tools.retry, max_retries: val } } })}
                    class="rounded-lg border px-2 py-1 text-ui-base font-semibold transition-all cursor-pointer {config.tools.retry.max_retries === val ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400' : 'border-border bg-background text-muted-foreground hover:text-foreground'}">{val}</button>
                {/each}
              </div>
              <span class="text-ui-xs text-muted-foreground/60">{t("llm.tools.retryMaxRetriesDesc")}</span>
            </div>
            <div class="flex-1 flex flex-col gap-1">
              <label for="retry-delay" class="text-ui-sm text-muted-foreground">{t("llm.tools.retryBaseDelay")}</label>
              <div class="flex items-center gap-1">
                {#each [500, 1000, 2000, 3000] as val}
                  <button onclick={async () => await onConfigChange({ ...config, tools: { ...config.tools, retry: { ...config.tools.retry, base_delay_ms: val } } })}
                    class="rounded-lg border px-2 py-1 text-ui-base font-semibold transition-all cursor-pointer {config.tools.retry.base_delay_ms === val ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400' : 'border-border bg-background text-muted-foreground hover:text-foreground'}">{val}{t("llm.tools.retryMs")}</button>
                {/each}
              </div>
              <span class="text-ui-xs text-muted-foreground/60">{t("llm.tools.retryBaseDelayDesc")}</span>
            </div>
          </div>
        </div>
      </div>
    </CardContent>
  </Card>
</section>
