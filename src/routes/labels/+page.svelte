<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Labels List — browse, search, edit, and delete all EEG annotations. -->
<script lang="ts">
  import { onMount }    from "svelte";
  import { invoke }     from "@tauri-apps/api/core";
  import { Button }     from "$lib/components/ui/button";
  import { Badge }      from "$lib/components/ui/badge";
  import { Separator }  from "$lib/components/ui/separator";
  import { Spinner }    from "$lib/components/ui/spinner";
  import { t }          from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/window-title.svelte";
  import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";

  // ── Actions ───────────────────────────────────────────────────────────────
  function focusOnMount(node: HTMLElement) { node.focus(); }

  // ── Types ──────────────────────────────────────────────────────────────────
  interface LabelRow {
    id:          number;
    eeg_start:   number;
    eeg_end:     number;
    label_start: number;
    label_end:   number;
    text:        string;
    context:     string;
    created_at:  number;
  }

  /** Result shape returned by the `search_labels_by_text` Tauri command. */
  interface LabelNeighbor {
    label_id:         number;
    text:             string;
    context:          string;
    eeg_start:        number;
    eeg_end:          number;
    created_at:       number;
    embedding_model?: string;
    /** Cosine distance in [0, 2]: 0 = identical, 2 = opposite. */
    distance:         number;
  }

  // ── State ──────────────────────────────────────────────────────────────────
  let labels   = $state<LabelRow[]>([]);
  let loading  = $state(true);
  let search   = $state("");
  let editingId      = $state<number | null>(null);
  let editText       = $state("");
  let editContext    = $state("");
  let savingId       = $state<number | null>(null);
  let deletingId  = $state<number | null>(null);
  let confirmDel  = $state<number | null>(null);

  // ── Search mode ────────────────────────────────────────────────────────────

  type SearchMode = "exact" | "semantic";
  let searchMode        = $state<SearchMode>("exact");
  let semanticResults   = $state<LabelNeighbor[]>([]);
  let semanticSearching = $state(false);
  let semanticError     = $state("");

  /** Debounce handle so we don't fire on every keystroke in semantic mode. */
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  /** Invoke the HNSW semantic search command (runs in a Tauri worker thread). */
  async function searchSemantic() {
    const q = search.trim();
    if (!q || searchMode !== "semantic") return;
    semanticSearching = true;
    semanticError     = "";
    try {
      semanticResults = await invoke<LabelNeighbor[]>(
        "search_labels_by_text", { query: q, k: 50 }
      );
    } catch (e) {
      semanticError   = String(e);
      semanticResults = [];
    } finally {
      semanticSearching = false;
    }
  }

  // ── Filtered + paginated ───────────────────────────────────────────────────

  const PAGE_SIZE = 50;
  let page = $state(0);

  /** When the user switches to semantic mode or changes the query, (re-)run
   *  the semantic search with a short debounce.  In exact mode just reset the
   *  page so the substring filter re-applies immediately. */
  $effect(() => {
    const q = search;
    const m = searchMode;
    clearTimeout(debounceTimer);
    page = 0;
    if (m === "semantic") {
      if (q.trim()) {
        debounceTimer = setTimeout(searchSemantic, 420);
      } else {
        semanticResults   = [];
        semanticError     = "";
        semanticSearching = false;
      }
    }
  });

  /**
   * Exact-mode: instant client-side substring filter.
   * Semantic-mode: ordered by HNSW distance, joined back to the full LabelRow
   * so edit/delete actions work unchanged.
   */
  const activeLabels = $derived.by((): LabelRow[] => {
    if (searchMode === "exact") {
      const q = search.trim().toLowerCase();
      return q
        ? labels.filter(l =>
            l.text.toLowerCase().includes(q) ||
            l.context.toLowerCase().includes(q))
        : labels;
    }
    // Semantic mode ──────────────────────────────────────────────────────
    if (!search.trim()) return labels;                   // empty query → show all
    // Re-order labels to match the HNSW ranking.  Labels absent from the
    // index (never embedded) are silently omitted.
    return semanticResults
      .map(r => labels.find(l => l.id === r.label_id))
      .filter((l): l is LabelRow => l !== undefined);
  });

  /**
   * label_id → cosine distance for the current semantic result set.
   * Empty in exact mode.
   */
  const distanceMap = $derived.by((): Map<number, number> => {
    if (searchMode !== "semantic" || !search.trim()) return new Map();
    const m = new Map<number, number>();
    for (const r of semanticResults) m.set(r.label_id, r.distance);
    return m;
  });

  /**
   * Similarity percentage from cosine distance (0 = identical, 2 = opposite).
   * We clamp to [0, 100] and display as an integer.
   */
  function simPct(distance: number): number {
    return Math.round(Math.max(0, Math.min(100, (1 - distance / 2) * 100)));
  }

  /** Colour class for the similarity badge based on cosine distance. */
  function simColor(distance: number): string {
    if (distance < 0.10) return "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400";
    if (distance < 0.25) return "bg-blue-500/15 text-blue-600 dark:text-blue-400";
    if (distance < 0.45) return "bg-amber-500/15 text-amber-600 dark:text-amber-400";
    return "bg-muted text-muted-foreground/60";
  }

  let totalPages      = $derived(Math.max(1, Math.ceil(activeLabels.length / PAGE_SIZE)));
  let paginatedLabels = $derived(activeLabels.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE));

  // ── Helpers ────────────────────────────────────────────────────────────────
  function formatDate(unix: number): string {
    return new Date(unix * 1000).toLocaleString(undefined, {
      month: "short", day: "numeric", year: "numeric",
      hour: "2-digit", minute: "2-digit",
    });
  }

  function formatDuration(start: number, end: number): string {
    const s = Math.max(0, end - start);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60), ss = s % 60;
    return `${m}m ${String(ss).padStart(2,"0")}s`;
  }

  // ── Data loading ────────────────────────────────────────────────────────────
  async function loadLabels() {
    loading = true;
    try {
      labels = await invoke<LabelRow[]>("query_annotations", {
        startUtc: undefined,
        endUtc:   undefined,
      });
    } catch (e) {
      console.error("Failed to load labels:", e);
    } finally {
      loading = false;
    }
  }

  // ── Edit ────────────────────────────────────────────────────────────────────
  function startEdit(label: LabelRow) {
    editingId   = label.id;
    editText    = label.text;
    editContext = label.context;
  }

  function cancelEdit() {
    editingId   = null;
    editText    = "";
    editContext = "";
  }

  async function saveEdit(labelId: number) {
    if (!editText.trim() || savingId !== null) return;
    savingId = labelId;
    try {
      await invoke("update_label", { labelId, text: editText.trim(), context: editContext.trim() });
      // Update in-place
      const idx = labels.findIndex(l => l.id === labelId);
      if (idx !== -1) labels[idx] = { ...labels[idx], text: editText.trim(), context: editContext.trim() };
      cancelEdit();
    } catch (e) {
      console.error("Failed to update label:", e);
    } finally {
      savingId = null;
    }
  }

  // ── Delete ──────────────────────────────────────────────────────────────────
  function askDelete(labelId: number) {
    confirmDel = labelId;
  }

  function cancelDelete() {
    confirmDel = null;
  }

  async function doDelete(labelId: number) {
    deletingId = labelId;
    try {
      await invoke("delete_label", { labelId });
      labels = labels.filter(l => l.id !== labelId);
    } catch (e) {
      console.error("Failed to delete label:", e);
    } finally {
      deletingId = null;
      confirmDel = null;
    }
  }

  // ── View session ────────────────────────────────────────────────────────────
  // Open history window (labels don't carry the CSV path, so navigate to history).
  async function viewSession(_label: LabelRow) {
    try {
      await invoke("open_history_window");
    } catch (_) {}
  }

  onMount(() => { loadLabels(); });

  useWindowTitle("window.title.labels");
