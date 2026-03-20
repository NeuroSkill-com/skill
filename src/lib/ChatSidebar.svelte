<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  ChatSidebar — conversation history panel with archive support.

  Exposes two methods via bind:this so the parent can push updates without
  forcing a full re-fetch:
    • refresh()                   – re-fetch the full list from the backend
    • updateTitle(id, title)      – patch a single item's title in-place
      (used for the auto-title applied when the first message is sent)
-->
<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke }        from "@tauri-apps/api/core";
  import { t }             from "$lib/i18n/index.svelte";
  import { fmtDate }       from "$lib/format";

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
  let archived   = $state<SessionSummary[]>([]);
  let showArchive = $state(false);
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
    if (showArchive) {
      try {
        archived = await invoke<SessionSummary[]>("list_archived_chat_sessions");
      } catch (e) { console.warn("[chat-sidebar] list_archived_chat_sessions failed:", e); }
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
    } catch (e) { console.warn("[chat-sidebar] rename_chat_session failed:", e); }
  }

  function cancelEdit(e?: KeyboardEvent) {
    if (e && e.key !== "Escape") return;
    editingId = null;
  }

  // ── Archive / Unarchive / Delete ───────────────────────────────────────────

  async function doArchive(id: number, e: MouseEvent) {
    e.stopPropagation();
    const session = sessions.find(s => s.id === id);
    sessions = sessions.filter(s => s.id !== id);
    if (session) archived = [session, ...archived];
    try { await invoke("archive_chat_session", { id }); } catch (e) { console.warn("[chat-sidebar] archive_chat_session failed:", e); }
    onDelete(id);
  }

  async function doUnarchive(id: number, e: MouseEvent) {
    e.stopPropagation();
    const session = archived.find(s => s.id === id);
    archived = archived.filter(s => s.id !== id);
    if (session) sessions = [session, ...sessions];
    try { await invoke("unarchive_chat_session", { id }); } catch (e) { console.warn("[chat-sidebar] unarchive_chat_session failed:", e); }
  }

  async function doDelete(id: number, e: MouseEvent) {
    e.stopPropagation();
    archived = archived.filter(s => s.id !== id);
    try { await invoke("delete_chat_session", { id }); } catch (e) { console.warn("[chat-sidebar] delete_chat_session failed:", e); }
  }

  async function toggleArchive() {
    showArchive = !showArchive;
    if (showArchive) {
      try {
        archived = await invoke<SessionSummary[]>("list_archived_chat_sessions");
      } catch (e) { console.warn("[chat-sidebar] list_archived_chat_sessions failed:", e); }
    }
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  function displayLabel(s: SessionSummary): string {
    if (s.title)   return s.title;
    if (s.preview) return s.preview;
    return t("chat.sidebar.newConvo");
  }

  function shortLabel(s: SessionSummary): string {
    const full = displayLabel(s);
    return full.length > 10 ? full.slice(0, 10) + "…" : full;
  }

  function relTime(ms: number): string {
    const diff = Date.now() - ms;
    const m = Math.floor(diff / 60_000);
    if (m < 1)  return t("chat.sidebar.justNow");
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 3_600);
    if (h < 24) return `${h}h ago`;
    const d = Math.floor(h / 24);
    if (d === 1) return t("chat.sidebar.yesterday");
    if (d < 7)  return `${d}d ago`;
    return fmtDate(Math.floor(ms / 1000));
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
      {t("chat.sidebar.chats")}
    </span>
    <button
      onclick={onNew}
      title={t("chat.btn.newChat")}
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

    {#if sessions.length === 0 && !showArchive}
      <p class="text-center text-[0.65rem] text-muted-foreground/40 px-3 py-6 leading-snug">
        {t("chat.sidebar.empty")}
      </p>
    {:else}
      <ul class="flex flex-col py-1">
        {#each sessions as s (s.id)}
          {@const isActive = s.id === activeId}
          {@const isEditing = editingId === s.id}

          <li>
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
                       ? 'bg-primary/10 dark:bg-primary/15'
                       : 'hover:bg-muted dark:hover:bg-white/[0.04]'}
                     cursor-pointer relative">

              {#if isActive}
                <span class="absolute left-0 top-2 bottom-2 w-0.5
                              rounded-full bg-primary"></span>
              {/if}

              <div class="flex-1 min-w-0 pr-6 pl-1.5">
                {#if isEditing}
                  <input
                    bind:this={editEl}
                    bind:value={editTitle}
                    onblur={commitEdit}
                    onkeydown={(e) => {
                      if (e.key === "Enter") { e.preventDefault(); commitEdit(); }
                      else cancelEdit(e);
                    }}
                    onclick={(e) => e.stopPropagation()}
                    class="w-full text-[0.72rem] font-medium bg-background border border-primary/40
                           rounded px-1.5 py-0.5 text-foreground focus:outline-none
                           focus:ring-1 focus:ring-primary/50"
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

              <!-- Archive button (hover only) -->
              {#if !isEditing}
                <button
                  onclick={(e) => doArchive(s.id, e)}
                  title={t("chat.sidebar.archive")}
                  class="absolute right-2 top-1/2 -translate-y-1/2
                         p-1 rounded-md transition-all cursor-pointer
                         opacity-0 group-hover:opacity-100
                         text-muted-foreground/40 hover:text-amber-500 hover:bg-amber-500/10">
                  <!-- Archive box icon -->
                  <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                       stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
                       class="w-3 h-3">
                    <rect x="1" y="2" width="14" height="4" rx="1"/>
                    <path d="M2 6v7a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V6"/>
                    <path d="M6 9h4"/>
                  </svg>
                </button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}

    <!-- Archive section -->
    <div class="border-t border-border dark:border-white/[0.06] mt-1">
      <button
        onclick={toggleArchive}
        class="w-full flex items-center gap-1.5 px-3 py-2 transition-colors cursor-pointer
               text-muted-foreground/50 hover:text-muted-foreground hover:bg-muted/50">
        <!-- Chevron -->
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
             stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
             class="w-2.5 h-2.5 shrink-0 transition-transform {showArchive ? 'rotate-90' : ''}">
          <polyline points="6 4 10 8 6 12"/>
        </svg>
        <!-- Archive icon -->
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
             stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
             class="w-3 h-3 shrink-0">
          <rect x="1" y="2" width="14" height="4" rx="1"/>
          <path d="M2 6v7a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V6"/>
          <path d="M6 9h4"/>
        </svg>
        <span class="text-[0.56rem] font-semibold uppercase tracking-widest">
          {t("chat.sidebar.archiveSection")}
        </span>
        {#if archived.length > 0}
          <span class="text-[0.5rem] tabular-nums opacity-60">{archived.length}</span>
        {/if}
      </button>

      {#if showArchive}
        {#if archived.length === 0}
          <p class="text-center text-[0.6rem] text-muted-foreground/30 px-3 py-3">
            {t("chat.sidebar.archiveEmpty")}
          </p>
        {:else}
          <ul class="flex flex-col pb-1">
            {#each archived as s (s.id)}
              <li>
                <div
                  role="button"
                  tabindex="0"
                  onclick={() => onSelect(s.id)}
                  onkeydown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onSelect(s.id);
                    }
                  }}
                  title={s.title || displayLabel(s)}
                  class="group w-full text-left flex items-start gap-0 px-3 py-1.5 transition-colors
                         hover:bg-muted dark:hover:bg-white/[0.04]
                         cursor-pointer relative opacity-60">

                  <div class="flex-1 min-w-0 pr-12 pl-1.5">
                    <p class="text-[0.68rem] font-medium text-foreground truncate leading-tight">
                      {shortLabel(s)}
                    </p>
                    <div class="flex items-center gap-1.5 mt-0.5">
                      <span class="text-[0.55rem] text-muted-foreground/50 shrink-0">
                        {relTime(s.created_at)}
                      </span>
                      {#if s.message_count > 0}
                        <span class="text-[0.48rem] text-muted-foreground/30 tabular-nums">
                          {s.message_count} msg{s.message_count !== 1 ? "s" : ""}
                        </span>
                      {/if}
                    </div>
                  </div>

                  <!-- Restore + Delete buttons (hover only) -->
                  <div class="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-0.5
                              opacity-0 group-hover:opacity-100 transition-all">
                    <!-- Restore -->
                    <button
                      onclick={(e) => doUnarchive(s.id, e)}
                      title={t("chat.sidebar.restore")}
                      class="p-1 rounded-md transition-colors cursor-pointer
                             text-muted-foreground/40 hover:text-primary hover:bg-primary/10">
                      <!-- Undo arrow icon -->
                      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                           stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
                           class="w-3 h-3">
                        <path d="M3 7h7a3 3 0 0 1 0 6H8"/>
                        <polyline points="6 4 3 7 6 10"/>
                      </svg>
                    </button>
                    <!-- Permanent delete -->
                    <button
                      onclick={(e) => doDelete(s.id, e)}
                      title={t("chat.sidebar.deletePermanent")}
                      class="p-1 rounded-md transition-colors cursor-pointer
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
                  </div>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    </div>
  </div>
</div>
