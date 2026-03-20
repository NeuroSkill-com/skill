<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Chat window — Ollama-style interface for the embedded LLM server.

  Architecture:
  • Token streaming goes through `invoke("chat_completions_ipc", {channel})` —
    a Tauri IPC Channel — instead of a raw HTTP fetch, avoiding CORS entirely.
  • `invoke("get_llm_server_status")` polls server state.
  • `invoke("start_llm_server")` / `invoke("stop_llm_server")` control the actor.
  • `listen("llm:status")` gives real-time loading → running → stopped events.

  Components:
  • ChatHeader — top bar (sidebar toggle, tools/EEG badges, server controls)
  • ChatSettingsPanel — system prompt, EEG context, thinking level, gen params
  • ChatToolsPanel — tool allow-list, execution mode, limits
  • ChatMessageList — message bubbles with thinking, tool cards, markdown
  • ChatInputBar — textarea, image attachments, prompt library
-->
<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { invoke, Channel }          from "@tauri-apps/api/core";
  import { listen }                   from "@tauri-apps/api/event";

  import ChatSidebar                  from "$lib/ChatSidebar.svelte";
  import ChatHeader                   from "$lib/ChatHeader.svelte";
  import ChatSettingsPanel            from "$lib/ChatSettingsPanel.svelte";
  import ChatToolsPanel               from "$lib/ChatToolsPanel.svelte";
  import ChatMessageList              from "$lib/ChatMessageList.svelte";
  import ChatInputBar                 from "$lib/ChatInputBar.svelte";
  import ChatContextBreakdown, { type ContextSegment } from "$lib/ChatContextBreakdown.svelte";
  import ChatContextViewer            from "$lib/ChatContextViewer.svelte";
  import { t }                        from "$lib/i18n/index.svelte";
  import { chatTitlebarState }        from "$lib/chat-titlebar.svelte";
  import { buildEegBlock }            from "$lib/chat-eeg";
  import { parseAssistantOutput }     from "$lib/chat-utils";
  import {
    type Role, type ServerStatus, type Message, type ToolUseEvent,
    type Attachment, type UsageInfo, type ToolConfig,
    type ThinkingLevel, type BandSnapshot,
    type ChatChunk, type ChatSessionResponse, type StoredMessage,
    type ServerStatusPayload,
    THINKING_LEVELS, DEFAULT_TOOL_CONFIG,
    SYSTEM_PROMPT_DEFAULT, SYSTEM_PROMPT_KEY,
    storedToMessage, buildUserContent, estimateTokens,
  } from "$lib/chat-types";

  // ── State ──────────────────────────────────────────────────────────────────

  let status         = $state<ServerStatus>("stopped");
  let modelName      = $state("");
  let nCtx           = $state(0);
  let supportsVision = $state(false);
  let supportsTools  = $state(false);
  let toolConfig     = $state<ToolConfig>({ ...DEFAULT_TOOL_CONFIG });
  let messages       = $state<Message[]>([]);
  let sessionId      = $state(0);
  let input          = $state("");

  // ── System prompt ──────────────────────────────────────────────────────────
  function loadSystemPrompt(): string {
    try { return localStorage.getItem(SYSTEM_PROMPT_KEY) ?? SYSTEM_PROMPT_DEFAULT; }
    catch { return SYSTEM_PROMPT_DEFAULT; }
  }
  let systemPrompt = $state(loadSystemPrompt());
  $effect(() => { try { localStorage.setItem(SYSTEM_PROMPT_KEY, systemPrompt); } catch (e) { console.warn("[chat] persist system prompt failed:", e); } });

  /** Keep the titlebar model name + status in sync. */
  $effect(() => { chatTitlebarState.modelName = modelName; chatTitlebarState.status = status; });

  // ── Tool config ────────────────────────────────────────────────────────────
  const enabledToolCount = $derived(
    toolConfig.enabled
      ? [toolConfig.date, toolConfig.location, toolConfig.web_search, toolConfig.web_fetch,
         toolConfig.bash, toolConfig.read_file, toolConfig.write_file, toolConfig.edit_file,
         toolConfig.skill_api]
          .filter(Boolean).length
      : 0
  );

  async function updateToolConfig(patch: Partial<ToolConfig>) {
    toolConfig = { ...toolConfig, ...patch };
    try {
      const cfg = await invoke<any>("get_llm_config");
      cfg.tools = { ...toolConfig };
      await invoke("set_llm_config", { config: cfg });
    } catch (e) { console.warn("[chat] failed to persist tool config:", e); }
  }

  // ── Generation state ───────────────────────────────────────────────────────
  let generating     = $state(false);
  let aborting       = $state(false);
  let streamStartMs  = $state(0);
  let streamTokens   = $state(0);
  let realPromptTokens     = $state<number | null>(null);
  let streamCompletionToks = $state(0);
  let msgId          = $state(0);

  // ── Component refs ─────────────────────────────────────────────────────────
  let msgListRef     = $state<ChatMessageList | null>(null);
  let inputBarRef    = $state<ChatInputBar | null>(null);
  let sidebarRef     = $state<ChatSidebar | null>(null);
  let attachments    = $state<Attachment[]>([]);

  // ── Sidebar ────────────────────────────────────────────────────────────────
  let sidebarOpen = $state(true);

  // ── EEG context ────────────────────────────────────────────────────────────
  let latestBands  = $state<BandSnapshot | null>(null);
  let eegContext   = $state(true);
  const eegActive  = $derived(eegContext && latestBands !== null);

  // ── Input history navigation ───────────────────────────────────────────────
  let histIdx        = $state(-1);
  let histDraft      = $state("");
  const userHistory = $derived(
    messages
      .filter(m => m.role === "user" && m.content.trim())
      .map(m => m.content)
      .reverse()
      .filter((c, i, a) => i === 0 || c !== a[i - 1])
  );

  // ── Settings panel ─────────────────────────────────────────────────────────
  let showSettings   = $state(false);
  let showTools      = $state(false);
  let showContextBreakdown = $state(false);
  let showContextViewer    = $state(false);
  let temperature    = $state(0.8);
  let maxTokens      = $state(2048);
  let topK           = $state(40);
  let topP           = $state(0.9);
  let thinkingLevel  = $state<ThinkingLevel>("minimal");

  /** Auto-save generation params when they change. */
  let paramsSaveTimer: ReturnType<typeof setTimeout> | undefined;
  const paramsSig = $derived(`${temperature}|${maxTokens}|${topK}|${topP}|${thinkingLevel}`);
  $effect(() => {
    void paramsSig;
    clearTimeout(paramsSaveTimer);
    paramsSaveTimer = setTimeout(() => saveSessionParams(), 500);
  });

  async function saveSessionParams() {
    if (sessionId <= 0) return;
    const p = { temperature, maxTokens, topK, topP, thinkingLevel };
    try { await invoke("set_session_params", { id: sessionId, paramsJson: JSON.stringify(p) }); } catch (e) { console.warn("[chat] failed to save session params:", e); }
  }

  async function loadSessionParams(id: number): Promise<boolean> {
    try {
      const json = await invoke<string>("get_session_params", { id });
      if (!json) return false;
      const p = JSON.parse(json);
      if (p.temperature !== undefined) temperature = p.temperature;
      if (p.maxTokens   !== undefined) maxTokens   = p.maxTokens;
      if (p.topK        !== undefined) topK        = p.topK;
      if (p.topP        !== undefined) topP        = p.topP;
      if (p.thinkingLevel !== undefined) thinkingLevel = p.thinkingLevel;
      return true;
    } catch (e) { console.warn("[chat] failed to load session params:", e); return false; }
  }

  const thinkingBudget = $derived(
    THINKING_LEVELS.find(l => l.key === thinkingLevel)?.budget ?? null
  );

  // ── Derived ────────────────────────────────────────────────────────────────
  const canSend   = $derived(
    status === "running" && (input.trim().length > 0 || attachments.length > 0) && !generating
  );
  const canStart  = $derived(status === "stopped");
  const canStop   = $derived(status === "running" || status === "loading");

  // ── Live context usage estimation ──────────────────────────────────────────
  const estimatedPromptTokens = $derived.by(() => {
    let total = estimateTokens(systemPrompt) + 10;
    if (supportsTools && toolConfig.enabled) {
      const enabledCount = [toolConfig.date, toolConfig.location, toolConfig.web_search,
        toolConfig.web_fetch, toolConfig.bash, toolConfig.read_file,
        toolConfig.write_file, toolConfig.edit_file, toolConfig.skill_api].filter(Boolean).length;
      if (enabledCount > 0) total += nCtx <= 4096 ? 30 : 500;
    }
    for (const m of messages) {
      total += estimateTokens(m.content) + 10;
      if (m.thinking) total += estimateTokens(m.thinking) + 5;
      if (m.leadIn) total += estimateTokens(m.leadIn);
    }
    if (input.trim()) total += estimateTokens(input) + 10;
    return total;
  });

  const liveUsedTokens = $derived.by(() => {
    if (realPromptTokens !== null) return realPromptTokens + streamCompletionToks;
    return estimatedPromptTokens + streamCompletionToks;
  });

  // ── Context breakdown segments ─────────────────────────────────────────────
  // Use the SAME overhead constants as estimatedPromptTokens so segments
  // sum exactly to liveUsedTokens when no real server count is available.

  const contextSegments = $derived.by((): ContextSegment[] => {
    const segs: ContextSegment[] = [];

    // 1. System prompt — mirrors: estimateTokens(systemPrompt) + 10
    //    Split into base system prompt vs EEG injection
    let sysTokens = estimateTokens(systemPrompt) + 10; // +10 matches estimatedPromptTokens
    let eegTokens = 0;
    if (eegActive && latestBands) {
      const eegBlock = buildEegBlock(latestBands);
      eegTokens = estimateTokens(eegBlock);
      // EEG is appended to the system message, so remove it from sys
    }
    if (sysTokens > 0) {
      segs.push({ key: "system", labelKey: "chat.ctx.system", tokens: sysTokens, color: "#8b5cf6" });
    }
    if (eegTokens > 0) {
      segs.push({ key: "eeg", labelKey: "chat.ctx.eeg", tokens: eegTokens, color: "#06b6d4" });
    }

    // 2. Tool definitions — mirrors: nCtx <= 4096 ? 30 : 500
    if (supportsTools && toolConfig.enabled) {
      const enabledCount = [toolConfig.date, toolConfig.location, toolConfig.web_search,
        toolConfig.web_fetch, toolConfig.bash, toolConfig.read_file,
        toolConfig.write_file, toolConfig.edit_file, toolConfig.skill_api].filter(Boolean).length;
      if (enabledCount > 0) {
        const toolDefTokens = nCtx <= 4096 ? 30 : 500;
        segs.push({ key: "toolDefs", labelKey: "chat.ctx.toolDefs", tokens: toolDefTokens, color: "#f59e0b" });
      }
    }

    // 3. Messages — mirrors: estimateTokens(m.content) + 10, + thinking + 5, + leadIn
    let userTokens = 0;
    let assistantTokens = 0;
    let thinkingTokens = 0;
    let toolResultTokens = 0;

    for (const m of messages) {
      const msgOverhead = 10; // same as estimatedPromptTokens
      if (m.role === "user") {
        userTokens += estimateTokens(m.content) + msgOverhead;
      } else if (m.role === "assistant") {
        assistantTokens += estimateTokens(m.content) + msgOverhead;
        if (m.leadIn) assistantTokens += estimateTokens(m.leadIn);
        if (m.thinking) {
          thinkingTokens += estimateTokens(m.thinking) + 5; // +5 matches estimatedPromptTokens
        }
        // Tool call args/results are injected as messages in the conversation
        if (m.toolUses) {
          for (const tu of m.toolUses) {
            if (tu.args) toolResultTokens += estimateTokens(JSON.stringify(tu.args)) + 4;
            if (tu.result) toolResultTokens += estimateTokens(JSON.stringify(tu.result)) + 4;
          }
        }
      }
    }

    // Current input (not yet sent) — mirrors: estimateTokens(input) + 10
    if (input.trim()) {
      userTokens += estimateTokens(input) + 10;
    }

    if (userTokens > 0) segs.push({ key: "user", labelKey: "chat.ctx.user", tokens: userTokens, color: "#3b82f6" });
    if (assistantTokens > 0) segs.push({ key: "assistant", labelKey: "chat.ctx.assistant", tokens: assistantTokens, color: "#10b981" });
    if (thinkingTokens > 0) segs.push({ key: "thinking", labelKey: "chat.ctx.thinking", tokens: thinkingTokens, color: "#a855f7" });
    if (toolResultTokens > 0) segs.push({ key: "toolResults", labelKey: "chat.ctx.toolResults", tokens: toolResultTokens, color: "#ef4444" });

    // 4. Completion tokens (currently generating)
    if (streamCompletionToks > 0) {
      segs.push({ key: "completion", labelKey: "chat.ctx.completion", tokens: streamCompletionToks, color: "#64748b" });
    }

    // 5. When the server has reported real prompt tokens, the segment estimates
    //    won't match exactly. Scale all prompt segments proportionally so the
    //    breakdown sums to liveUsedTokens.
    if (realPromptTokens !== null) {
      const promptSegKeys = new Set(["system", "eeg", "toolDefs", "user", "assistant", "thinking", "toolResults"]);
      const estPromptSum = segs.filter(s => promptSegKeys.has(s.key)).reduce((a, s) => a + s.tokens, 0);
      if (estPromptSum > 0) {
        const scale = realPromptTokens / estPromptSum;
        for (const s of segs) {
          if (promptSegKeys.has(s.key)) {
            s.tokens = Math.round(s.tokens * scale);
          }
        }
      }
    }

    return segs;
  });

  // ── Helpers ────────────────────────────────────────────────────────────────

  function autoResizeInput() { inputBarRef?.autoResize(); }

  /** Cancel a specific tool call. */
  async function cancelToolCall(msgId: number, tuIdx: number, toolCallId: string | undefined) {
    if (!toolCallId) return;
    try { await invoke("cancel_tool_call", { toolCallId }); } catch (e) { console.warn("[chat] cancel_tool_call failed:", e); }
    messages = messages.map(m => {
      if (m.id !== msgId) return m;
      const uses = [...(m.toolUses ?? [])];
      if (uses[tuIdx] && uses[tuIdx].status === "calling") {
        uses[tuIdx] = { ...uses[tuIdx], status: "cancelled" };
      }
      return { ...m, toolUses: uses };
    });
  }

  /** Regenerate: remove the last assistant message and resend. */
  function regenerate() {
    if (generating || status !== "running") return;
    const lastUserIdx = messages.findLastIndex(m => m.role === "user");
    if (lastUserIdx < 0) return;
    const userMsg = messages[lastUserIdx];
    messages = messages.slice(0, lastUserIdx);
    input = userMsg.content;
    attachments = userMsg.attachments ?? [];
    sendMessage();
  }

  /** Edit a user message: populate the input and remove everything from that point. */
  function editAndResend(msg: Message) {
    if (generating || status !== "running") return;
    const idx = messages.findIndex(m => m.id === msg.id);
    if (idx < 0) return;
    messages = messages.slice(0, idx);
    input = msg.content;
    attachments = msg.attachments ?? [];
    tick().then(() => { autoResizeInput(); inputBarRef?.focus(); });
  }

  function updateMessage(id: number, patch: Partial<Message>) {
    messages = messages.map(m => m.id === id ? { ...m, ...patch } : m);
  }

  function updateToolUse(msgId: number, tuIdx: number, patch: Partial<ToolUseEvent>) {
    messages = messages.map(m => {
      if (m.id !== msgId) return m;
      const uses = [...(m.toolUses ?? [])];
      uses[tuIdx] = { ...uses[tuIdx], ...patch };
      return { ...m, toolUses: uses };
    });
  }

  // ── Input history navigation ───────────────────────────────────────────────
  function inputKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendMessage(); return; }

    if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
      const el = inputBarRef?.getInputEl();
      if (!el) return;
      const cur = el.selectionStart ?? 0;
      const onFirst = !input.slice(0, cur).includes("\n");
      const onLast  = !input.slice(cur).includes("\n");

      if (e.key === "ArrowUp" && onFirst) {
        if (userHistory.length === 0) return;
        e.preventDefault();
        if (histIdx === -1) histDraft = input;
        const next = Math.min(histIdx + 1, userHistory.length - 1);
        if (next === histIdx) return;
        histIdx = next;
        input   = userHistory[histIdx];
        autoResizeInput();
        tick().then(() => el.setSelectionRange(input.length, input.length));
        return;
      }

      if (e.key === "ArrowDown" && onLast) {
        if (histIdx === -1) return;
        e.preventDefault();
        const next = histIdx - 1;
        if (next < 0) { histIdx = -1; input = histDraft; }
        else { histIdx = next; input = userHistory[histIdx]; }
        autoResizeInput();
        tick().then(() => el.setSelectionRange(input.length, input.length));
      }
    }
  }

  // ── Server control ─────────────────────────────────────────────────────────

  async function startServer() {
    status = "loading";
    try { await invoke("start_llm_server"); }
    catch (e) { console.error("start_llm_server failed:", e); status = "stopped"; }
  }

  async function stopServer() {
    if (generating) { abort(); await new Promise(r => setTimeout(r, 100)); }
    await invoke("stop_llm_server");
    status = "stopped";
    modelName = "";
  }

  function abort() {
    if (aborting) return;
    aborting = true;
    invoke("abort_llm_stream").catch(e => console.warn("[chat] abort failed:", e)).finally(() => { aborting = false; });
  }

  // ── Chat ───────────────────────────────────────────────────────────────────

  async function sendMessage() {
    const text = input.trim();
    if ((!text && attachments.length === 0) || generating || status !== "running") return;
    input     = "";
    histIdx   = -1;
    histDraft = "";
    autoResizeInput();
    const sentAttachments = attachments;
    attachments = [];

    const userMsg: Message = {
      id: ++msgId, role: "user",
      content: text,
      attachments: sentAttachments.length ? sentAttachments : undefined,
    };

    // Auto-title
    const isFirstUserMsg = !messages.some(m => m.role === "user" && m.content.trim());
    if (isFirstUserMsg && text && sessionId > 0) {
      const autoTitle = text.slice(0, 60).replace(/\n+/g, " ").trim();
      invoke("rename_chat_session", { id: sessionId, title: autoTitle }).catch(e => console.warn("[chat] rename failed:", e));
      sidebarRef?.updateTitle(sessionId, autoTitle);
    }

    messages = [...messages, userMsg];
    if (sessionId > 0 && text) {
      invoke("save_chat_message", { sessionId, role: "user", content: text, thinking: null }).catch(e => console.warn("[chat] save user msg failed:", e));
    }

    const assistantMsg: Message = { id: ++msgId, role: "assistant", content: "", pending: true };
    messages = [...messages, assistantMsg];
    await msgListRef?.scrollBottom(true);

    generating = true;
    realPromptTokens     = null;
    streamCompletionToks = 0;
    const t0   = performance.now();
    let   ttft: number | undefined;

    // Build API messages
    const historyMsgs = messages
      .filter(m => !m.pending)
      .map(m => {
        if (m.role === "user" && m.attachments?.length) {
          return { role: m.role, content: buildUserContent(m.content, m.attachments) };
        }
        return { role: m.role, content: m.content };
      });

    const systemParts: string[] = [];
    if (systemPrompt.trim()) systemParts.push(systemPrompt.trim());
    if (eegActive && latestBands) systemParts.push(buildEegBlock(latestBands));

    const apiMessages = [
      ...(systemParts.length ? [{ role: "system", content: systemParts.join("\n\n") }] : []),
      ...historyMsgs,
    ];

    let rawAcc = "";
    let usage: UsageInfo | undefined;

    // Multi-round tool state
    let hadToolUse     = false;
    let frozenLeadIn   = "";
    let frozenThinking = "";

    function mergeWithFrozen(parsed: { leadIn: string; thinking: string; content: string }) {
      return {
        leadIn:   [frozenLeadIn,   parsed.leadIn  ].filter(s => s.trim()).join("\n\n"),
        thinking: [frozenThinking, parsed.thinking].filter(s => s.trim()).join("\n\n"),
        content:  parsed.content,
      };
    }

    // ── IPC Channel ──
    const channel = new Channel<ChatChunk>();
    channel.onmessage = async (chunk: ChatChunk) => {
      if (chunk.type === "delta") {
        if (ttft === undefined) { ttft = performance.now() - t0; streamStartMs = performance.now(); streamTokens = 0; }
        if (hadToolUse) hadToolUse = false;
        rawAcc += chunk.content;
        streamTokens++;
        streamCompletionToks = Math.ceil(rawAcc.length / 4);
        const { leadIn, thinking, content } = mergeWithFrozen(parseAssistantOutput(rawAcc));
        messages = messages.map(m =>
          m.id === assistantMsg.id ? { ...m, leadIn, content, thinking, thinkOpen: m.thinkOpen ?? false } : m
        );
        msgListRef?.scrollBottom();

      } else if (chunk.type === "tool_use") {
        const evt: ToolUseEvent = { tool: chunk.tool, status: chunk.status, detail: chunk.detail };
        if (evt.status === "calling" && !hadToolUse) {
          const prev = parseAssistantOutput(rawAcc);
          frozenLeadIn   = [frozenLeadIn,   prev.leadIn, prev.content].filter(s => s.trim()).join("\n\n");
          frozenThinking = [frozenThinking,  prev.thinking            ].filter(s => s.trim()).join("\n\n");
          rawAcc     = "";
          hadToolUse = true;
          messages = messages.map(m =>
            m.id === assistantMsg.id ? { ...m, leadIn: frozenLeadIn, thinking: frozenThinking, content: "" } : m
          );
        }
        messages = messages.map(m => {
          if (m.id !== assistantMsg.id) return m;
          const existing = m.toolUses ?? [];
          const idx = existing.findIndex(e => e.tool === evt.tool && e.status === "calling");
          if (evt.status !== "calling" && idx >= 0) {
            const updated = [...existing];
            updated[idx] = { ...updated[idx], ...evt };
            return { ...m, toolUses: updated };
          }
          return { ...m, toolUses: [...existing, evt] };
        });
        msgListRef?.scrollBottom();

      } else if (chunk.type === "tool_execution_start") {
        messages = messages.map(m => {
          if (m.id !== assistantMsg.id) return m;
          const existing = m.toolUses ?? [];
          const idx = existing.findIndex(e => e.tool === chunk.tool_name && e.status === "calling");
          if (idx >= 0) {
            const updated = [...existing];
            updated[idx] = { ...updated[idx], toolCallId: chunk.tool_call_id, args: chunk.args };
            return { ...m, toolUses: updated };
          }
          return m;
        });

      } else if (chunk.type === "tool_execution_end") {
        messages = messages.map(m => {
          if (m.id !== assistantMsg.id) return m;
          const existing = m.toolUses ?? [];
          const idx = existing.findIndex(e =>
            e.tool === chunk.tool_name && (
              e.toolCallId === chunk.tool_call_id || e.status === "calling" ||
              (!e.result && e.tool === chunk.tool_name)
            )
          );
          if (idx >= 0) {
            const updated = [...existing];
            updated[idx] = { ...updated[idx], result: chunk.result, status: chunk.is_error ? "error" : "done" };
            return { ...m, toolUses: updated };
          }
          return m;
        });

      } else if (chunk.type === "tool_cancelled") {
        messages = messages.map(m => {
          if (m.id !== assistantMsg.id) return m;
          const existing = m.toolUses ?? [];
          const idx = existing.findIndex(e =>
            e.tool === chunk.tool_name && (
              e.toolCallId === chunk.tool_call_id || e.status === "calling" ||
              (!e.result && e.tool === chunk.tool_name)
            )
          );
          if (idx >= 0) {
            const updated = [...existing];
            updated[idx] = { ...updated[idx], status: "cancelled" };
            return { ...m, toolUses: updated };
          }
          return m;
        });

      } else if (chunk.type === "done") {
        const elapsed = performance.now() - t0;
        const { leadIn, thinking, content } = mergeWithFrozen(parseAssistantOutput(rawAcc));
        usage = {
          prompt_tokens: chunk.prompt_tokens, completion_tokens: chunk.completion_tokens,
          total_tokens: chunk.prompt_tokens + chunk.completion_tokens, n_ctx: chunk.n_ctx,
        };
        realPromptTokens     = chunk.prompt_tokens;
        streamCompletionToks = chunk.completion_tokens;
        if (chunk.n_ctx > 0) nCtx = chunk.n_ctx;
        messages = messages.map(m =>
          m.id === assistantMsg.id ? { ...m, pending: false, leadIn, content, thinking, ttft, elapsed, usage } : m
        );
        msgListRef?.scrollBottom();

      } else if (chunk.type === "error") {
        const { leadIn, thinking, content } = mergeWithFrozen(parseAssistantOutput(rawAcc));
        messages = messages.map(m =>
          m.id === assistantMsg.id ? {
            ...m, pending: false, leadIn,
            content: chunk.message === "aborted" ? (content || "*(aborted)*") : `*Error: ${chunk.message}*`,
            thinking: chunk.message === "aborted" ? thinking : undefined,
          } : m
        );
        msgListRef?.scrollBottom();
      }
    };

    try {
      await invoke("chat_completions_ipc", {
        messages: apiMessages,
        params: { temperature, max_tokens: maxTokens, top_k: topK, top_p: topP, thinking_budget: thinkingBudget },
        channel,
      });
    } catch (err: any) {
      messages = messages.map(m =>
        m.id === assistantMsg.id ? { ...m, pending: false, content: `*Error: ${String(err)}*` } : m
      );
    } finally {
      messages = messages.map(m =>
        m.id === assistantMsg.id && m.pending
          ? { ...m, pending: false, ...mergeWithFrozen(parseAssistantOutput(rawAcc)) }
          : m
      );
      const finalAssistant = messages.find(m => m.id === assistantMsg.id);
      generating = false;
      msgListRef?.scrollBottom();
      await tick();
      inputBarRef?.focus();

      // Persist assistant message
      if (sessionId > 0 && finalAssistant && !finalAssistant.pending) {
        const parts: string[] = [];
        if (finalAssistant.leadIn?.trim())  parts.push(finalAssistant.leadIn.trim());
        if (finalAssistant.content?.trim()) parts.push(finalAssistant.content.trim());
        const fullContent = parts.join("\n\n");
        if (fullContent || (finalAssistant.toolUses?.length ?? 0) > 0) {
          invoke<number>("save_chat_message", {
            sessionId, role: "assistant", content: fullContent || "", thinking: finalAssistant.thinking ?? null,
          }).then((messageId: number) => {
            if (messageId > 0 && finalAssistant.toolUses?.length) {
              const toolCalls = finalAssistant.toolUses.map(tu => ({
                id: 0, message_id: messageId,
                tool: tu.tool, status: tu.status,
                detail: tu.detail ?? null, tool_call_id: tu.toolCallId ?? null,
                args: tu.args ?? null, result: tu.result ?? null, created_at: 0,
              }));
              invoke("save_chat_tool_calls", { messageId, toolCalls }).catch(e => console.warn("[chat] save tool calls failed:", e));
            }
          }).catch(e => console.warn("[chat] save assistant msg failed:", e));
        }
      }
    }
  }

  /** Create a new empty session. */
  async function newChat() {
    if (generating) abort();
    messages = []; histIdx = -1; histDraft = "";
    try { sessionId = await invoke<number>("new_chat_session"); await sidebarRef?.refresh(); }
    catch (e) { console.error("[chat] new_chat_session failed:", e); }
    await tick();
    inputBarRef?.focus();
  }

  /** Switch to an existing session. */
  async function loadSession(id: number) {
    if (id === sessionId) return;
    if (generating) abort();
    saveSessionParams();
    messages = []; histIdx = -1; histDraft = "";
    try {
      const resp = await invoke<ChatSessionResponse>("load_chat_session", { id });
      sessionId = resp.session_id;
      const idCounter = { value: msgId };
      messages = resp.messages.map(sm => storedToMessage(sm, idCounter));
      msgId = idCounter.value;
      await loadSessionParams(id);
      msgListRef?.scrollBottom(true);
    } catch (e) { console.error("[chat] load_chat_session failed:", e); }
    await tick();
    inputBarRef?.focus();
  }

  /** Called by sidebar on delete. */
  async function handleSidebarDelete(deletedId: number) {
    if (deletedId !== sessionId) return;
    messages = []; sessionId = 0;
    try {
      const resp = await invoke<ChatSessionResponse>("get_last_chat_session");
      sessionId = resp.session_id;
      if (resp.messages.length > 0) {
        const idCounter = { value: msgId };
        messages = resp.messages.map(sm => storedToMessage(sm, idCounter));
        msgId = idCounter.value;
        msgListRef?.scrollBottom(true);
      }
      await sidebarRef?.refresh();
    } catch (e) { console.error("[chat] handleSidebarDelete failed:", e); }
  }

  // ── Typing-label auto-labeller ───────────────────────────────────────────
  const TYPING_LABEL_INTERVAL_MS = 5_000;
  const WORD_BOUNDARY_TIMEOUT_MS = 1_500;
  let typedCharsInWindow   = $state("");
  let deletedCharsInWindow = $state("");
  let typingLabelTimer: ReturnType<typeof setInterval> | undefined;
  let windowStartUtc       = 0;
  let pendingFlush         = false;
  let wordBoundaryTimeout: ReturnType<typeof setTimeout> | undefined;

  function isWordBoundary(ch: string): boolean { return /[\s\p{P}]/u.test(ch); }

  function captureDeletedText(e: InputEvent): string {
    const el = inputBarRef?.getInputEl();
    if (!el) return "";
    const val = el.value;
    const start = el.selectionStart ?? 0;
    const end   = el.selectionEnd   ?? 0;
    if (start !== end) return val.slice(start, end);
    switch (e.inputType) {
      case "deleteContentBackward": return start > 0 ? val.slice(start - 1, start) : "";
      case "deleteContentForward":  return start < val.length ? val.slice(start, start + 1) : "";
      case "deleteWordBackward": {
        let i = start; while (i > 0 && /\s/.test(val[i - 1])) i--; while (i > 0 && !/\s/.test(val[i - 1])) i--;
        return val.slice(i, start);
      }
      case "deleteWordForward": {
        let i = start; while (i < val.length && !/\s/.test(val[i])) i++; while (i < val.length && /\s/.test(val[i])) i++;
        return val.slice(start, i);
      }
      case "deleteByCut": return start !== end ? val.slice(start, end) : "";
      default: return "";
    }
  }

  function onChatBeforeInput(e: InputEvent) {
    if (e.inputType.startsWith("delete")) {
      const removed = captureDeletedText(e);
      if (removed) deletedCharsInWindow += " " + removed;
      return;
    }
    if (e.inputType === "insertText" && e.data) {
      typedCharsInWindow += e.data;
      if (pendingFlush && isWordBoundary(e.data)) commitTypingLabel();
    } else if (e.inputType === "insertLineBreak" || e.inputType === "insertParagraph") {
      typedCharsInWindow += " ";
      if (pendingFlush) commitTypingLabel();
    }
  }

  function buildSessionContext(): string {
    const parts: string[] = [`Chat session #${sessionId}`];
    if (modelName) parts.push(`Model: ${modelName}`);
    const recent = messages.slice(-6);
    if (recent.length > 0) {
      parts.push("Recent messages:");
      for (const m of recent) {
        const prefix = m.role === "user" ? "User" : "Assistant";
        const snippet = (m.content || "").slice(0, 300).replace(/\n+/g, " ").trim();
        if (snippet) parts.push(`  [${prefix}] ${snippet}`);
      }
    }
    return parts.join("\n");
  }

  async function commitTypingLabel() {
    pendingFlush = false;
    if (wordBoundaryTimeout) { clearTimeout(wordBoundaryTimeout); wordBoundaryTimeout = undefined; }
    const rawTyped   = typedCharsInWindow.trim();
    const rawDeleted = deletedCharsInWindow.trim();
    typedCharsInWindow = ""; deletedCharsInWindow = "";
    const labelStartUtc = windowStartUtc;
    windowStartUtc = Math.floor(Date.now() / 1000);
    if (!rawTyped) return;

    const isAlphaNum = (w: string) =>
      /[a-zA-Z0-9\u00C0-\u024F\u0400-\u04FF\u0590-\u05FF\u0600-\u06FF]/.test(w);
    const typedWords = rawTyped.split(/\s+/).filter(isAlphaNum);
    if (typedWords.length === 0) return;

    const deletedCounts = new Map<string, number>();
    if (rawDeleted) {
      for (const w of rawDeleted.split(/\s+/).filter(isAlphaNum)) {
        const lc = w.toLowerCase();
        deletedCounts.set(lc, (deletedCounts.get(lc) ?? 0) + 1);
      }
    }

    const rendered = typedWords.map(w => {
      const lc = w.toLowerCase();
      const dCnt = deletedCounts.get(lc) ?? 0;
      if (dCnt > 0) { deletedCounts.set(lc, dCnt - 1); return `<del>${w}</del>`; }
      return w;
    });

    try {
      await invoke("submit_label", { labelStartUtc, text: rendered.join(" "), context: buildSessionContext() });
    } catch (e) { console.warn("[chat] submit_label failed:", e); }
  }

  function onTypingWindowTick() {
    if (!typedCharsInWindow) { windowStartUtc = Math.floor(Date.now() / 1000); return; }
    const lastChar = typedCharsInWindow.at(-1) ?? "";
    if (isWordBoundary(lastChar) || !lastChar) { commitTypingLabel(); }
    else { pendingFlush = true; wordBoundaryTimeout = setTimeout(() => commitTypingLabel(), WORD_BOUNDARY_TIMEOUT_MS); }
  }

  function startTypingLabelTimer() {
    stopTypingLabelTimer();
    typedCharsInWindow = ""; deletedCharsInWindow = "";
    pendingFlush = false;
    windowStartUtc = Math.floor(Date.now() / 1000);
    typingLabelTimer = setInterval(onTypingWindowTick, TYPING_LABEL_INTERVAL_MS);
  }

  function stopTypingLabelTimer() {
    if (typingLabelTimer) { clearInterval(typingLabelTimer); typingLabelTimer = undefined; }
    if (wordBoundaryTimeout) { clearTimeout(wordBoundaryTimeout); wordBoundaryTimeout = undefined; }
    pendingFlush = false;
    commitTypingLabel();
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  let unlistenStatus: (() => void) | undefined;
  let unlistenBands:  (() => void) | undefined;
  let pollTimer:       ReturnType<typeof setInterval> | undefined;

  onMount(async () => {
    // Initial status
    try {
      const s = await invoke<{ status: ServerStatus; model_name: string; n_ctx: number; supports_vision: boolean; supports_tools: boolean }>("get_llm_server_status");
      status = s.status; modelName = s.model_name; nCtx = s.n_ctx ?? 0;
      supportsVision = s.supports_vision ?? false; supportsTools = s.supports_tools ?? false;
    } catch (e) { console.warn("[chat] get_llm_server_status failed:", e); }

    // Load tool config
    try {
      const cfg = await invoke<any>("get_llm_config");
      if (cfg?.tools) {
        toolConfig = {
          enabled: cfg.tools.enabled ?? true,
          date: cfg.tools.date ?? true, location: cfg.tools.location ?? true,
          web_search: cfg.tools.web_search ?? true, web_fetch: cfg.tools.web_fetch ?? true,
          bash: cfg.tools.bash ?? false, read_file: cfg.tools.read_file ?? false,
          write_file: cfg.tools.write_file ?? false, edit_file: cfg.tools.edit_file ?? false,
          skill_api: cfg.tools.skill_api ?? true,
          execution_mode: cfg.tools.execution_mode ?? "parallel",
          max_rounds: cfg.tools.max_rounds ?? 3, max_calls_per_round: cfg.tools.max_calls_per_round ?? 4,
          thinking_budget: cfg.tools.thinking_budget ?? null,
          context_compression: cfg.tools.context_compression ?? { level: "normal", max_search_results: 0, max_result_chars: 0 },
        };
      }
    } catch (e) { console.warn("[chat] get_llm_config failed:", e); }

    // Live status events
    try {
      unlistenStatus = await listen<ServerStatusPayload>("llm:status", ev => {
        status    = ev.payload.status ?? status;
        modelName = (ev.payload as any).model ?? ev.payload.model_name ?? modelName;
        if (ev.payload.supports_vision !== undefined) supportsVision = ev.payload.supports_vision;
        if ((ev.payload as any).supports_tools !== undefined) supportsTools = (ev.payload as any).supports_tools;
        if ((ev.payload as any).n_ctx !== undefined) nCtx = (ev.payload as any).n_ctx;
        if (status === "running") clearInterval(pollTimer!);
        if (status === "stopped") { supportsVision = false; supportsTools = false; nCtx = 0; }
      });
    } catch (e) { console.warn("[chat] listen llm:status failed:", e); }

    // Poll while loading
    let ranAfterRunning = false;
    pollTimer = setInterval(async () => {
      if (status !== "loading" && (status !== "running" || ranAfterRunning)) { clearInterval(pollTimer!); return; }
      if (status === "running") ranAfterRunning = true;
      try {
        const s = await invoke<{ status: ServerStatus; model_name: string; n_ctx: number; supports_vision: boolean; supports_tools: boolean }>("get_llm_server_status");
        status = s.status; modelName = s.model_name; nCtx = s.n_ctx ?? 0;
        supportsVision = s.supports_vision ?? false; supportsTools = s.supports_tools ?? false;
      } catch (e) { console.warn("[chat] poll status failed:", e); }
    }, 1500);

    // EEG bands
    try { const b = await invoke<BandSnapshot | null>("get_latest_bands"); if (b) latestBands = b; } catch (e) { console.warn("[chat] get_latest_bands failed:", e); }
    try { unlistenBands = await listen<BandSnapshot>("eeg-bands", ev => { latestBands = ev.payload; }); } catch (e) { console.warn("[chat] listen eeg-bands failed:", e); }

    // Load persisted chat history
    try {
      const resp = await invoke<ChatSessionResponse>("get_last_chat_session");
      sessionId = resp.session_id;
      if (resp.messages.length > 0) {
        const idCounter = { value: msgId };
        messages = resp.messages.map(sm => storedToMessage(sm, idCounter));
        msgId = idCounter.value;
        msgListRef?.scrollBottom(true);
      }
      if (sessionId > 0) await loadSessionParams(sessionId);
    } catch (e) { console.error("[chat] load last session failed:", e); }

    startTypingLabelTimer();
    await tick();
    inputBarRef?.focus();
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenBands?.();
    clearInterval(pollTimer);
    stopTypingLabelTimer();
    if (generating) invoke("abort_llm_stream").catch(e => console.warn("[chat] abort_llm_stream cleanup failed:", e));
  });
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<div class="flex h-full min-h-0 bg-background text-foreground overflow-hidden rounded-b-[10px]">

  <!-- Sidebar -->
  {#if sidebarOpen}
    <aside class="w-52 shrink-0 flex flex-col
                  border-r border-border dark:border-white/[0.06]
                  bg-slate-50/50 dark:bg-[#0c0c14] overflow-hidden">
      <ChatSidebar
        bind:this={sidebarRef}
        activeId={sessionId}
        onSelect={loadSession}
        onNew={newChat}
        onDelete={handleSidebarDelete}
      />
    </aside>
  {/if}

  <!-- Main chat column -->
  <div class="min-h-0 flex flex-col flex-1 min-w-0 overflow-hidden">

    <ChatHeader
      {sidebarOpen}
      {showSettings}
      {showTools}
      {status}
      {modelName}
      {supportsTools}
      {enabledToolCount}
      {nCtx}
      {liveUsedTokens}
      {realPromptTokens}
      {eegContext}
      {latestBands}
      {canStart}
      {canStop}
      onToggleSidebar={() => sidebarOpen = !sidebarOpen}
      onToggleSettings={() => { showSettings = !showSettings; if (showSettings) showTools = false; }}
      onToggleTools={() => { showTools = !showTools; if (showTools) showSettings = false; }}
      onStartServer={startServer}
      onStopServer={stopServer}
      onNewChat={newChat}
      onToggleEeg={() => eegContext = !eegContext}
      onToggleContextBreakdown={() => showContextBreakdown = !showContextBreakdown}
    />

    {#if showContextBreakdown && nCtx > 0}
      <ChatContextBreakdown
        segments={contextSegments}
        totalUsed={liveUsedTokens}
        {nCtx}
        isEstimate={realPromptTokens === null && liveUsedTokens > 0}
        onClose={() => showContextBreakdown = false}
        onViewFull={() => { showContextBreakdown = false; showContextViewer = true; }}
      />
    {/if}

    {#if showContextViewer}
      <ChatContextViewer
        {messages}
        {systemPrompt}
        eegActive={eegActive}
        {latestBands}
        {toolConfig}
        {supportsTools}
        {nCtx}
        onClose={() => showContextViewer = false}
      />
    {/if}

    {#if showSettings}
      <ChatSettingsPanel
        bind:systemPrompt
        bind:eegContext
        {latestBands}
        bind:thinkingLevel
        bind:temperature
        bind:maxTokens
        bind:topK
        bind:topP
      />
    {/if}

    {#if showTools}
      <ChatToolsPanel
        {toolConfig}
        {enabledToolCount}
        onUpdate={updateToolConfig}
      />
    {/if}

    <ChatMessageList
      bind:this={msgListRef}
      {messages}
      {status}
      {generating}
      {streamStartMs}
      {streamTokens}
      onUpdateMessage={updateMessage}
      onUpdateToolUse={updateToolUse}
      onCancelToolCall={cancelToolCall}
      onEditAndResend={editAndResend}
      onRegenerate={regenerate}
      onStartServer={startServer}
    />

    <ChatInputBar
      bind:this={inputBarRef}
      bind:input
      bind:attachments
      {status}
      {generating}
      {aborting}
      {canSend}
      {supportsVision}
      {nCtx}
      {liveUsedTokens}
      onSend={sendMessage}
      onAbort={abort}
      onInputKeydown={inputKeydown}
      onBeforeInput={onChatBeforeInput}
    />

  </div>
</div>
