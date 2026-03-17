<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Chat message list — renders user and assistant message bubbles. -->
<script lang="ts">
  import { tick }            from "svelte";
  import { invoke }          from "@tauri-apps/api/core";
  import { t }               from "$lib/i18n/index.svelte";
  import { fmtMs }           from "$lib/format";
  import { cleanLeadInForDisplay } from "$lib/chat-utils";
  import MarkdownRenderer    from "$lib/MarkdownRenderer.svelte";
  import ChatToolCard        from "$lib/ChatToolCard.svelte";
  import type { Message, ServerStatus } from "$lib/chat-types";

  interface Props {
    messages: Message[];
    status: ServerStatus;
    generating: boolean;
    streamStartMs: number;
    streamTokens: number;
    /** Callback to update a single message by id */
    onUpdateMessage: (id: number, patch: Partial<Message>) => void;
    /** Callback to update a specific toolUse entry */
    onUpdateToolUse: (msgId: number, tuIdx: number, patch: Partial<import("$lib/chat-types").ToolUseEvent>) => void;
    onCancelToolCall: (msgId: number, tuIdx: number, toolCallId: string | undefined) => void;
    onEditAndResend: (msg: Message) => void;
    onRegenerate: () => void;
    onStartServer: () => void;
  }

  let {
    messages,
    status,
    generating,
    streamStartMs,
    streamTokens,
    onUpdateMessage,
    onUpdateToolUse,
    onCancelToolCall,
    onEditAndResend,
    onRegenerate,
    onStartServer,
  }: Props = $props();

  let msgsEl = $state<HTMLElement | null>(null);
  let pinned = $state(true);
  let copiedMsgId = $state<number | null>(null);

  const SNAP_PX = 48;

  export function scrollBottom(force = false) {
    tick().then(() => {
      if (msgsEl && (pinned || force)) msgsEl.scrollTop = msgsEl.scrollHeight;
    });
  }

  export function getElement() { return msgsEl; }

  function onMsgsScroll() {
    if (!msgsEl) return;
    pinned = msgsEl.scrollHeight - msgsEl.scrollTop - msgsEl.clientHeight < SNAP_PX;
  }

  function copyMessage(msg: Message) {
    navigator.clipboard.writeText(msg.content).catch(() => {});
    copiedMsgId = msg.id;
    setTimeout(() => { if (copiedMsgId === msg.id) copiedMsgId = null; }, 1500);
  }
</script>

