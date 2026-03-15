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
  import { chatTitlebarState }        from "$lib/chat-titlebar.svelte";
  import { fmtMs }                    from "$lib/format";

  // ── Types ──────────────────────────────────────────────────────────────────

  type Role = "user" | "assistant" | "system";
  type ServerStatus = "stopped" | "loading" | "running";

  interface ToolUseEvent {
    tool: string;
    status: string;           // "calling" | "done" | "error" | "approval_required"
    detail?: string;
    toolCallId?: string;
    args?: any;               // structured arguments from tool_execution_start
    result?: any;             // structured result from tool_execution_end
    expanded?: boolean;       // UI toggle
  }

  interface Message {
    id:           number;
    role:         Role;
    content:      string;
    /** Assistant text emitted before a <think> block, shown as its own bubble */
    leadIn?:      string;
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
  function cleanAssistantLeadIn(raw: string): string {
    return raw
      .replace(/```[a-z]*\s*/gi, "")
      .split("\n")
      .filter(line => !/^\s*(json|copy)\s*$/i.test(line))
      .join("\n")
      .trim();
  }

  /**
   * Strip tool-call JSON code fences that leaked into rawAcc.
   *
   * The streaming sanitizer on the Rust side holds back tool-call JSON, but
   * it can only recognise a fence as a tool call once it has seen BOTH a
   * "tool"/"name" key AND a "parameters"/"arguments" key.  Tokens emitted
   * before that threshold are already in rawAcc and need to be cleaned here.
   *
   * Two patterns are handled:
   *   1. Complete fenced block  (```json … ```)  whose body is a tool-call object.
   *   2. Incomplete fenced block (no closing ```) that either precedes a <think>
   *      tag (final-state leak) or sits at the very end of the string (mid-stream).
   */
  function stripToolCallFences(raw: string): string {
    // Known built-in tool names — must stay in sync with KNOWN_TOOL_NAMES in tools.rs
    const KNOWN_TOOLS = new Set(["date", "location", "web_search", "web_fetch", "bash", "read_file", "write_file", "edit_file", "search_output"]);

    function isToolCallObject(v: Record<string, unknown>): boolean {
      // Standard single-call: has name/tool + parameters/arguments
      if (
        ("name" in v || "tool" in v || "tool_calls" in v) &&
        ("parameters" in v || "arguments" in v || "tool_calls" in v)
      ) return true;
      // Dict-style multi-tool: {"date": {}, "location": {}}
      const keys = Object.keys(v);
      return keys.length > 0 &&
        keys.some(k => KNOWN_TOOLS.has(k)) &&
        Object.values(v).every(val => typeof val === "object" && val !== null);
    }

    function looksLikeToolCallJsonPrefix(s: string): boolean {
      const trimmed = s.trimStart();
      // Accept both object `{` and array `[` prefixes (Qwen3.5 emits arrays).
      if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return false;

      const probe = trimmed.slice(0, 320).toLowerCase();
      const isDictStyle = [...KNOWN_TOOLS].some(name =>
        probe.includes(`"${name}":`) || probe.includes(`"${name}": `)
      );
      if (isDictStyle) return true;

      const mentionsToolName =
        probe.includes('"name"') ||
        probe.includes('"tool"') ||
        probe.includes('"tool_calls"') ||
        probe.includes('"function"');
      const mentionsArgs =
        probe.includes('"parameter') ||
        probe.includes('"argument') ||
        probe.includes('<think>');

      return mentionsToolName && mentionsArgs;
    }

    function isToolCallArray(v: unknown): boolean {
      if (!Array.isArray(v)) return false;
      return v.some(item =>
        typeof item === "object" && item !== null && !Array.isArray(item) &&
        isToolCallObject(item as Record<string, unknown>)
      );
    }

    // 1. Complete fenced blocks
    let s = raw.replace(/```(?:json)?\n([\s\S]*?)\n?```/g, (match, body: string) => {
      const trimmedBody = body.trim();
      try {
        const v = JSON.parse(trimmedBody);
        if (typeof v === "object" && v !== null && !Array.isArray(v) && isToolCallObject(v))
          return "";
        if (isToolCallArray(v))
          return "";
      } catch { /* not JSON — keep */ }
      if (looksLikeToolCallJsonPrefix(trimmedBody)) return "";
      return match;
    });

    // 2a. Incomplete fence immediately before a <think> tag.
    s = s.replace(/```(?:json)?\n([\s\S]*?)(?=\n*<think>)/g, (match, body: string) =>
      looksLikeToolCallJsonPrefix(body) ? "" : match
    );

    // 2b. Incomplete fence at end of string (still streaming).
    s = s.replace(/```(?:json)?\n([\s\S]*)$/g, (match, body: string) =>
      looksLikeToolCallJsonPrefix(body) ? "" : match
    );

    // 3. Bare inline tool-call JSON (not fenced) — strip any { or [ balanced/partial
    //    block that looks like a tool call, possibly with trailing garbage.
    s = s.replace(/(?:^|\n)\s*[\[{][\s\S]*$/gm, (match) => {
      if (looksLikeToolCallJsonPrefix(match.trim())) return "";
      return match;
    });

    // 4. Complete [TOOL_CALL]…[/TOOL_CALL] blocks
    s = s.replace(/\[TOOL_CALL\][\s\S]*?\[\/TOOL_CALL\]/g, "");

    // 5. Incomplete [TOOL_CALL] at end of string (mid-stream): [TOOL_CA, [TOOL_CALL]{...
    s = s.replace(/\[TOOL_C[\s\S]*$/g, "");

    return s;
  }

  /**
   * Clean lead-in text for display.  When tool calls are active, aggressively
   * strip ALL incomplete code fences and any JSON-like fragments — these are
   * almost always tool-call artifacts the model emitted mid-stream.
   */
  function cleanLeadInForDisplay(raw: string, hasToolUses: boolean): string {
    if (!raw.trim()) return "";

    let s = stripToolCallFences(raw);

    if (hasToolUses) {
      // Strip any remaining incomplete fenced code block (opening ``` with no closing ```)
      s = s.replace(/```[a-z]*\n[\s\S]*$/gi, "");
      // Strip bare JSON-like fragments at end of string
      s = s.replace(/\n\s*[\[{][\s\S]*$/g, "");
    }

    return s.trim();
  }

  // ── Danger detection ────────────────────────────────────────────────────

  /** Bash patterns that indicate a potentially dangerous command (mirrored from Rust). */
  const DANGEROUS_BASH_PATTERNS = [
    "rm ", "rm\t", "rmdir", "shred",
    "mkfs", "dd if=", "dd of=",
    "sudo ", "su -", "su\t",
    "> /dev/", "chmod", "chown",
    "kill ", "killall", "pkill",
    "shutdown", "reboot", "halt", "poweroff",
    "systemctl stop", "systemctl disable",
    ":(){ :|:& };:",
    "/etc/", "/boot/", "/usr/", "/var/", "/sys/", "/proc/",
  ];

  /** Sensitive path prefixes (mirrored from Rust). */
  const SENSITIVE_PATH_PREFIXES = [
    "/etc/", "/boot/", "/usr/", "/var/", "/sys/", "/proc/",
    "/bin/", "/sbin/", "/lib/", "/opt/",
  ];

  /**
   * Check if a tool call looks dangerous based on its name and arguments.
   * Returns a danger reason string, or null if safe.
   */
  function detectToolDanger(tu: ToolUseEvent): string | null {
    if (tu.tool === "bash" && tu.args?.command) {
      const cmd = tu.args.command.toLowerCase();
      for (const pat of DANGEROUS_BASH_PATTERNS) {
        if (cmd.includes(pat)) {
          return `chat.tools.dangerBash`;
        }
      }
    }
    if (["write_file", "edit_file", "read_file"].includes(tu.tool) && tu.args?.path) {
      const path = tu.args.path;
      for (const prefix of SENSITIVE_PATH_PREFIXES) {
        if (path.startsWith(prefix)) {
          return `chat.tools.dangerPath`;
        }
      }
    }
    return null;
  }

  /** Cancel a specific tool call by its tool_call_id. */
  async function cancelToolCall(msgId: number, tuIdx: number, toolCallId: string | undefined) {
    if (!toolCallId) return;
    try {
      await invoke("cancel_tool_call", { toolCallId });
    } catch { /* server not running */ }

    // Immediately update the UI to show cancelled status
    messages = messages.map(m => {
      if (m.id !== msgId) return m;
      const uses = [...(m.toolUses ?? [])];
      if (uses[tuIdx] && uses[tuIdx].status === "calling") {
        uses[tuIdx] = { ...uses[tuIdx], status: "cancelled" };
      }
      return { ...m, toolUses: uses };
    });
  }

  /**
   * Split raw assistant output (which may contain multiple <think>…</think>
   * blocks from multi-turn tool-calling) into three display zones.
   *
   * The model typically emits:
   *   <think>pre-tool reasoning</think>
   *   I'll use the X tool…           ← lead-in / inter-think text
   *   ```json{"tool":"X",…}```       ← stripped by stripToolCallFences
   *   <think>post-tool reasoning</think>
   *   Final answer                   ← content
   *
   * All <think> blocks are merged into `thinking`.
   * Text segments between/after think blocks are arranged so that
   * - `leadIn`  = everything except the last non-empty segment
   * - `content` = the last non-empty segment (the actual final answer)
   */
  function parseAssistantOutput(raw: string): { leadIn: string; thinking: string; content: string } {
    const s = stripToolCallFences(raw);

    if (!s.includes("<think>")) return { leadIn: "", thinking: "", content: s };

    // Collect all complete think blocks; replace each with a NUL sentinel
    const thinkingParts: string[] = [];
    let withoutThink = s.replace(/<think>([\s\S]*?)<\/think>/g, (_: string, inner: string) => {
      thinkingParts.push(inner.trim());
      return "\x00";
    });

    // Handle an unclosed <think> at the end (still streaming in thinking phase)
    const openIdx = withoutThink.indexOf("<think>");
    if (openIdx !== -1) {
      thinkingParts.push(withoutThink.slice(openIdx + 7).trim());
      withoutThink = withoutThink.slice(0, openIdx);
    }

    const thinking = thinkingParts.join("\n\n");

    // Non-thinking segments (split by sentinel, trim, drop empty)
    const parts = withoutThink.split("\x00").map((p: string) => p.trim()).filter(Boolean);

    if (parts.length === 0) return { leadIn: "", thinking, content: "" };

    // Last segment = final answer; earlier segments = inter-think lead-in text
    const content = parts[parts.length - 1];
    const leadIn  = parts.slice(0, -1).map(cleanAssistantLeadIn).filter(Boolean).join("\n\n");

    return { leadIn, thinking, content };
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
  interface ChatChunkToolExecStart { type: "tool_execution_start"; tool_call_id: string; tool_name: string; args: any; }
  interface ChatChunkToolExecEnd   { type: "tool_execution_end";   tool_call_id: string; tool_name: string; result: any; is_error: boolean; }
  interface ChatChunkToolCancelled { type: "tool_cancelled"; tool_call_id: string; tool_name: string; }
  interface ChatChunkDone  {
    type:              "done";
    finish_reason:     string;
    prompt_tokens:     number;
    completion_tokens: number;
    n_ctx:             number;
  }
  interface ChatChunkError { type: "error"; message: string; }
  type ChatChunk = ChatChunkDelta | ChatChunkToolUse | ChatChunkToolExecStart | ChatChunkToolExecEnd | ChatChunkToolCancelled | ChatChunkDone | ChatChunkError;

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

  interface StoredToolCallRow {
    id:           number;
    message_id:   number;
    tool:         string;
    status:       string;
    detail?:      string | null;
    tool_call_id?: string | null;
    args?:        any;
    result?:      any;
    created_at:   number;
  }

  interface StoredMessage {
    id:         number;
    session_id: number;
    role:       string;
    content:    string;
    thinking:   string | null;
    created_at: number;
    tool_calls: StoredToolCallRow[];
  }

  interface ChatSessionResponse {
    session_id: number;
    messages:   StoredMessage[];
  }

  function storedToMessage(sm: StoredMessage): Message {
    const msg: Message = {
      id:        ++msgId,
      role:      sm.role as Role,
      content:   sm.content,
      thinking:  sm.thinking ?? undefined,
      thinkOpen: false,
      pending:   false,
    };
    // Restore persisted tool calls.
    if (sm.tool_calls && sm.tool_calls.length > 0) {
      msg.toolUses = sm.tool_calls.map((tc: StoredToolCallRow): ToolUseEvent => ({
        tool:       tc.tool,
        status:     tc.status,
        detail:     tc.detail ?? undefined,
        toolCallId: tc.tool_call_id ?? undefined,
        args:       tc.args ?? undefined,
        result:     tc.result ?? undefined,
        expanded:   false,
      }));
    }
    return msg;
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

  type ToolExecutionMode = "sequential" | "parallel";
  interface ToolConfig {
    date: boolean; location: boolean; web_search: boolean; web_fetch: boolean;
    bash: boolean; read_file: boolean; write_file: boolean; edit_file: boolean;
    execution_mode: ToolExecutionMode;
    max_rounds: number;
    max_calls_per_round: number;
  }

  let status         = $state<ServerStatus>("stopped");
  let modelName      = $state("");
  let nCtx           = $state(0);      // context window size from server
  let supportsVision = $state(false);
  let supportsTools  = $state(false);
  let toolConfig     = $state<ToolConfig>({
    date: true, location: true, web_search: true, web_fetch: true,
    bash: false, read_file: false, write_file: false, edit_file: false,
    execution_mode: "parallel", max_rounds: 3, max_calls_per_round: 4,
  });
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

  /** Keep the titlebar model name + status in sync. */
  $effect(() => {
    chatTitlebarState.modelName = modelName;
    chatTitlebarState.status = status;
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
    [toolConfig.date, toolConfig.location, toolConfig.web_search, toolConfig.web_fetch,
     toolConfig.bash, toolConfig.read_file, toolConfig.write_file, toolConfig.edit_file]
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
  /** Live estimated context usage — updated in real-time as messages change.
   *  Snaps to real values when a `done` chunk arrives from llama.cpp.
   *  `null` means "use estimate", otherwise holds real usage from the server. */
  let realPromptTokens     = $state<number | null>(null);
  let streamCompletionToks = $state(0);  // completion tokens counted during streaming
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
  let showTools      = $state(false);
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

  // ── Live context usage estimation ────────────────────────────────────────
  // Rough estimate: ~4 chars per token, ~10 tokens overhead per message for
  // role tags / separators / chat template markup.
  function estimateTokens(text: string): number {
    return Math.ceil(text.length / 4) + 1;
  }

  /** Estimated prompt tokens based on current messages + system prompt. */
  const estimatedPromptTokens = $derived.by(() => {
    let total = estimateTokens(systemPrompt) + 10; // system message
    // Tool prompt overhead (compact ~30 tok, full ~500 tok)
    if (supportsTools) {
      const enabledCount = [toolConfig.date, toolConfig.location, toolConfig.web_search,
        toolConfig.web_fetch, toolConfig.bash, toolConfig.read_file,
        toolConfig.write_file, toolConfig.edit_file].filter(Boolean).length;
      if (enabledCount > 0) total += nCtx <= 4096 ? 30 : 500;
    }
    for (const m of messages) {
      total += estimateTokens(m.content) + 10;
      if (m.thinking) total += estimateTokens(m.thinking) + 5;
      if (m.leadIn) total += estimateTokens(m.leadIn);
    }
    // Include the current input being typed
    if (input.trim()) total += estimateTokens(input) + 10;
    return total;
  });

  /** Best-available context usage: real values when available, estimate otherwise. */
  const liveUsedTokens = $derived.by(() => {
    if (realPromptTokens !== null) {
      return realPromptTokens + streamCompletionToks;
    }
    return estimatedPromptTokens + streamCompletionToks;
  });

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
        sessionId, role: "user", content: text, thinking: null,
      }).catch(() => {});
    }

    const assistantMsg: Message = { id: ++msgId, role: "assistant", content: "", pending: true };
    messages = [...messages, assistantMsg];
    await scrollBottom(true);   // force — user just sent, must see response start

    generating = true;
    realPromptTokens     = null;  // reset — use estimates until done chunk arrives
    streamCompletionToks = 0;
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

    let rawAcc = ""; // raw text for the current inference round (reset between rounds)
    let usage: UsageInfo | undefined;

    // Multi-round tool-calling state.
    // Each time the LLM finishes a round and the backend dispatches tool calls, we
    // "freeze" the accumulated thinking and lead-in text, reset rawAcc, and begin
    // accumulating the next round's output fresh.  This prevents cross-round text
    // from being concatenated into one blob and ensures that whatever the LLM said
    // before a tool call (e.g. "I'll use the date tool") is cleanly shown as leadIn
    // rather than flickering in the response bubble while the tool is running.
    let hadToolUse     = false;
    let frozenLeadIn   = "";   // lead-in text collected from all completed rounds
    let frozenThinking = "";   // thinking text collected from all completed rounds

    // Merge the per-round parsed state with accumulated frozen state from prior rounds.
    function mergeWithFrozen(parsed: { leadIn: string; thinking: string; content: string }) {
      return {
        leadIn:   [frozenLeadIn,   parsed.leadIn  ].filter(s => s.trim()).join("\n\n"),
        thinking: [frozenThinking, parsed.thinking].filter(s => s.trim()).join("\n\n"),
        content:  parsed.content,
      };
    }

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
        // First delta after tool calls: rawAcc was already reset in the tool_use
        // handler; just clear the hadToolUse flag so we know we're back in streaming.
        if (hadToolUse) hadToolUse = false;
        rawAcc += chunk.content;
        // Approximate completion token count from streamed text (~4 chars/token)
        streamCompletionToks = Math.ceil(rawAcc.length / 4);
        const { leadIn, thinking, content } = mergeWithFrozen(parseAssistantOutput(rawAcc));
        messages = messages.map(m =>
          m.id === assistantMsg.id
            ? { ...m, leadIn, content, thinking, thinkOpen: m.thinkOpen ?? false }
            : m
        );
        await scrollBottom();

      } else if (chunk.type === "tool_use") {
        const evt: ToolUseEvent = { tool: chunk.tool, status: chunk.status, detail: chunk.detail };

        if (evt.status === "calling" && !hadToolUse) {
          // First tool call of a new round.  Freeze whatever the LLM has streamed so
          // far (thinking + any content like "I'll use the X tool") into the frozen
          // accumulators, then reset rawAcc so the next LLM round starts clean.
          const prev = parseAssistantOutput(rawAcc);
          frozenLeadIn   = [frozenLeadIn,   prev.leadIn, prev.content].filter(s => s.trim()).join("\n\n");
          frozenThinking = [frozenThinking,  prev.thinking            ].filter(s => s.trim()).join("\n\n");
          rawAcc     = "";
          hadToolUse = true;
          // Immediately reflect the updated leadIn/thinking and empty content so the
          // response bubble disappears while tools are running.
          messages = messages.map(m =>
            m.id === assistantMsg.id
              ? { ...m, leadIn: frozenLeadIn, thinking: frozenThinking, content: "" }
              : m
          );
        }

        messages = messages.map(m => {
          if (m.id !== assistantMsg.id) return m;
          const existing = m.toolUses ?? [];
          // Update an existing "calling" entry for the same tool, or add a new row.
          const idx = existing.findIndex(e => e.tool === evt.tool && e.status === "calling");
          if (evt.status !== "calling" && idx >= 0) {
            const updated = [...existing];
            // Merge — preserve args/result/toolCallId from prior enrichment events
            updated[idx] = { ...updated[idx], ...evt };
            return { ...m, toolUses: updated };
          }
          return { ...m, toolUses: [...existing, evt] };
        });
        await scrollBottom();

      } else if (chunk.type === "tool_execution_start") {
        // Enrich the existing "calling" pill with structured args.
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
        // Enrich the tool pill with the result.
        messages = messages.map(m => {
          if (m.id !== assistantMsg.id) return m;
          const existing = m.toolUses ?? [];
          // Match by toolCallId first, then by tool name + any active status
          const idx = existing.findIndex(e =>
            e.tool === chunk.tool_name && (
              e.toolCallId === chunk.tool_call_id ||
              e.status === "calling" ||
              // Also match entries that were already updated by tool_use(error/done)
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
        // Mark a tool call as cancelled in the UI.
        messages = messages.map(m => {
          if (m.id !== assistantMsg.id) return m;
          const existing = m.toolUses ?? [];
          const idx = existing.findIndex(e =>
            e.tool === chunk.tool_name && (
              e.toolCallId === chunk.tool_call_id ||
              e.status === "calling" ||
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
          prompt_tokens:     chunk.prompt_tokens,
          completion_tokens: chunk.completion_tokens,
          total_tokens:      chunk.prompt_tokens + chunk.completion_tokens,
          n_ctx:             chunk.n_ctx,
        };
        // Snap to real values from llama.cpp
        realPromptTokens     = chunk.prompt_tokens;
        streamCompletionToks = chunk.completion_tokens;
        if (chunk.n_ctx > 0) nCtx = chunk.n_ctx;
        messages = messages.map(m =>
          m.id === assistantMsg.id
            ? { ...m, pending: false, leadIn, content, thinking, ttft, elapsed, usage }
            : m
        );
        await scrollBottom();

      } else if (chunk.type === "error") {
        // "aborted" → keep whatever was streamed so far as the final answer.
        // Any other message → show as an error bubble.
        const { leadIn, thinking, content } = mergeWithFrozen(parseAssistantOutput(rawAcc));
        messages = messages.map(m =>
          m.id === assistantMsg.id
            ? {
                ...m, pending: false,
                leadIn,
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
          ? { ...m, pending: false, ...mergeWithFrozen(parseAssistantOutput(rawAcc)) }
          : m
      );

      const finalAssistant = messages.find(m => m.id === assistantMsg.id);

      generating = false;
      await scrollBottom();
      await tick();
      inputEl?.focus();

      // Persist the completed assistant message (fire-and-forget).
      // Combine leadIn + content so the full response is preserved on reload.
      if (sessionId > 0 && finalAssistant && !finalAssistant.pending) {
        const parts: string[] = [];
        if (finalAssistant.leadIn?.trim())  parts.push(finalAssistant.leadIn.trim());
        if (finalAssistant.content?.trim()) parts.push(finalAssistant.content.trim());
        const fullContent = parts.join("\n\n");
        if (fullContent || (finalAssistant.toolUses?.length ?? 0) > 0) {
          invoke<number>("save_chat_message", {
            sessionId,
            role:       "assistant",
            content:    fullContent || "",
            thinking:   finalAssistant.thinking ?? null,
          }).then((messageId: number) => {
            // Persist tool calls associated with this assistant message.
            if (messageId > 0 && finalAssistant.toolUses?.length) {
              const toolCalls = finalAssistant.toolUses.map(tu => ({
                id:           0,
                message_id:   messageId,
                tool:         tu.tool,
                status:       tu.status,
                detail:       tu.detail ?? null,
                tool_call_id: tu.toolCallId ?? null,
                args:         tu.args ?? null,
                result:       tu.result ?? null,
                created_at:   0,
              }));
              invoke("save_chat_tool_calls", { messageId, toolCalls }).catch(() => {});
            }
          }).catch(() => {});
        }
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

  // ── Typing-label auto-labeller ───────────────────────────────────────────
  // Every 5 seconds (EPOCH_S from the EXG model window size), if the user
  // *typed* (not pasted) any text in the chat input, we submit a label
  // containing only the typed words.  Context = current chat session summary.
  //
  // We use the `beforeinput` event's `inputType` to distinguish physical
  // keyboard strokes (`insertText`, `deleteContentBackward`, etc.) from
  // paste/drop (`insertFromPaste`, `insertFromDrop`).

  const TYPING_LABEL_INTERVAL_MS = 5_000;      // 5 s — matches EPOCH_S
  const WORD_BOUNDARY_TIMEOUT_MS = 1_500;      // max wait for word to finish
  let typedCharsInWindow   = $state("");        // accumulates typed chars
  let deletedCharsInWindow = $state("");        // accumulates deleted chars
  let typingLabelTimer: ReturnType<typeof setInterval> | undefined;
  let windowStartUtc       = 0;                 // UTC second when window opened
  let pendingFlush         = false;             // true = 5 s fired, waiting for word boundary
  let wordBoundaryTimeout: ReturnType<typeof setTimeout> | undefined;

  /** Is `ch` a word-boundary character (space, punctuation, newline)? */
  function isWordBoundary(ch: string): boolean {
    return /[\s\p{P}]/u.test(ch);
  }

  /** Extract the text about to be removed by a delete-type inputEvent. */
  function captureDeletedText(e: InputEvent): string {
    if (!inputEl) return "";
    const val   = inputEl.value;
    const start = inputEl.selectionStart ?? 0;
    const end   = inputEl.selectionEnd   ?? 0;

    // If there's a selection, the whole selection is deleted.
    if (start !== end) return val.slice(start, end);

    switch (e.inputType) {
      case "deleteContentBackward":
        return start > 0 ? val.slice(start - 1, start) : "";
      case "deleteContentForward":
        return start < val.length ? val.slice(start, start + 1) : "";
      case "deleteWordBackward": {
        // Walk backwards past whitespace, then past word chars.
        let i = start;
        while (i > 0 && /\s/.test(val[i - 1])) i--;
        while (i > 0 && !/\s/.test(val[i - 1])) i--;
        return val.slice(i, start);
      }
      case "deleteWordForward": {
        let i = start;
        while (i < val.length && !/\s/.test(val[i])) i++;
        while (i < val.length && /\s/.test(val[i])) i++;
        return val.slice(start, i);
      }
      case "deleteByCut":
        return start !== end ? val.slice(start, end) : "";
      default:
        return "";
    }
  }

  /** Called from the textarea `beforeinput` handler. */
  function onChatBeforeInput(e: InputEvent) {
    // ── Deletions — capture what's about to be removed ────────────────────
    if (e.inputType.startsWith("delete")) {
      const removed = captureDeletedText(e);
      if (removed) deletedCharsInWindow += " " + removed;
      return;
    }

    // ── Insertions — only count keyboard-originated (not paste/drop) ─────
    if (e.inputType === "insertText" && e.data) {
      typedCharsInWindow += e.data;
      // If we're waiting for a word boundary to flush, check each character.
      if (pendingFlush && isWordBoundary(e.data)) {
        commitTypingLabel();
      }
    } else if (
      e.inputType === "insertLineBreak" ||
      e.inputType === "insertParagraph"
    ) {
      typedCharsInWindow += " ";
      if (pendingFlush) {
        commitTypingLabel();
      }
    }
  }

  /**
   * Build a compact context string from the current chat session for the
   * auto-label's context field.
   */
  function buildSessionContext(): string {
    const parts: string[] = [];
    parts.push(`Chat session #${sessionId}`);
    if (modelName) parts.push(`Model: ${modelName}`);
    // Include a compact digest of the last few messages (up to ~2 KB).
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

  /**
   * Actually submit the label for the current window.
   * Called either at a word boundary after the 5 s timer, or by the safety
   * timeout if the user pauses mid-word.
   *
   * Words that were typed and then deleted within the same window are
   * wrapped in `<del>…</del>` so downstream consumers can see edits.
   */
  async function commitTypingLabel() {
    pendingFlush = false;
    if (wordBoundaryTimeout) { clearTimeout(wordBoundaryTimeout); wordBoundaryTimeout = undefined; }

    const rawTyped   = typedCharsInWindow.trim();
    const rawDeleted = deletedCharsInWindow.trim();
    typedCharsInWindow   = "";
    deletedCharsInWindow = "";
    const labelStartUtc = windowStartUtc;
    windowStartUtc = Math.floor(Date.now() / 1000);   // new window starts now
    if (!rawTyped) return;

    const isAlphaNum = (w: string) =>
      /[a-zA-Z0-9\u00C0-\u024F\u0400-\u04FF\u0590-\u05FF\u0600-\u06FF]/.test(w);

    // Extract recognisable words (strip stray punctuation-only tokens).
    const typedWords = rawTyped.split(/\s+/).filter(isAlphaNum);
    if (typedWords.length === 0) return;

    // Build a deletion multiset: count how many times each word was deleted.
    const deletedCounts = new Map<string, number>();
    if (rawDeleted) {
      for (const w of rawDeleted.split(/\s+/).filter(isAlphaNum)) {
        const lc = w.toLowerCase();
        deletedCounts.set(lc, (deletedCounts.get(lc) ?? 0) + 1);
      }
    }

    // Render each typed word, wrapping in <del> if it was also deleted.
    const rendered = typedWords.map(w => {
      const lc   = w.toLowerCase();
      const dCnt = deletedCounts.get(lc) ?? 0;
      if (dCnt > 0) {
        deletedCounts.set(lc, dCnt - 1);
        return `<del>${w}</del>`;
      }
      return w;
    });

    const labelText = rendered.join(" ");
    const context   = buildSessionContext();

    try {
      await invoke("submit_label", {
        labelStartUtc,
        text:    labelText,
        context,
      });
    } catch {
      // Label store might be unavailable — silently ignore.
    }
  }

  /**
   * Called by the 5 s interval.  If the buffer ends mid-word, defer the
   * flush until the next word boundary (space/punctuation/Enter) or a
   * safety timeout.
   */
  function onTypingWindowTick() {
    if (!typedCharsInWindow) {
      // Nothing typed — just reset the window start.
      windowStartUtc = Math.floor(Date.now() / 1000);
      return;
    }

    const lastChar = typedCharsInWindow.at(-1) ?? "";
    if (isWordBoundary(lastChar) || !lastChar) {
      // Buffer ends at a clean word boundary — flush immediately.
      commitTypingLabel();
    } else {
      // Mid-word — wait for the next boundary character.
      pendingFlush = true;
      // Safety: if no further typing arrives within 1.5 s, flush anyway
      // (the user may have stopped typing mid-word or switched focus).
      wordBoundaryTimeout = setTimeout(() => commitTypingLabel(), WORD_BOUNDARY_TIMEOUT_MS);
    }
  }

  function startTypingLabelTimer() {
    stopTypingLabelTimer();
    typedCharsInWindow   = "";
    deletedCharsInWindow = "";
    pendingFlush = false;
    windowStartUtc = Math.floor(Date.now() / 1000);
    typingLabelTimer = setInterval(onTypingWindowTick, TYPING_LABEL_INTERVAL_MS);
  }

  function stopTypingLabelTimer() {
    if (typingLabelTimer) { clearInterval(typingLabelTimer); typingLabelTimer = undefined; }
    if (wordBoundaryTimeout) { clearTimeout(wordBoundaryTimeout); wordBoundaryTimeout = undefined; }
    pendingFlush = false;
    // Flush any remaining typed text before stopping.
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
      status         = s.status;
      modelName      = s.model_name;
      nCtx           = s.n_ctx ?? 0;
      supportsVision = s.supports_vision ?? false;
      supportsTools  = s.supports_tools ?? false;
    } catch {}

    // Load tool config from persisted settings
    try {
      const cfg = await invoke<any>("get_llm_config");
      if (cfg?.tools) {
        toolConfig = {
          date:                cfg.tools.date                ?? true,
          location:            cfg.tools.location            ?? true,
          web_search:          cfg.tools.web_search          ?? true,
          web_fetch:           cfg.tools.web_fetch           ?? true,
          bash:                cfg.tools.bash                ?? false,
          read_file:           cfg.tools.read_file           ?? false,
          write_file:          cfg.tools.write_file          ?? false,
          edit_file:           cfg.tools.edit_file           ?? false,
          execution_mode:      cfg.tools.execution_mode      ?? "parallel",
          max_rounds:          cfg.tools.max_rounds          ?? 3,
          max_calls_per_round: cfg.tools.max_calls_per_round ?? 4,
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
        if ((ev.payload as any).n_ctx !== undefined) {
          nCtx = (ev.payload as any).n_ctx;
        }
        if (status === "running") clearInterval(pollTimer!);
        // When the server stops, capabilities reset.
        if (status === "stopped") { supportsVision = false; supportsTools = false; nCtx = 0; }
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
        const s = await invoke<{ status: ServerStatus; model_name: string; n_ctx: number; supports_vision: boolean; supports_tools: boolean }>("get_llm_server_status");
        status         = s.status;
        modelName      = s.model_name;
        nCtx           = s.n_ctx ?? 0;
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

    // Start the typing-label auto-labeller (5 s windows).
    startTypingLabelTimer();

    await tick();
    inputEl?.focus();
  });

  onDestroy(() => {
    unlistenStatus?.();
    unlistenBands?.();
    clearInterval(pollTimer);
    stopTypingLabelTimer();
    if (generating) invoke("abort_llm_stream").catch(() => {});
  });

</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Root container (full window height, dark/light theme-aware)                -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<div class="flex h-full min-h-0 bg-background text-foreground overflow-hidden rounded-b-[10px]">

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
  <header class="relative flex flex-nowrap items-center gap-2 px-3 py-2 border-b border-border dark:border-white/[0.06]
                  bg-white dark:bg-[#0f0f18] shrink-0 overflow-hidden min-h-0"
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

    <!-- Spacer to push right-side controls to the end -->
    <div class="flex-1 min-w-0" data-tauri-drag-region></div>

    <!-- Tools badge -->
    {#if supportsTools}
      <button
        onclick={() => { showTools = !showTools; if (showTools) showSettings = false; }}
        title="{enabledToolCount} tool{enabledToolCount !== 1 ? 's' : ''} enabled"
        class="flex items-center gap-1 px-1.5 py-0.5 rounded-md transition-colors cursor-pointer
               shrink-0 text-[0.6rem] font-semibold
               {showTools
                 ? 'bg-primary/20 text-primary ring-1 ring-primary/30'
                 : enabledToolCount > 0
                   ? 'bg-primary/10 text-primary hover:bg-primary/20'
                   : 'bg-muted text-muted-foreground/50 hover:bg-muted/80'}">
        <!-- Wrench icon -->
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6"
             stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
          <path d="M14.7 6.3a1 1 0 0 0 0-1.4l-.6-.6a1 1 0 0 0-1.4 0L6.3 10.7a1 1 0 0 0 0 1.4l.6.6a1 1 0 0 0 1.4 0z"/>
          <path d="M16 2l2 2-1.5 1.5L14.5 3.5z"/>
          <path d="M2 18l4-1 9.3-9.3-3-3L3 14z"/>
        </svg>
        <span>{t("chat.tools.badge")}</span>
        {#if enabledToolCount > 0}
          <span class="tabular-nums opacity-70">{enabledToolCount}</span>
        {/if}
      </button>
    {/if}

    <!-- Context usage circular indicator (next to tools) -->
    {#if nCtx > 0}
      {@const ctxPct = liveUsedTokens > 0 ? Math.min(Math.round((liveUsedTokens / nCtx) * 100), 100) : 0}
      {@const ctxIsEstimate = realPromptTokens === null && liveUsedTokens > 0}
      {@const ringStroke = ctxPct >= 90 ? 'stroke-red-500' : ctxPct >= 70 ? 'stroke-amber-500' : 'stroke-primary'}
      {@const circumference = 2 * Math.PI * 7}
      {@const dashOffset = circumference - (circumference * ctxPct / 100)}
      <div class="flex items-center gap-1 shrink-0 select-none"
           title="{t('chat.ctxUsage')}: {ctxIsEstimate ? '~' : ''}{liveUsedTokens.toLocaleString()}/{nCtx.toLocaleString()} ({ctxPct}%)">
        <svg viewBox="0 0 18 18" class="w-4 h-4 -rotate-90">
          <!-- Track -->
          <circle cx="9" cy="9" r="7" fill="none" stroke-width="2.2"
                  class="stroke-muted-foreground/15" />
          <!-- Progress arc -->
          <circle cx="9" cy="9" r="7" fill="none" stroke-width="2.2"
                  class="{ringStroke} transition-all duration-150"
                  stroke-linecap="round"
                  stroke-dasharray="{circumference}"
                  stroke-dashoffset="{dashOffset}" />
        </svg>
        <span class="text-[0.5rem] tabular-nums font-semibold
                      {ctxPct >= 90 ? 'text-red-500' : ctxPct >= 70 ? 'text-amber-500' : 'text-muted-foreground/60'}">
          {ctxPct}%
        </span>
      </div>
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
      onclick={() => { showSettings = !showSettings; if (showSettings) showTools = false; }}
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
    <div class="min-h-0 max-h-[50vh] overflow-y-auto border-b border-border dark:border-white/[0.06]
                bg-slate-50/60 dark:bg-[#111118] px-4 py-3 flex flex-col gap-3
                scrollbar-thin scrollbar-track-transparent scrollbar-thumb-border">

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

  <!-- ── Tools panel (slide-in) ──────────────────────────────────────────── -->
  {#if showTools}
    <div class="min-h-0 max-h-[50vh] overflow-y-auto border-b border-border dark:border-white/[0.06]
                bg-slate-50/60 dark:bg-[#111118] px-4 py-3 flex flex-col gap-3
                scrollbar-thin scrollbar-track-transparent scrollbar-thumb-border">

      <!-- Tools allow-list -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between gap-2">
          <span class="text-[0.58rem] font-semibold uppercase tracking-widest text-muted-foreground">
            {t("chat.tools.label")}
          </span>
          <span class="text-[0.55rem] tabular-nums text-muted-foreground/40 select-none">
            {enabledToolCount}/8
          </span>
        </div>
        <div class="grid grid-cols-2 gap-1.5">
          {#each [
            { key: "date"       as const, icon: "🕐" },
            { key: "location"   as const, icon: "📍" },
            { key: "web_search" as const, icon: "🔍" },
            { key: "web_fetch"  as const, icon: "🌐" },
            { key: "bash"       as const, icon: "💻" },
            { key: "read_file"  as const, icon: "📄" },
            { key: "write_file" as const, icon: "✏️" },
            { key: "edit_file"  as const, icon: "🔧" },
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

        <!-- Tool execution mode toggle -->
        <div class="mt-1.5">
          <div class="flex items-center justify-between gap-2 mb-1">
            <span class="text-[0.53rem] text-muted-foreground/60">{t("chat.tools.executionMode")}</span>
          </div>
          <div class="flex rounded-md overflow-hidden border border-border text-[0.6rem] font-medium">
            {#each [
              { key: "parallel"   as ToolExecutionMode, labelKey: "chat.tools.parallel" },
              { key: "sequential" as ToolExecutionMode, labelKey: "chat.tools.sequential" },
            ] as mode}
              <button
                onclick={() => updateToolConfig({ execution_mode: mode.key })}
                class="flex-1 py-1 transition-colors cursor-pointer
                       {toolConfig.execution_mode === mode.key
                         ? 'bg-primary text-primary-foreground'
                         : 'bg-background text-muted-foreground hover:bg-muted'}">
                {t(mode.labelKey)}
              </button>
            {/each}
          </div>
        </div>
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
            <!-- Avatar / spinner -->
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

              <!-- Thinking block (collapsible) — shown first: happens before lead-in/tools -->
              {#if msg.thinking || (msg.pending && msg.content === "" && !msg.thinking && !msg.toolUses?.length && !msg.leadIn?.trim())}
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
                                border-t border-violet-500/10 text-[0.68rem] break-words overflow-hidden">
                      <MarkdownRenderer content={msg.thinking} className="mdr-muted" />
                    </div>
                  {/if}
                </div>
              {/if}

              <!-- Lead-in bubble (text the model emitted before calling a tool) -->
              {#if cleanLeadInForDisplay(msg.leadIn ?? "", !!msg.toolUses?.length)}
                <div class="rounded-2xl rounded-tl-sm border border-border/70 bg-background/80
                            px-3 py-2 text-[0.72rem] leading-relaxed text-muted-foreground
                            whitespace-pre-wrap break-words">
                  {cleanLeadInForDisplay(msg.leadIn ?? "", !!msg.toolUses?.length)}
                </div>
              {/if}

              <!-- Tool-use expandable cards -->
              {#if msg.toolUses?.length}
                <div class="flex flex-col gap-1.5">
                  {#each msg.toolUses as tu, tuIdx}
                    {@const icons: Record<string, string> = { date: "🕐", location: "📍", web_search: "🔍", web_fetch: "🌐", bash: "💻", read_file: "📄", write_file: "✏️", edit_file: "🔧", search_output: "🔎" }}
                    {@const icon = icons[tu.tool] ?? "🔧"}
                    {@const bashCmd = tu.tool === "bash" ? (tu.args?.command || tu.result?.command || "") : ""}
                    {@const hasNonEmptyArgs = tu.args && Object.keys(tu.args).length > 0}
                    {@const hasDetails = !!(hasNonEmptyArgs || tu.result || tu.detail || bashCmd)}
                    {@const dangerKey = detectToolDanger(tu)}
                    {@const isDangerous = !!dangerKey}
                    {@const borderColor =
                        tu.status === 'cancelled' ? 'border-amber-500/30'
                      : tu.status === 'calling' && isDangerous ? 'border-red-500/40'
                      : tu.status === 'calling'   ? 'border-primary/25'
                      : tu.status === 'done'      ? 'border-emerald-500/25'
                      :                             'border-red-500/25'}
                    {@const bgColor =
                        tu.status === 'cancelled' ? 'bg-amber-500/5'
                      : tu.status === 'calling' && isDangerous ? 'bg-red-500/8'
                      : tu.status === 'calling'   ? 'bg-primary/5'
                      : tu.status === 'done'      ? 'bg-emerald-500/5'
                      :                             'bg-red-500/5'}

                    <div class="rounded-xl border {borderColor} {bgColor} overflow-hidden text-[0.68rem]">
                      <!-- Header row: clickable to expand -->
                      <div class="flex items-center">
                        <button
                          onclick={() => {
                            if (!hasDetails) return;
                            messages = messages.map(m => {
                              if (m.id !== msg.id) return m;
                              const uses = [...(m.toolUses ?? [])];
                              uses[tuIdx] = { ...uses[tuIdx], expanded: !uses[tuIdx].expanded };
                              return { ...m, toolUses: uses };
                            });
                          }}
                          class="flex-1 min-w-0 flex items-center gap-1.5 px-3 py-1.5 text-left
                                 transition-colors
                                 {hasDetails ? 'cursor-pointer hover:bg-black/5 dark:hover:bg-white/5' : 'cursor-default'}
                                 {tu.status === 'cancelled' ? 'text-amber-600 dark:text-amber-400'
                                : tu.status === 'calling' && isDangerous ? 'text-red-600 dark:text-red-400'
                                : tu.status === 'calling'   ? 'text-primary'
                                : tu.status === 'done'      ? 'text-emerald-700 dark:text-emerald-300'
                                :                             'text-red-700 dark:text-red-300'}">
                          <!-- Expand chevron -->
                          {#if hasDetails}
                            <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3 shrink-0 opacity-50
                                 transition-transform {tu.expanded ? 'rotate-90' : ''}">
                              <path d="M6 3l5 5-5 5V3z"/>
                            </svg>
                          {/if}
                          <span class="text-sm">{icon}</span>
                          <span class="font-medium">{t(`chat.tools.${tu.tool}`)}</span>

                          <!-- Danger badge (inline, visible even when collapsed) -->
                          {#if isDangerous && (tu.status === 'calling' || tu.status === 'cancelled')}
                            <span class="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-md
                                         text-[0.55rem] font-semibold shrink-0
                                         bg-red-500/15 text-red-600 dark:text-red-400 border border-red-500/20">
                              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"
                                   stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5 shrink-0">
                                <path d="M7.15 2.43L1.41 12a1 1 0 0 0 .86 1.5h11.46a1 1 0 0 0 .86-1.5L8.85 2.43a1 1 0 0 0-1.7 0z"/>
                                <line x1="8" y1="6" x2="8" y2="9"/><line x1="8" y1="11" x2="8.01" y2="11"/>
                              </svg>
                              {t("chat.tools.dangerWarning")}
                            </span>
                          {/if}

                          <!-- Brief summary of args in header -->
                          {#if hasNonEmptyArgs || bashCmd}
                            <span class="text-[0.6rem] text-muted-foreground/60 truncate ml-1 flex-1 min-w-0 font-mono">
                              {#if tu.tool === "bash" && bashCmd}
                                {bashCmd.length > 60 ? bashCmd.slice(0, 60) + "…" : bashCmd}
                              {:else if (tu.tool === "read_file" || tu.tool === "write_file" || tu.tool === "edit_file") && tu.args.path}
                                {tu.args.path}
                              {:else if tu.tool === "web_search" && tu.args.query}
                                {tu.args.query}
                              {:else if tu.tool === "web_fetch" && tu.args.url}
                                {tu.args.url}
                              {:else if tu.tool === "search_output" && tu.args.pattern}
                                /{tu.args.pattern}/ in {tu.args.path?.split("/").pop() ?? tu.args.path}
                              {:else if tu.tool === "search_output" && tu.args.path}
                                {tu.args.path.split("/").pop() ?? tu.args.path}
                              {/if}
                            </span>
                          {/if}

                          <!-- Status indicator -->
                          <span class="ml-auto shrink-0 flex items-center gap-1">
                            {#if tu.status === "calling"}
                              <span class="flex gap-0.5">
                                {#each [0,1,2] as i}
                                  <span class="w-1 h-1 rounded-full bg-current animate-bounce"
                                        style="animation-delay:{i*0.1}s"></span>
                                {/each}
                              </span>
                            {:else if tu.status === "cancelled"}
                              <!-- Slash-circle icon for cancelled -->
                              <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5"
                                   stroke-linecap="round" class="w-3 h-3">
                                <circle cx="6" cy="6" r="4.5"/>
                                <line x1="3.2" y1="8.8" x2="8.8" y2="3.2"/>
                              </svg>
                            {:else if tu.status === "done"}
                              <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
                                   stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                                <polyline points="2 6 5 9 10 3"/>
                              </svg>
                            {:else}
                              <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
                                   stroke-linecap="round" class="w-3 h-3">
                                <line x1="3" y1="3" x2="9" y2="9"/><line x1="9" y1="3" x2="3" y2="9"/>
                              </svg>
                            {/if}
                          </span>
                        </button>

                        <!-- Cancel button (only shown while calling) -->
                        {#if tu.status === "calling"}
                          <button
                            onclick={(e) => { e.stopPropagation(); cancelToolCall(msg.id, tuIdx, tu.toolCallId); }}
                            title={t("chat.tools.cancel")}
                            class="shrink-0 flex items-center gap-1 px-2 py-1 mr-1.5
                                   rounded-lg text-[0.6rem] font-semibold transition-all cursor-pointer
                                   {isDangerous
                                     ? 'bg-red-500/15 text-red-600 dark:text-red-400 hover:bg-red-500/25 border border-red-500/30'
                                     : 'bg-muted text-muted-foreground/70 hover:bg-red-500/10 hover:text-red-600 dark:hover:text-red-400 border border-border'}">
                            <!-- Stop/cancel icon -->
                            <svg viewBox="0 0 12 12" fill="currentColor" class="w-2.5 h-2.5">
                              <rect x="2" y="2" width="8" height="8" rx="1"/>
                            </svg>
                            {t("chat.tools.cancel")}
                          </button>
                        {/if}
                      </div>

                      <!-- Danger detail banner (shown below header when dangerous + calling) -->
                      {#if isDangerous && tu.status === 'calling' && dangerKey}
                        <div class="flex items-center gap-2 mx-3 mb-1.5 px-2 py-1 rounded-lg
                                    bg-red-500/10 border border-red-500/20 text-red-600 dark:text-red-400">
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                               stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
                            <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
                            <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
                          </svg>
                          <span class="text-[0.58rem] font-medium leading-snug">
                            {t(dangerKey)}
                          </span>
                        </div>
                      {/if}

                      <!-- Expanded detail panel -->
                      {#if tu.expanded && hasDetails}
                        <div class="border-t border-current/10 px-3 py-2 flex flex-col gap-2
                                    text-[0.63rem] text-muted-foreground">
                          <!-- Bash: show command prominently -->
                          {#if tu.tool === "bash" && bashCmd}
                            <div class="flex flex-col gap-0.5">
                              <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
                                {t("chat.tools.commandLabel")}
                              </span>
                              <pre class="font-mono text-[0.65rem] leading-relaxed whitespace-pre-wrap break-all
                                          bg-black/8 dark:bg-white/8 rounded-lg px-2.5 py-2 max-h-48 overflow-y-auto
                                          text-foreground select-text">{bashCmd}</pre>
                            </div>
                          <!-- File tools: show path prominently -->
                          {:else if (tu.tool === "read_file" || tu.tool === "write_file" || tu.tool === "edit_file") && tu.args?.path}
                            <div class="flex flex-col gap-0.5">
                              <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
                                {tu.tool === "read_file" ? t("chat.tools.fileLabel") : t("chat.tools.fileLabel")}
                              </span>
                              <pre class="font-mono text-[0.65rem] leading-relaxed whitespace-pre-wrap break-all
                                          bg-black/8 dark:bg-white/8 rounded-lg px-2.5 py-2
                                          text-foreground select-text">{tu.args.path}</pre>
                              {#if tu.tool === "edit_file" && tu.args.old_text}
                                <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50 mt-1">
                                  {t("chat.tools.editOldLabel")}
                                </span>
                                <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                                            bg-red-500/5 border border-red-500/10 rounded-lg px-2 py-1.5 max-h-32 overflow-y-auto
                                            text-foreground select-text">{tu.args.old_text}</pre>
                              {/if}
                              {#if tu.tool === "edit_file" && tu.args.new_text}
                                <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50 mt-1">
                                  {t("chat.tools.editNewLabel")}
                                </span>
                                <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                                            bg-emerald-500/5 border border-emerald-500/10 rounded-lg px-2 py-1.5 max-h-32 overflow-y-auto
                                            text-foreground select-text">{tu.args.new_text}</pre>
                              {/if}
                              {#if tu.tool === "write_file" && tu.args.content}
                                <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50 mt-1">
                                  {t("chat.tools.contentLabel")}
                                </span>
                                <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                                            bg-black/5 dark:bg-white/5 rounded-lg px-2 py-1.5 max-h-48 overflow-y-auto
                                            text-foreground select-text">{tu.args.content}</pre>
                              {/if}
                            </div>
                          <!-- Web search: show query -->
                          {:else if tu.tool === "web_search" && tu.args?.query}
                            <div class="flex flex-col gap-0.5">
                              <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
                                {t("chat.tools.queryLabel")}
                              </span>
                              <pre class="font-mono text-[0.65rem] leading-relaxed whitespace-pre-wrap break-all
                                          bg-black/8 dark:bg-white/8 rounded-lg px-2.5 py-2
                                          text-foreground select-text">{tu.args.query}</pre>
                            </div>
                          <!-- Web fetch: show URL -->
                          {:else if tu.tool === "web_fetch" && tu.args?.url}
                            <div class="flex flex-col gap-0.5">
                              <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
                                URL
                              </span>
                              <pre class="font-mono text-[0.65rem] leading-relaxed whitespace-pre-wrap break-all
                                          bg-black/8 dark:bg-white/8 rounded-lg px-2.5 py-2
                                          text-foreground select-text">{tu.args.url}</pre>
                            </div>
                          <!-- Generic: show raw JSON args (skip empty objects) -->
                          {:else if hasNonEmptyArgs}
                            <div class="flex flex-col gap-0.5">
                              <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
                                {t("chat.tools.argsLabel")}
                              </span>
                              <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                                          bg-black/5 dark:bg-white/5 rounded-lg px-2 py-1.5 max-h-48 overflow-y-auto
                                          select-text">{JSON.stringify(tu.args, null, 2)}</pre>
                            </div>
                          {/if}
                          <!-- Result -->
                          {#if tu.result}
                            <div class="flex flex-col gap-0.5">
                              <span class="text-[0.55rem] font-semibold uppercase tracking-wider text-muted-foreground/50">
                                {t("chat.tools.resultLabel")}
                              </span>
                              <pre class="font-mono text-[0.6rem] leading-relaxed whitespace-pre-wrap break-all
                                          bg-black/5 dark:bg-white/5 rounded-lg px-2 py-1.5 max-h-64 overflow-y-auto
                                          select-text {tu.status === 'error' ? 'text-red-500' : ''}">{#if typeof tu.result === "string"}{tu.result}{:else}{JSON.stringify(tu.result, null, 2)}{/if}</pre>
                            </div>
                          {/if}

                          <!-- Cancel button in expanded view too (for tools with details) -->
                          {#if tu.status === "calling"}
                            <div class="flex justify-end pt-1">
                              <button
                                onclick={() => cancelToolCall(msg.id, tuIdx, tu.toolCallId)}
                                class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[0.62rem]
                                       font-semibold transition-all cursor-pointer
                                       {isDangerous
                                         ? 'bg-red-500 text-white hover:bg-red-600'
                                         : 'bg-red-500/10 text-red-600 dark:text-red-400 hover:bg-red-500/20 border border-red-500/30'}">
                                <svg viewBox="0 0 12 12" fill="currentColor" class="w-2.5 h-2.5">
                                  <rect x="2" y="2" width="8" height="8" rx="1"/>
                                </svg>
                                {t("chat.tools.cancel")}
                              </button>
                            </div>
                          {/if}
                        </div>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}

              <!-- Response bubble (shown once we're past the <think> block) -->
              {#if msg.content.trim()}
                <div class="group/bubble flex flex-col gap-0.5">
                  <div class="rounded-2xl rounded-tl-sm bg-muted dark:bg-[#1a1a28]
                              px-3.5 py-2.5 text-[0.78rem] leading-relaxed text-foreground
                              break-words overflow-hidden">
                    <MarkdownRenderer content={msg.content} pending={msg.pending} />
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

    <!-- LLM accuracy warning -->
    <div class="flex items-center justify-center gap-1.5 mb-1.5 px-2 py-1 rounded-md
                bg-amber-500/8 border border-amber-500/15">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"
           stroke-linecap="round" stroke-linejoin="round"
           class="w-3 h-3 shrink-0 text-amber-500/70">
        <path d="M7.15 2.43L1.41 12a1 1 0 0 0 .86 1.5h11.46a1 1 0 0 0 .86-1.5L8.85 2.43a1 1 0 0 0-1.7 0z"/>
        <line x1="8" y1="6" x2="8" y2="9"/><line x1="8" y1="11" x2="8.01" y2="11"/>
      </svg>
      <span class="text-[0.52rem] text-amber-600/70 dark:text-amber-400/70 leading-tight select-none">
        {t("chat.hint.llmWarning")}
      </span>
    </div>

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
        onbeforeinput={onChatBeforeInput}
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
        {t("chat.hint.running")}
      {:else if status === "loading"}
        {t("chat.hint.loading")}
      {:else}
        {t("chat.hint.stopped")}
      {/if}
    </p>
  </footer>

  </div><!-- end main chat column -->
</div><!-- end root flex container -->
