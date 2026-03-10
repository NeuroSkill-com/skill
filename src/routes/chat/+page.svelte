<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Chat window — Ollama-style interface for the embedded LLM server.

  Architecture:
  • `invoke("get_ws_config")` gives us the port; all inference goes through
    `fetch("http://localhost:{port}/v1/chat/completions", {stream:true})`.
  • `invoke("get_llm_server_status")` polls server state.
  • `invoke("start_llm_server")` / `invoke("stop_llm_server")` control the actor.
  • `listen("llm:status")` gives real-time loading → running → stopped events.
-->
<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { invoke }                   from "@tauri-apps/api/core";
  import { listen }                   from "@tauri-apps/api/event";
  import ThemeToggle                  from "$lib/ThemeToggle.svelte";
  import MarkdownRenderer             from "$lib/MarkdownRenderer.svelte";
  import LanguagePicker               from "$lib/LanguagePicker.svelte";
  import { t }                        from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────

  type Role = "user" | "assistant" | "system";
  type ServerStatus = "stopped" | "loading" | "running";

  interface Message {
    id:           number;
    role:         Role;
    content:      string;
    /** Images attached to a user message */
    attachments?: Attachment[];
    /** Chain-of-thought text between <think>…</think> (stripped from content) */
    thinking?:    string;
    /** Whether the thinking block is expanded in the UI */
    thinkOpen?:   boolean;
    /** True while we're streaming tokens in */
    pending?:     boolean;
    /** ms taken for first token */
    ttft?:        number;
    /** ms for full response */
    elapsed?:     number;
    /** Token usage from the final SSE chunk */
    usage?:       UsageInfo;
  }

  /**
   * Split a raw model response into thinking and visible parts.
   *
   * Qwen3.5 wraps chain-of-thought in <think>…</think>.
   * We strip those tags and store the content separately so the UI can
   * show it collapsed (or hide it entirely).
   */
  function parseThinking(raw: string): { thinking: string; content: string } {
    // Strip any leading whitespace/newlines the model emits before <think>.
    const trimmed = raw.trimStart();
    // Completed think block
    const full = trimmed.match(/^<think>([\s\S]*?)<\/think>\s*([\s\S]*)$/);
    if (full) return { thinking: full[1].trim(), content: full[2] };
    // Still-open think block (streaming)
    const open = trimmed.match(/^<think>([\s\S]*)$/);
    if (open) return { thinking: open[1], content: "" };
    return { thinking: "", content: raw };
  }

  interface ServerStatusPayload { status: ServerStatus; model_name: string; }

  interface UsageInfo {
    prompt_tokens:     number;
    completion_tokens: number;
    total_tokens:      number;
    n_ctx:             number;
  }

  /** Thinking budget levels: token limit for <think> block. null = unlimited. */
  type ThinkingLevel = "minimal" | "normal" | "extended" | "unlimited";
  const THINKING_LEVELS: { labelKey: string; key: ThinkingLevel; budget: number | null }[] = [
    { labelKey: "chat.think.minimal",   key: "minimal",   budget: 512   },
    { labelKey: "chat.think.normal",    key: "normal",    budget: 2048  },
    { labelKey: "chat.think.extended",  key: "extended",  budget: 8192  },
    { labelKey: "chat.think.unlimited", key: "unlimited", budget: null  },
  ];

  interface Attachment { dataUrl: string; mimeType: string; name: string; }

  // ── State ──────────────────────────────────────────────────────────────────

  let port           = $state(8375);
  let status         = $state<ServerStatus>("stopped");
  let modelName      = $state("");
  let supportsVision = $state(false);
  let messages       = $state<Message[]>([]);
  let input          = $state("");
  let systemPrompt   = $state("You are a helpful assistant.");
  let showSystem     = $state(false);
  let generating     = $state(false);
  let abortCtrl      = $state<AbortController | null>(null);
  let msgId          = $state(0);
  let msgsEl         = $state<HTMLElement | null>(null);
  let inputEl        = $state<HTMLTextAreaElement | null>(null);
  let fileInputEl    = $state<HTMLInputElement | null>(null);
  let attachments    = $state<Attachment[]>([]);

  // ── Input history navigation (↑ / ↓) ──────────────────────────────────────
  // histIdx = -1  →  showing the live draft
  // histIdx =  0  →  last sent user message
  // histIdx =  1  →  second-to-last, etc.
  let histIdx        = $state(-1);
  let histDraft      = $state("");   // preserves the in-progress draft while browsing

  /** Sent user messages, newest-first, deduplicated consecutive entries. */
  const userHistory = $derived(
    messages
      .filter(m => m.role === "user" && m.content.trim())
      .map(m => m.content)
      .reverse()
      .filter((c, i, a) => i === 0 || c !== a[i - 1])
  );

  // Id of the last message whose copy button was just clicked (drives ✓ flash)
  let copiedMsgId    = $state<number | null>(null);

  function copyMessage(msg: Message) {
    navigator.clipboard.writeText(msg.content).catch(() => {});
    copiedMsgId = msg.id;
    setTimeout(() => { if (copiedMsgId === msg.id) copiedMsgId = null; }, 1500);
  }

  // Settings panel
  let showSettings   = $state(false);
  let temperature    = $state(0.8);
  let maxTokens      = $state(2048);
  let topK           = $state(40);
  let topP           = $state(0.9);
  let thinkingLevel  = $state<ThinkingLevel>("minimal");

  // Derived: budget value for current level
  const thinkingBudget = $derived(
    THINKING_LEVELS.find(l => l.key === thinkingLevel)?.budget ?? null
  );

  // Derived
  const canSend   = $derived(
    status === "running" && (input.trim().length > 0 || attachments.length > 0) && !generating
  );
  const canStart  = $derived(status === "stopped");
  const canStop   = $derived(status === "running" || status === "loading");

  const statusLabel = $derived(
    status === "running" ? modelName || t("chat.status.running")
    : status === "loading" ? t("chat.status.loading")
    : t("chat.status.stopped")
  );
  const statusColor = $derived(
    status === "running" ? "text-emerald-500"
    : status === "loading" ? "text-amber-500 animate-pulse"
    : "text-muted-foreground/40"
  );

  // ── Scroll pinning ─────────────────────────────────────────────────────────
  // When the user scrolls up we stop auto-scrolling; as soon as they return
  // to within SNAP_PX of the bottom we re-enable it.

  let pinned = $state(true);
  const SNAP_PX = 48;

  function onMsgsScroll() {
    if (!msgsEl) return;
    pinned = msgsEl.scrollHeight - msgsEl.scrollTop - msgsEl.clientHeight < SNAP_PX;
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  async function scrollBottom(force = false) {
    await tick();
    if (msgsEl && (pinned || force)) msgsEl.scrollTop = msgsEl.scrollHeight;
  }

  function autoResizeInput() {
    if (!inputEl) return;
    inputEl.style.height = "auto";
    inputEl.style.height = Math.min(inputEl.scrollHeight, 200) + "px";
  }

  function inputKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendMessage(); return; }

    // ── History navigation ────────────────────────────────────────────────
    if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
      if (!inputEl) return;

      const cur    = inputEl.selectionStart ?? 0;
      const onFirst = !input.slice(0, cur).includes("\n");
      const onLast  = !input.slice(cur).includes("\n");

      if (e.key === "ArrowUp" && onFirst) {
        if (userHistory.length === 0) return;
        e.preventDefault();
        if (histIdx === -1) histDraft = input;            // save live draft
        const next = Math.min(histIdx + 1, userHistory.length - 1);
        if (next === histIdx) return;
        histIdx = next;
        input   = userHistory[histIdx];
        autoResizeInput();
        // place cursor at end on next tick
        tick().then(() => inputEl?.setSelectionRange(input.length, input.length));
        return;
      }

      if (e.key === "ArrowDown" && onLast) {
        if (histIdx === -1) return;                       // already at draft
        e.preventDefault();
        const next = histIdx - 1;
        if (next < 0) {
          histIdx = -1;
          input   = histDraft;
        } else {
          histIdx = next;
          input   = userHistory[histIdx];
        }
        autoResizeInput();
        tick().then(() => inputEl?.setSelectionRange(input.length, input.length));
      }
    }
  }

  // ── Image attachments ──────────────────────────────────────────────────────

  function openFilePicker() { fileInputEl?.click(); }

  async function onFilesSelected(e: Event) {
    const files = (e.target as HTMLInputElement).files;
    if (!files) return;
    for (const file of Array.from(files)) {
      if (!file.type.startsWith("image/")) continue;
      const dataUrl = await readFileAsDataUrl(file);
      attachments = [...attachments, { dataUrl, mimeType: file.type, name: file.name }];
    }
    // Reset input so the same file can be re-selected
    if (fileInputEl) fileInputEl.value = "";
  }

  function removeAttachment(i: number) {
    attachments = attachments.filter((_, idx) => idx !== i);
  }

  function readFileAsDataUrl(file: File): Promise<string> {
    return new Promise((res, rej) => {
      const reader = new FileReader();
      reader.onload  = () => res(reader.result as string);
      reader.onerror = rej;
      reader.readAsDataURL(file);
    });
  }

  /** Build the content field for a user message (plain string or parts array). */
  function buildUserContent(text: string, imgs: Attachment[]) {
    if (imgs.length === 0) return text;
    const parts: any[] = [];
    if (text.trim()) parts.push({ type: "text", text });
    for (const img of imgs) {
      parts.push({ type: "image_url", image_url: { url: img.dataUrl } });
    }
    return parts;
  }

  // ── Server control ─────────────────────────────────────────────────────────

  async function startServer() {
    status = "loading";
    try {
      await invoke("start_llm_server");
    } catch (e) {
      console.error("start_llm_server failed:", e);
      status = "stopped";
    }
  }

  async function stopServer() {
    if (generating) abort();
    await invoke("stop_llm_server");
    status = "stopped";
    modelName = "";
  }

  function abort() {
    abortCtrl?.abort();
    abortCtrl = null;
    generating = false;
    // The catch block in sendMessage will finalize the message content.
  }

  // ── Chat ───────────────────────────────────────────────────────────────────

  async function sendMessage() {
    const text = input.trim();
    if ((!text && attachments.length === 0) || generating || status !== "running") return;
    input     = "";
    histIdx   = -1;
    histDraft = "";
    pinned    = true;          // always snap to bottom when the user sends
    autoResizeInput();
    const sentAttachments = attachments;
    attachments = [];

    const userContent = buildUserContent(text, sentAttachments);
    const userMsg: Message = {
      id: ++msgId, role: "user",
      // For display we always show plain text; images shown as thumbnails
      content: text,
      attachments: sentAttachments.length ? sentAttachments : undefined,
    };
    messages = [...messages, userMsg];

    const assistantMsg: Message = { id: ++msgId, role: "assistant", content: "", pending: true };
    messages = [...messages, assistantMsg];
    await scrollBottom(true);   // force — user just sent, must see response start

    generating = true;
    abortCtrl  = new AbortController();
    const t0   = performance.now();
    let   ttft: number | undefined;

    // Build the messages array for the API.
    // History uses plain text content; only the newest user turn may carry images.
    // Thinking content is excluded from history.
    const historyMsgs = messages
      .filter(m => !m.pending)
      .map(m => {
        if (m.role === "user" && m.attachments?.length) {
          // Include images for the current turn (last user message with attachments)
          return { role: m.role, content: buildUserContent(m.content, m.attachments) };
        }
        return { role: m.role, content: m.content };
      });

    const apiMessages = [
      ...(systemPrompt.trim() ? [{ role: "system", content: systemPrompt }] : []),
      ...historyMsgs,
    ];

    let rawAcc = ""; // full raw text including <think> tags
    let usage: UsageInfo | undefined;

    try {
      const resp = await fetch(`http://127.0.0.1:${port}/v1/chat/completions`, {
        method:  "POST",
        headers: { "Content-Type": "application/json" },
        body:    JSON.stringify({
          model:            modelName || "default",
          messages:         apiMessages,
          stream:           true,
          temperature,
          max_tokens:       maxTokens,
          top_k:            topK,
          top_p:            topP,
          thinking_budget:  thinkingBudget,
        }),
        signal: abortCtrl.signal,
      });

      if (!resp.ok) {
        const errJson = await resp.json().catch(() => null);
        const errMsg = errJson?.error?.message ?? `HTTP ${resp.status}`;
        messages = messages.map(m =>
          m.id === assistantMsg.id
            ? { ...m, pending: false, content: `*Error: ${errMsg}*` }
            : m
        );
        return;
      }

      const reader  = resp.body!.getReader();
      const decoder = new TextDecoder();
      let   buf     = "";

      outer: while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buf += decoder.decode(value, { stream: true });
        const lines = buf.split("\n");
        buf = lines.pop() ?? "";

        for (const line of lines) {
          if (!line.startsWith("data: ")) continue;
          const data = line.slice(6).trim();
          if (data === "[DONE]") break outer;

          try {
            const json  = JSON.parse(data);
            const delta = json.choices?.[0]?.delta?.content ?? "";
            if (delta) {
              if (ttft === undefined) ttft = performance.now() - t0;
              rawAcc += delta;
              const { thinking, content } = parseThinking(rawAcc);
              messages = messages.map(m =>
                m.id === assistantMsg.id
                  ? { ...m, content, thinking, thinkOpen: m.thinkOpen ?? false }
                  : m
              );
              await scrollBottom();
            }

            // Capture usage when server sends it (on Done chunk)
            if (json.usage?.n_ctx) usage = json.usage as UsageInfo;

            // Check finish_reason
            const fr = json.choices?.[0]?.finish_reason;
            if (fr && fr !== "null") break outer;
          } catch { /* partial JSON chunk — skip */ }
        }
      }

      const elapsed = performance.now() - t0;
      const { thinking, content } = parseThinking(rawAcc);
      messages = messages.map(m =>
        m.id === assistantMsg.id
          ? { ...m, pending: false, content, thinking, ttft, elapsed, usage }
          : m
      );
    } catch (err: any) {
      if (err?.name !== "AbortError") {
        messages = messages.map(m =>
          m.id === assistantMsg.id
            ? { ...m, pending: false, content: `*Connection error: ${err.message}*` }
            : m
        );
      } else {
        // Aborted — keep whatever we have so far, parsed
        const { thinking, content } = parseThinking(rawAcc);
        messages = messages.map(m =>
          m.id === assistantMsg.id
            ? { ...m, pending: false, content: content || "*(aborted)*", thinking }
            : m
        );
      }
    } finally {
      generating = false;
      abortCtrl  = null;
      await scrollBottom();
      await tick();
      inputEl?.focus();
    }
  }

  function clearChat() {
    messages = [];
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  let unlistenStatus: (() => void) | undefined;
  let pollTimer:       ReturnType<typeof setInterval> | undefined;

  onMount(async () => {
    // Port
    try {
      const [, p] = await invoke<[string, number]>("get_ws_config");
      port = p;
    } catch {}

    // Initial status
    try {
      const s = await invoke<{ status: ServerStatus; model_name: string; supports_vision: boolean }>("get_llm_server_status");
      status         = s.status;
      modelName      = s.model_name;
      supportsVision = s.supports_vision ?? false;
    } catch {}

    // Live status events
    try {
      unlistenStatus = await listen<ServerStatusPayload>("llm:status", ev => {
        status    = ev.payload.status ?? (ev.payload as any).status ?? status;
        modelName = (ev.payload as any).model ?? modelName;
        if (status === "running") clearInterval(pollTimer!);
      });
    } catch {}

    // Poll while loading (in case events are delayed)
    pollTimer = setInterval(async () => {
      if (status !== "loading") { clearInterval(pollTimer!); return; }
      try {
        const s = await invoke<{ status: ServerStatus; model_name: string; supports_vision: boolean }>("get_llm_server_status");
        status         = s.status;
        modelName      = s.model_name;
        supportsVision = s.supports_vision ?? false;
      } catch {}
    }, 1500);

    await tick();
    inputEl?.focus();
  });

  onDestroy(() => {
    unlistenStatus?.();
    clearInterval(pollTimer);
    abortCtrl?.abort();
  });

  // ── Formatting helpers ─────────────────────────────────────────────────────

  function fmtMs(ms: number): string {
    return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`;
  }
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Root container (full window height, dark/light theme-aware)                -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<div class="flex flex-col h-screen bg-background text-foreground overflow-hidden">

  <!-- ── Top bar ─────────────────────────────────────────────────────────── -->
  <header class="flex items-center gap-2 px-3 py-2 border-b border-border dark:border-white/[0.06]
                  bg-white dark:bg-[#0f0f18] shrink-0"
          data-tauri-drag-region>

    <!-- Model / status -->
    <div class="flex items-center gap-1.5 flex-1 min-w-0">
      <!-- Live indicator -->
      <span class="w-2 h-2 rounded-full shrink-0
                    {status === 'running'  ? 'bg-emerald-500'
                    : status === 'loading' ? 'bg-amber-500 animate-pulse'
                    :                       'bg-slate-400/50'}"></span>
      <span class="text-[0.72rem] font-semibold truncate {statusColor}">{statusLabel}</span>
    </div>

    <!-- Control buttons -->
    {#if canStart}
      <button
        onclick={startServer}
        class="flex items-center gap-1 text-[0.65rem] font-semibold px-2.5 py-1
               rounded-lg bg-violet-600 hover:bg-violet-700 text-white transition-colors cursor-pointer">
        <svg viewBox="0 0 24 24" fill="currentColor" class="w-3 h-3">
          <polygon points="5,3 19,12 5,21"/>
        </svg>
        {t("chat.btn.start")}
      </button>
    {:else if canStop}
      <button
        onclick={stopServer}
        class="flex items-center gap-1 text-[0.65rem] font-semibold px-2.5 py-1
               rounded-lg border border-red-500/40 text-red-500 hover:bg-red-500/10
               transition-colors cursor-pointer">
        <svg viewBox="0 0 24 24" fill="currentColor" class="w-3 h-3">
          <rect x="4" y="4" width="16" height="16" rx="2"/>
        </svg>
        {status === "loading" ? t("chat.btn.cancel") : t("chat.btn.stop")}
      </button>
    {/if}

    <!-- New chat -->
    <button
      onclick={clearChat}
      disabled={messages.length === 0}
      title={t("chat.btn.newChat")}
      class="p-1.5 rounded-lg text-muted-foreground/60 hover:text-foreground hover:bg-muted
             disabled:opacity-30 disabled:cursor-not-allowed transition-colors cursor-pointer">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
        <path d="M12 5v14M5 12h14"/>
      </svg>
    </button>

    <!-- Language picker -->
    <LanguagePicker />

    <!-- Theme toggle -->
    <ThemeToggle />

    <!-- Settings toggle -->
    <button
      onclick={() => showSettings = !showSettings}
      title={t("chat.btn.params")}
      class="p-1.5 rounded-lg transition-colors cursor-pointer
             {showSettings
               ? 'text-violet-600 bg-violet-500/10'
               : 'text-muted-foreground/60 hover:text-foreground hover:bg-muted'}">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
        <circle cx="12" cy="12" r="3"/>
        <path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>
        <path d="M4.93 4.93a10 10 0 0 0 0 14.14"/>
      </svg>
    </button>
  </header>

  <!-- ── Parameters sidebar (slide-in) ────────────────────────────────────── -->
  {#if showSettings}
    <div class="shrink-0 border-b border-border dark:border-white/[0.06]
                bg-slate-50/60 dark:bg-[#111118] px-4 py-3 flex flex-col gap-3">

      <!-- System prompt -->
      <div class="flex flex-col gap-1">
        <label for="chat-system-prompt"
               class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
          {t("chat.systemPrompt")}
        </label>
        <textarea
          id="chat-system-prompt"
          bind:value={systemPrompt}
          rows="2"
          class="w-full rounded-lg border border-border bg-background text-[0.73rem]
                 text-foreground px-2.5 py-1.5 resize-none focus:outline-none
                 focus:ring-1 focus:ring-violet-500/50"
        ></textarea>
      </div>

      <!-- Thinking level -->
      <div class="flex flex-col gap-1">
        <span class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
          {t("chat.thinkDepth")}
        </span>
        <div class="flex rounded-lg overflow-hidden border border-border text-[0.65rem] font-medium">
          {#each THINKING_LEVELS as lvl}
            <button
              onclick={() => thinkingLevel = lvl.key}
              class="flex-1 py-1 transition-colors cursor-pointer
                     {thinkingLevel === lvl.key
                       ? 'bg-violet-600 text-white'
                       : 'bg-background text-muted-foreground hover:bg-muted'}">
              {t(lvl.labelKey)}
            </button>
          {/each}
        </div>
        <p class="text-[0.58rem] text-muted-foreground/60">
          {t(`chat.think.${thinkingLevel}Desc`)}
        </p>
      </div>

      <!-- Sliders row -->
      <div class="grid grid-cols-2 gap-3">
        {#each [
          { labelKey: "chat.param.temperature", min: 0,  max: 2,    step: 0.05, value: temperature, set: (v: number) => temperature = v },
          { labelKey: "chat.param.maxTokens",  min: 64, max: 8192, step: 64,  value: maxTokens,   set: (v: number) => maxTokens   = v },
          { labelKey: "chat.param.topK",       min: 1,  max: 200,  step: 1,   value: topK,        set: (v: number) => topK        = v },
          { labelKey: "chat.param.topP",       min: 0,  max: 1,    step: 0.05, value: topP,       set: (v: number) => topP        = v },
        ] as s}
          <div class="flex flex-col gap-0.5">
            <div class="flex items-baseline justify-between">
              <span class="text-[0.6rem] text-muted-foreground">{t(s.labelKey)}</span>
              <span class="text-[0.62rem] font-mono text-foreground tabular-nums">{s.value}</span>
            </div>
            <input type="range" min={s.min} max={s.max} step={s.step} value={s.value}
              oninput={(e) => s.set(+(e.target as HTMLInputElement).value)}
              class="w-full accent-violet-500 h-1 cursor-pointer" />
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- ── Message list ──────────────────────────────────────────────────────── -->
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
            <button
              onclick={startServer}
              class="mt-1 px-4 py-2 rounded-xl bg-violet-600 hover:bg-violet-700
                     text-white text-[0.72rem] font-semibold transition-colors cursor-pointer">
              {t("chat.btn.startServer")}
            </button>
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
              <!-- Attached images -->
              {#if msg.attachments?.length}
                <div class="flex flex-wrap gap-1.5 justify-end">
                  {#each msg.attachments as att}
                    <img src={att.dataUrl} alt={att.name}
                         class="h-28 max-w-[14rem] rounded-xl object-cover border border-white/20 shadow-sm" />
                  {/each}
                </div>
              {/if}
              <!-- Text bubble (only if there is text) -->
              {#if msg.content}
                <div class="rounded-2xl rounded-tr-sm bg-violet-600 text-white
                            px-3.5 py-2.5 text-[0.78rem] leading-relaxed whitespace-pre-wrap break-words">
                  {msg.content}
                </div>
              {/if}
            </div>
          </div>

        <!-- Assistant message -->
        {:else if msg.role === "assistant"}
          <div class="flex justify-start gap-2.5">
            <!-- Avatar -->
            <div class="w-6 h-6 rounded-full bg-gradient-to-br from-violet-500 to-indigo-600
                        flex items-center justify-center shrink-0 mt-0.5 text-white text-[0.55rem] font-bold">
              AI
            </div>

            <div class="flex flex-col gap-1 max-w-[82%]">

              <!-- Thinking block (collapsible) -->
              {#if msg.thinking || (msg.pending && msg.content === "" && !msg.thinking)}
                <div class="rounded-xl border border-violet-500/20 bg-violet-500/5
                            text-[0.7rem] overflow-hidden">
                  <button
                    onclick={() => {
                      messages = messages.map(m =>
                        m.id === msg.id ? { ...m, thinkOpen: !m.thinkOpen } : m
                      );
                    }}
                    class="w-full flex items-center gap-1.5 px-3 py-1.5 text-left
                           text-violet-600 dark:text-violet-400 hover:bg-violet-500/10
                           transition-colors cursor-pointer">
                    {#if msg.pending && !msg.thinking?.trim()}
                      <!-- still waiting for thinking content -->
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
                                whitespace-pre-wrap border-t border-violet-500/10 text-[0.68rem]">
                      {msg.thinking}
                    </div>
                  {/if}
                </div>
              {/if}

              <!-- Response bubble (shown once we're past the <think> block) -->
              {#if msg.content || (!msg.pending)}
                <div class="group/bubble flex flex-col gap-0.5">
                  <div class="rounded-2xl rounded-tl-sm bg-muted dark:bg-[#1a1a28]
                              px-3.5 py-2.5 text-[0.78rem] leading-relaxed text-foreground
                              break-words overflow-hidden">
                    {#if !(msg.pending && msg.content === "")}
                      <MarkdownRenderer content={msg.content} pending={msg.pending} />
                    {/if}
                  </div>

                  <!-- Copy raw markdown — visible on hover once generation is done -->
                  {#if !msg.pending && msg.content}
                    <div class="flex opacity-0 group-hover/bubble:opacity-100 transition-opacity duration-150">
                      <button
                        onclick={() => copyMessage(msg)}
                        title="Copy"
                        class="flex items-center gap-1 px-1.5 py-0.5 rounded-md
                               text-muted-foreground/50 hover:text-muted-foreground
                               hover:bg-muted transition-colors cursor-pointer text-[0.6rem]">
                        {#if copiedMsgId === msg.id}
                          <!-- Checkmark -->
                          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                               class="w-3 h-3 text-emerald-500">
                            <polyline points="2 8 6 12 14 4"/>
                          </svg>
                          <span class="text-emerald-500">Copied</span>
                        {:else}
                          <!-- Copy icon -->
                          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                               stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
                               class="w-3 h-3">
                            <rect x="5" y="5" width="9" height="9" rx="1.5"/>
                            <path d="M11 5V3.5A1.5 1.5 0 0 0 9.5 2h-6A1.5 1.5 0 0 0 2 3.5v6A1.5 1.5 0 0 0 3.5 11H5"/>
                          </svg>
                          Copy
                        {/if}
                      </button>
                    </div>
                  {/if}
                </div>
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

  <!-- Jump-to-bottom button — appears when user has scrolled up -->
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

  <!-- Hidden file input for image uploads -->
  <input
    bind:this={fileInputEl}
    type="file"
    accept="image/*"
    multiple
    class="hidden"
    onchange={onFilesSelected}
  />

  <!-- ── Input bar ─────────────────────────────────────────────────────────── -->
  <footer class="shrink-0 border-t border-border dark:border-white/[0.06]
                  bg-white dark:bg-[#0f0f18] px-3 py-2.5">

    <!-- Vision-not-supported warning (shown when images attached but no mmproj loaded) -->
    {#if attachments.length > 0 && !supportsVision}
      <div class="flex items-center gap-2 mb-2 px-2 py-1.5 rounded-lg
                   bg-amber-500/10 border border-amber-500/30 text-amber-600 dark:text-amber-400">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 shrink-0">
          <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
          <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
        </svg>
        <span class="text-[0.65rem] font-medium leading-snug">
          {t("chat.noVision")}
        </span>
      </div>
    {/if}

    <!-- Attachment thumbnails strip -->
    {#if attachments.length > 0}
      <div class="flex flex-wrap gap-1.5 mb-2 px-1">
        {#each attachments as att, i}
          <div class="relative group">
            <img src={att.dataUrl} alt={att.name}
                 class="h-16 w-16 rounded-lg object-cover border border-border shadow-sm
                        {!supportsVision ? 'opacity-50 grayscale' : ''}" />
            <button
              onclick={() => removeAttachment(i)}
              aria-label={t("chat.removeAttachment")}
              class="absolute -top-1.5 -right-1.5 w-4 h-4 rounded-full bg-red-500 text-white
                     flex items-center justify-center opacity-0 group-hover:opacity-100
                     transition-opacity cursor-pointer shadow">
              <svg viewBox="0 0 10 10" fill="currentColor" class="w-2 h-2">
                <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.5"
                      stroke-linecap="round"/>
              </svg>
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <div class="flex items-end gap-2 rounded-xl border border-border dark:border-white/[0.08]
                bg-background px-3 py-2
                focus-within:ring-1 focus-within:ring-violet-500/50
                focus-within:border-violet-500/30 transition-all">

      <!-- Image attach button -->
      <button
        onclick={openFilePicker}
        disabled={status !== "running" || generating}
        title={supportsVision ? t("chat.attachImage") : t("chat.attachImageNoVision")}
        class="shrink-0 p-1 rounded-md transition-colors cursor-pointer self-center
               disabled:opacity-30 disabled:cursor-not-allowed
               {supportsVision
                 ? 'text-muted-foreground/50 hover:text-foreground hover:bg-muted'
                 : 'text-amber-500/70 hover:text-amber-500 hover:bg-amber-500/10'}">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
             stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
          <rect x="3" y="3" width="18" height="18" rx="2"/>
          <circle cx="8.5" cy="8.5" r="1.5"/>
          <polyline points="21 15 16 10 5 21"/>
        </svg>
      </button>

      <textarea
        bind:this={inputEl}
        bind:value={input}
        onkeydown={inputKeydown}
        oninput={autoResizeInput}
        placeholder={status === "running" ? t("chat.inputPlaceholder")
                     : status === "loading" ? t("chat.status.loading")
                     : t("chat.inputPlaceholderStopped")}
        disabled={status !== "running" || generating}
        rows="1"
        class="flex-1 bg-transparent text-[0.78rem] text-foreground resize-none
               placeholder:text-muted-foreground/40 focus:outline-none
               disabled:opacity-50 disabled:cursor-not-allowed
               max-h-48 leading-relaxed"
      ></textarea>

      {#if generating}
        <!-- Abort button -->
        <button
          onclick={abort}
          aria-label={t("chat.btn.stop")}
          class="shrink-0 w-7 h-7 rounded-lg flex items-center justify-center
                 bg-red-500/10 text-red-500 hover:bg-red-500/20 transition-colors cursor-pointer">
          <svg viewBox="0 0 24 24" fill="currentColor" class="w-3.5 h-3.5">
            <rect x="4" y="4" width="16" height="16" rx="2"/>
          </svg>
        </button>
      {:else}
        <!-- Send button -->
        <button
          onclick={sendMessage}
          disabled={!canSend}
          aria-label={t("chat.btn.send")}
          class="shrink-0 w-7 h-7 rounded-lg flex items-center justify-center transition-colors
                 {canSend
                   ? 'bg-violet-600 hover:bg-violet-700 text-white cursor-pointer'
                   : 'bg-muted text-muted-foreground/30 cursor-not-allowed'}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
               stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 -rotate-90">
            <line x1="12" y1="19" x2="12" y2="5"/>
            <polyline points="5 12 12 5 19 12"/>
          </svg>
        </button>
      {/if}
    </div>

    <!-- Footer hint -->
    <p class="text-[0.55rem] text-muted-foreground/30 text-center mt-1.5">
      {#if status === "running"}
        {modelName} · {t("chat.hint.running")}
      {:else if status === "loading"}
        {t("chat.hint.loading")}
      {:else}
        {t("chat.hint.stopped")}
      {/if}
    </p>
  </footer>

</div>
