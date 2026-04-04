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
import { onMount } from "svelte";
import { Card, CardContent } from "$lib/components/ui/card";
import {
  getLlmConfig,
  getSkillsLastSync,
  getSkillsLicense,
  getSkillsRefreshInterval,
  getSkillsSyncOnLaunch,
  listSkills,
  setDisabledSkills,
  setLlmConfig,
  setSkillsRefreshInterval,
  setSkillsSyncOnLaunch,
  syncSkillsNow,
  webCacheClear,
  webCacheList,
  webCacheRemoveDomain,
  webCacheRemoveEntry,
  webCacheStats,
} from "$lib/daemon/client";
import { t } from "$lib/i18n/index.svelte";
import { addToast } from "$lib/stores/toast.svelte";
import AgentSkillsSection from "$lib/tools/AgentSkillsSection.svelte";
import ChatToolsSection from "$lib/tools/ChatToolsSection.svelte";
import SkillsRefreshSection from "$lib/tools/SkillsRefreshSection.svelte";
import SuggestSkillCta from "$lib/tools/SuggestSkillCta.svelte";
import { formatPrefixedError } from "$lib/utils/error";

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

interface CalendarEvent {
  id: string;
  title: string;
  start_utc: number;
  end_utc: number;
  all_day: boolean;
  location?: string | null;
  calendar?: string | null;
  status: string;
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

// ── Calendar tool diagnostics ─────────────────────────────────────────────
let calendarStatus = $state<"authorized" | "denied" | "restricted" | "not_determined" | "unknown">("unknown");
let calendarTesting = $state(false);
let calendarEvents = $state<CalendarEvent[]>([]);
let calendarTestError = $state("");

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

async function refreshCache() {
  try {
    cacheStats = await webCacheStats<typeof cacheStats>();
    cacheEntries = await webCacheList<CacheEntryInfo>();
  } catch {
    /* cache not initialised yet */
  }
}

async function clearCache() {
  await webCacheClear();
  await refreshCache();
}

async function removeDomain(domain: string) {
  await webCacheRemoveDomain(domain);
  await refreshCache();
}

async function removeEntry(key: string) {
  await webCacheRemoveEntry(key);
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

// ── Data loading ───────────────────────────────────────────────────────────

async function loadConfig() {
  try {
    config = await getLlmConfig<LlmConfig>();
    skillsRefreshInterval = config.tools.skills_refresh_interval_secs ?? 86400;
  } catch (e) {}
}

async function saveConfig() {
  configSaving = true;
  try {
    await setLlmConfig(config);
  } finally {
    configSaving = false;
  }
}

async function loadSkillsMeta() {
  try {
    skillsRefreshInterval = await getSkillsRefreshInterval();
    skillsSyncOnLaunch = await getSkillsSyncOnLaunch();
    skillsLastSync = await getSkillsLastSync();
  } catch (e) {}
}

async function setSkillsInterval(secs: number) {
  const prev = skillsRefreshInterval;
  skillsRefreshInterval = secs;
  config = { ...config, tools: { ...config.tools, skills_refresh_interval_secs: secs } };
  try {
    await setSkillsRefreshInterval(secs);
    await saveConfig();
  } catch (e) {
    skillsRefreshInterval = prev;
    config = { ...config, tools: { ...config.tools, skills_refresh_interval_secs: prev } };
    addToast("error", t("llm.tools.skillsSection"), formatPrefixedError(t("common.error"), e));
  }
}

async function syncNow() {
  skillsSyncing = true;
  try {
    await syncSkillsNow();
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
    skills = await listSkills();
  } catch {
    skills = [];
  } finally {
    skillsLoading = false;
  }
}

async function loadSkillsLicense() {
  try {
    skillsLicense = (await getSkillsLicense()) ?? "";
  } catch {
    skillsLicense = "";
  }
}

async function loadCalendarStatus() {
  try {
    const s = await invoke<string>("get_calendar_permission_status");
    if (s === "authorized" || s === "denied" || s === "restricted" || s === "not_determined") {
      calendarStatus = s;
    } else {
      calendarStatus = "unknown";
    }
  } catch {
    calendarStatus = "unknown";
  }
}

async function testCalendarFetch() {
  calendarTesting = true;
  calendarTestError = "";
  try {
    const now = Math.floor(Date.now() / 1000);
    const end = now + 24 * 60 * 60;
    calendarEvents = await invoke<CalendarEvent[]>("get_calendar_events", { startUtc: now, endUtc: end });
  } catch (e) {
    calendarTestError = e instanceof Error ? e.message : String(e ?? "Calendar fetch failed");
    calendarEvents = [];
  } finally {
    calendarTesting = false;
  }
}

async function toggleSkill(name: string, enabled: boolean) {
  const prev = skills;
  // Update local state immediately for responsiveness.
  skills = skills.map((s) => (s.name === name ? { ...s, enabled } : s));
  const disabled = skills.filter((s) => !s.enabled).map((s) => s.name);
  try {
    await setDisabledSkills(disabled);
  } catch (e) {
    skills = prev;
    addToast("error", t("llm.tools.skillsSection"), formatPrefixedError(t("common.error"), e));
  }
}

async function setAllSkills(enabled: boolean) {
  const prev = skills;
  skills = skills.map((s) => ({ ...s, enabled }));
  const disabled = enabled ? [] : skills.map((s) => s.name);
  try {
    await setDisabledSkills(disabled);
  } catch (e) {
    skills = prev;
    addToast("error", t("llm.tools.skillsSection"), formatPrefixedError(t("common.error"), e));
  }
}

// ── Lifecycle ──────────────────────────────────────────────────────────────

onMount(async () => {
  await loadConfig();
  await refreshCache();
  await loadSkillsMeta();
  await loadSkills();
  await loadSkillsLicense();
  await loadCalendarStatus();
});
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Chat tools                                                                  -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<ChatToolsSection
  {config}
  {configSaving}
  toolRows={TOOL_ROWS}
  {cacheStats}
  {cacheEntries}
  onConfigChange={async (next) => { config = next as LlmConfig; await saveConfig(); }}
  onRefreshCache={refreshCache}
  onClearCache={clearCache}
  onRemoveDomain={removeDomain}
  onRemoveEntry={removeEntry}
/>

<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">Calendar</span>
    <span class="text-[0.52rem] text-muted-foreground/50">tools diagnostics</span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-2.5 px-4 py-3.5">
      <div class="flex items-center gap-2 text-[0.64rem]">
        <span class="text-muted-foreground">Permission:</span>
        <span class="rounded-full border px-2 py-0.5 text-[0.58rem] font-semibold
                     {calendarStatus === 'authorized' ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400' : calendarStatus === 'denied' || calendarStatus === 'restricted' ? 'border-red-500/30 bg-red-500/10 text-red-600 dark:text-red-400' : 'border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400'}">
          {calendarStatus}
        </span>
        <button
          class="ml-auto rounded-md border border-border px-2 py-1 text-[0.58rem] font-semibold text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
          onclick={loadCalendarStatus}>
          Refresh
        </button>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <button
          class="rounded-md border border-border px-2.5 py-1 text-[0.6rem] font-semibold text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
          onclick={async () => {
            await invoke("request_calendar_permission").catch(() => false);
            await loadCalendarStatus();
          }}>
          Request permission
        </button>
        <button
          class="rounded-md border border-blue-500/30 bg-blue-500/10 px-2.5 py-1 text-[0.6rem] font-semibold text-blue-600 dark:text-blue-400 hover:bg-blue-500/15 transition-colors disabled:opacity-50"
          onclick={testCalendarFetch}
          disabled={calendarTesting}>
          {calendarTesting ? "Testing…" : "Test fetch next 24h"}
        </button>
      </div>

      {#if calendarTestError}
        <p class="text-[0.58rem] text-red-600 dark:text-red-400 leading-relaxed">{calendarTestError}</p>
      {/if}

      {#if calendarEvents.length > 0}
        <div class="rounded-lg border border-border/60 dark:border-white/[0.08] bg-muted/40 dark:bg-[#111118] px-2.5 py-2 max-h-44 overflow-y-auto">
          <p class="text-[0.54rem] uppercase tracking-wider text-muted-foreground/70 mb-1.5">Upcoming ({calendarEvents.length})</p>
          <div class="flex flex-col gap-1">
            {#each calendarEvents.slice(0, 20) as ev}
              <div class="text-[0.58rem] leading-relaxed">
                <span class="font-semibold text-foreground/85">{ev.title}</span>
                <span class="text-muted-foreground/70"> · {new Date(ev.start_utc * 1000).toLocaleString()}</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}
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
<AgentSkillsSection
  {skills}
  {skillsLoading}
  {skillsLicense}
  onToggleSkill={toggleSkill}
  onSetAllSkills={setAllSkills}
/>

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
    const prev = skillsSyncOnLaunch;
    skillsSyncOnLaunch = !skillsSyncOnLaunch;
    try {
      await setSkillsSyncOnLaunch(skillsSyncOnLaunch);
    } catch (e) {
      skillsSyncOnLaunch = prev;
      addToast("error", t("llm.tools.skillsSection"), formatPrefixedError(t("common.error"), e));
    }
  }}
  onSyncNow={syncNow}
/>
