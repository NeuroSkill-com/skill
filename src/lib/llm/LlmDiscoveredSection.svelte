<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Discovered Local Models
  ───────────────────────
  GGUF models that other apps on this machine (LM Studio, Ollama, Lemonade,
  the HuggingFace cache, …) already downloaded. Surfaced by the daemon's
  `rlx-models` filesystem scanner and runnable directly by the embedded engine.

  DATA & PRIVACY: discovery is a local, offline filesystem scan and inference
  runs on-device — nothing here leaves the machine. This is deliberately made
  explicit in the UI (see the "Local only" badge + egress note) because what
  data leaves neuroskill matters.
-->
<script lang="ts">
import { onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { ToggleRow } from "$lib/components/ui/toggle-row";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { fmtGB } from "$lib/format";
import { t } from "$lib/i18n/index.svelte";
import { discoveredSource, type LlmModelEntry, sourceLabel } from "$lib/llm/llm-helpers";

interface DiscoveryConfig {
  enabled: boolean;
  sources: string[];
  extra_dirs: string[];
}

interface Props {
  /** Currently-active model filename (to flag the selected row). */
  activeModel: string;
  /** Discovery settings (from LlmConfig). */
  discovery: DiscoveryConfig;
  /** True while a config save is in flight (disables controls). */
  configSaving: boolean;
  /** Persist updated discovery settings. */
  onSetDiscovery: (d: DiscoveryConfig) => void | Promise<void>;
  /** Called after a model is selected so the parent can reload the catalog. */
  onModelSelected: () => void | Promise<void>;
}

let { activeModel, discovery, configSaving, onSetDiscovery, onModelSelected }: Props = $props();

/** Every source the scanner knows about (stable display order). */
const ALL_SOURCES = ["lmstudio", "ollama", "lemonade", "hf", "mlx", "vllm", "rlx"];

let models = $state<LlmModelEntry[]>([]);
let scanning = $state(false);
let error = $state("");
let selecting = $state<string | null>(null);
let scanned = $state(false);
let showAdvanced = $state(false);
let newDir = $state("");

// Group discovered models by source (LM Studio, Ollama, …) for display.
const grouped = $derived.by(() => {
  const map = new Map<string, LlmModelEntry[]>();
  for (const m of models) {
    const src = discoveredSource(m);
    let arr = map.get(src);
    if (!arr) {
      arr = [];
      map.set(src, arr);
    }
    arr.push(m);
  }
  return [...map.entries()].sort((a, b) => sourceLabel(a[0]).localeCompare(sourceLabel(b[0])));
});

// Empty `sources` = all sources enabled.
function isSourceOn(src: string): boolean {
  return discovery.sources.length === 0 || discovery.sources.includes(src);
}

async function rescan() {
  if (!discovery.enabled) {
    models = [];
    scanned = true;
    return;
  }
  scanning = true;
  error = "";
  try {
    const resp = await daemonInvoke<{ ok: boolean; models?: LlmModelEntry[]; error?: string }>("discover_local_models");
    if (resp.ok && resp.models) {
      models = resp.models;
    } else {
      error = resp.error || t("llm.discovered.scanFailed");
    }
  } catch (e: unknown) {
    error = e instanceof Error ? e.message : t("llm.discovered.scanFailed");
  } finally {
    scanning = false;
    scanned = true;
  }
}

async function toggleEnabled() {
  const next = { ...discovery, enabled: !discovery.enabled };
  await onSetDiscovery(next);
  if (next.enabled) await rescan();
  else models = [];
}

async function toggleSource(src: string) {
  // Materialize the effective set, flip `src`, then normalize "all" back to [].
  let set = discovery.sources.length === 0 ? [...ALL_SOURCES] : [...discovery.sources];
  set = set.includes(src) ? set.filter((s) => s !== src) : [...set, src];
  const normalized = ALL_SOURCES.every((s) => set.includes(s)) ? [] : set;
  await onSetDiscovery({ ...discovery, sources: normalized });
  await rescan();
}

async function addDir() {
  const dir = newDir.trim();
  if (!dir || discovery.extra_dirs.includes(dir)) return;
  await onSetDiscovery({ ...discovery, extra_dirs: [...discovery.extra_dirs, dir] });
  newDir = "";
  await rescan();
}

async function removeDir(dir: string) {
  await onSetDiscovery({ ...discovery, extra_dirs: discovery.extra_dirs.filter((d) => d !== dir) });
  await rescan();
}

async function use(entry: LlmModelEntry) {
  selecting = entry.filename;
  try {
    await daemonInvoke("switch_llm_model", { filename: entry.filename });
    await onModelSelected();
  } catch (e: unknown) {
    error = e instanceof Error ? e.message : t("llm.discovered.selectFailed");
  } finally {
    selecting = null;
  }
}

onMount(() => {
  if (discovery.enabled) rescan();
});
</script>

<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <SectionHeader>{t("llm.discovered.title")}</SectionHeader>
    <Badge
      variant="outline"
      class="text-ui-2xs py-0 px-1.5 border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
      title={t("llm.discovered.egress")}
    >
      {t("llm.discovered.localOnly")}
    </Badge>
    {#if discovery.enabled}
      <button
        onclick={rescan}
        disabled={scanning || configSaving}
        class="ml-auto text-ui-sm text-violet-600 dark:text-violet-400 hover:text-violet-600 cursor-pointer px-1.5 py-0.5 rounded disabled:opacity-50"
      >
        {scanning ? t("llm.discovered.scanning") : t("llm.discovered.rescan")}
      </button>
    {/if}
  </div>

  <!-- Data-egress reassurance — the whole point of surfacing local models. -->
  <div
    class="flex items-start gap-2 rounded-lg border border-emerald-500/20 bg-emerald-500/[0.06] px-3 py-2 text-ui-sm text-muted-foreground"
  >
    <span aria-hidden="true" class="text-emerald-600 dark:text-emerald-400">🔒</span>
    <span>{t("llm.discovered.egress")}</span>
  </div>

  <!-- Master enable toggle -->
  <div class="rounded-xl border border-border dark:border-white/[0.06] bg-surface-1 overflow-hidden">
    <ToggleRow
      checked={discovery.enabled}
      label={t("llm.discovered.enable")}
      description={t("llm.discovered.enableDesc")}
      ontoggle={toggleEnabled}
      showBadge={true}
    />
  </div>

  {#if discovery.enabled}
    <!-- Advanced: source filter + extra folders -->
    <button
      onclick={() => (showAdvanced = !showAdvanced)}
      class="self-start text-ui-sm text-muted-foreground/70 hover:text-foreground transition-colors cursor-pointer px-1"
    >
      {showAdvanced ? "▲" : "▼"} {t("llm.discovered.advanced")}
    </button>

    {#if showAdvanced}
      <div class="flex flex-col gap-3 rounded-xl border border-border/60 dark:border-white/[0.05] bg-surface-2 px-3 py-3">
        <!-- Sources -->
        <div class="flex flex-col gap-1.5">
          <span class="text-ui-xs font-semibold uppercase tracking-widest text-muted-foreground/60">
            {t("llm.discovered.sourcesLabel")}
          </span>
          <div class="flex flex-wrap gap-1.5">
            {#each ALL_SOURCES as src (src)}
              {@const on = isSourceOn(src)}
              <button
                onclick={() => toggleSource(src)}
                disabled={configSaving || scanning}
                aria-pressed={on}
                class="text-ui-sm px-2 py-0.5 rounded-full border transition-colors cursor-pointer disabled:opacity-50
                  {on
                  ? 'border-violet-500/40 bg-violet-500/15 text-violet-600 dark:text-violet-300'
                  : 'border-border/60 dark:border-white/[0.08] bg-transparent text-muted-foreground/60'}"
              >
                {sourceLabel(src)}
              </button>
            {/each}
          </div>
          {#if discovery.sources.length > 0 && discovery.sources.length < ALL_SOURCES.length}
            <span class="text-ui-xs text-muted-foreground/50">{t("llm.discovered.sourcesFiltered")}</span>
          {/if}
        </div>

        <!-- Extra folders -->
        <div class="flex flex-col gap-1.5">
          <span class="text-ui-xs font-semibold uppercase tracking-widest text-muted-foreground/60">
            {t("llm.discovered.extraDirsLabel")}
          </span>
          {#each discovery.extra_dirs as dir (dir)}
            <div class="flex items-center gap-2">
              <span class="text-ui-sm font-mono text-muted-foreground/80 truncate flex-1">{dir}</span>
              <button
                onclick={() => removeDir(dir)}
                disabled={configSaving}
                class="text-ui-xs text-destructive hover:underline cursor-pointer disabled:opacity-50"
              >
                {t("llm.discovered.removeDir")}
              </button>
            </div>
          {/each}
          <div class="flex items-center gap-2">
            <input
              type="text"
              bind:value={newDir}
              placeholder={t("llm.discovered.addDirPlaceholder")}
              onkeydown={(e) => { if (e.key === "Enter") addDir(); }}
              class="flex-1 rounded-lg border border-border dark:border-white/[0.06] bg-surface-1 text-foreground text-ui-sm px-2.5 py-1.5 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            />
            <Button size="sm" variant="outline" class="h-7 text-ui-sm px-2" disabled={configSaving || !newDir.trim()} onclick={addDir}>
              {t("llm.discovered.addDir")}
            </Button>
          </div>
        </div>
      </div>
    {/if}

    <!-- Discovered model list -->
    {#if error}
      <p class="text-ui-sm text-destructive px-1">{error}</p>
    {/if}

    {#if models.length === 0}
      {#if scanning || !scanned}
        <p class="text-ui-sm text-muted-foreground px-1 animate-pulse">{t("llm.discovered.scanning")}</p>
      {:else}
        <p class="text-ui-sm text-muted-foreground px-1">{t("llm.discovered.empty")}</p>
      {/if}
    {:else}
      <div class="flex flex-col gap-2.5">
        {#each grouped as [source, entries] (source)}
          <div class="flex flex-col gap-1">
            <div class="flex items-center gap-2 px-1">
              <Badge
                variant="outline"
                class="text-ui-2xs py-0 px-1.5 border-violet-500/20 bg-violet-500/10 text-violet-600 dark:text-violet-400"
              >
                {sourceLabel(source)}
              </Badge>
              <span class="text-ui-xs text-muted-foreground/60">{entries.length}</span>
            </div>

            <Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
              <CardContent class="py-0 px-0">
                <div class="flex flex-col divide-y divide-border/40 dark:divide-white/[0.05]">
                  {#each entries as m (m.filename)}
                    {@const isActive = m.filename === activeModel}
                    {@const isSelecting = selecting === m.filename}
                    <div class="grid grid-cols-[1fr_4rem_4rem_auto] gap-x-2 items-center px-4 py-2">
                      <div class="flex items-center gap-1.5 min-w-0">
                        <span class="text-ui-md font-semibold text-foreground truncate">{m.family_name}</span>
                        {#if m.is_mmproj}
                          <Badge
                            variant="outline"
                            class="text-ui-2xs py-0 px-1 border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400 shrink-0"
                            >mmproj</Badge
                          >
                        {/if}
                      </div>
                      <span class="text-ui-sm font-bold font-mono text-muted-foreground truncate">{m.quant}</span>
                      <span class="text-ui-sm tabular-nums text-muted-foreground">{fmtGB(m.size_gb)}</span>
                      <div class="shrink-0">
                        {#if isActive}
                          <Badge
                            variant="outline"
                            class="text-ui-2xs py-0 px-2 border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                            >{t("llm.discovered.active")}</Badge
                          >
                        {:else if !m.is_mmproj}
                          <Button
                            size="sm"
                            variant="outline"
                            class="h-6 text-ui-sm px-2"
                            disabled={isSelecting}
                            onclick={() => use(m)}
                          >
                            {isSelecting ? "…" : t("llm.discovered.useBtn")}
                          </Button>
                        {/if}
                      </div>
                    </div>
                  {/each}
                </div>
              </CardContent>
            </Card>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <p class="text-ui-sm text-muted-foreground px-1">{t("llm.discovered.off")}</p>
  {/if}
</section>
