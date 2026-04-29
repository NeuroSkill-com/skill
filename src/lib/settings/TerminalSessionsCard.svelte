<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Terminal Sessions browser. Lists recent shell sessions captured by the PTY
  shim, lets the user expand any one to see every command + its stripped
  output, and offers two search modes (keyword via FTS5, semantic via
  embeddings). Click-to-expand is local state — no extra API call until
  the user actually opens a session.
-->
<script lang="ts">
import { onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { CardContent } from "$lib/components/ui/card";
import SectionHeader from "$lib/components/ui/section-header/SectionHeader.svelte";
import SettingsCard from "$lib/components/ui/settings-card/SettingsCard.svelte";
import { Spinner } from "$lib/components/ui/spinner";
import { daemonPost } from "$lib/daemon/http";

interface SessionRow {
  id: string;
  started_at: number;
  ended_at: number | null;
  shell: string;
  initial_cwd: string;
  command_count: number;
  avg_focus: number | null;
  avg_mood: number | null;
  has_session_text?: boolean;
}

interface CommandRow {
  command_id: number;
  time_start_us: number;
  time_end_us: number;
  stripped_text: string;
  raw_b64: string | null;
}

interface SessionExpansion {
  kind: "loading" | "commands" | "session_text" | "empty";
  commands?: CommandRow[];
  text?: string;
  textBytes?: number;
}

type FilterKind = "all" | "withOutput" | "today" | "week" | "live";

let sessions = $state<SessionRow[]>([]);
let orphanCount = $state(0);
let loading = $state(true);
let error = $state("");

let expanded = $state<Record<string, SessionExpansion | undefined>>({});
let collapsedDays = $state<Record<string, boolean>>({});
let filter = $state<FilterKind>("all");

let searchQuery = $state("");
let searchMode = $state<"keyword" | "semantic">("keyword");
let searchResults = $state<{ id: number; text: string; score?: number }[] | null>(null);
let searching = $state(false);

// ── Derived: filter + day-bucketing ─────────────────────────────────────────
// All `$derived` runs happen synchronously when their dependencies change, so
// the layout stays a pure function of (sessions, filter).

const filtered = $derived.by((): SessionRow[] => {
  const now = Date.now() / 1000;
  const todayStart = startOfLocalDay(now);
  const weekStart = todayStart - 6 * 86400; // last 7 days incl. today
  return sessions.filter((s) => {
    switch (filter) {
      case "withOutput":
        return s.command_count > 0 || !!s.has_session_text;
      case "today":
        return s.started_at >= todayStart;
      case "week":
        return s.started_at >= weekStart;
      case "live":
        return s.ended_at == null;
      default:
        return true;
    }
  });
});

/** Days are arrays in newest-first order, each with newest-first sessions. */
const dayBuckets = $derived.by((): { key: string; label: string; rows: SessionRow[] }[] => {
  const map = new Map<string, SessionRow[]>();
  for (const s of filtered) {
    const key = ymd(s.started_at);
    if (!map.has(key)) map.set(key, []);
    map.get(key)?.push(s);
  }
  // sessions are returned newest-first by the API, so `entries()` keeps that.
  return Array.from(map.entries()).map(([key, rows]) => ({
    key,
    label: dayLabel(key, rows[0].started_at),
    rows,
  }));
});

/** Hide the F/M columns entirely if every visible row is missing both. */
const showEegColumns = $derived.by((): boolean => {
  return filtered.some((s) => s.avg_focus != null || s.avg_mood != null);
});

/** Longest session in the visible set; used to scale the duration bar. */
const maxDurationSecs = $derived.by((): number => {
  let m = 1;
  for (const s of filtered) {
    const end = s.ended_at ?? Math.floor(Date.now() / 1000);
    m = Math.max(m, end - s.started_at);
  }
  return m;
});

onMount(loadSessions);

async function loadSessions() {
  loading = true;
  error = "";
  try {
    const r = await daemonPost<{
      sessions: SessionRow[];
      orphan_command_count?: number;
    }>("/v1/brain/terminal-sessions", { limit: 50 });
    sessions = r.sessions ?? [];
    orphanCount = r.orphan_command_count ?? 0;
  } catch (e) {
    error = e instanceof Error ? e.message : "Failed to load sessions";
  }
  loading = false;
}

async function toggleSession(s: SessionRow) {
  const id = s.id;
  if (expanded[id] && expanded[id]?.kind !== "loading") {
    delete expanded[id];
    expanded = { ...expanded };
    return;
  }
  expanded[id] = { kind: "loading" };
  expanded = { ...expanded };
  try {
    // Prefer per-command outputs (new shim path). Fall back to
    // session-level stripped text (legacy backfill) if no commands are
    // attached or none have output rows.
    if (s.command_count > 0) {
      const r = await daemonPost<{ commands: CommandRow[] }>("/v1/brain/terminal-session-output", {
        sessionId: id,
        raw: false,
      });
      const cmds = r.commands ?? [];
      if (cmds.length > 0) {
        expanded[id] = { kind: "commands", commands: cmds };
        expanded = { ...expanded };
        return;
      }
    }
    if (s.has_session_text) {
      const r = await daemonPost<{
        found: boolean;
        text: string;
        original_size: number;
      }>("/v1/brain/terminal-session-text", { sessionId: id });
      if (r.found) {
        expanded[id] = {
          kind: "session_text",
          text: r.text,
          textBytes: r.original_size,
        };
        expanded = { ...expanded };
        return;
      }
    }
    expanded[id] = { kind: "empty" };
    expanded = { ...expanded };
  } catch {
    expanded[id] = { kind: "empty" };
    expanded = { ...expanded };
  }
}

async function runSearch() {
  if (!searchQuery.trim()) {
    searchResults = null;
    return;
  }
  searching = true;
  try {
    if (searchMode === "keyword") {
      const r = await daemonPost<{ command_ids: number[] }>("/v1/brain/terminal-search", {
        query: searchQuery,
        limit: 20,
      });
      searchResults = await hydrateSearch(r.command_ids ?? []);
    } else {
      const r = await daemonPost<{ command_ids: number[]; scores: number[] }>("/v1/brain/terminal-semantic-search", {
        query: searchQuery,
        limit: 20,
      });
      const ids = r.command_ids ?? [];
      const scores = r.scores ?? [];
      const hydrated = await hydrateSearch(ids);
      hydrated.forEach((row, i) => {
        row.score = scores[i];
      });
      searchResults = hydrated;
    }
  } catch (e) {
    error = e instanceof Error ? e.message : "Search failed";
    searchResults = [];
  }
  searching = false;
}

async function hydrateSearch(ids: number[]): Promise<{ id: number; text: string; score?: number }[]> {
  // Fetch each command's stripped text. Could be batched server-side; for now
  // 20 sequential calls is fine for the search-result pane.
  const out: { id: number; text: string }[] = [];
  for (const id of ids) {
    try {
      const r = await daemonPost<{ found: boolean; stripped_text: string }>("/v1/brain/terminal-output", {
        commandId: id,
        raw: false,
      });
      if (r.found) out.push({ id, text: r.stripped_text });
    } catch {
      /* skip on error */
    }
  }
  return out;
}

function fmtTime(unix: number): string {
  return new Date(unix * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function fmtDuration(start: number, end: number | null): string {
  if (end == null) return "active";
  const secs = end - start;
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  return `${(secs / 3600).toFixed(1)}h`;
}

function fmtFocus(v: number | null): string {
  return v == null ? "—" : v.toFixed(0);
}

function snippet(text: string, max = 120): string {
  const t = text.replace(/\s+/g, " ").trim();
  return t.length > max ? `${t.slice(0, max)}…` : t;
}

/** Local-time YYYY-MM-DD key for grouping sessions into day buckets. */
function ymd(unix: number): string {
  const d = new Date(unix * 1000);
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

function startOfLocalDay(unix: number): number {
  const d = new Date(unix * 1000);
  d.setHours(0, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

/** "Today", "Yesterday", or "Mon, Apr 25" — driven by an actual session ts so
 *  we don't drift across midnight while the page is open. */
function dayLabel(key: string, sampleTs: number): string {
  const today = ymd(Date.now() / 1000);
  const yesterday = ymd(Date.now() / 1000 - 86400);
  if (key === today) return "Today";
  if (key === yesterday) return "Yesterday";
  return new Date(sampleTs * 1000).toLocaleDateString([], {
    weekday: "short",
    month: "short",
    day: "numeric",
  });
}

function durationBarPct(s: SessionRow): number {
  const end = s.ended_at ?? Math.floor(Date.now() / 1000);
  const secs = Math.max(0, end - s.started_at);
  // Log scale so a 12 s session and a 4 h session both show meaningfully.
  const norm = Math.log1p(secs) / Math.log1p(maxDurationSecs);
  return Math.min(100, Math.max(2, norm * 100));
}

function toggleDay(key: string) {
  collapsedDays[key] = !collapsedDays[key];
  collapsedDays = { ...collapsedDays };
}
</script>

<SettingsCard>
  <SectionHeader>Terminal Sessions</SectionHeader>
  <p class="-mt-1 mb-1 px-0.5 text-sm text-muted-foreground">
    Each shell from launch to exit. Click a row to see every command and its output.
    Search keyword (FTS) or by meaning (embeddings).
  </p>
  <CardContent class="space-y-3">
    <!-- Search bar -->
    <div class="flex flex-wrap items-center gap-2">
      <input
        type="text"
        aria-label="Search command output"
        bind:value={searchQuery}
        onkeydown={(e) => { if (e.key === "Enter") runSearch(); }}
        placeholder="Search command output…"
        class="flex-1 min-w-[12rem] rounded-md border border-border/60 bg-background px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-ring"
      />
      <div class="flex rounded-md border border-border/60 text-xs">
        <button
          class="px-2 py-1 transition-colors {searchMode === 'keyword' ? 'bg-muted font-medium' : 'hover:bg-muted/50'}"
          onclick={() => { searchMode = "keyword"; if (searchQuery) runSearch(); }}
        >Keyword</button>
        <button
          class="px-2 py-1 border-l border-border/60 transition-colors {searchMode === 'semantic' ? 'bg-muted font-medium' : 'hover:bg-muted/50'}"
          onclick={() => { searchMode = "semantic"; if (searchQuery) runSearch(); }}
        >Meaning</button>
      </div>
      <Button size="sm" variant="outline" onclick={runSearch} disabled={searching}>
        {searching ? "…" : "Search"}
      </Button>
      {#if searchResults}
        <Button size="sm" variant="ghost" onclick={() => { searchResults = null; searchQuery = ""; }}>
          Clear
        </Button>
      {/if}
    </div>

    {#if error}
      <div class="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">{error}</div>
    {/if}

    <!-- Scope summary so the user can see the full capture surface at a glance. -->
    {#if !loading}
      {@const totalCmds = sessions.reduce((acc, s) => acc + s.command_count, 0)}
      {@const backfilled = sessions.filter((s) => s.has_session_text).length}
      <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
        <span>{sessions.length} session{sessions.length === 1 ? "" : "s"}</span>
        <span>{totalCmds} command{totalCmds === 1 ? "" : "s"} linked</span>
        {#if backfilled > 0}
          <span title="Sessions whose raw .log.zst was decoded into searchable text — no per-command timing.">
            {backfilled} legacy (text only)
          </span>
        {/if}
        {#if orphanCount > 0}
          <span title="Commands tracked but not yet linked to any session — typically because they ran in a shell that didn't go through the recording shim.">
            {orphanCount} untracked command{orphanCount === 1 ? "" : "s"}
          </span>
        {/if}
      </div>
    {/if}

    <!-- Search results override the session list when active -->
    {#if searchResults}
      <div class="space-y-1.5">
        <p class="text-xs text-muted-foreground">
          {searchResults.length} match{searchResults.length === 1 ? "" : "es"} ({searchMode})
        </p>
        {#each searchResults as r}
          <div class="rounded-md border border-border/40 px-3 py-2 text-xs">
            <div class="flex items-center justify-between gap-2 text-muted-foreground">
              <span>cmd #{r.id}</span>
              {#if r.score !== undefined}
                <span class="font-mono">score {r.score.toFixed(3)}</span>
              {/if}
            </div>
            <p class="mt-1 font-mono text-foreground/90">{snippet(r.text, 200)}</p>
          </div>
        {/each}
        {#if searchResults.length === 0}
          <p class="text-xs italic text-muted-foreground">No matches.</p>
        {/if}
      </div>

    {:else if loading}
      <div class="flex justify-center py-6"><Spinner /></div>

    {:else if sessions.length === 0}
      <p class="text-sm italic text-muted-foreground">
        No sessions yet. Open a new terminal — recording starts automatically.
      </p>

    {:else}
      <!-- Quick filter chips -->
      <div class="flex flex-wrap gap-1.5" data-testid="filter-chips">
        {#each [
          ["all", "All", sessions.length],
          ["withOutput", "With output", sessions.filter((s) => s.command_count > 0 || !!s.has_session_text).length],
          ["today", "Today", sessions.filter((s) => s.started_at >= startOfLocalDay(Date.now() / 1000)).length],
          ["week", "Last 7 days", sessions.filter((s) => s.started_at >= startOfLocalDay(Date.now() / 1000) - 6 * 86400).length],
          ["live", "Live", sessions.filter((s) => s.ended_at == null).length],
        ] as const as [kind, label, count]}
          <button
            class="rounded-full border px-2 py-0.5 text-[11px] transition-colors {filter === kind ? 'border-primary bg-primary/15 text-primary' : 'border-border/50 text-muted-foreground hover:border-border hover:bg-muted/40'}"
            onclick={() => (filter = kind as FilterKind)}
            data-filter={kind}
          >
            {label} <span class="opacity-70">{count}</span>
          </button>
        {/each}
      </div>

      {#if filtered.length === 0}
        <p class="py-3 text-sm italic text-muted-foreground">
          No sessions match this filter.
        </p>
      {:else}
        <div class="space-y-3">
          {#each dayBuckets as bucket}
            {@const collapsed = collapsedDays[bucket.key]}
            <div data-testid="day-bucket" data-day={bucket.key}>
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <button
                class="flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left hover:bg-muted/30"
                onclick={() => toggleDay(bucket.key)}
              >
                <span class="text-[10px] font-mono text-muted-foreground/70">{collapsed ? "▸" : "▾"}</span>
                <span class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {bucket.label}
                </span>
                <span class="text-[10px] text-muted-foreground/70">
                  {bucket.rows.length} session{bucket.rows.length === 1 ? "" : "s"}
                </span>
              </button>

              {#if !collapsed}
                <div class="mt-1 divide-y divide-border/20 border-t border-border/20">
                  {#each bucket.rows as s}
                    {@const exp = expanded[s.id]}
                    {@const isOpen = exp !== undefined}
                    {@const isLegacy = !s.shell && !!s.has_session_text}
                    {@const isLive = s.ended_at == null}
                    <div class="py-1.5" data-testid="session-row">
                      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                      <!-- svelte-ignore a11y_click_events_have_key_events -->
                      <div
                        class="flex cursor-pointer items-center justify-between gap-3 rounded px-2 py-1 hover:bg-muted/40"
                        role="button"
                        tabindex="0"
                        onclick={() => toggleSession(s)}
                      >
                        <div class="flex min-w-0 flex-1 items-center gap-3">
                          <span class="text-muted-foreground text-xs font-mono shrink-0 w-4">
                            {isOpen ? "▾" : "▸"}
                          </span>
                          <span class="font-mono text-xs tabular-nums text-muted-foreground shrink-0 w-12 text-right">
                            {fmtTime(s.started_at)}
                          </span>
                          <!-- Duration bar — log-scaled so 10 s and 4 h both show. -->
                          <div
                            class="hidden sm:block h-1 w-12 shrink-0 rounded-full bg-muted/40 overflow-hidden"
                            title="{fmtDuration(s.started_at, s.ended_at)}"
                          >
                            <div
                              class="h-full {isLive ? 'bg-green-400/70' : isLegacy ? 'bg-amber-400/60' : 'bg-foreground/40'}"
                              style="width: {durationBarPct(s)}%"
                            ></div>
                          </div>
                          <div class="min-w-0 flex-1">
                            <div class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-sm">
                              <span class="text-xs text-muted-foreground">
                                {s.shell || "(legacy)"} · {fmtDuration(s.started_at, s.ended_at)}
                                {#if s.command_count > 0}
                                  · {s.command_count} cmd{s.command_count === 1 ? "" : "s"}
                                {:else if s.has_session_text}
                                  · session text only
                                {:else}
                                  · no captured output
                                {/if}
                              </span>
                              {#if isLive}
                                <span class="rounded-full bg-green-500/20 px-1.5 py-0.5 text-[10px] font-medium text-green-400">
                                  live
                                </span>
                              {:else if isLegacy}
                                <span class="rounded-full bg-amber-500/15 px-1.5 py-0.5 text-[10px] font-medium text-amber-400/80" title="Imported from a pre-redesign .log.zst — no per-command timing.">
                                  legacy
                                </span>
                              {/if}
                            </div>
                            {#if s.initial_cwd}
                              <div class="truncate text-xs text-muted-foreground" title={s.initial_cwd}>
                                {s.initial_cwd}
                              </div>
                            {/if}
                          </div>
                        </div>
                        {#if showEegColumns}
                          <div class="flex shrink-0 items-center gap-3 text-xs" data-testid="eeg-cols">
                            <span class="text-muted-foreground" title="Avg focus">
                              <span class="opacity-60">F</span> {fmtFocus(s.avg_focus)}
                            </span>
                            <span class="text-muted-foreground" title="Avg mood">
                              <span class="opacity-60">M</span> {fmtFocus(s.avg_mood)}
                            </span>
                          </div>
                        {/if}
                      </div>

                      {#if isOpen && exp}
                        <div class="ml-6 mt-1 space-y-1 border-l border-border/30 pl-3">
                          {#if exp.kind === "loading"}
                            <div class="py-2"><Spinner size="w-3 h-3" /></div>

                          {:else if exp.kind === "commands" && exp.commands}
                            {#each exp.commands as cmd}
                              <details class="group rounded border border-border/30 px-2 py-1">
                                <summary class="cursor-pointer list-none text-xs">
                                  <span class="text-muted-foreground font-mono">#{cmd.command_id}</span>
                                  <span class="ml-2 text-foreground/80">{snippet(cmd.stripped_text, 80)}</span>
                                </summary>
                                <pre class="mt-1 max-h-64 overflow-auto whitespace-pre-wrap font-mono text-[11px] leading-snug text-foreground/90">{cmd.stripped_text}</pre>
                              </details>
                            {/each}

                          {:else if exp.kind === "session_text" && exp.text !== undefined}
                            <p class="text-[11px] text-muted-foreground">
                              Stripped session text ({exp.textBytes ?? exp.text.length} chars). No per-command slicing — the source .log.zst predates timing-index sidecars.
                            </p>
                            <pre class="max-h-96 overflow-auto whitespace-pre-wrap rounded border border-border/30 bg-muted/30 p-2 font-mono text-[11px] leading-snug text-foreground/90">{exp.text || "(empty after ANSI strip)"}</pre>

                          {:else}
                            <p class="py-2 text-xs italic text-muted-foreground">
                              No output captured. The shell ran but its output stream wasn't recorded.
                            </p>
                          {/if}
                        </div>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {/if}

    <div class="flex justify-end">
      <Button size="sm" variant="ghost" onclick={loadSessions}>Refresh</Button>
    </div>
  </CardContent>
</SettingsCard>
