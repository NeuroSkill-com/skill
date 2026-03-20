<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Full context viewer modal — shows the entire prompt as it would be sent to the LLM. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { type Message, type BandSnapshot, type ToolConfig,
           buildUserContent, estimateTokens } from "$lib/chat-types";
  import { buildEegBlock } from "$lib/chat-eeg";

  interface Props {
    messages: Message[];
    systemPrompt: string;
    eegActive: boolean;
    latestBands: BandSnapshot | null;
    toolConfig: ToolConfig;
    supportsTools: boolean;
    nCtx: number;
    onClose: () => void;
  }

  let {
    messages, systemPrompt, eegActive, latestBands,
    toolConfig, supportsTools, nCtx, onClose,
  }: Props = $props();

  /** Reconstruct the message list as it would be sent to the API. */
  const apiMessages = $derived.by(() => {
    const result: { role: string; content: string; label?: string }[] = [];

    // System prompt
    const systemParts: string[] = [];
    if (systemPrompt.trim()) systemParts.push(systemPrompt.trim());
    if (eegActive && latestBands) systemParts.push(buildEegBlock(latestBands));
    if (systemParts.length) {
      result.push({ role: "system", content: systemParts.join("\n\n"), label: t("chat.ctx.system") });
    }

    // Conversation messages
    for (const m of messages) {
      if (m.pending) continue;

      if (m.role === "user") {
        const content = m.attachments?.length
          ? JSON.stringify(buildUserContent(m.content, m.attachments), null, 2)
          : m.content;
        result.push({ role: "user", content, label: t("chat.ctx.user") });
      } else if (m.role === "assistant") {
        // Reconstruct full assistant output including thinking
        let full = "";
        if (m.leadIn) full += m.leadIn + "\n";
        if (m.thinking) full += `<think>\n${m.thinking}\n</think>\n`;
        full += m.content;

        result.push({ role: "assistant", content: full, label: t("chat.ctx.assistant") });

        // Tool uses injected as separate messages
        if (m.toolUses) {
          for (const tu of m.toolUses) {
            if (tu.args) {
              result.push({
                role: "assistant",
                content: `[Tool Call: ${tu.tool}]\n${JSON.stringify(tu.args, null, 2)}`,
                label: `${t("chat.ctx.toolDefs")} — ${tu.tool}`,
              });
            }
            if (tu.result) {
              result.push({
                role: "tool",
                content: typeof tu.result === "string" ? tu.result : JSON.stringify(tu.result, null, 2),
                label: `${t("chat.ctx.toolResults")} — ${tu.tool}`,
              });
            }
          }
        }
      }
    }

    return result;
  });

  const totalTokens = $derived(
    apiMessages.reduce((acc, m) => acc + estimateTokens(m.content) + 10, 0)
  );

  let copied = $state(false);

  function copyAll() {
    const text = apiMessages
      .map(m => `--- [${m.role.toUpperCase()}] ---\n${m.content}`)
      .join("\n\n");
    navigator.clipboard.writeText(text).then(() => {
      copied = true;
      setTimeout(() => copied = false, 2000);
    });
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { e.stopPropagation(); onClose(); }
  }

  /** Role → accent color for the left border. */
  function roleColor(role: string): string {
    switch (role) {
      case "system":    return "#8b5cf6";
      case "user":      return "#3b82f6";
      case "assistant": return "#10b981";
      case "tool":      return "#ef4444";
      default:          return "#64748b";
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />
<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="fixed inset-0 z-[100] flex items-center justify-center" onclick={onClose}>
  <!-- Backdrop -->
  <div class="absolute inset-0 bg-black/50 backdrop-blur-sm animate-in fade-in duration-150"></div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- Modal -->
  <div
    class="relative z-10 flex flex-col w-[90vw] max-w-4xl h-[85vh]
           rounded-2xl shadow-2xl border border-border dark:border-white/10
           bg-white dark:bg-[#161622]
           animate-in fade-in zoom-in-95 duration-200"
    onclick={(e) => e.stopPropagation()}
  >
    <!-- Header -->
    <div class="flex items-center justify-between px-5 py-3.5 border-b border-border dark:border-white/[0.06] shrink-0">
      <div class="flex items-center gap-3">
        <h2 class="text-sm font-semibold text-foreground">{t("chat.ctx.viewerTitle")}</h2>
        <span class="text-xs text-muted-foreground tabular-nums">
          ~{totalTokens.toLocaleString()} {t("chat.ctx.tokens")} / {nCtx.toLocaleString()} {t("chat.ctx.nCtx")}
        </span>
      </div>
      <div class="flex items-center gap-2">
        <!-- Copy button -->
        <button
          class="flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-md
                 text-muted-foreground hover:text-foreground hover:bg-muted/50
                 transition-colors"
          onclick={copyAll}
        >
          {#if copied}
            <!-- Checkmark -->
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                 stroke-linecap="round" stroke-linejoin="round" class="size-3.5">
              <polyline points="20 6 9 17 4 12" />
            </svg>
            {t("chat.ctx.copied")}
          {:else}
            <!-- Copy icon -->
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                 stroke-linecap="round" stroke-linejoin="round" class="size-3.5">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
            {t("chat.ctx.copy")}
          {/if}
        </button>
        <!-- Close button -->
        <button
          class="p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
          onclick={onClose}
          aria-label="Close"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
               stroke-linecap="round" stroke-linejoin="round" class="size-4">
            <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Message list -->
    <div class="flex-1 overflow-y-auto px-5 py-4 space-y-3">
      {#each apiMessages as msg, i (i)}
        {@const tokens = estimateTokens(msg.content) + 10}
        <div class="rounded-lg border border-border/50 dark:border-white/[0.04] overflow-hidden">
          <!-- Message header -->
          <div
            class="flex items-center justify-between px-3 py-1.5 text-xs"
            style="background: {roleColor(msg.role)}15; border-left: 3px solid {roleColor(msg.role)};"
          >
            <div class="flex items-center gap-2">
              <span class="font-semibold uppercase tracking-wide" style="color: {roleColor(msg.role)};">
                {msg.role}
              </span>
              {#if msg.label}
                <span class="text-muted-foreground">— {msg.label}</span>
              {/if}
            </div>
            <span class="text-muted-foreground/60 tabular-nums">~{tokens.toLocaleString()} {t("chat.ctx.tokens")}</span>
          </div>
          <!-- Message content -->
          <pre class="px-3 py-2.5 text-xs leading-relaxed text-foreground/90
                      whitespace-pre-wrap break-words font-mono
                      max-h-80 overflow-y-auto
                      bg-muted/20 dark:bg-white/[0.02]">{msg.content}</pre>
        </div>
      {/each}

      {#if apiMessages.length === 0}
        <div class="flex items-center justify-center h-32 text-sm text-muted-foreground">
          {t("chat.ctx.empty")}
        </div>
      {/if}
    </div>

    <!-- Footer -->
    <div class="px-5 py-3 border-t border-border dark:border-white/[0.06] shrink-0
                flex items-center justify-between text-xs text-muted-foreground">
      <span>{apiMessages.length} {t("chat.ctx.messagesCount")}</span>
      <span class="tabular-nums">
        ~{totalTokens.toLocaleString()} / {nCtx.toLocaleString()} {t("chat.ctx.tokens")}
        ({nCtx > 0 ? ((totalTokens / nCtx) * 100).toFixed(1) : "0.0"}%)
      </span>
    </div>
  </div>
</div>
