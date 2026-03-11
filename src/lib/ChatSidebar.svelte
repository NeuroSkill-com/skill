<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  ChatSidebar — conversation history panel.

  Exposes two methods via bind:this so the parent can push updates without
  forcing a full re-fetch:
    • refresh()                   – re-fetch the full list from the backend
    • updateTitle(id, title)      – patch a single item's title in-place
      (used for the auto-title applied when the first message is sent)
-->
<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke }        from "@tauri-apps/api/core";

  // ── Types ──────────────────────────────────────────────────────────────────

  export interface SessionSummary {
    id:            number;
    title:         string;
    preview:       string;
    created_at:    number;
    message_count: number;
  }

  // ── Props ──────────────────────────────────────────────────────────────────

  let {
    activeId,
    onSelect,
    onNew,
    onDelete,
  }: {
    activeId:  number;
    onSelect:  (id: number) => void;
    onNew:     () => void;
    onDelete:  (id: number) => void;
  } = $props();

  // ── State ──────────────────────────────────────────────────────────────────

  let sessions   = $state<SessionSummary[]>([]);
  let editingId  = $state<number | null>(null);
  let editTitle  = $state("");
  let editEl     = $state<HTMLInputElement | null>(null);

  // ── Exposed API (bind:this) ────────────────────────────────────────────────

  /** Reload the session list from the backend. */
  export async function refresh() {
    try {
      sessions = await invoke<SessionSummary[]>("list_chat_sessions");
    } catch (e) {
      console.error("[ChatSidebar] list_chat_sessions:", e);
    }
  }

  /** Patch the title of a single session in the local list (no round-trip). */
  export function updateTitle(id: number, title: string) {
    sessions = sessions.map(s => s.id === id ? { ...s, title } : s);
  }

  // ── Inline rename ──────────────────────────────────────────────────────────

  async function startEdit(s: SessionSummary, e: MouseEvent) {
    e.stopPropagation();
    editingId = s.id;
    editTitle = s.title || displayLabel(s);
    await tick();
    editEl?.focus();
    editEl?.select();
  }

  async function commitEdit() {
    const id = editingId;
    editingId = null;
    if (id === null) return;
    const title = editTitle.trim();
    if (!title) return;
    try {
      await invoke("rename_chat_session", { id, title });
      sessions = sessions.map(s => s.id === id ? { ...s, title } : s);
    } catch {}
  }

  function cancelEdit(e?: KeyboardEvent) {
    if (e && e.key !== "Escape") return;
    editingId = null;
  }

  // ── Delete ─────────────────────────────────────────────────────────────────

  async function doDelete(id: number, e: MouseEvent) {
    e.stopPropagation();
    sessions = sessions.filter(s => s.id !== id);
    try { await invoke("delete_chat_session", { id }); } catch {}
    onDelete(id);
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  /** Full label used for the native tooltip and the inline rename seed. */
  function displayLabel(s: SessionSummary): string {
    if (s.title)   return s.title;
    if (s.preview) return s.preview;
    return "New conversation";
  }

  /** Truncated label shown in the list — max 10 chars + ellipsis. */
  function shortLabel(s: SessionSummary): string {
    const full = displayLabel(s);
    return full.length > 10 ? full.slice(0, 10) + "…" : full;
  }

  function relTime(ms: number): string {
    const diff = Date.now() - ms;
    const m = Math.floor(diff / 60_000);
    if (m < 1)  return "just now";
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 3_600);
    if (h < 24) return `${h}h ago`;
    const d = Math.floor(h / 24);
    if (d === 1) return "yesterday";
    if (d < 7)  return `${d}d ago`;
    return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  onMount(refresh);
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<div class="flex flex-col h-full select-none">

  <!-- Header -->
  <div class="flex items-center justify-between gap-1
              px-3 py-2.5 shrink-0
              border-b border-border dark:border-white/[0.06]">
    <span class="text-[0.56rem] font-semibold uppercase tracking-widest text-muted-foreground">
      Chats
    </span>
    <button
      onclick={onNew}
      title="New conversation"
      class="p-1 rounded-md text-muted-foreground/60
             hover:text-foreground hover:bg-muted transition-colors cursor-pointer">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
           stroke-width="2" stroke-linecap="round" class="w-3 h-3">
        <line x1="8" y1="2" x2="8" y2="14"/>
        <line x1="2" y1="8" x2="14" y2="8"/>
      </svg>
    </button>
  </div>

  <!-- Session list -->
  <div class="flex-1 overflow-y-auto
              scrollbar-thin scrollbar-track-transparent scrollbar-thumb-border">

    {#if sessions.length === 0}
      <p class="text-center text-[0.65rem] text-muted-foreground/40 px-3 py-6 leading-snug">
        No conversations yet.<br/>Start chatting to create one.
      </p>
    {:else}
      <ul class="flex flex-col py-1">
        {#each sessions as s (s.id)}
          {@const isActive = s.id === activeId}
          {@const isEditing = editingId === s.id}

          <li>
            <!--
              Row is a <div> with role="button" so the delete <button> inside it
              is valid HTML (a <button> cannot be a descendant of another <button>).
            -->
            <div
              role="button"
              tabindex="0"
              onclick={() => { if (!isEditing) onSelect(s.id); }}
              ondblclick={(e) => startEdit(s, e)}
              onkeydown={(e) => {
                if (!isEditing && (e.key === "Enter" || e.key === " ")) {
                  e.preventDefault();
                  onSelect(s.id);
                }
              }}
              title={isEditing ? undefined : (s.title || displayLabel(s))}
              class="group w-full text-left flex items-start gap-0 px-3 py-2 transition-colors
                     {isActive
                       ? 'bg-violet-500/10 dark:bg-violet-500/15'
                       : 'hover:bg-muted dark:hover:bg-white/[0.04]'}
                     cursor-pointer relative">

              <!-- Active indicator bar -->
              {#if isActive}
                <span class="absolute left-0 top-2 bottom-2 w-0.5
                              rounded-full bg-violet-500"></span>
              {/if}

              <!-- Text content -->
              <div class="flex-1 min-w-0 pr-6 pl-1.5">
                {#if isEditing}
                  <!-- Inline title editor -->
                  <input
                    bind:this={editEl}
                    bind:value={editTitle}
                    onblur={commitEdit}
                    onkeydown={(e) => {
                      if (e.key === "Enter") { e.preventDefault(); commitEdit(); }
                      else cancelEdit(e);
                    }}
                    onclick={(e) => e.stopPropagation()}
                    class="w-full text-[0.72rem] font-medium bg-background border border-violet-500/40
                           rounded px-1.5 py-0.5 text-foreground focus:outline-none
                           focus:ring-1 focus:ring-violet-500/50"
                  />
                {:else}
                  <p class="text-[0.72rem] font-medium text-foreground truncate leading-tight">
                    {shortLabel(s)}
                  </p>
                {/if}

                <div class="flex items-center gap-1.5 mt-0.5">
                  <span class="text-[0.58rem] text-muted-foreground/50 shrink-0">
                    {relTime(s.created_at)}
                  </span>
                  {#if s.message_count > 0}
                    <span class="text-[0.52rem] text-muted-foreground/30 tabular-nums">
                      {s.message_count} msg{s.message_count !== 1 ? "s" : ""}
                    </span>
                  {/if}
                </div>
              </div>

              <!-- Delete button (hover only) — valid here because the row is a <div> not a <button> -->
              {#if !isEditing}
                <button
                  onclick={(e) => doDelete(s.id, e)}
                  title="Delete conversation"
                  class="absolute right-2 top-1/2 -translate-y-1/2
                         p-1 rounded-md transition-all cursor-pointer
                         opacity-0 group-hover:opacity-100
                         text-muted-foreground/40 hover:text-red-500 hover:bg-red-500/10">
                  <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                       stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
                       class="w-3 h-3">
                    <polyline points="2 4 4 4 14 4"/>
                    <path d="M5 4V2h6v2"/>
                    <path d="M6 7v5M10 7v5"/>
                    <rect x="3" y="4" width="10" height="10" rx="1.5"/>
                  </svg>
                </button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
