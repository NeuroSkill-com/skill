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
import SectionHeader from "$lib/components/ui/section-header/SectionHeader.svelte";
import SettingsCard from "$lib/components/ui/settings-card/SettingsCard.svelte";
import { CardContent } from "$lib/components/ui/card";
import { Button } from "$lib/components/ui/button";
import { Spinner } from "$lib/components/ui/spinner";
import { daemonPost } from "$lib/daemon/http";
import { onMount } from "svelte";

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

let sessions = $state<SessionRow[]>([]);
let orphanCount = $state(0);
let loading = $state(true);
let error = $state("");

let expanded = $state<Record<string, SessionExpansion | undefined>>({});

let searchQuery = $state("");
let searchMode = $state<"keyword" | "semantic">("keyword");
let searchResults = $state<{ id: number; text: string; score?: number }[] | null>(null);
let searching = $state(false);

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
  } catch (e: any) {
    error = e?.message ?? "Failed to load sessions";
  }
  loading = false;
}

async function toggleSession(s: SessionRow) {
  const id = s.id;
  if (expanded[id] && expanded[id]!.kind !== "loading") {
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
      const r = await daemonPost<{ commands: CommandRow[] }>(
        "/v1/brain/terminal-session-output",
        { sessionId: id, raw: false },
      );
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
      const r = await daemonPost<{ command_ids: number[] }>(
        "/v1/brain/terminal-search",
        { query: searchQuery, limit: 20 },
      );
      searchResults = await hydrateSearch(r.command_ids ?? []);
    } else {
      const r = await daemonPost<{ command_ids: number[]; scores: number[] }>(
        "/v1/brain/terminal-semantic-search",
        { query: searchQuery, limit: 20 },
      );
      const ids = r.command_ids ?? [];
      const scores = r.scores ?? [];
      const hydrated = await hydrateSearch(ids);
      hydrated.forEach((row, i) => (row.score = scores[i]));
      searchResults = hydrated;
    }
  } catch (e: any) {
    error = e?.message ?? "Search failed";
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
      const r = await daemonPost<{ found: boolean; stripped_text: string }>(
        "/v1/brain/terminal-output",
        { commandId: id, raw: false },
      );
      if (r.found) out.push({ id, text: r.stripped_text });
    } catch { /* skip on error */ }
  }
  return out;
}

function fmtTime(unix: number): string {
  return new Date(unix * 1000).toLocaleString([], {
    month: "short",
    day: "numeric",
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
  return t.length > max ? t.slice(0, max) + "…" : t;
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
      <div class="divide-y divide-border/30">
        {#each sessions as s}
          {@const exp = expanded[s.id]}
          {@const isOpen = exp !== undefined}
          {@const isLegacy = !s.shell && !!s.has_session_text}
          {@const isLive = s.ended_at == null}
          <div class="py-1.5">
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
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-sm">
                    <span class="font-medium">{fmtTime(s.started_at)}</span>
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
              <div class="flex shrink-0 items-center gap-3 text-xs">
                <span class="text-muted-foreground" title="Avg focus">
                  <span class="opacity-60">F</span> {fmtFocus(s.avg_focus)}
                </span>
                <span class="text-muted-foreground" title="Avg mood">
                  <span class="opacity-60">M</span> {fmtFocus(s.avg_mood)}
                </span>
              </div>
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

    <div class="flex justify-end">
      <Button size="sm" variant="ghost" onclick={loadSessions}>Refresh</Button>
    </div>
  </CardContent>
</SettingsCard>