</script>

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden">

  <!-- ── Search bar ─────────────────────────────────────────────────────────── -->
  <div class="px-4 py-2.5 border-b border-border dark:border-white/[0.06] flex flex-col gap-1.5">

    <div class="flex items-center gap-2">

      <!-- Text input -->
      <div class="relative flex-1 flex items-center">
        <!-- Spinner (semantic) or magnifier icon -->
        {#if semanticSearching}
          <div class="absolute left-2.5 w-3.5 h-3.5 text-violet-500 pointer-events-none">
            <Spinner size="w-3.5 h-3.5" />
          </div>
        {:else}
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
               class="absolute left-2.5 w-3.5 h-3.5 pointer-events-none
                      {searchMode === 'semantic'
                        ? 'text-violet-500/70'
                        : 'text-muted-foreground/60'}">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
        {/if}

        <input
          type="text"
          bind:value={search}
          onkeydown={(e) => {
            if (e.key === "Enter" && searchMode === "semantic") {
              clearTimeout(debounceTimer);
              searchSemantic();
            }
          }}
          placeholder={searchMode === "semantic"
            ? t("labels.search.semanticPlaceholder")
            : t("labels.searchPlaceholder")}
          class="w-full pl-8 pr-7 py-1.5 text-[0.78rem] rounded-lg
                 border bg-muted/30 dark:bg-white/[0.04]
                 text-foreground placeholder:text-muted-foreground/50
                 focus:outline-none focus:ring-2 transition-all
                 {searchMode === 'semantic'
                   ? 'border-violet-400/40 dark:border-violet-500/30 focus:ring-violet-500/30'
                   : 'border-border dark:border-white/[0.09] focus:ring-blue-500/30'}"
        />
        {#if search}
          <button
            onclick={() => { search = ""; semanticResults = []; semanticError = ""; }}
            class="absolute right-2.5 text-muted-foreground/50 hover:text-foreground transition-colors"
            aria-label={t("common.clear")}
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                 stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                 class="w-3.5 h-3.5">
              <line x1="18" y1="6" x2="6" y2="18"/>
              <line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        {/if}
      </div>

      <!-- Mode segmented toggle -->
      <div class="flex rounded-md border border-border dark:border-white/[0.1]
                  overflow-hidden text-[0.63rem] font-semibold shrink-0">
        <button
          onclick={() => { searchMode = "exact"; semanticResults = []; semanticError = ""; }}
          title={t("labels.search.exactTitle")}
          class="px-2.5 py-1 transition-colors leading-none
                 {searchMode === 'exact'
                   ? 'bg-blue-500 text-white'
                   : 'text-muted-foreground hover:text-foreground hover:bg-muted/40'}">
          {t("labels.search.exact")}
        </button>
        <div class="w-px bg-border dark:bg-white/[0.1]"></div>
        <button
          onclick={() => { searchMode = "semantic"; if (search.trim()) searchSemantic(); }}
          title={t("labels.search.semanticTitle")}
          class="flex items-center gap-1 px-2.5 py-1 transition-colors leading-none
                 {searchMode === 'semantic'
                   ? 'bg-violet-600 text-white'
                   : 'text-muted-foreground hover:text-foreground hover:bg-muted/40'}">
          <!-- Sparkle icon -->
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
               stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"
               class="w-2.5 h-2.5 shrink-0">
            <path d="M8 1.5l.75 2.25L11 4.5l-2.25.75L8 7.5l-.75-2.25L5 4.5l2.25-.75Z"/>
            <path d="M12 9l.4 1.2L13.5 10.5l-1.1.3L12 12l-.4-1.2L10.5 10.5l1.1-.3Z"/>
          </svg>
          {t("labels.search.semantic")}
        </button>
      </div>
    </div>

    <!-- Semantic mode: error / hint row -->
    {#if searchMode === "semantic"}
      {#if semanticError}
        <p class="text-[0.62rem] text-destructive/80 px-0.5">
          {semanticError.includes("not initialized")
            ? t("labels.search.noIndex")
            : semanticError}
        </p>
      {:else if search.trim() && !semanticSearching && semanticResults.length === 0 && !semanticError}
        <!-- Triggered after a completed search with 0 results -->
        <p class="text-[0.62rem] text-muted-foreground/50 px-0.5">
          {t("labels.search.noResults", { q: search.trim() })}
        </p>
      {:else if !search.trim()}
        <p class="text-[0.58rem] text-muted-foreground/40 px-0.5 select-none">
          {t("labels.search.semanticHint")}
        </p>
      {/if}
    {/if}
  </div>

  <!-- ── Label list ─────────────────────────────────────────────────────────── -->
  <div class="flex-1 overflow-y-auto min-h-0">
    {#if loading}
      <div class="flex items-center justify-center gap-2 py-16 text-muted-foreground/60 text-[0.78rem]">
        <Spinner size="w-4 h-4" />
        {t("labels.loading")}
      </div>

    {:else if activeLabels.length === 0 && !semanticSearching}
      <div class="flex flex-col items-center justify-center gap-3 py-16 px-8 text-center">
        <div class="w-10 h-10 rounded-full bg-muted/40 flex items-center justify-center">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
               class="w-5 h-5 text-muted-foreground/60">
            <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
            <line x1="7" y1="7" x2="7.01" y2="7"/>
          </svg>
        </div>
        {#if labels.length === 0}
          <p class="text-[0.8rem] font-medium text-foreground/70">{t("labels.noLabels")}</p>
          <p class="text-[0.68rem] text-muted-foreground/50 max-w-xs leading-relaxed">
            {t("labels.noLabelsHint")}
          </p>
        {:else}
          <p class="text-[0.8rem] font-medium text-foreground/70">
            {t("labels.search.noResults", { q: search.trim() })}
          </p>
        {/if}
      </div>

    {:else}
      <div class="divide-y divide-border dark:divide-white/[0.05]">
        {#each paginatedLabels as label (label.id)}
          <div class="px-4 py-3 flex flex-col gap-2 hover:bg-muted/20 transition-colors">

            {#if editingId === label.id}
              <!-- ── Edit mode ─────────────────────────────────────────── -->
              <div class="flex flex-col gap-2">
                <input
                  type="text"
                  bind:value={editText}
                  onkeydown={(e) => {
                    if (e.key === "Escape") cancelEdit();
                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) saveEdit(label.id);
                  }}
                  class="w-full px-2.5 py-1.5 text-[0.78rem] rounded-md
                         border border-blue-500/40
                         bg-background text-foreground
                         focus:outline-none focus:ring-2 focus:ring-blue-500/30"
                  use:focusOnMount
                />
                <textarea
                  bind:value={editContext}
                  placeholder={t("label.contextPlaceholder")}
                  onkeydown={(e) => {
                    if (e.key === "Escape") cancelEdit();
                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) saveEdit(label.id);
                  }}
                  class="w-full px-2.5 py-1.5 text-[0.72rem] rounded-md
                         border border-border dark:border-white/[0.08]
                         bg-background text-foreground placeholder:text-muted-foreground/40
                         focus:outline-none focus:ring-1 focus:ring-blue-500/30
                         resize-y leading-relaxed"
                  style="min-height: 80px"
                ></textarea>
                <div class="flex gap-2 justify-end">
                  <Button variant="ghost" size="sm" class="h-6 text-[0.68rem]"
                          onclick={cancelEdit}>{t("common.cancel")}</Button>
                  <Button size="sm" class="h-6 text-[0.68rem]"
                          disabled={savingId === label.id || !editText.trim()}
                          onclick={() => saveEdit(label.id)}>
                    {savingId === label.id ? t("common.saving") : t("labels.saveEdit")}
                  </Button>
                </div>
              </div>

            {:else if confirmDel === label.id}
              <!-- ── Delete confirmation ──────────────────────────────── -->
              <div class="flex items-center gap-3 py-0.5">
                <span class="text-[0.75rem] text-foreground/80 flex-1">
                  {t("labels.confirmDelete")}
                </span>
                <Button variant="ghost" size="sm" class="h-6 text-[0.68rem]"
                        onclick={cancelDelete}>{t("common.cancel")}</Button>
                <Button variant="destructive" size="sm" class="h-6 text-[0.68rem]"
                        disabled={deletingId === label.id}
                        onclick={() => doDelete(label.id)}>
                  {deletingId === label.id ? "…" : t("labels.yesDelete")}
                </Button>
              </div>

            {:else}
              <!-- ── Normal view ──────────────────────────────────────── -->
              <div class="flex items-start gap-2">
                <!-- Label text + optional context -->
                <div class="flex-1 flex flex-col gap-0.5 min-w-0">
                  <p class="text-[0.8rem] text-foreground leading-snug">{label.text}</p>
                  {#if label.context}
                    {@const preview = label.context.slice(0, 200)}
                    {@const isTruncated = label.context.length > 200}
                    <details class="group/ctx">
                      <summary class="list-none cursor-pointer text-[0.68rem]
                                      text-muted-foreground/55 italic leading-snug
                                      hover:text-muted-foreground/80 transition-colors select-none">
                        {preview}{isTruncated ? "…" : ""}
                        {#if isTruncated}
                          <span class="not-italic font-medium text-blue-500/70
                                       group-open/ctx:hidden ml-1">
                            {t("labels.showMore")}
                          </span>
                        {/if}
                      </summary>
                      {#if isTruncated}
                        <p class="mt-1 text-[0.68rem] text-muted-foreground/55 italic
                                  leading-relaxed whitespace-pre-wrap">
                          {label.context}
                        </p>
                      {/if}
                    </details>
                  {/if}
                </div>

                <!-- Action buttons -->
                <div class="flex gap-1 shrink-0 opacity-0 group-hover:opacity-100
                            [.group:hover_&]:opacity-100 transition-opacity">
                  <button
                    onclick={() => startEdit(label)}
                    title={t("labels.edit")}
                    class="w-6 h-6 rounded flex items-center justify-center
                           text-muted-foreground hover:text-foreground hover:bg-muted/50
                           transition-colors"
                  >
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                         class="w-3 h-3">
                      <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/>
                      <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/>
                    </svg>
                  </button>
                  <button
                    onclick={() => askDelete(label.id)}
                    title={t("history.delete")}
                    class="w-6 h-6 rounded flex items-center justify-center
                           text-muted-foreground hover:text-destructive hover:bg-destructive/10
                           transition-colors"
                  >
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                         class="w-3 h-3">
                      <polyline points="3,6 5,6 21,6"/>
                      <path d="M19,6l-1,14H6L5,6"/>
                      <path d="M10,11v6M14,11v6"/>
                      <path d="M9,6V4h6v2"/>
                    </svg>
                  </button>
                </div>
              </div>  <!-- /flex items-start gap-2 -->

              <!-- Metadata row -->
              <div class="flex items-center gap-3 text-[0.65rem] text-muted-foreground/60">
                <span>{formatDate(label.created_at)}</span>
                <span>·</span>
                <span>{t("labels.duration")}: {formatDuration(label.eeg_start, label.eeg_end)}</span>

                <!-- Semantic similarity badge -->
                {#if distanceMap.has(label.id)}
                  {@const dist = distanceMap.get(label.id)!}
                  <span class="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-full
                               text-[0.55rem] font-semibold {simColor(dist)}"
                        title="{t('labels.search.simTitle')}: {simPct(dist)}% (dist {dist.toFixed(3)})">
                    <!-- Sparkle icon -->
                    <svg viewBox="0 0 10 10" fill="currentColor" class="w-2 h-2 shrink-0 opacity-80">
                      <path d="M5 .5l.5 1.5L7 2.5 5.5 3 5 4.5 4.5 3 3 2.5l1.5-.5Z"/>
                    </svg>
                    {simPct(dist)}%
                  </span>
                {/if}

                <span class="flex-1"></span>
                <button
                  onclick={() => viewSession(label)}
                  class="text-blue-500/80 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300
                         underline underline-offset-2 text-[0.65rem] transition-colors"
                >
                  {t("search.viewSession")}
                </button>
              </div>
            {/if}

          </div>
        {/each}
      </div>

      <!-- ── Pagination controls ──────────────────────────────────────────── -->
      {#if totalPages > 1}
        <div class="flex items-center justify-center gap-2 pt-3 pb-1">
          <button
            onclick={() => page = Math.max(0, page - 1)}
            disabled={page === 0}
            class="px-3 py-1 text-[0.65rem] rounded-md border border-border dark:border-white/[0.08]
                   text-muted-foreground hover:text-foreground hover:bg-accent transition-colors
                   disabled:opacity-30 disabled:cursor-not-allowed"
          >← {t("common.prev")}</button>

          <span class="text-[0.65rem] tabular-nums text-muted-foreground">
            {page + 1} / {totalPages}
          </span>

          <button
            onclick={() => page = Math.min(totalPages - 1, page + 1)}
            disabled={page >= totalPages - 1}
            class="px-3 py-1 text-[0.65rem] rounded-md border border-border dark:border-white/[0.08]
                   text-muted-foreground hover:text-foreground hover:bg-accent transition-colors
                   disabled:opacity-30 disabled:cursor-not-allowed"
          >{t("common.next")} →</button>
        </div>
      {/if}
    {/if}
  </div>

  <Separator />
  <DisclaimerFooter />
</main>

<style>
  /* Show action buttons on row hover (pure CSS since Svelte doesn't support group-hover easily) */
  div:hover > div > div > button,
  div:focus-within > div > div > button {
    opacity: 1;
  }
</style>
