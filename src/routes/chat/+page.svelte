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
  }

  /**
   * Split a raw model response into thinking and visible parts.
   *
   * Qwen3.5 wraps chain-of-thought in <think>…</think>.
   * We strip those tags and store the content separately so the UI can
   * show it collapsed (or hide it entirely).
   */
  function parseThinking(raw: string): { thinking: string; content: string } {
    // Completed think block
    const full = raw.match(/^<think>([\s\S]*?)<\/think>\s*([\s\S]*)$/);
    if (full) return { thinking: full[1].trim(), content: full[2] };
    // Still-open think block (streaming)
    const open = raw.match(/^<think>([\s\S]*)$/);
    if (open) return { thinking: open[1], content: "" };
    return { thinking: "", content: raw };
  }

  interface ServerStatusPayload { status: ServerStatus; model_name: string; }

  /** Thinking budget levels: token limit for <think> block. null = unlimited. */
  type ThinkingLevel = "minimal" | "normal" | "extended" | "unlimited";
  const THINKING_LEVELS: { label: string; key: ThinkingLevel; budget: number | null }[] = [
    { label: "Minimal",   key: "minimal",   budget: 512   },
    { label: "Normal",    key: "normal",    budget: 2048  },
    { label: "Extended",  key: "extended",  budget: 8192  },
    { label: "Unlimited", key: "unlimited", budget: null  },
  ];

  interface Attachment { dataUrl: string; mimeType: string; name: string; }

  // ── State ──────────────────────────────────────────────────────────────────

  let port           = $state(8375);
  let status         = $state<ServerStatus>("stopped");
  let modelName      = $state("");
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
    status === "running" ? modelName || "Running"
    : status === "loading" ? "Loading model…"
    : "Server stopped"
  );
  const statusColor = $derived(
    status === "running" ? "text-emerald-500"
    : status === "loading" ? "text-amber-500 animate-pulse"
    : "text-muted-foreground/40"
  );

  // ── Helpers ────────────────────────────────────────────────────────────────

  async function scrollBottom() {
    await tick();
    if (msgsEl) msgsEl.scrollTop = msgsEl.scrollHeight;
  }

  function autoResizeInput() {
    if (!inputEl) return;
    inputEl.style.height = "auto";
    inputEl.style.height = Math.min(inputEl.scrollHeight, 200) + "px";
  }

  function inputKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendMessage(); }
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
    input = "";
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
    await scrollBottom();

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
          ? { ...m, pending: false, content, thinking, ttft, elapsed }
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
      const s = await invoke<{ status: ServerStatus; model_name: string }>("get_llm_server_status");
      status    = s.status;
      modelName = s.model_name;
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
        const s = await invoke<{ status: ServerStatus; model_name: string }>("get_llm_server_status");
        status    = s.status;
        modelName = s.model_name;
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
        Start
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
        {status === "loading" ? "Cancel" : "Stop"}
      </button>
    {/if}

    <!-- New chat -->
    <button
      onclick={clearChat}
      disabled={messages.length === 0}
      title="New chat"
      class="p-1.5 rounded-lg text-muted-foreground/60 hover:text-foreground hover:bg-muted
             disabled:opacity-30 disabled:cursor-not-allowed transition-colors cursor-pointer">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
        <path d="M12 5v14M5 12h14"/>
      </svg>
    </button>

    <!-- Theme toggle -->
    <ThemeToggle />

    <!-- Settings toggle -->
    <button
      onclick={() => showSettings = !showSettings}
      title="Parameters"
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
          System prompt
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
          Thinking depth
        </span>
        <div class="flex rounded-lg overflow-hidden border border-border text-[0.65rem] font-medium">
          {#each THINKING_LEVELS as lvl}
            <button
              onclick={() => thinkingLevel = lvl.key}
              class="flex-1 py-1 transition-colors cursor-pointer
                     {thinkingLevel === lvl.key
                       ? 'bg-violet-600 text-white'
                       : 'bg-background text-muted-foreground hover:bg-muted'}">
              {lvl.label}
            </button>
          {/each}
        </div>
        <p class="text-[0.58rem] text-muted-foreground/60">
          {thinkingLevel === "minimal"
            ? "Up to 512 tokens of reasoning — fast, good for simple queries"
            : thinkingLevel === "normal"
            ? "Up to 2 048 tokens — balanced for most tasks"
            : thinkingLevel === "extended"
            ? "Up to 8 192 tokens — best for complex reasoning"
            : "No limit — let the model think as long as it needs"}
        </p>
      </div>

      <!-- Sliders row -->
      <div class="grid grid-cols-2 gap-3">
        {#each [
          { label: "Temperature", min: 0, max: 2,    step: 0.05, value: temperature,  set: (v: number) => temperature = v },
          { label: "Max tokens",  min: 64, max: 8192, step: 64,  value: maxTokens,    set: (v: number) => maxTokens   = v },
          { label: "Top-K",       min: 1,  max: 200,  step: 1,   value: topK,         set: (v: number) => topK        = v },
          { label: "Top-P",       min: 0,  max: 1,    step: 0.05, value: topP,        set: (v: number) => topP        = v },
        ] as s}
          <div class="flex flex-col gap-0.5">
            <div class="flex items-baseline justify-between">
              <span class="text-[0.6rem] text-muted-foreground">{s.label}</span>
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
  <main bind:this={msgsEl}
        class="flex-1 overflow-y-auto px-4 py-4 flex flex-col gap-4
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
            <p class="text-[0.82rem] font-semibold text-foreground">LLM server is not running</p>
            <p class="text-[0.7rem] text-muted-foreground max-w-xs leading-relaxed">
              Start the server to begin chatting. Make sure a model is downloaded in Settings → LLM.
            </p>
            <button
              onclick={startServer}
              class="mt-1 px-4 py-2 rounded-xl bg-violet-600 hover:bg-violet-700
                     text-white text-[0.72rem] font-semibold transition-colors cursor-pointer">
              Start LLM server
            </button>
          </div>
        {:else if status === "loading"}
          <div class="flex flex-col items-center gap-2">
            <p class="text-[0.82rem] font-semibold text-foreground">Loading model…</p>
            <p class="text-[0.7rem] text-muted-foreground">
              Watch the server log in Settings → LLM for progress.
            </p>
            <div class="mt-1 flex gap-1">
              {#each [0,1,2] as i}
                <span class="w-2 h-2 rounded-full bg-violet-500/60 animate-bounce"
                      style="animation-delay: {i * 0.15}s"></span>
              {/each}
            </div>
          </div>
        {:else}
          <p class="text-[0.8rem] text-muted-foreground">Type a message to start chatting.</p>
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
                      <span class="text-[0.65rem]">Thinking…</span>
                    {:else}
                      <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3 shrink-0
                           transition-transform {msg.thinkOpen ? 'rotate-90' : ''}">
                        <path d="M6 3l5 5-5 5V3z"/>
                      </svg>
                      <span class="text-[0.65rem] font-medium">
                        {msg.pending ? "Thinking…" : "Thought"}
                      </span>
                      {#if !msg.pending && msg.thinking}
                        <span class="ml-auto text-[0.6rem] text-muted-foreground/50">
                          {msg.thinking.trim().split(/\s+/).length} words
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
                <div class="rounded-2xl rounded-tl-sm bg-muted dark:bg-[#1a1a28]
                            px-3.5 py-2.5 text-[0.78rem] leading-relaxed text-foreground
                            whitespace-pre-wrap break-words">
                  {#if msg.pending && msg.content === ""}
                    <!-- Generating but still in think block — show nothing yet -->
                  {:else}
                    {msg.content}{#if msg.pending}<span
                      class="inline-block w-0.5 h-[1em] bg-foreground/70 animate-pulse ml-0.5 align-middle"
                    ></span>{/if}
                  {/if}
                </div>
              {/if}

              <!-- Timing info -->
              {#if !msg.pending && msg.elapsed !== undefined}
                <span class="text-[0.55rem] text-muted-foreground/50 px-1">
                  {fmtMs(msg.elapsed)}
                  {#if msg.ttft !== undefined} · first token {fmtMs(msg.ttft)}{/if}
                </span>
              {/if}
            </div>
          </div>
        {/if}
      {/each}
    {/if}

  </main>

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

    <!-- Attachment thumbnails strip -->
    {#if attachments.length > 0}
      <div class="flex flex-wrap gap-1.5 mb-2 px-1">
        {#each attachments as att, i}
          <div class="relative group">
            <img src={att.dataUrl} alt={att.name}
                 class="h-16 w-16 rounded-lg object-cover border border-border shadow-sm" />
            <button
              onclick={() => removeAttachment(i)}
              aria-label="Remove attachment"
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
        title="Attach image"
        class="shrink-0 p-1 rounded-md text-muted-foreground/50 hover:text-foreground
               hover:bg-muted disabled:opacity-30 disabled:cursor-not-allowed
               transition-colors cursor-pointer self-center">
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
        placeholder={status === "running" ? "Message… (Enter to send, Shift+Enter for newline)"
                     : status === "loading" ? "Loading model…"
                     : "Start the server first"}
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
          aria-label="Stop generating"
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
          aria-label="Send message"
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
        {modelName} · Enter to send · Shift+Enter for newline
      {:else if status === "loading"}
        Loading — check Settings → LLM for progress
      {:else}
        Start the server in the top bar or Settings → LLM
      {/if}
    </p>
  </footer>

</div>
