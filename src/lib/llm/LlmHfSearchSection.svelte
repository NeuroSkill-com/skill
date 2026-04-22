<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  HuggingFace GGUF Model Search
  ──────────────────────────────
  • Search HF Hub for GGUF repos
  • Browse files in a repo
  • Add model + optionally start download
-->
<script lang="ts">
import { marked } from "marked";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { daemonGet, daemonPost } from "$lib/daemon/http";
import { fmtGB } from "$lib/format";
import { t } from "$lib/i18n/index.svelte";

/** Strip YAML frontmatter (--- ... ---) from markdown. */
function stripFrontmatter(md: string): string {
  return md.replace(/^---\s*\n[\s\S]*?\n---\s*\n/, "");
}

function renderReadme(md: string): string {
  return marked.parse(stripFrontmatter(md)) as string;
}

interface Props {
  onModelAdded: () => void | Promise<void>;
}

let { onModelAdded }: Props = $props();

// ── State ──────────────────────────────────────────────────────────────────

let query = $state("");
let searching = $state(false);
let searchError = $state("");
let results = $state<HfSearchResult[]>([]);

let expandedRepo = $state<string | null>(null);
let loadingFiles = $state(false);
let filesError = $state("");
let repoFiles = $state<HfFile[]>([]);
let repoReadme = $state<string | null>(null);
let readmeExpanded = $state(false);

let addingFile = $state<string | null>(null);

interface HfSearchResult {
  repo: string;
  author: string;
  downloads: number;
  likes: number;
  tags: string[];
  pipeline_tag: string;
  last_modified: string;
}

interface HfFile {
  filename: string;
  size_gb: number;
  quant: string;
  is_mmproj: boolean;
}

// ── Actions ────────────────────────────────────────────────────────────────

let debounceTimer: ReturnType<typeof setTimeout> | undefined;

function onInput() {
  clearTimeout(debounceTimer);
  if (query.trim().length < 2) {
    results = [];
    return;
  }
  debounceTimer = setTimeout(doSearch, 400);
}

async function doSearch() {
  const q = query.trim();
  if (q.length < 2) return;
  searching = true;
  searchError = "";
  try {
    const resp = await daemonGet<{ ok: boolean; results?: HfSearchResult[]; error?: string }>(
      `/v1/llm/catalog/search?q=${encodeURIComponent(q)}&limit=12`,
    );
    if (resp.ok && resp.results) {
      results = resp.results;
    } else {
      searchError = resp.error || "Search failed";
      results = [];
    }
  } catch (e: unknown) {
    searchError = e instanceof Error ? e.message : "Search failed";
    results = [];
  } finally {
    searching = false;
  }
}

async function toggleRepo(repo: string) {
  if (expandedRepo === repo) {
    expandedRepo = null;
    repoFiles = [];
    repoReadme = null;
    readmeExpanded = false;
    return;
  }
  expandedRepo = repo;
  loadingFiles = true;
  filesError = "";
  repoFiles = [];
  repoReadme = null;
  readmeExpanded = false;
  try {
    const resp = await daemonGet<{ ok: boolean; files?: HfFile[]; readme?: string | null; error?: string }>(
      `/v1/llm/catalog/search/files?repo=${encodeURIComponent(repo)}`,
    );
    if (resp.ok && resp.files) {
      repoFiles = resp.files;
      repoReadme = resp.readme ?? null;
    } else {
      filesError = resp.error || "Failed to fetch files";
    }
  } catch (e: unknown) {
    filesError = e instanceof Error ? e.message : "Failed to fetch files";
  } finally {
    loadingFiles = false;
  }
}

async function addModel(repo: string, file: HfFile, download: boolean) {
  addingFile = file.filename;
  try {
    await daemonPost("/v1/llm/catalog/add-model", {
      repo,
      filename: file.filename,
      size_gb: file.size_gb,
      download,
    });
    await onModelAdded();
  } catch (e: unknown) {
    // silently handled — catalog refresh will show the entry
  } finally {
    addingFile = null;
  }
}

function fmtDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function fmtDate(iso: string): string {
  if (!iso) return "";
  try {
    const d = new Date(iso);
    return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
  } catch {
    return iso.slice(0, 10);
  }
}

const NOTABLE_TAGS = new Set([
  "text-generation",
  "text2text-generation",
  "conversational",
  "image-text-to-text",
  "vision",
]);
</script>

<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <SectionHeader>{t("llm.hfSearch.title")}</SectionHeader>
  </div>

  <!-- Search input -->
  <div class="relative">
    <input
      type="text"
      aria-label={t("llm.hfSearch.placeholder")}
      placeholder={t("llm.hfSearch.placeholder")}
      bind:value={query}
      oninput={onInput}
      onkeydown={(e) => { if (e.key === "Enter") { clearTimeout(debounceTimer); doSearch(); } }}
      class="w-full rounded-xl border border-border dark:border-white/[0.06]
             bg-surface-1 text-foreground text-ui-lg
             px-3.5 py-2.5 pr-16 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
    />
    {#if searching}
      <span class="absolute right-3 top-1/2 -translate-y-1/2 text-ui-sm text-muted-foreground animate-pulse">
        {t("llm.hfSearch.searching")}
      </span>
    {:else if query.trim().length >= 2}
      <button
        onclick={doSearch}
        class="absolute right-2 top-1/2 -translate-y-1/2 text-ui-sm text-violet-600 dark:text-violet-400 hover:text-violet-600 cursor-pointer px-1.5 py-0.5 rounded"
      >
        {t("llm.hfSearch.searchBtn")}
      </button>
    {/if}
  </div>

  {#if searchError}
    <p class="text-ui-sm text-destructive px-1">{searchError}</p>
  {/if}

  <!-- Results -->
  {#if results.length > 0}
    <div class="flex flex-col gap-1.5">
      {#each results as r (r.repo)}
        {@const isExpanded = expandedRepo === r.repo}
        <Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
          <CardContent class="py-0 px-0">
            <!-- Repo header -->
            <button
              onclick={() => toggleRepo(r.repo)}
              class="w-full flex flex-col gap-1 px-4 py-3 text-left cursor-pointer hover:bg-accent/50 dark:hover:bg-white/[0.02] transition-colors"
            >
              <div class="flex items-center gap-2 min-w-0">
                <span class="text-ui-md font-semibold text-foreground truncate">{r.repo}</span>
                <span class="ml-auto text-ui-2xs text-muted-foreground/40 shrink-0">{isExpanded ? "▲" : "▼"}</span>
              </div>
              <div class="flex items-center gap-2 flex-wrap text-ui-xs text-muted-foreground/70">
                <span title="Downloads">↓ {fmtDownloads(r.downloads)}</span>
                <span title="Likes">♥ {r.likes}</span>
                {#if r.pipeline_tag}
                  <Badge variant="outline" class="text-ui-2xs py-0 px-1.5 border-violet-500/20 bg-violet-500/10 text-violet-600 dark:text-violet-400">{r.pipeline_tag}</Badge>
                {/if}
                {#each r.tags.filter((tag) => NOTABLE_TAGS.has(tag) && tag !== r.pipeline_tag).slice(0, 3) as tag}
                  <Badge variant="outline" class="text-ui-2xs py-0 px-1.5 border-slate-500/20 bg-slate-500/10 text-slate-600 dark:text-slate-300">{tag}</Badge>
                {/each}
                {#if r.last_modified}
                  <span class="ml-auto text-ui-xs text-muted-foreground/50">{fmtDate(r.last_modified)}</span>
                {/if}
              </div>
            </button>

            <!-- Expanded: readme + file list -->
            {#if isExpanded}
              <!-- Open in browser + README -->
              <div class="border-t border-border/40 dark:border-white/[0.04] px-4 py-2 flex flex-col gap-2">
                <div class="flex items-center gap-2">
                  <a
                    href="https://huggingface.co/{r.repo}"
                    target="_blank"
                    rel="noopener noreferrer"
                    class="inline-flex items-center gap-1 text-ui-sm text-violet-600 dark:text-violet-400 hover:text-violet-600 hover:underline"
                  >
                    {t("llm.hfSearch.openInBrowser")} ↗
                  </a>
                  {#if repoReadme}
                    <button
                      onclick={() => (readmeExpanded = !readmeExpanded)}
                      class="text-ui-sm text-muted-foreground/60 hover:text-foreground transition-colors cursor-pointer"
                    >
                      {readmeExpanded ? t("llm.hfSearch.hideReadme") : t("llm.hfSearch.showReadme")}
                    </button>
                  {/if}
                </div>
                {#if readmeExpanded && repoReadme}
                  <div class="hf-readme rounded-lg border border-border/40 dark:border-white/[0.04] bg-surface-3 px-3 py-2 max-h-64 overflow-y-auto">
                    {@html renderReadme(repoReadme)}
                  </div>
                {/if}
              </div>

              <div class="border-t border-border/40 dark:border-white/[0.04]">
                {#if loadingFiles}
                  <div class="px-4 py-3 text-ui-sm text-muted-foreground animate-pulse">{t("llm.hfSearch.loadingFiles")}</div>
                {:else if filesError}
                  <div class="px-4 py-3 text-ui-sm text-destructive">{filesError}</div>
                {:else if repoFiles.length === 0}
                  <div class="px-4 py-3 text-ui-sm text-muted-foreground">{t("llm.hfSearch.noFiles")}</div>
                {:else}
                  <!-- Column headers -->
                  <div class="grid grid-cols-[4rem_4rem_1fr_auto] gap-x-2 items-center px-4 py-1.5 bg-surface-3">
                    <span class="text-ui-xs font-semibold uppercase tracking-widest text-muted-foreground/60">{t("llm.hfSearch.colQuant")}</span>
                    <span class="text-ui-xs font-semibold uppercase tracking-widest text-muted-foreground/60">{t("llm.hfSearch.colSize")}</span>
                    <span class="text-ui-xs font-semibold uppercase tracking-widest text-muted-foreground/60">{t("llm.hfSearch.colFile")}</span>
                    <span></span>
                  </div>

                  <div class="flex flex-col divide-y divide-border/40 dark:divide-white/[0.05]">
                    {#each repoFiles as file (file.filename)}
                      {@const isAdding = addingFile === file.filename}
                      <div class="grid grid-cols-[4rem_4rem_1fr_auto] gap-x-2 items-center px-4 py-2 {file.is_mmproj ? 'bg-amber-50/30 dark:bg-amber-950/10' : ''}">
                        <span class="text-ui-md font-bold font-mono text-foreground truncate">{file.quant}</span>
                        <span class="text-ui-md tabular-nums font-semibold text-muted-foreground">{fmtGB(file.size_gb)}</span>
                        <div class="flex items-center gap-1 min-w-0">
                          <span class="text-ui-sm text-muted-foreground/70 truncate font-mono">{file.filename}</span>
                          {#if file.is_mmproj}
                            <Badge variant="outline" class="text-ui-2xs py-0 px-1 border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400 shrink-0">mmproj</Badge>
                          {/if}
                        </div>
                        <div class="flex items-center gap-1 shrink-0">
                          <Button size="sm" variant="outline" class="h-6 text-ui-sm px-2"
                            disabled={isAdding}
                            onclick={() => addModel(r.repo, file, false)}>
                            {isAdding ? "…" : t("llm.hfSearch.addBtn")}
                          </Button>
                          <Button size="sm" class="h-6 text-ui-sm px-2 bg-violet-600 hover:bg-violet-700 text-white"
                            disabled={isAdding}
                            onclick={() => addModel(r.repo, file, true)}>
                            {isAdding ? "…" : t("llm.hfSearch.addDownloadBtn")}
                          </Button>
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
          </CardContent>
        </Card>
      {/each}
    </div>
  {:else if !searching && query.trim().length >= 2 && !searchError}
    <p class="text-ui-sm text-muted-foreground px-1">{t("llm.hfSearch.noResults")}</p>
  {/if}
</section>

<style>
  :global(.hf-readme) {
    font-size: 0.58rem;
    line-height: 1.6;
    color: var(--muted-foreground, #6b7280);
    word-break: break-word;
  }
  :global(.hf-readme h1) { font-size: 0.82rem; font-weight: 700; margin: 0.8em 0 0.4em; color: var(--foreground, #111); }
  :global(.hf-readme h2) { font-size: 0.72rem; font-weight: 700; margin: 0.7em 0 0.3em; color: var(--foreground, #111); }
  :global(.hf-readme h3) { font-size: 0.66rem; font-weight: 600; margin: 0.6em 0 0.2em; color: var(--foreground, #111); }
  :global(.hf-readme h4),
  :global(.hf-readme h5),
  :global(.hf-readme h6) { font-size: 0.6rem; font-weight: 600; margin: 0.5em 0 0.2em; color: var(--foreground, #111); }
  :global(.hf-readme p) { margin: 0.4em 0; }
  :global(.hf-readme ul),
  :global(.hf-readme ol) { margin: 0.3em 0; padding-left: 1.4em; }
  :global(.hf-readme li) { margin: 0.15em 0; }
  :global(.hf-readme a) { color: #7c3aed; text-decoration: underline; }
  :global(.hf-readme code) {
    font-size: 0.54rem;
    background: rgba(0,0,0,0.06);
    border-radius: 3px;
    padding: 0.1em 0.3em;
  }
  :global(.hf-readme pre) {
    font-size: 0.54rem;
    background: rgba(0,0,0,0.06);
    border-radius: 6px;
    padding: 0.5em 0.7em;
    overflow-x: auto;
    margin: 0.4em 0;
  }
  :global(.hf-readme pre code) { background: none; padding: 0; }
  :global(.hf-readme img) { max-width: 100%; height: auto; border-radius: 4px; margin: 0.4em 0; }
  :global(.hf-readme blockquote) {
    border-left: 3px solid rgba(124,58,237,0.3);
    padding-left: 0.7em;
    margin: 0.4em 0;
    color: var(--muted-foreground, #6b7280);
    font-style: italic;
  }
  :global(.hf-readme hr) { border: none; border-top: 1px solid rgba(0,0,0,0.1); margin: 0.6em 0; }
  /* ── Tables ─────────────────────────────────────────────────── */
  :global(.hf-readme table) {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.56rem;
    margin: 0.5em 0;
  }
  :global(.hf-readme thead) { border-bottom: 2px solid rgba(0,0,0,0.12); }
  :global(.hf-readme th) {
    text-align: left;
    font-weight: 600;
    padding: 0.35em 0.6em;
    color: var(--foreground, #111);
    background: rgba(0,0,0,0.03);
  }
  :global(.hf-readme td) {
    padding: 0.3em 0.6em;
    border-bottom: 1px solid rgba(0,0,0,0.06);
  }
  :global(.hf-readme tr:last-child td) { border-bottom: none; }
  /* ── Dark mode overrides ────────────────────────────────────── */
  :global(.dark .hf-readme code) { background: rgba(255,255,255,0.08); }
  :global(.dark .hf-readme pre) { background: rgba(255,255,255,0.06); }
  :global(.dark .hf-readme a) { color: #a78bfa; }
  :global(.dark .hf-readme thead) { border-bottom-color: rgba(255,255,255,0.12); }
  :global(.dark .hf-readme th) { background: rgba(255,255,255,0.04); }
  :global(.dark .hf-readme td) { border-bottom-color: rgba(255,255,255,0.06); }
  :global(.dark .hf-readme hr) { border-top-color: rgba(255,255,255,0.1); }
</style>
