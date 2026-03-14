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
-->
<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { invoke, Channel }          from "@tauri-apps/api/core";
  import { listen }                   from "@tauri-apps/api/event";
  import MarkdownRenderer             from "$lib/MarkdownRenderer.svelte";
  import ChatSidebar                  from "$lib/ChatSidebar.svelte";
  import PromptLibrary                from "$lib/PromptLibrary.svelte";
  import { t }                        from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────

  type Role = "user" | "assistant" | "system";
  type ServerStatus = "stopped" | "loading" | "running";

  interface ToolUseEvent { tool: string; status: string; detail?: string; }

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
    /** Tool calls made during this response */
    toolUses?:    ToolUseEvent[];
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

  interface ServerStatusPayload { status: ServerStatus; model_name: string; model?: string; supports_vision?: boolean; }

  interface UsageInfo {
    prompt_tokens:     number;
    completion_tokens: number;
    total_tokens:      number;
    n_ctx:             number;
  }

  // ── IPC streaming types (mirror Rust ChatChunk) ───────────────────────────

  interface ChatChunkDelta { type: "delta"; content: string; }
  interface ChatChunkToolUse { type: "tool_use"; tool: string; status: string; detail?: string; }
  interface ChatChunkDone  {
    type:              "done";
    finish_reason:     string;
    prompt_tokens:     number;
    completion_tokens: number;
    n_ctx:             number;
  }
  interface ChatChunkError { type: "error"; message: string; }
  type ChatChunk = ChatChunkDelta | ChatChunkToolUse | ChatChunkDone | ChatChunkError;

  /** Thinking budget levels: token limit for <think> block. null = unlimited. */
  type ThinkingLevel = "minimal" | "normal" | "extended" | "unlimited";
  const THINKING_LEVELS: { labelKey: string; key: ThinkingLevel; budget: number | null }[] = [
    { labelKey: "chat.think.minimal",   key: "minimal",   budget: 512   },
    { labelKey: "chat.think.normal",    key: "normal",    budget: 2048  },
    { labelKey: "chat.think.extended",  key: "extended",  budget: 8192  },
    { labelKey: "chat.think.unlimited", key: "unlimited", budget: null  },
  ];

  interface Attachment { dataUrl: string; mimeType: string; name: string; }

  // ── Stored-message type (mirrors Rust StoredMessage) ──────────────────────

  interface StoredMessage {
    id:         number;
    session_id: number;
    role:       string;
    content:    string;
    thinking:   string | null;
    created_at: number;
  }

  interface ChatSessionResponse {
    session_id: number;
    messages:   StoredMessage[];
  }

  function storedToMessage(sm: StoredMessage): Message {
    return {
      id:        ++msgId,
      role:      sm.role as Role,
      content:   sm.content,
      thinking:  sm.thinking ?? undefined,
      thinkOpen: false,
      pending:   false,
    };
  }

  // ── EEG band snapshot type (mirrors Rust BandSnapshot) ────────────────────

  interface BandPowers {
    channel:       string;
    rel_delta:     number;
    rel_theta:     number;
    rel_alpha:     number;
    rel_beta:      number;
    rel_gamma:     number;
    rel_high_gamma: number;
    dominant:      string;
    dominant_symbol: string;
  }

  interface BandSnapshot {
    timestamp:     number;
    channels:      BandPowers[];
    faa:           number;
    tar:           number;
    bar:           number;
    dtr:           number;
    pse:           number;
    apf:           number;
    bps:           number;
    snr:           number;
    coherence:     number;
    mu_suppression: number;
    mood:          number;
    tbr:           number;
    sef95:         number;
    hjorth_activity: number;
    hjorth_mobility: number;
    hjorth_complexity: number;
    permutation_entropy: number;
    higuchi_fd:    number;
    dfa_exponent:  number;
    sample_entropy: number;
    pac_theta_gamma: number;
    laterality_index: number;
    headache_index: number;
    migraine_index: number;
    consciousness_lzc: number;
    consciousness_wakefulness: number;
    consciousness_integration: number;
    hr?:           number;
    rmssd?:        number;
    sdnn?:         number;
    pnn50?:        number;
    lf_hf_ratio?:  number;
    respiratory_rate?: number;
    spo2_estimate?: number;
    perfusion_index?: number;
    stress_index?: number;
    blink_count?:  number;
    blink_rate?:   number;
    meditation?:   number;
    cognitive_load?: number;
    drowsiness?:   number;
  }

  // ── State ──────────────────────────────────────────────────────────────────

  interface ToolConfig { date: boolean; location: boolean; web_search: boolean; web_fetch: boolean; }

  let status         = $state<ServerStatus>("stopped");
  let modelName      = $state("");
  let supportsVision = $state(false);
  let supportsTools  = $state(false);
  let toolConfig     = $state<ToolConfig>({ date: true, location: true, web_search: true, web_fetch: true });
  let messages       = $state<Message[]>([]);
  let sessionId      = $state(0);   // current chat_history.sqlite session id
  let input          = $state("");

  // ── System prompt (persisted in localStorage) ──────────────────────────────

  const SYSTEM_PROMPT_DEFAULT = "You are a helpful assistant.";
  const SYSTEM_PROMPT_KEY     = "chat.systemPrompt";

  /**
   * Preset personas — stored as plain strings so users can pick a starting
   * point and then edit freely.  The key is used only for i18n lookup.
   */
  const SYSTEM_PROMPT_PRESETS: { key: string; icon: string; prompt: string }[] = [
    {
      key:    "default",
      icon:   "🤖",
      prompt: "You are a helpful assistant.",
    },
    {
      key:    "coach",
      icon:   "🧘",
      prompt:
        "You are a neurofeedback coach specialising in relaxation and stress reduction. " +
        "Give practical, evidence-based advice grounded in the user's live EEG data. " +
        "Be encouraging, clear, and concise.",
    },
    {
      key:    "focus",
      icon:   "🎯",
      prompt:
        "You are a focus and cognitive-performance coach. " +
        "Help the user optimise their attention, working memory, and mental endurance " +
        "using insights from their EEG readings. Offer actionable protocols.",
    },
    {
      key:    "educator",
      icon:   "📚",
      prompt:
        "You are a neuroscience educator. " +
        "Explain brain-wave patterns and EEG metrics in plain, accessible language. " +
        "Use analogies freely and avoid unnecessary jargon.",
    },
    {
      key:    "sleep",
      icon:   "😴",
      prompt:
        "You are a sleep and recovery specialist. " +
        "Interpret the user's EEG data to provide personalised guidance on improving " +
        "sleep quality, recovery, and circadian regulation.",
    },
    {
      key:    "mindfulness",
      icon:   "🌿",
      prompt:
        "You are a mindfulness and meditation guide. " +
        "Use the user's real-time brainwave data to suggest meditation techniques, " +
        "breathing exercises, and awareness practices tailored to their current state.",
    },
  ];

  /**
   * Load the persisted system prompt from localStorage, falling back to the
   * default.  This runs synchronously before the component renders so there
   * is no flash of the wrong value.
   */
  function loadSystemPrompt(): string {
    try {
      return localStorage.getItem(SYSTEM_PROMPT_KEY) ?? SYSTEM_PROMPT_DEFAULT;
    } catch {
      return SYSTEM_PROMPT_DEFAULT;
    }
  }

  let systemPrompt = $state(loadSystemPrompt());

  /** Persist on every change via a reactive effect. */
  $effect(() => {
    try { localStorage.setItem(SYSTEM_PROMPT_KEY, systemPrompt); } catch { /* storage full */ }
  });

  /** Apply a preset and focus the textarea so the user can refine it. */
  let systemPromptEl = $state<HTMLTextAreaElement | null>(null);
  function applyPreset(prompt: string) {
    systemPrompt = prompt;
    tick().then(() => systemPromptEl?.focus());
  }

  /** Whether the prompt has been edited away from the default. */
  const isDefaultPrompt = $derived(systemPrompt.trim() === SYSTEM_PROMPT_DEFAULT.trim());

  /** Number of enabled tools. */
  const enabledToolCount = $derived(
    [toolConfig.date, toolConfig.location, toolConfig.web_search, toolConfig.web_fetch]
      .filter(Boolean).length
  );

  /** Persist tool config changes to the Rust backend. */
  async function updateToolConfig(patch: Partial<ToolConfig>) {
    toolConfig = { ...toolConfig, ...patch };
    try {
      const cfg = await invoke<any>("get_llm_config");
      cfg.tools = { ...toolConfig };
      await invoke("set_llm_config", { config: cfg });
    } catch { /* settings unavailable */ }
  }

  let generating     = $state(false);
  let aborting       = $state(false);   // true while abort_llm_stream is in flight
  let msgId          = $state(0);
  let msgsEl         = $state<HTMLElement | null>(null);
  let inputEl        = $state<HTMLTextAreaElement | null>(null);
  let fileInputEl    = $state<HTMLInputElement | null>(null);
  let promptLibRef   = $state<PromptLibrary | null>(null);
  let attachments    = $state<Attachment[]>([]);

  // ── Sidebar ────────────────────────────────────────────────────────────────
  let sidebarOpen = $state(true);
  let sidebarRef  = $state<ChatSidebar | null>(null);

  // ── EEG context injection ──────────────────────────────────────────────────

  /** Latest EEG band snapshot from the `eeg-bands` Tauri event. */
  let latestBands  = $state<BandSnapshot | null>(null);
  /** Whether to prepend the live EEG brain state to each LLM system prompt. */
  let eegContext   = $state(true);

  /** Derived: EEG context is active (enabled AND signal available). */
  const eegActive  = $derived(eegContext && latestBands !== null);

  /**
   * Build a compact EEG brain-state block suitable for injection into a
   * system prompt.  Averages relative band powers across all 4 channels.
   */
  function buildEegBlock(b: BandSnapshot): string {
    const n = b.channels.length || 1;
    const avg = (key: keyof BandPowers) =>
      b.channels.reduce((s, ch) => s + (ch[key] as number), 0) / n;

    const pct = (v: number) => (v * 100).toFixed(0) + "%";
    const f1  = (v: number) => v.toFixed(1);
    const f2  = (v: number) => (v >= 0 ? "+" : "") + v.toFixed(2);

    const rD = avg("rel_delta");
    const rT = avg("rel_theta");
    const rA = avg("rel_alpha");
    const rB = avg("rel_beta");
    const rG = avg("rel_gamma");

    const dominant = b.channels[0]?.dominant ?? "—";

    const lines: string[] = [
      "--- Live EEG Brain State (auto-updated) ---",
      `Signal quality (SNR): ${f1(b.snr)} dB | Dominant band: ${dominant}`,
      `Relative band powers: δ=${pct(rD)} θ=${pct(rT)} α=${pct(rA)} β=${pct(rB)} γ=${pct(rG)}`,
      `Mood: ${b.mood.toFixed(0)}/100 | FAA (approach): ${f2(b.faa)}`,
      `TAR (θ/α — drowsiness): ${f1(b.tar)} | BAR (β/α — focus/stress): ${f1(b.bar)}`,
      `Coherence (α sync): ${(b.coherence * 100).toFixed(0)}% | Consciousness: wakefulness=${b.consciousness_wakefulness.toFixed(0)} integration=${b.consciousness_integration.toFixed(0)}`,
    ];

    if (b.meditation   != null) lines.push(`Meditation: ${b.meditation.toFixed(0)}/100`);
    if (b.cognitive_load != null) lines.push(`Cognitive load: ${b.cognitive_load.toFixed(0)}/100`);
    if (b.drowsiness   != null) lines.push(`Drowsiness: ${b.drowsiness.toFixed(0)}/100`);
    if (b.hr           != null) lines.push(`Heart rate: ${b.hr.toFixed(0)} bpm${b.rmssd != null ? ` | HRV (RMSSD): ${b.rmssd.toFixed(1)} ms` : ""}`);
    if (b.stress_index != null) lines.push(`Stress index: ${b.stress_index.toFixed(1)}`);
    if (b.respiratory_rate != null) lines.push(`Respiratory rate: ${b.respiratory_rate.toFixed(1)} breaths/min`);

    lines.push("--- End EEG State ---");
    return lines.join("\n");
  }

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
    if (generating) { abort(); await new Promise(r => setTimeout(r, 100)); }
    await invoke("stop_llm_server");
    status = "stopped";
    modelName = "";
  }

  function abort() {
    if (aborting) return;
    aborting = true;
    // Tell the Rust actor to stop generating; the IPC channel will receive
    // ChatChunk::Error { message: "aborted" } and finalise the message.
    invoke("abort_llm_stream").catch(() => {}).finally(() => { aborting = false; });
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

    // Auto-title: use the first user message text as the session title.
    // We check BEFORE pushing so the count is 0 for the very first message.
    const isFirstUserMsg = !messages.some(m => m.role === "user" && m.content.trim());
    if (isFirstUserMsg && text && sessionId > 0) {
      const autoTitle = text.slice(0, 60).replace(/\n+/g, " ").trim();
      invoke("rename_chat_session", { id: sessionId, title: autoTitle }).catch(() => {});
      sidebarRef?.updateTitle(sessionId, autoTitle);
    }

    messages = [...messages, userMsg];

    // Persist user message immediately (fire-and-forget)
    if (sessionId > 0 && text) {
      invoke("save_chat_message", {
        session_id: sessionId, role: "user", content: text, thinking: null,
      }).catch(() => {});
    }

    const assistantMsg: Message = { id: ++msgId, role: "assistant", content: "", pending: true };
    messages = [...messages, assistantMsg];
    await scrollBottom(true);   // force — user just sent, must see response start

    generating = true;
    const t0   = performance.now();
    let   ttft: number | undefined;

    // Build the messages array for the IPC command.
    // History uses plain text content; the newest user turn may carry images.
    // Thinking content is excluded from history sent to the model.
    const historyMsgs = messages
      .filter(m => !m.pending)
      .map(m => {
        if (m.role === "user" && m.attachments?.length) {
          return { role: m.role, content: buildUserContent(m.content, m.attachments) };
        }
        return { role: m.role, content: m.content };
      });

    // Build system message: base persona + optional live EEG brain-state block.
    const systemParts: string[] = [];
    if (systemPrompt.trim()) systemParts.push(systemPrompt.trim());
    if (eegActive && latestBands) systemParts.push(buildEegBlock(latestBands));

    const apiMessages = [
      ...(systemParts.length ? [{ role: "system", content: systemParts.join("\n\n") }] : []),
      ...historyMsgs,
    ];

    let rawAcc = ""; // full raw text including any <think> tags
    let usage: UsageInfo | undefined;

    // ── IPC Channel — tokens stream directly from the Rust actor, no HTTP ──
    //
    // Each message is a ChatChunk tagged union:
    //   { type:"delta",  content:"…" }
    //   { type:"done",   finish_reason, prompt_tokens, completion_tokens, n_ctx }
    //   { type:"error",  message:"…" }  (message=="aborted" when user cancelled)
    //
    // The channel is alive until `invoke("chat_completions_ipc")` resolves.
    // Calling `abort_llm_stream` causes the Rust side to send an "aborted"
    // error chunk and return, which resolves the invoke Promise.

    const channel = new Channel<ChatChunk>();
    channel.onmessage = async (chunk: ChatChunk) => {
      if (chunk.type === "delta") {
        if (ttft === undefined) ttft = performance.now() - t0;
        rawAcc += chunk.content;
        const { thinking, content } = parseThinking(rawAcc);
        messages = messages.map(m =>
          m.id === assistantMsg.id
            ? { ...m, content, thinking, thinkOpen: m.thinkOpen ?? false }
            : m
        );
        await scrollBottom();

      } else if (chunk.type === "tool_use") {
        const evt: ToolUseEvent = { tool: chunk.tool, status: chunk.status, detail: chunk.detail };
        messages = messages.map(m => {
          if (m.id !== assistantMsg.id) return m;
          const existing = m.toolUses ?? [];
          // Update existing entry for same tool or add new one
          const idx = existing.findIndex(e => e.tool === evt.tool && e.status === "calling");
          if (evt.status !== "calling" && idx >= 0) {
            const updated = [...existing];
            updated[idx] = evt;
            return { ...m, toolUses: updated };
          }
          return { ...m, toolUses: [...existing, evt] };
        });
        await scrollBottom();

      } else if (chunk.type === "done") {
        const elapsed = performance.now() - t0;
        const { thinking, content } = parseThinking(rawAcc);
        usage = {
          prompt_tokens:     chunk.prompt_tokens,
          completion_tokens: chunk.completion_tokens,
          total_tokens:      chunk.prompt_tokens + chunk.completion_tokens,
          n_ctx:             chunk.n_ctx,
        };
        messages = messages.map(m =>
          m.id === assistantMsg.id
            ? { ...m, pending: false, content, thinking, ttft, elapsed, usage }
            : m
        );
        await scrollBottom();

      } else if (chunk.type === "error") {
        // "aborted" → keep whatever was streamed so far as the final answer.
        // Any other message → show as an error bubble.
        const { thinking, content } = parseThinking(rawAcc);
        messages = messages.map(m =>
          m.id === assistantMsg.id
            ? {
                ...m, pending: false,
                content:  chunk.message === "aborted"
                  ? (content || "*(aborted)*")
                  : `*Error: ${chunk.message}*`,
                thinking: chunk.message === "aborted" ? thinking : undefined,
              }
            : m
        );
        await scrollBottom();
      }
    };

    try {
      await invoke("chat_completions_ipc", {
        messages: apiMessages,
        params: {
          temperature,
          max_tokens:      maxTokens,
          top_k:           topK,
          top_p:           topP,
          thinking_budget: thinkingBudget,
        },
        channel,
      });
    } catch (err: any) {
      // Command-level error (server not running, serialisation failure, etc.)
      messages = messages.map(m =>
        m.id === assistantMsg.id
          ? { ...m, pending: false, content: `*Error: ${String(err)}*` }
          : m
      );
    } finally {
      // Safety net: if the channel closed without a done/error chunk
      // (e.g. actor crashed), mark the message as no longer pending.
      messages = messages.map(m =>
        m.id === assistantMsg.id && m.pending
          ? { ...m, pending: false, ...parseThinking(rawAcc) }
          : m
      );

      const finalAssistant = messages.find(m => m.id === assistantMsg.id);

      generating = false;
      await scrollBottom();
      await tick();
      inputEl?.focus();

      // Persist the completed assistant message (fire-and-forget).
      if (sessionId > 0 && finalAssistant && !finalAssistant.pending && finalAssistant.content) {
        invoke("save_chat_message", {
          session_id: sessionId,
          role:       "assistant",
          content:    finalAssistant.content,
          thinking:   finalAssistant.thinking ?? null,
        }).catch(() => {});
      }
    }
  }

  /** Insert a prompt-library template into the input field. */
  async function selectPrompt(text: string) {
    input   = text;
    histIdx = -1;
    await tick();
    autoResizeInput();
    inputEl?.focus();
    inputEl?.setSelectionRange(input.length, input.length);
  }

  /** Create a new empty session and switch to it. */
  async function newChat() {
    if (generating) abort();
    messages  = [];
    histIdx   = -1;
    histDraft = "";
    try {
      sessionId = await invoke<number>("new_chat_session");
      await sidebarRef?.refresh();
    } catch { /* store unavailable — continue without persistence */ }
    await tick();
    inputEl?.focus();
  }

  /** Switch to an existing session by id. */
  async function loadSession(id: number) {
    if (id === sessionId) return;
    if (generating) abort();
    messages  = [];
    histIdx   = -1;
    histDraft = "";
    pinned    = true;
    try {
      const resp = await invoke<ChatSessionResponse>("load_chat_session", { id });
      sessionId  = resp.session_id;
      messages   = resp.messages.map(storedToMessage);
      await scrollBottom(true);
    } catch {}
    await tick();
    inputEl?.focus();
  }

  /** Called by the sidebar when the user deletes a session. */
  async function handleSidebarDelete(deletedId: number) {
    if (deletedId !== sessionId) return;
    // The active session was deleted — fall back to the most-recent remaining one.
    messages  = [];
    sessionId = 0;
    try {
      const resp = await invoke<ChatSessionResponse>("get_last_chat_session");
      sessionId  = resp.session_id;
      if (resp.messages.length > 0) {
        messages = resp.messages.map(storedToMessage);
        await scrollBottom(true);
      }
      // Sidebar already removed the item locally; refresh syncs any cascade.
      await sidebarRef?.refresh();
    } catch {}
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  let unlistenStatus: (() => void) | undefined;
  let unlistenBands:  (() => void) | undefined;
  let pollTimer:       ReturnType<typeof setInterval> | undefined;

  onMount(async () => {
    // Initial status
    try {
      const s = await invoke<{ status: ServerStatus; model_name: string; supports_vision: boolean; supports_tools: boolean }>("get_llm_server_status");
      status         = s.status;
      modelName      = s.model_name;
      supportsVision = s.supports_vision ?? false;
      supportsTools  = s.supports_tools ?? false;
    } catch {}

    // Load tool config from persisted settings
    try {
      const cfg = await invoke<any>("get_llm_config");
      if (cfg?.tools) {
        toolConfig = {
          date:       cfg.tools.date       ?? true,
          location:   cfg.tools.location   ?? true,
          web_search: cfg.tools.web_search ?? true,
          web_fetch:  cfg.tools.web_fetch  ?? true,
        };
      }
    } catch {}

    // Live status events
    try {
      unlistenStatus = await listen<ServerStatusPayload>("llm:status", ev => {
        status    = ev.payload.status ?? (ev.payload as any).status ?? status;
        modelName = (ev.payload as any).model ?? ev.payload.model_name ?? modelName;
        // The actor includes supports_vision in the "running" payload so we can
        // update immediately without waiting for the next poll cycle.
        if (ev.payload.supports_vision !== undefined) {
          supportsVision = ev.payload.supports_vision;
        }
        if ((ev.payload as any).supports_tools !== undefined) {
          supportsTools = (ev.payload as any).supports_tools;
        }
        if (status === "running") clearInterval(pollTimer!);
        // When the server stops, capabilities reset.
        if (status === "stopped") { supportsVision = false; supportsTools = false; }
      });
    } catch {}

    // Poll while loading (in case events are delayed).
    // We intentionally do NOT stop on the first "running" tick — we do one
    // extra fetch after the transition to ensure supportsVision is current
    // even if the llm:status event arrived before vision_flag was written.
    let ranAfterRunning = false;
    pollTimer = setInterval(async () => {
      if (status !== "loading" && (status !== "running" || ranAfterRunning)) {
        clearInterval(pollTimer!);
        return;
      }
      if (status === "running") ranAfterRunning = true;
      try {
        const s = await invoke<{ status: ServerStatus; model_name: string; supports_vision: boolean; supports_tools: boolean }>("get_llm_server_status");
        status         = s.status;
        modelName      = s.model_name;
        supportsVision = s.supports_vision ?? false;
        supportsTools  = s.supports_tools ?? false;
      } catch {}
    }, 1500);

    // Seed with latest bands if headset already connected
    try {
      const b = await invoke<BandSnapshot | null>("get_latest_bands");
      if (b) latestBands = b;
    } catch {}

    // Live EEG band updates — keep latestBands fresh for context injection
    try {
      unlistenBands = await listen<BandSnapshot>("eeg-bands", ev => {
        latestBands = ev.payload;
      });
    } catch {}

    // Load persisted chat history
    try {
      const resp = await invoke<ChatSessionResponse>("get_last_chat_session");
      sessionId = resp.session_id;
      if (resp.messages.length > 0) {
        messages = resp.messages.map(storedToMessage);
        await scrollBottom(true);
      }
    } catch { /* store unavailable — start fresh */ }

    // Sidebar loads its own session list via onMount; no extra call needed.

    await tick();
    inputEl?.focus();
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenBands?.();
    clearInterval(pollTimer);
    if (generating) invoke("abort_llm_stream").catch(() => {});
  });

  // ── Formatting helpers ─────────────────────────────────────────────────────

  function fmtMs(ms: number): string {
    return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`;
  }
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Root container (full window height, dark/light theme-aware)                -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<div class="flex h-full min-h-0 bg-background text-foreground overflow-hidden">

  <!-- ── Conversation sidebar ───────────────────────────────────────────── -->
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

  <!-- ── Main chat column ───────────────────────────────────────────────── -->
  <div class="min-h-0 flex flex-col flex-1 min-w-0 overflow-hidden">

  <!-- ── Top bar ─────────────────────────────────────────────────────────── -->
  <header class="flex items-center gap-2 px-3 py-2 border-b border-border dark:border-white/[0.06]
                  bg-white dark:bg-[#0f0f18] shrink-0"
          data-tauri-drag-region>

    <!-- Sidebar toggle -->
    <button
      onclick={() => sidebarOpen = !sidebarOpen}
      title={sidebarOpen ? "Hide conversations" : "Show conversations"}
      class="p-1.5 rounded-lg transition-colors cursor-pointer shrink-0
             {sidebarOpen
               ? 'text-violet-600 dark:text-violet-400 bg-violet-500/10'
               : 'text-muted-foreground/60 hover:text-foreground hover:bg-muted'}">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" class="w-3.5 h-3.5">
        <line x1="3" y1="6"  x2="21" y2="6"/>
        <line x1="3" y1="12" x2="21" y2="12"/>
        <line x1="3" y1="18" x2="21" y2="18"/>
      </svg>
    </button>

    <!-- Model / status -->
    <div class="flex items-center gap-1.5 flex-1 min-w-0">
      <!-- Live indicator -->
      <span class="w-2 h-2 rounded-full shrink-0
                    {status === 'running'  ? 'bg-emerald-500'
                    : status === 'loading' ? 'bg-amber-500 animate-pulse'
                    :                       'bg-slate-400/50'}"></span>
      <span class="text-[0.72rem] font-semibold truncate {statusColor}">{statusLabel}</span>
    </div>

    <!-- Tools badge -->
    {#if supportsTools && enabledToolCount > 0}
      <button
        onclick={() => showSettings = true}
        title="{enabledToolCount} tool{enabledToolCount !== 1 ? 's' : ''} enabled"
        class="flex items-center gap-1 px-1.5 py-0.5 rounded-md transition-colors cursor-pointer
               shrink-0 text-[0.6rem] font-semibold
               bg-primary/10 text-primary hover:bg-primary/20">
        <!-- Wrench icon -->
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6"
             stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
          <path d="M14.7 6.3a1 1 0 0 0 0-1.4l-.6-.6a1 1 0 0 0-1.4 0L6.3 10.7a1 1 0 0 0 0 1.4l.6.6a1 1 0 0 0 1.4 0z"/>
          <path d="M16 2l2 2-1.5 1.5L14.5 3.5z"/>
          <path d="M2 18l4-1 9.3-9.3-3-3L3 14z"/>
        </svg>
        <span>{t("chat.tools.badge")}</span>
        <span class="tabular-nums opacity-70">{enabledToolCount}</span>
      </button>
    {/if}

    <!-- EEG context badge -->
    {#if latestBands}
      <button
        onclick={() => eegContext = !eegContext}
        title={eegContext ? t("chat.eeg.on") : t("chat.eeg.off")}
        class="flex items-center gap-1 px-1.5 py-0.5 rounded-md transition-colors cursor-pointer
               shrink-0 text-[0.6rem] font-semibold
               {eegContext
                 ? 'bg-cyan-500/15 text-cyan-600 dark:text-cyan-400 hover:bg-cyan-500/25'
                 : 'bg-muted text-muted-foreground/40 hover:bg-muted/80'}">
        <!-- Brain-wave icon -->
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6"
             stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
          <path d="M2 10 Q4 6 6 10 Q8 14 10 10 Q12 6 14 10 Q16 14 18 10"/>
        </svg>
        <span>{t("chat.eeg.label")}</span>
        {#if eegContext && latestBands}
          <span class="tabular-nums opacity-70">{latestBands.snr.toFixed(1)}dB</span>
        {/if}
      </button>
    {/if}

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
      onclick={newChat}
      title={t("chat.btn.newChat")}
      class="p-1.5 rounded-lg text-muted-foreground/60 hover:text-foreground hover:bg-muted
             disabled:opacity-30 disabled:cursor-not-allowed transition-colors cursor-pointer">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
        <path d="M12 5v14M5 12h14"/>
      </svg>
    </button>

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

      <!-- ── System prompt ─────────────────────────────────────────────────── -->
      <div class="flex flex-col gap-1.5">

        <!-- Header row: label + char count + reset -->
        <div class="flex items-baseline justify-between gap-2">
          <label for="chat-system-prompt"
                 class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
            {t("chat.systemPrompt")}
          </label>
          <div class="flex items-center gap-2">
            <!-- Character counter -->
            <span class="text-[0.55rem] tabular-nums text-muted-foreground/40 select-none">
              {t("chat.systemPrompt.chars", { n: systemPrompt.length })}
            </span>
            <!-- Reset to default -->
            {#if !isDefaultPrompt}
              <button
                onclick={() => applyPreset(SYSTEM_PROMPT_DEFAULT)}
                title={t("chat.systemPrompt.reset")}
                class="flex items-center gap-0.5 text-[0.58rem] text-muted-foreground/50
                       hover:text-violet-600 dark:hover:text-violet-400 transition-colors
                       cursor-pointer select-none">
                <svg viewBox="0 0 16 16" fill="none" stroke="currentColor"
                     stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"
                     class="w-2.5 h-2.5">
                  <path d="M13.5 8A5.5 5.5 0 1 1 8 2.5"/>
                  <polyline points="13.5 2.5 13.5 6 10 6"/>
                </svg>
                {t("chat.systemPrompt.reset")}
              </button>
            {/if}
          </div>
        </div>

        <!-- Textarea -->
        <textarea
          id="chat-system-prompt"
          bind:this={systemPromptEl}
          bind:value={systemPrompt}
          rows="4"
          spellcheck="true"
          class="w-full rounded-lg border border-border bg-background text-[0.73rem]
                 text-foreground px-2.5 py-1.5 resize-y leading-relaxed
                 focus:outline-none focus:ring-1 focus:ring-violet-500/50
                 min-h-[4.5rem] max-h-48 transition-shadow"
          style="field-sizing: content"
        ></textarea>

        <!-- Preset persona chips -->
        <div class="flex flex-col gap-1">
          <span class="text-[0.55rem] font-semibold uppercase tracking-widest
                       text-muted-foreground/50 select-none">
            {t("chat.systemPrompt.presets")}
          </span>
          <div class="flex flex-wrap gap-1">
            {#each SYSTEM_PROMPT_PRESETS as preset}
              {@const isActive = systemPrompt.trim() === preset.prompt.trim()}
              <button
                onclick={() => applyPreset(preset.prompt)}
                title={preset.prompt}
                class="flex items-center gap-1 px-2 py-0.5 rounded-md text-[0.63rem]
                       border transition-all cursor-pointer select-none
                       {isActive
                         ? 'border-violet-500/50 bg-violet-500/10 text-violet-700 dark:text-violet-300'
                         : 'border-border bg-background text-muted-foreground/70 hover:border-violet-400/40 hover:bg-violet-500/8 hover:text-foreground'}">
                <span aria-hidden="true">{preset.icon}</span>
                {t(`chat.systemPrompt.preset.${preset.key}`)}
              </button>
            {/each}
          </div>
        </div>
      </div>

      <!-- EEG context injection toggle -->
      <div class="flex items-center justify-between gap-2">
        <div class="flex items-center gap-1.5">
          <!-- Brain-wave icon -->
          <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6"
               stroke-linecap="round" stroke-linejoin="round"
               class="w-3 h-3 shrink-0 {latestBands ? 'text-cyan-500' : 'text-muted-foreground/30'}">
            <path d="M2 10 Q4 6 6 10 Q8 14 10 10 Q12 6 14 10 Q16 14 18 10"/>
          </svg>
          <span class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
            {t("chat.eeg.contextLabel")}
          </span>
        </div>
        <button
          onclick={() => eegContext = !eegContext}
          disabled={!latestBands}
          title={latestBands ? (eegContext ? t("chat.eeg.on") : t("chat.eeg.off")) : t("chat.eeg.noSignal")}
          class="relative inline-flex h-4 w-7 shrink-0 cursor-pointer items-center
                 rounded-full transition-colors disabled:opacity-30 disabled:cursor-not-allowed
                 {eegContext && latestBands ? 'bg-cyan-500' : 'bg-muted-foreground/20'}">
          <span class="inline-block h-3 w-3 rounded-full bg-white shadow
                       transition-transform
                       {eegContext && latestBands ? 'translate-x-3.5' : 'translate-x-0.5'}">
          </span>
        </button>
      </div>

      <!-- Live EEG stats preview (when context active) -->
      {#if eegActive && latestBands}
        {@const b = latestBands}
        {@const n = b.channels.length || 1}
        {@const rA = b.channels.reduce((s,c)=>s+c.rel_alpha,0)/n}
        {@const rB = b.channels.reduce((s,c)=>s+c.rel_beta,0)/n}
        {@const rT = b.channels.reduce((s,c)=>s+c.rel_theta,0)/n}
        <div class="grid grid-cols-4 gap-1.5 rounded-lg border border-cyan-500/20
                     bg-cyan-500/5 px-2 py-1.5">
          {#each [
            { label: "SNR",   val: b.snr.toFixed(1) + "dB" },
            { label: "Mood",  val: b.mood.toFixed(0) + "/100" },
            { label: "α",     val: (rA*100).toFixed(0) + "%" },
            { label: "β/α",   val: b.bar.toFixed(2) },
            { label: "θ/α",   val: b.tar.toFixed(2) },
            { label: "FAA",   val: (b.faa>=0?"+":"")+b.faa.toFixed(2) },
            ...(b.hr != null ? [{ label: "HR", val: b.hr.toFixed(0)+"bpm" }] : []),
            ...(b.meditation != null ? [{ label: "Med", val: b.meditation.toFixed(0)+"/100" }] : []),
          ] as s}
            <div class="flex flex-col items-center gap-0">
              <span class="text-[0.48rem] text-cyan-600/60 dark:text-cyan-400/60 font-medium uppercase">
                {s.label}
              </span>
              <span class="text-[0.6rem] font-semibold tabular-nums text-cyan-700 dark:text-cyan-300">
                {s.val}
              </span>
            </div>
          {/each}
        </div>
      {/if}

      <!-- Tools allow-list -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between gap-2">
          <span class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
            {t("chat.tools.label")}
          </span>
          {#if supportsTools}
            <span class="text-[0.55rem] tabular-nums text-muted-foreground/40 select-none">
              {enabledToolCount}/4
            </span>
          {/if}
        </div>
        {#if !supportsTools}
          <p class="text-[0.58rem] text-muted-foreground/50 italic">
            {t("chat.tools.unsupported")}
          </p>
        {:else}
          <div class="grid grid-cols-2 gap-1.5">
            {#each [
              { key: "date"       as const, icon: "🕐" },
              { key: "location"   as const, icon: "📍" },
              { key: "web_search" as const, icon: "🔍" },
              { key: "web_fetch"  as const, icon: "🌐" },
            ] as tool}
              <button
                onclick={() => updateToolConfig({ [tool.key]: !toolConfig[tool.key] })}
                class="flex items-center gap-2 px-2.5 py-1.5 rounded-lg border transition-all
                       cursor-pointer select-none text-left
                       {toolConfig[tool.key]
                         ? 'border-primary/40 bg-primary/8 text-foreground'
                         : 'border-border bg-background text-muted-foreground/50 hover:border-muted-foreground/30'}">
                <span class="text-sm shrink-0">{tool.icon}</span>
                <div class="flex flex-col gap-0 min-w-0">
                  <span class="text-[0.63rem] font-medium truncate">
                    {t(`chat.tools.${tool.key}`)}
                  </span>
                  <span class="text-[0.5rem] text-muted-foreground/50 truncate leading-tight">
                    {t(`chat.tools.${tool.key}Desc`)}
                  </span>
                </div>
                <!-- Toggle indicator -->
                <div class="ml-auto shrink-0 w-3 h-3 rounded-full border-2 flex items-center justify-center
                            {toolConfig[tool.key]
                              ? 'border-primary bg-primary'
                              : 'border-muted-foreground/30 bg-transparent'}">
                  {#if toolConfig[tool.key]}
                    <svg viewBox="0 0 10 10" fill="none" stroke="white" stroke-width="2"
                         stroke-linecap="round" stroke-linejoin="round" class="w-2 h-2">
                      <polyline points="2 5 4 7 8 3"/>
                    </svg>
                  {/if}
                </div>
              </button>
            {/each}
          </div>
        {/if}
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

              <!-- Tool-use indicators -->
              {#if msg.toolUses?.length}
                <div class="flex flex-wrap gap-1.5 px-1">
                  {#each msg.toolUses as tu}
                    {@const icons: Record<string, string> = { date: "🕐", location: "📍", web_search: "🔍", web_fetch: "🌐" }}
                    {@const icon = icons[tu.tool] ?? "🔧"}
                    <div class="flex items-center gap-1 px-2 py-0.5 rounded-md text-[0.6rem] font-medium
                                border transition-all
                                {tu.status === 'calling'
                                  ? 'border-primary/30 bg-primary/8 text-primary animate-pulse'
                                  : tu.status === 'done'
                                    ? 'border-emerald-500/30 bg-emerald-500/8 text-emerald-600 dark:text-emerald-400'
                                    : 'border-red-500/30 bg-red-500/8 text-red-600 dark:text-red-400'}">
                      <span>{icon}</span>
                      <span>{t(`chat.tools.${tu.tool}`)}</span>
                      {#if tu.status === "calling"}
                        <span class="flex gap-0.5 ml-0.5">
                          {#each [0,1,2] as i}
                            <span class="w-0.5 h-0.5 rounded-full bg-current animate-bounce"
                                  style="animation-delay:{i*0.1}s"></span>
                          {/each}
                        </span>
                      {:else if tu.status === "done"}
                        <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
                             stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5">
                          <polyline points="2 6 5 9 10 3"/>
                        </svg>
                      {:else}
                        <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
                             stroke-linecap="round" class="w-2.5 h-2.5">
                          <line x1="3" y1="3" x2="9" y2="9"/><line x1="9" y1="3" x2="3" y2="9"/>
                        </svg>
                      {/if}
                    </div>
                  {/each}
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
  <footer class="relative shrink-0 border-t border-border dark:border-white/[0.06]
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

    <!-- Prompt-library floating panel (anchored to this footer) -->
    <PromptLibrary
      bind:this={promptLibRef}
      onSelect={selectPrompt}
    />

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

      <!-- Prompt library button -->
      <button
        onclick={() => promptLibRef?.toggle()}
        disabled={status !== "running" || generating}
        title={t("chat.prompts.btn")}
        aria-label={t("chat.prompts.btn")}
        class="shrink-0 p-1 rounded-md transition-colors cursor-pointer self-center
               disabled:opacity-30 disabled:cursor-not-allowed
               {promptLibRef?.isOpen()
                 ? 'text-violet-600 dark:text-violet-400 bg-violet-500/10'
                 : 'text-muted-foreground/50 hover:text-violet-600 dark:hover:text-violet-400 hover:bg-violet-500/10'}">
        <!-- Sparkle / wand icon -->
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
             stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
          <path d="M12 2l1.5 4.5L18 8l-4.5 1.5L12 14l-1.5-4.5L6 8l4.5-1.5Z"/>
          <path d="M19 14l.75 2.25L22 17l-2.25.75L19 20l-.75-2.25L16 17l2.25-.75Z"/>
          <path d="M5 17l.5 1.5L7 19l-1.5.5L5 21l-.5-1.5L3 19l1.5-.5Z"/>
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

  </div><!-- end main chat column -->
</div><!-- end root flex container -->
