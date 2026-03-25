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
import { t } from "$lib/i18n/index.svelte";
import AgentSkillsSection from "$lib/tools/AgentSkillsSection.svelte";
import ChatToolsSection from "$lib/tools/ChatToolsSection.svelte";
import SkillsRefreshSection from "$lib/tools/SkillsRefreshSection.svelte";
import SuggestSkillCta from "$lib/tools/SuggestSkillCta.svelte";

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
    skillsSyncOnLaunch = !skillsSyncOnLaunch;
    await invoke("set_skills_sync_on_launch", { enabled: skillsSyncOnLaunch });
  }}
  onSyncNow={syncNow}
/>