<div class="relative flex-1 min-h-0">
<main bind:this={msgsEl}
      onscroll={onMsgsScroll}
      class="h-full overflow-y-auto px-4 py-4 flex flex-col gap-4
             scrollbar-thin scrollbar-track-transparent scrollbar-thumb-border">

  <!-- Empty state -->
  {#if messages.length === 0}
    <div class="flex flex-col items-center justify-center flex-1 gap-4 text-center py-12">
      <div class="w-14 h-14 rounded-2xl bg-violet-500/10 flex items-center justify-center">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
             class="w-7 h-7 text-violet-500">
          <path stroke-linecap="round" stroke-linejoin="round"
                d="M8.625 12a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Zm4.125 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Zm4.125 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Z"/>
          <path stroke-linecap="round" stroke-linejoin="round"
                d="M12 21a9 9 0 1 0-9-9c0 1.657.45 3.208 1.236 4.54L3 21l4.46-1.236A8.967 8.967 0 0 0 12 21Z"/>
        </svg>
      </div>
      {#if status === "stopped"}
        <div class="flex flex-col items-center gap-2">
          <p class="text-[0.82rem] font-semibold text-foreground">{t("chat.empty.stopped")}</p>
          <p class="text-[0.7rem] text-muted-foreground max-w-xs leading-relaxed">
            {t("chat.empty.stoppedHint")}
          </p>
          <div class="flex gap-2 mt-1">
            <button
              onclick={onStartServer}
              class="px-4 py-2 rounded-xl bg-violet-600 hover:bg-violet-700
                     text-white text-[0.72rem] font-semibold transition-colors cursor-pointer">
              {t("chat.btn.startServer")}
            </button>
            <button
              onclick={() => invoke("open_settings_window").then(() => invoke("open_model_tab")).catch(() => {})}
              class="px-4 py-2 rounded-xl border border-violet-500/40
                     text-violet-600 dark:text-violet-400 text-[0.72rem] font-semibold
                     hover:bg-violet-500/10 transition-colors cursor-pointer">
              {t("chat.noModelBtn")}
            </button>
          </div>
        </div>
      {:else if status === "loading"}
        <div class="flex flex-col items-center gap-2">
          <p class="text-[0.82rem] font-semibold text-foreground">{t("chat.status.loading")}</p>
          <p class="text-[0.7rem] text-muted-foreground">
            {t("chat.empty.loadingHint")}
          </p>
          <div class="mt-1 flex gap-1">
            {#each [0,1,2] as i}
              <span class="w-2 h-2 rounded-full bg-violet-500/60 animate-bounce"
                    style="animation-delay: {i * 0.15}s"></span>
            {/each}
          </div>
        </div>
      {:else}
        <p class="text-[0.8rem] text-muted-foreground">{t("chat.empty.ready")}</p>
      {/if}
    </div>

  {:else}
    {#each messages as msg (msg.id)}
      <!-- User message -->
      {#if msg.role === "user"}
        <div class="flex justify-end">
          <div class="flex flex-col items-end gap-1.5 max-w-[78%]">
            {#if msg.attachments?.length}
              <div class="flex flex-wrap gap-1.5 justify-end">
                {#each msg.attachments as att}
                  <img src={att.dataUrl} alt={att.name}
                       class="h-28 max-w-[14rem] rounded-xl object-cover border border-white/20 shadow-sm" />
                {/each}
              </div>
            {/if}
            {#if msg.content}
              <div class="group/user-bubble">
                <div class="rounded-2xl rounded-tr-sm bg-violet-600 text-white
                            px-3.5 py-2.5 text-[0.78rem] leading-relaxed whitespace-pre-wrap break-words">
                  {msg.content}
                </div>
                {#if status === "running" && !generating}
                  <div class="flex justify-end opacity-0 group-hover/user-bubble:opacity-100
                              transition-opacity duration-150 mt-0.5">
                    <button
                      onclick={() => onEditAndResend(msg)}
                      title={t("chat.btn.editResend")}
                      class="flex items-center gap-1 px-1.5 py-0.5 rounded-md
                             text-muted-foreground/50 hover:text-violet-600 dark:hover:text-violet-400
                             hover:bg-violet-500/10 transition-colors cursor-pointer text-[0.6rem]">
                      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                           stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
                           class="w-3 h-3">
                        <path d="M11 2l3 3-8 8H3v-3z"/>
                      </svg>
                      {t("chat.btn.editResend")}
                    </button>
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        </div>

      <!-- Assistant message -->
      {:else if msg.role === "assistant"}
        <div class="flex justify-start gap-2.5">
          {#if msg.pending}
            <div class="w-6 h-6 shrink-0 mt-0.5 flex items-center justify-center">
              <svg class="w-5 h-5 animate-spin text-violet-500" viewBox="0 0 24 24" fill="none">
                <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2.5" class="opacity-20"/>
                <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" class="opacity-80"/>
              </svg>
            </div>
          {:else}
            <div class="w-6 h-6 rounded-full bg-gradient-to-br from-violet-500 to-indigo-600
                        flex items-center justify-center shrink-0 mt-0.5 text-white text-[0.55rem] font-bold">
              AI
            </div>
          {/if}

          <div class="flex flex-col gap-1 max-w-[82%]">
            <!-- Thinking block -->
            {#if msg.thinking || (msg.pending && msg.content === "" && !msg.thinking && !msg.toolUses?.length && !msg.leadIn?.trim())}
              <div class="rounded-xl border border-violet-500/20 bg-violet-500/5
                          text-[0.7rem] overflow-hidden">
                <button
                  onclick={() => onUpdateMessage(msg.id, { thinkOpen: !msg.thinkOpen })}
                  class="w-full flex items-center gap-1.5 px-3 py-1.5 text-left
                         text-violet-600 dark:text-violet-400 hover:bg-violet-500/10
                         transition-colors cursor-pointer">
                  {#if msg.pending && !msg.thinking?.trim()}
                    <span class="flex gap-0.5">
                      {#each [0,1,2] as i}
                        <span class="w-1 h-1 rounded-full bg-violet-400 animate-bounce"
                              style="animation-delay:{i*0.12}s"></span>
                      {/each}
                    </span>
                    <span class="text-[0.65rem]">{t("chat.thinking")}</span>
                  {:else}
                    <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3 shrink-0
                         transition-transform {msg.thinkOpen ? 'rotate-90' : ''}">
                      <path d="M6 3l5 5-5 5V3z"/>
                    </svg>
                    <span class="text-[0.65rem] font-medium">
                      {msg.pending ? t("chat.thinking") : t("chat.thought")}
                    </span>
                    {#if !msg.pending && msg.thinking}
                      <span class="ml-auto text-[0.6rem] text-muted-foreground/50">
                        {t("chat.words", { count: msg.thinking.trim().split(/\s+/).length })}
                      </span>
                    {/if}
                  {/if}
                </button>
                {#if msg.thinkOpen && msg.thinking}
                  <div class="px-3 pb-2 pt-0 text-muted-foreground/70 leading-relaxed
                              border-t border-violet-500/10 text-[0.68rem] break-words overflow-hidden">
                    <MarkdownRenderer content={msg.thinking} className="mdr-muted" />
                  </div>
                {/if}
              </div>
            {/if}

            <!-- Lead-in bubble -->
            {#if cleanLeadInForDisplay(msg.leadIn ?? "", !!msg.toolUses?.length)}
              <div class="rounded-2xl rounded-tl-sm border border-border/70 bg-background/80
                          px-3 py-2 text-[0.72rem] leading-relaxed text-muted-foreground
                          whitespace-pre-wrap break-words">
                {cleanLeadInForDisplay(msg.leadIn ?? "", !!msg.toolUses?.length)}
              </div>
            {/if}

            <!-- Tool-use cards -->
            {#if msg.toolUses?.length}
              <div class="flex flex-col gap-1.5">
                {#each msg.toolUses as tu, tuIdx}
                  <ChatToolCard
                    {tu}
                    onToggleExpand={() => onUpdateToolUse(msg.id, tuIdx, { expanded: !tu.expanded })}
                    onCancel={() => onCancelToolCall(msg.id, tuIdx, tu.toolCallId)}
                  />
                {/each}
              </div>
            {/if}

            <!-- Response bubble -->
            {#if msg.content.trim()}
              <div class="group/bubble flex flex-col gap-0.5">
                <div class="rounded-2xl rounded-tl-sm bg-muted dark:bg-[#1a1a28]
                            px-3.5 py-2.5 text-[0.78rem] leading-relaxed text-foreground
                            break-words overflow-hidden">
                  <MarkdownRenderer content={msg.content} pending={msg.pending} />
                </div>
                {#if !msg.pending && msg.content}
                  <div class="flex gap-1 opacity-0 group-hover/bubble:opacity-100 transition-opacity duration-150">
                    <button
                      onclick={() => copyMessage(msg)}
                      title="Copy"
                      class="flex items-center gap-1 px-1.5 py-0.5 rounded-md
                             text-muted-foreground/50 hover:text-muted-foreground
                             hover:bg-muted transition-colors cursor-pointer text-[0.6rem]">
                      {#if copiedMsgId === msg.id}
                        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                             stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                             class="w-3 h-3 text-emerald-500">
                          <polyline points="2 8 6 12 14 4"/>
                        </svg>
                        <span class="text-emerald-500">Copied</span>
                      {:else}
                        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                             stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
                             class="w-3 h-3">
                          <rect x="5" y="5" width="9" height="9" rx="1.5"/>
                          <path d="M11 5V3.5A1.5 1.5 0 0 0 9.5 2h-6A1.5 1.5 0 0 0 2 3.5v6A1.5 1.5 0 0 0 3.5 11H5"/>
                        </svg>
                        Copy
                      {/if}
                    </button>
                    {#if msg.id === messages[messages.length - 1]?.id && status === "running" && !generating}
                      <button
                        onclick={onRegenerate}
                        title={t("chat.btn.regenerate")}
                        class="flex items-center gap-1 px-1.5 py-0.5 rounded-md
                               text-muted-foreground/50 hover:text-violet-600 dark:hover:text-violet-400
                               hover:bg-violet-500/10 transition-colors cursor-pointer text-[0.6rem]">
                        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                             stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"
                             class="w-3 h-3">
                          <path d="M13.5 8A5.5 5.5 0 1 1 8 2.5"/>
                          <polyline points="13.5 2.5 13.5 6 10 6"/>
                        </svg>
                        {t("chat.btn.regenerate")}
                      </button>
                    {/if}
                  </div>
                {/if}
              </div>
            {/if}

            <!-- Live tok/s -->
            {#if msg.pending && generating && streamTokens > 0}
              {@const elapsedSec = (performance.now() - streamStartMs) / 1000}
              {@const tokSec = elapsedSec > 0.1 ? (streamTokens / elapsedSec).toFixed(1) : "…"}
              <span class="text-[0.55rem] text-violet-500/70 tabular-nums px-1 animate-pulse">
                {tokSec} {t("chat.tokSec")}
              </span>
            {/if}

            <!-- Timing + context usage -->
            {#if !msg.pending && (msg.elapsed !== undefined || msg.usage)}
              <div class="flex flex-col gap-1 px-1">
                {#if msg.elapsed !== undefined}
                  <span class="text-[0.55rem] text-muted-foreground/50">
                    {fmtMs(msg.elapsed)}
                    {#if msg.ttft !== undefined} · {t("chat.firstToken")} {fmtMs(msg.ttft)}{/if}
                    {#if msg.usage}
                       · {msg.usage.prompt_tokens}+{msg.usage.completion_tokens} {t("chat.tok")}
                       {#if msg.elapsed && msg.usage.completion_tokens > 0}
                         · {(msg.usage.completion_tokens / (msg.elapsed / 1000)).toFixed(1)} {t("chat.tokSec")}
                       {/if}
                    {/if}
                  </span>
                {/if}
                {#if msg.usage && msg.usage.n_ctx > 0}
                  {@const usedPct = Math.round((msg.usage.total_tokens / msg.usage.n_ctx) * 100)}
                  {@const barColor = usedPct >= 90 ? "bg-red-500"
                                   : usedPct >= 70 ? "bg-amber-500"
                                   :                 "bg-emerald-500"}
                  <div class="flex items-center gap-1.5" title="{t('chat.ctxUsage')}: {msg.usage.total_tokens} / {msg.usage.n_ctx} {t('chat.tok')} ({usedPct}%)">
                    <div class="flex-1 h-1 rounded-full bg-muted overflow-hidden">
                      <div class="h-full rounded-full {barColor} transition-all"
                           style="width: {Math.min(usedPct, 100)}%"></div>
                    </div>
                    <span class="text-[0.5rem] tabular-nums text-muted-foreground/40 shrink-0">
                      {msg.usage.total_tokens}/{msg.usage.n_ctx}
                    </span>
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        </div>
      {/if}
    {/each}
  {/if}

</main>

<!-- Jump-to-bottom button -->
{#if !pinned}
  <button
    onclick={() => { pinned = true; msgsEl && (msgsEl.scrollTop = msgsEl.scrollHeight); }}
    aria-label="Scroll to bottom"
    class="absolute bottom-3 left-1/2 -translate-x-1/2
           flex items-center gap-1.5 px-3 py-1.5 rounded-full
           bg-background border border-border shadow-md
           text-[0.65rem] font-medium text-muted-foreground
           hover:text-foreground hover:border-violet-500/40 hover:shadow-violet-500/10
           transition-all cursor-pointer select-none">
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
         stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
      <line x1="8" y1="2" x2="8" y2="13"/>
      <polyline points="4 9 8 13 12 9"/>
    </svg>
    Jump to bottom
  </button>
{/if}
</div>
