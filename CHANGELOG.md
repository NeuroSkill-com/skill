# Changelog

All notable changes to NeuroSkill™ are documented here.

---

## [Unreleased]

### CLI — Proactive Hooks CRUD

- **Full hook management from the CLI**: hooks can now be fully created, listed, updated, enabled/disabled, and deleted from the command line, matching the UI's capabilities. New subcommands:
  - `hooks list` — list raw hook rules (name, keywords, threshold, scenario, command, text)
  - `hooks add <name> [--keywords "k1,k2"] [--scenario cognitive] [--command cmd] [--hook-text txt] [--threshold 0.14] [--recent 12]` — create a new hook
  - `hooks remove <name>` — delete a hook by name
  - `hooks enable <name>` / `hooks disable <name>` — toggle a hook on or off
  - `hooks update <name> [--keywords …] [--scenario …] [--threshold …] …` — modify fields on an existing hook
- **New WS commands**: `hooks_get` (return raw hook rules) and `hooks_set` (replace all hooks with sanitised input) added to the WebSocket API, reusing the same `sanitize_hook` validation as the Tauri IPC path.

### Chat

- **Auto-label typed text every 5 seconds**: when the user types (not pastes) in the chat input, a 5-second interval — matching the EXG model's epoch window size (`EPOCH_S`) — accumulates only keyboard-originated characters using the `beforeinput` event's `inputType`. At the end of each 5-second window, if any recognisable words were typed, a label is automatically submitted via `submit_label` with the typed words as label text and the current chat session summary (session ID, model name, last 6 messages) as context. Paste, drop, and autocomplete inputs are excluded so only genuine typing is captured. The timer starts on component mount and flushes any remaining typed text on destroy. **Word-boundary aware**: if the 5-second window ends mid-word, the flush is deferred until the user types the next word-boundary character (space, punctuation, Enter), so labels always contain complete words; a 1.5-second safety timeout forces the flush if the user stops typing mid-word. **Deletion tracking**: deletions within the same 5-second window are captured via `beforeinput` delete events (`deleteContentBackward`, `deleteContentForward`, `deleteWordBackward`, `deleteWordForward`, `deleteByCut`); the deleted text is extracted from the textarea value before the event mutates it; at flush time, typed words that were also deleted are wrapped in `<del>…</del>` tags (e.g. `the <del>blue</del> sky`) using case-insensitive multiset matching so duplicate occurrences are correctly handled one-for-one.

### All Windows

- **Fix windows not brought forward when opened from tray menu**: clicking a tray menu item (Settings, Help, History, Chat, etc.) would sometimes open the window behind the current foreground window or fail to raise a minimized window. All window-opening functions now call `unminimize()` before `show()` + `set_focus()` for existing windows, and newly created windows explicitly call `set_focus()` after `.build()` instead of discarding the window handle. Affects all windows: main, settings, help, history, compare, chat, downloads, search, labels, focus timer, calibration, about, API, onboarding, What's New, and session detail.
- **Rounded window corners**: all windows now have rounded corners (10px border-radius). Enabled via transparent Tauri windows (`transparent: true` on every `WebviewWindowBuilder`) combined with CSS `border-radius` and `overflow: hidden` on the root `<html>` element. On macOS, requires the `macos-private-api` feature flag (`macOSPrivateApi: true` in tauri.conf.json). Applies to the main window, settings, help, history, chat, about, calibration, downloads, search, session detail, labels, focus timer, onboarding, What's New, compare, and API windows.
- **Fix bottom corners not rounded**: the `<html>` element had `border-radius: 10px` but child elements (`body`, `#main-content`) painted over the rounded bottom corners. Added matching `border-radius` to `body` and bottom-corner rounding + `overflow: hidden` to `#main-content`, plus explicit `rounded-b-[10px]` on the chat page root container, so all windows now properly display rounded corners on all four sides.

### Chat History

- **Tool calls now persisted in SQLite**: tool-call events (tool name, status, arguments, results, tool_call_id) are now saved alongside assistant messages in a new `chat_tool_calls` table. When reloading a session, tool-call cards are fully restored with expandable details — previously tool calls were lost on reload and only the text content was preserved. The new Tauri command `save_chat_tool_calls` writes tool rows after the assistant message is saved; `load_session` joins them back in a single efficient query. Existing databases are migrated seamlessly via `CREATE TABLE IF NOT EXISTS`.

### All Windows

- **Real-time context usage prediction**: the context usage bar now updates live instead of only after inference completes:
  - **Before sending**: estimates prompt tokens from all messages + system prompt + tool prompt overhead + current input text (~4 chars/token heuristic)
  - **During streaming**: completion tokens are counted in real-time as deltas arrive
  - **After completion**: snaps to real token counts from llama.cpp (`done` chunk)
  - Shows `~` prefix when displaying estimated values vs. real ones
  - Bar animation reduced from 300ms to 150ms for more responsive feel
- **Cmd/Ctrl+W now closes windows**: added a global keydown handler in the root layout that calls `getCurrentWindow().close()` on ⌘W (macOS) or Ctrl+W (Linux/Windows). The main window is hidden (existing behavior), while secondary windows (chat, settings, help, about, etc.) are closed.

### Chat UI

- **Model name in the custom titlebar**: the active model name and status indicator (green/amber/grey dot) have been moved from the in-page chat header into the shared custom titlebar at the very top of the window. The model name is centered in the titlebar with absolute positioning, matching the style used by other secondary windows (history, help, etc.). The in-page header no longer shows the model name, freeing space for tool and EEG badges.

- **Context usage as circular progress next to tools button**: the separate full-width context-usage bar below the chat header has been replaced by a compact circular SVG progress ring that sits next to the tools badge. The ring fills proportionally (green → amber at 70% → red at 90%) with a colour-matched percentage label; full token counts (`~used/total`) shown in the tooltip on hover. The tools button retains its original wrench icon, label, and enabled count.

- **Tools removed from parameters panel**: the tool allow-list and execution mode selector have been removed from the parameters/settings slide-in panel. Tools are now configured exclusively via the dedicated tools panel opened by the tools badge button in the header, eliminating duplicate UI.

- **Bash command always visible in tool cards**: the bash tool result JSON now includes the `command` field. The UI extracts the command from `tu.args.command` first, falling back to `tu.result.command` when args are empty (common with small models). This ensures the executed command is always shown in both the collapsed header summary and the expanded detail view.

- **Fix bash tool calls with empty arguments**: small models often emit a `[TOOL_CALL]` with empty `{}` arguments alongside a `` ```bash `` code fence containing the actual command. The extractor now post-processes bash calls with empty args and fills them from the first bash/sh code fence found in the text. Also accept `"tool"` as alias for `"name"` and `"parameters"` as alias for `"arguments"` in `[TOOL_CALL]` blocks for broader model compatibility.

- **Fix scripts storage**: tool script and output files were stored in per-server-start timestamp directories under `chats/scripts/<ts>/`, creating empty directories on every LLM server start even when no tools were used. Now `scripts_dir` is the base `chats/scripts/` path and `run_<ts>/` subdirectories are created lazily only when a tool actually writes a file.

- **Fix tool cards showing empty arguments**: tool-call cards no longer display "ARGUMENTS: {}" when a tool was called with an empty parameter object. Empty `{}` args are now excluded from the "has details" check, so cards with no meaningful args, result, or detail text collapse cleanly without an expand chevron. The header summary also skips showing empty args inline.

- **Filter out empty bash tool calls**: when the model emits a bash `[TOOL_CALL]` with empty `{}` arguments and no code fence to fill from, the call is now silently dropped instead of executing and producing a "missing command" error card. This eliminates the common pattern of empty-args bash error cards that clutter the conversation.

- **Cross-round tool call dedup**: the model can no longer re-execute the exact same tool call (same name + same arguments) across multiple inference rounds. A `(name, args)` set tracks all executed calls; duplicate re-invocations are filtered out. If all calls in a round are filtered, the model's text is returned directly without entering an empty tool-execution phase.

- **Fix model not calling tools on 4096 context**: the compact tool prompt was used for context windows ≤4096 tokens, giving models like Qwen3.5-4B a terse 4-line prompt with no examples — causing them to think about tools but never emit `[TOOL_CALL]` blocks. Threshold lowered to ≤2048 so 4096+ context models get the full prompt with parameter docs, explicit instructions, and concrete examples. The compact prompt (for ≤2048) was also improved with inline examples.

- **Expandable tool-call cards with rich detail views**: tool-call bubbles are now always expandable (like thinking bubbles) whenever any details are available. Each tool type has a purpose-built expanded view:
  - **Bash**: shows the full command under a "Command" header in a prominent monospace block
  - **File tools** (`read_file`/`write_file`/`edit_file`): shows the file path; `edit_file` shows find/replace diffs in red/green blocks; `write_file` shows file content
  - **Web search**: shows the search query
  - **Web fetch**: shows the URL
  - **Other tools**: shows raw JSON arguments as before
  - All detail blocks support text selection for easy copying

- **Fix LLM not calling tools**: strengthened the tool-calling system prompt to explicitly instruct the model to never show commands in code blocks and always use `[TOOL_CALL]` blocks instead. Added a fallback extraction layer that catches bare `` ```bash ``/`` ```sh `` code fences emitted by small models and automatically converts them into proper bash tool calls. The fallback is suppressed when a proper `[TOOL_CALL]` is already present. Added more examples to the prompt (listing files, checking location). 21 Rust tests pass including 3 new fallback tests.

- **Chat session archive (soft-delete)**: the default action on chat sessions is now "archive" (box icon) instead of permanent delete. Archived sessions are hidden from the main conversation list and collected in a collapsible "Archive" section at the bottom of the sidebar. From the archive, users can restore a session back to the main list or permanently delete it. Backend adds an `archived` column to `chat_sessions` with seamless migration of existing databases.

- **Model name moved to titlebar**: the active model name is now shown in the header drag region (acting as the window title), freeing horizontal space for badges and controls. The footer hint no longer repeats the model name.

- **Deduplicated tools UI**: when the dedicated tools panel is open, the tools allow-list in the parameters/settings panel is automatically hidden to avoid showing the same controls in two places simultaneously.

- **Tools badge now toggles a dedicated tools panel**: clicking the wrench/tools badge in the chat header now opens and closes a dedicated tools-only panel instead of opening the full settings drawer. The panel displays the tool allow-list grid, execution mode toggle, and a context-length progress bar showing the model's `n_ctx` window size with token usage from the most recent assistant message. The badge is always visible when the model supports tools (even with 0 tools enabled) and highlights when the panel is open. The tools panel and settings panel are mutually exclusive — opening one closes the other.

- **Added LLM accuracy warning banner**: a persistent amber-tinted warning is now displayed above the footer hint in the chat window, reminding users that LLM output can be inaccurate and to always verify tool results and generated content. Fully localised across all 5 supported languages (en/de/fr/uk/he).

- **Fixed settings/tools panel not scrollable**: the parameters panel (system prompt, EEG bands, tool toggles, thinking level, sliders) had no overflow handling and no max-height constraint. When its content exceeded the window height, it pushed the message list off-screen entirely. Now capped at `max-h-[50vh]` with `overflow-y-auto` so it scrolls internally while always leaving room for the chat messages.

### Chat History

- **Fixed chat history not preserving full responses**: assistant messages that included `leadIn` text (what the model says before calling tools, e.g. "I'll check that for you") were not fully saved — only the final `content` was persisted, and `leadIn` was discarded. Additionally, the save condition required non-empty `content`, so messages with only lead-in text or thinking were skipped entirely. Now `leadIn` and `content` are combined into a single string before saving, ensuring the complete response is visible when loading old conversations.

### macOS

- **Fixed copy/paste in chat window**: added the standard Edit menu (Undo, Redo, Cut, Copy, Paste, Select All) to the macOS app menu bar. Without this menu, macOS Tauri webviews do not route ⌘C / ⌘V / ⌘X / ⌘A to the web content, making text selection and clipboard operations non-functional.

### LLM — Coding-Agent Tools

- **Bash tool** (`bash`): execute shell commands from the LLM chat with configurable timeout. Output is tail-truncated to 2 000 lines / 50 KB (keeps the end where errors appear). Commands run in the user's home directory.
- **Fixed tool results not visible to model**: two issues prevented the model from seeing bash output:
  1. Tool result messages used `"role": "tool"` which most local model chat templates (Qwen, Llama, etc.) do not support. Now mapped to `"role": "user"` with a `[Tool Result]` prefix.
  2. When the model only emitted tool calls with no prose, the assistant message was skipped, creating consecutive user messages (original query + tool result) which break chat templates. Now a `[Calling tools…]` placeholder is always pushed to maintain proper user/assistant alternation.
- **Bash output saved to file**: the `bash` tool now always saves full command output to a timestamped text file (`output_<ts>.txt`) in the session scripts directory. Instead of returning the full output inline (which consumed context), it returns a compact summary — first 20 + last 20 lines for outputs over 200 lines, with the full `output_file` path for follow-up queries.
- **New `search_output` tool** 🔎: lets the LLM search and navigate large bash outputs without loading them into context. Supports:
  - **Regex search** (`pattern`): case-insensitive regex with configurable context lines around matches
  - **Head/tail** (`head`, `tail`): retrieve the first or last N lines
  - **Line range** (`line_start`, `line_end`): retrieve a specific range of lines
  - **Max matches** (`max_matches`): cap the number of regex results (default: 50)
  - All output includes line numbers for easy reference. Auto-enabled when bash is enabled — no separate toggle needed.
- **Context-aware tool calling**: tool-calling now adapts to the available context window to prevent "prompt too long" errors.
  - **Compact tool prompt** (≤ 4096 tokens): the verbose tool system prompt (descriptions, parameter docs, examples) is replaced with a minimal 3-line version listing tool names and the call format, saving ~500 tokens.
  - **Automatic history trimming**: before each inference round, the message history is checked against 75% of `n_ctx`. Long tool results in history are truncated to 2 KB, then oldest non-system messages are dropped until the conversation fits within budget.
  - **Full tool prompt** (> 4096 tokens): unchanged — includes per-parameter documentation, rules, and examples.
- **Long command → script file**: bash commands exceeding 8 KB are automatically written to a timestamped shell script (`cmd_<ts>_<ms>.sh`) in `skill_dir/chats/scripts/<session>/` and executed as `bash <script>` instead of `bash -c <command>`, avoiding OS ARG_MAX / "prompt too long" errors. Scripts include `set -euo pipefail`, are preserved per-session for inspection, and the tool result includes the `script_path` when one was used.
- **Read file tool** (`read_file`): read text file contents with `offset`/`limit` pagination for large files. Output is head-truncated to 2 000 lines / 50 KB with continuation hints.
- **Write file tool** (`write_file`): create or overwrite files with automatic parent directory creation.
- **Edit file tool** (`edit_file`): surgical find-and-replace edits with exact text matching, CRLF-aware line ending preservation, and duplicate-occurrence rejection.
- All four tools are **disabled by default** and marked with an "Advanced" warning badge in the UI — they must be explicitly enabled per-tool in Settings → LLM or the Chat sidebar.
- Path resolution supports `~` home-directory expansion and relative paths (resolved against home).
- Updated `KNOWN_TOOL_NAMES` for tool-call extraction and stripping in both Rust and frontend.
- **Expandable tool-call cards**: tool pills are now clickable cards (similar to the thinking expand/collapse) that show the command/path in the header and expand to reveal structured arguments and full results. Bash commands show the command inline; file tools show the path. The expanded panel displays formatted JSON with scrollable pre blocks.
- **Safety approval for dangerous operations**: bash commands containing destructive patterns (`rm`, `sudo`, `chmod`, system paths like `/etc/`, `/usr/`) trigger an OS-native approval dialog before execution. File write/edit to sensitive system paths also require approval. Denied operations return a clean error to the LLM. Patterns are defined in `DANGEROUS_BASH_PATTERNS` and `SENSITIVE_PATH_PREFIXES` for easy extension.
- **Fixed leaked `[TOOL_CALL]` markup**: `stripToolCallFences()` now strips complete `[TOOL_CALL]…[/TOOL_CALL]` blocks and incomplete `[TOOL_C…` prefixes during streaming, preventing raw tool-call tags from appearing in chat bubbles.
- **Fixed tool calling not executing**: the system prompt injected tool JSON schemas but gave the model no instructions on how to emit a call, so it described tool usage in prose instead of invoking tools. Rewrote `inject_tools_into_system_prompt` to include per-tool parameter documentation, explicit `[TOOL_CALL]…[/TOOL_CALL]` format instructions with rules (valid JSON, stop-and-wait, don't fabricate results), and concrete examples (date, bash, read_file).
- Full i18n for all 5 locales (en, de, fr, he, uk).
- **Per-tool-call cancel/stop button**: each tool-call card now displays a cancel button while the tool is actively executing. Clicking cancel sends a `cancel_tool_call` command to the backend, which adds the `tool_call_id` to a shared cancellation set. Both sequential and parallel execution paths check this set before and during tool execution, returning a clean `"cancelled by user"` error to the LLM if the tool was cancelled.
- **Danger detection and warnings**: tool-call cards now detect dangerous operations at the UI level — bash commands containing `rm`, `sudo`, `chmod`, system paths (`/etc/`, `/usr/`, `/var/`, etc.), and file operations targeting sensitive paths (`/boot/`, `/bin/`, `/sbin/`, etc.) show an inline `⚠ Potentially dangerous` badge with a red-highlighted card border. The danger banner with a specific description appears below the card header while the tool is running.
- **Cancelled status display**: cancelled tool calls show an amber-tinted card with a slash-circle icon and "Cancelled" label, distinct from both error (red) and success (green) states.
- **`ToolCancelled` IPC chunk**: new `tool_cancelled` chunk type sent through the Tauri IPC channel for real-time cancel feedback to the frontend.
- The expanded detail panel for running tools includes a prominent cancel button — styled as a filled red button for dangerous operations and a subtle bordered button for safe ones.

### Chat History Storage

- **Moved chat history into `chats/` subdirectory**: the LLM chat history database (`chat_history.sqlite`) is now stored under `skill_dir/chats/` instead of directly in `skill_dir`. The `chats/` directory is created automatically on first use.
- **Automatic migration**: existing `chat_history.sqlite` files in the old location are automatically moved to the new `chats/` subdirectory on startup, including WAL and SHM sidecar files. No manual action required.
- **Fixed chat messages not persisting**: the `save_chat_message` Tauri invoke used snake_case `session_id` as the parameter key, but Tauri v2's `#[tauri::command]` macro expects camelCase `sessionId` from the JS side. This caused every save call to silently fail (the `.catch(() => {})` handler swallowed the deserialization error). Both the user-message and assistant-message save calls are now fixed.

### i18n

- Translated all remaining English-fallback keys in Hebrew (`he.ts`): 138 dashboard/history/help keys, 39 hooks keys, 10 LLM tool keys, and 2 help-settings keys.
- Translated 39 hooks keys into German (`de.ts`), French (`fr.ts`), and Ukrainian (`uk.ts`).
- Removed all `// TODO: translate` markers — zero untranslated keys remain across all 4 non-English locales.

### Build / Tooling

- Renamed Homebrew cask to `neuroskill` and moved definition to `Casks/neuroskill.rb`; install path is now `brew tap NeuroSkill-com/skill && brew install --cask neuroskill`.
- Updated cask generation (`scripts/generate-homebrew-cask.sh`) to write `Casks/neuroskill.rb`, target `NeuroSkill.app` (without `™`), and preserve user data by removing `~/.skill` from `zap`.

### Naming / Windows

- Set `src-tauri/tauri.conf.json` `productName` to `NeuroSkill` (no trademark symbol) so generated artifact/file naming stays ASCII-safe and consistent.
- Updated frontend app-name normalization (`src/lib/app-name-store.svelte.ts`) to always render the UI-facing name as `NeuroSkill™`, even when backend/app config returns plain `NeuroSkill`.
- Hardened Windows packaging/release scripts (`scripts/create-windows-nsis.ps1`, `release-windows.ps1`) to initialize UTF-8 console/output encoding (`UTF8Encoding` + code page 65001) so `™` and other non-ASCII UI text render consistently.
- Updated NSIS generation so installer/Add-Remove-Programs display labels use `NeuroSkill™` while filesystem/registry key names remain `NeuroSkill`.
- Updated release workflow `latest.json` fallback notes in `.github/workflows/release-linux.yml`, `.github/workflows/release-mac.yml`, and `.github/workflows/release-windows.yml` to use `NeuroSkill™ v<version>` for user-facing updater notes while keeping file/artifact names ASCII-safe.
- Updated release workflow Discord notification titles/descriptions in `.github/workflows/release-linux.yml`, `.github/workflows/release-mac.yml`, and `.github/workflows/release-windows.yml` to use the UI-facing product name `NeuroSkill™`.

## [0.0.37] — 2026-03-14

### History — Calendar Heatmap View

- **Calendar heatmap** replaces the old single-day paginator as the default history view. Choose between **Year**, **Month** (default), **Week**, and **Day** granularity via a segmented control in the titlebar.
- **Year view**: GitHub-style contribution heatmap with weekly columns, month labels, and an intensity legend (emerald gradient). Click any day with recordings to drill into the day view.
- **Month view**: traditional calendar grid with per-day session counts and heat-colored backgrounds. Out-of-month days are dimmed; today is ring-highlighted.
- **Week view**: 24-hour timeline grid per day with canvas-rendered **epoch dots** — one dot per 5-second EEG embedding, color-coded by session. Each dot's Y position maps relaxation value within the session's band. Session bar fallback renders while timeseries loads. Day label sidebar shows weekday, date, and session count; click to navigate to day view. Empty days show a subtle placeholder.
- **Day view – epoch dot canvas**: new canvas timeline below the existing 24h bar renders all epoch dots for the day, with session color legend and label count summary. Label markers appear as amber triangles with text.
- **Always-rendered UI**: the calendar grid renders immediately, even while data is loading or when there are no recordings yet. A skeleton pulse animation shows in the day-label slot during loading; empty state shows a gentle hint with the clock icon and guidance text.
- **Reworked titlebar UX**: themed `history-viewmode-seg` segmented control (accent-aware via `--color-primary`); context-sensitive navigation — day mode shows prev/next arrows with position counter, calendar modes show period-appropriate prev/next with a formatted label (e.g. "March 2026"); skeleton loading animation for day labels.
- **i18n**: all new strings (`history.view.year/month/week/day`, `history.heatmap.less/more/none/hours/dayStreak`, `history.session`) translated across English, German, French, Hebrew, and Ukrainian.

### LLM — Hardware Fit Prediction

- **`get_model_hardware_fit` Tauri command**: uses `llmfit-core`'s `ModelFit::analyze` against the user's detected hardware (`SystemSpecs::detect()`, cached via `OnceLock`) to predict whether each catalog model will run. Returns fit level, run mode, estimated memory, tok/s, and composite score.
- **Per-quant fit badges** in the LLM settings tab: 🟢 Runs great / 🟡 Runs well / 🟠 Tight fit / 🔴 Won't fit. Hover tooltip shows run mode, memory breakdown, and estimated speed.
- **Hardware fit detail row** below each model entry showing run mode, memory utilization, estimated tok/s, and score.
- **Hardware summary line** above the family dropdown showing available memory from the detected GPU/RAM pool.
- **i18n**: `llm.fit.*` keys translated across all 5 locales.

### UI

- **Refactored `CustomTitleBar.svelte`**: collapsed the macOS and Windows/Linux duplicate branches into shared Svelte 5 snippets (`windowControls`, `centerContent`, `actionButtons`, `historyHead`, `tbBtn`, plus reusable icon snippets). Single unified template switches element order based on platform. Eliminated all duplicated HTML and CSS blocks. **975 → 533 lines (45% reduction).**
- Main-window titlebar now tints red when Bluetooth is unavailable (`bt_off` state), giving an immediate visual cue that the BLE adapter is off or missing. Uses the semantic `--color-error` token so the tint respects both light and dark themes.

### Chat — Tool Calling (pi-mono architecture)

- Implemented pi-mono style tool calling architecture with structured lifecycle events, argument validation, and configurable execution modes.
- Added **JSON Schema argument validation** for tool calls using the `jsonschema` crate — tool arguments are now validated against the tool's JSON Schema `parameters` definition before execution, with detailed error messages on validation failure (modelled after pi-mono's `validateToolArguments` with AJV).
- Added **configurable tool execution mode**: `parallel` (prepare sequentially, execute concurrently — default) and `sequential` (execute one-by-one in order). Persisted in `settings.json` under `llm.tools.execution_mode`.
- Added **configurable max tool rounds** (`max_rounds`, default 3) and **max tool calls per round** (`max_calls_per_round`, default 4) — both persisted in settings.
- Added **rich tool-execution lifecycle events** via IPC: `ToolExecutionStart` (with tool_call_id, tool_name, validated args) and `ToolExecutionEnd` (with result JSON and is_error flag), alongside the legacy `ToolUse` status events for backwards compatibility.
- Added **`BeforeToolCallFn` / `AfterToolCallFn` hook type definitions** for future extensibility — allows blocking tool execution or overriding results programmatically (modelled after pi-mono's `beforeToolCall`/`afterToolCall` hooks).
- Added execution mode toggle UI in both the Chat window settings panel and Settings → LLM tools section.
- Fully localised new strings in all five languages (EN, DE, FR, UK, HE).
- Added 4 new Rust unit tests for argument validation (valid args, missing required, no schema, wrong type) — all 15 tool tests pass.

### Chat — Tool Calling

- Added tool calling support to the LLM chat window with four built-in tools: **Date & Time**, **Location** (IP geolocation via ipwho.is), **Web Search** (DuckDuckGo Instant Answer API), and **Web Fetch** (fetch & read web pages).
- Added per-tool enable/disable toggles in the chat settings panel — persisted via `settings.json` under `llm.tools`.
- Added live tool-use indicators on assistant messages (calling → done/error) via a new `ToolUse` IPC chunk variant.
- Added a **Tools** badge in the chat header showing the number of enabled tools.
- Tool toggles are only shown when the model is running (`supports_tools` flag from the server status).
- Fully localised in all five languages (EN, DE, FR, UK, HE).

### Bugfixes

- Fixed quit confirmation dialog never receiving focus — set the parent window on the `rfd::MessageDialog` so the popup appears focused and modal on Linux/Windows instead of opening behind the main window.

- Fixed malformed thought traces that began with an unmatched opening `json` code fence and partial JSON fragments, which caused the rest of the thought bubble markdown to render incorrectly. The shared `normalizeMarkdown()` helper now strips that narrow orphaned preamble while preserving legitimate closed fenced code blocks.
- Fixed another chat tool-call transcript leak in the frontend parser. `stripToolCallFences()` now mirrors the Rust-side tool-call prefix heuristic instead of relying on narrow fence regexes, so incomplete or malformed fenced JSON blocks with blank lines or partial bodies are suppressed before they can appear in the lead-in bubble.
- Hardened chat markdown normalization for malformed model output. Emphasis repair now runs through a shared `normalizeMarkdown()` utility that protects fenced code blocks and inline code spans, trims stray spaces inside `*`/`**` delimiters, and falls back to raw `<strong>`/`<em>` tags when CommonMark flanking rules would still reject the emphasis. Added unit coverage for the repaired cases.
- Fixed expanded thought panels rendering raw markdown while final answers rendered correctly. The thought body now uses the shared `MarkdownRenderer`, so the same markdown normalization and parsing logic applies in both places. Added a muted renderer variant to preserve the thought-panel visual treatment.
- Fixed bold/italic not rendering when models emit `**word **` (space before closing delimiter) or `**Label:**value` (closing `**` preceded by punctuation followed by a non-whitespace character — CommonMark non-right-flanking edge case). Extended `normalizeMd()` in `MarkdownRenderer` with a trailing-space strip pass and a targeted conversion of punctuation-adjacent patterns to raw `<strong>`/`<em>` HTML so they always render as bold/italic regardless of CommonMark delimiter rules.

- Reworked chat-window assistant turn parsing/rendering so one streamed assistant turn can display as separate bubbles for lead-in text, tool activity, collapsed thinking, and final response, instead of merging tool chatter, `<think>` content, and the user-facing answer into one Markdown bubble.
- Fixed bold and italic text not rendering in the final answer bubble when a model emits `** Word:**` (space inside the `**` delimiters).  Added a `normalizeMd()` pre-pass in `MarkdownRenderer` that strips stray leading/trailing spaces inside `**…**` and `*…*` delimiters before handing the string to `marked`.
- Fixed multi-tool calls emitted as a single dict object: `{"date": {}, "location": {}}`.  Models like Qwen3 batch all tool calls as one JSON object where each key is a tool name and each value is the parameter object.  The previous extractor only recognised the OpenAI `{"name":"...","parameters":{}}` and `tool_calls:[...]` shapes.  Fix: added `KNOWN_TOOL_NAMES` constant (`date`, `location`, `web_search`, `web_fetch`), `is_dict_style_multi_tool()` helper, updated `extract_calls_from_value` (iterates over dict entries as calls), `is_tool_call_value` and `looks_like_tool_call_json_prefix` (early-exit when a known tool name appears as a JSON key), and frontend `stripToolCallFences` (same dict-style heuristic).  11/11 Rust unit tests pass.
- Fixed multi-tool and multi-round tool-calling rendering.  Root causes: (1) text emitted by the LLM before a tool call (e.g. "I'll use the date tool") appeared in the response bubble while the tool was running, then snapped to the lead-in position once the next inference round began; (2) with non-thinking models, consecutive rounds' text concatenated into one blob.  Fix: on the first `tool_use "calling"` event per round, the current `rawAcc` is parsed and frozen into `frozenLeadIn`/`frozenThinking`, `rawAcc` is reset to empty, and all subsequent delta/done/error handlers merge the frozen state back via `mergeWithFrozen()`.  Tools from multiple rounds accumulate correctly in the `toolUses` array.
- Replaced the blinking text cursor shown during LLM inference with a spinning SVG arc on the avatar column; the "AI" avatar is restored once generation completes.
- Reordered assistant turn sub-bubbles into strict chronological sequence: *thinking* (collapsed) → *lead-in text* → *tool-use indicators* → *response*.
- Fixed partial tool-call JSON fences and literal `</think>` tags appearing in the chat response bubble for tool-calling models (e.g. Qwen3).  Root causes: (1) the stream sanitizer emitted partial fence text before accumulating enough tokens to recognise the fence as a tool call; (2) tool-calling turns emit two separate `<think>` blocks (pre-tool and post-tool) which the single-pair extractor left unstripped in `content`.  Fix: added `stripToolCallFences()` to the frontend that removes both complete and incomplete fenced tool-call blocks, and rewrote `parseAssistantOutput()` to collect all `<think>…</think>` pairs across a multi-turn response, merging them into a single thinking block while routing the final segment to the answer bubble.
- Fixed chat message formatting when a model starts a JSON tool-call code fence and never closes it before `<think>`: the streaming sanitizer now suppresses incomplete trailing tool-call fences/JSON early enough that Markdown never treats the rest of the assistant reply as one giant code block.
- Fixed chat thinking-panel separation when `<think>` appears after other assistant text (for example after tool-call lead-in text): the chat UI now extracts think blocks from anywhere in the assistant message instead of only when `<think>` is the first visible token.
- Fixed chat tool-calling transcript leakage: assistant JSON tool payloads emitted in OpenAI-style inline/fenced blocks are now stripped from visible streamed output/history (not only `[TOOL_CALL]...[/TOOL_CALL]` markers), so users no longer see raw call JSON before the final natural-language answer.
- Updated the built-in `date` tool response to include explicit local-time metadata (`iso_local`, timezone abbreviation/name, and UTC offset) plus `iso_utc`, so assistant time answers can reliably default to the user's local timezone instead of guessing from epoch values.
- Fixed a follow-up chat tool-calling parser gap where some models output `{"tool":"date","parameters":{}}` instead of `{"name":"date",...}`; extractor now treats `tool` as a valid alias for function name so built-in tool execution triggers for that payload shape too.
- Fixed in-app chat tool-calling compatibility when models emit OpenAI-style function-call JSON directly in assistant text (including `{"name":"date","parameters":{}}`, fenced `json` blocks, and `{"tool_calls":[...]}` envelopes) instead of llama.cpp `[TOOL_CALL]...[/TOOL_CALL]` markers; the Rust extractor now detects these payload shapes and executes built-in tools correctly.
- Reduced title/menu redraw churn by deduplicating unchanged window-title writes (`setTitle`) and skipping no-op titlebar title-observer state updates.
- Reduced spacing between titlebar close/maximize/minimize controls across all windows by matching shared `CustomTitleBar` window-control button width to the other titlebar icon buttons (`30px`).
- Fixed Tailwind v4 `Invalid declaration: onMount` dev-server errors across `CustomTitleBar.svelte`, `+page.svelte`, `GpuChart.svelte`, `DisclaimerFooter.svelte`, and others — `@tailwindcss/vite` v4.2's `enforce:"pre"` transform matched `.svelte?svelte&type=style&lang.css` virtual modules before the Svelte compiler had extracted the `<style>` block, causing the CSS parser to choke on JavaScript imports. Patched `vite.config.js` with a shim that skips all `.svelte` style virtual module IDs in Tailwind's transform plugins. Also removed empty `<style></style>` blocks in `whats-new/+page.svelte` and `UmapViewer3D.svelte`.
- Fixed mmproj crash when the vision projector file is missing on disk — added an `exists()` guard before calling `mtmd_init_from_file` (which can abort/segfault on some platforms instead of returning null); switched from `active_mmproj_path()` to `resolve_mmproj_path(autoload)` so auto-detection works properly; stale paths where the file has been deleted are now filtered out with a warning instead of passed to the C library.
- Fixed app crash after mmproj fails to load — the clip/vision GPU warmup in `MtmdContextParams` (enabled by default) could corrupt Vulkan GPU state when the mmproj file was incompatible with the text model, causing the subsequent text-model warmup decode to abort the process. Disabled the clip warmup at init time (deferred to the first real multimodal request); wired up `no_mmproj_gpu` and `mmproj_n_threads` settings that were defined in `settings.rs` but never passed to the native library; added a file-size sanity check (files < 1 KB are rejected as truncated downloads); wrapped `init_from_file` in `catch_unwind` so a native panic cannot take down the application; improved error messages to include the file path and size for easier diagnostics.
- Fixed Linux mmproj startup crashes caused by unstable mtmd/Vulkan projector initialization paths on some driver stacks: mmproj now defaults to CPU projector init on Linux (while preserving normal text-model GPU offload), and advanced users can explicitly re-enable mmproj GPU init with `SKILL_FORCE_MMPROJ_GPU=1`.
- Fixed stale `mmproj` fallback selection on startup: when the active text model belongs to a known catalog repo, startup now rejects projector paths from a different repo (for example, a 27B projector with a 4B model), logs a clear mismatch warning, and continues in text-only mode without calling mtmd on the incompatible file.
- Fixed Linux WebKit startup abort on Wayland caused by `stacker::maybe_grow` swapping the main-thread stack before JavaScriptCoreGTK initialised. Linux now raises `RLIMIT_STACK` to 64 MiB and runs Tauri on the original main-thread stack; macOS and Windows keep the existing `stacker` path.
- Fixed Linux app auto-close after startup caused by implicit `RunEvent::ExitRequested` handling: implicit exits are now prevented consistently, main window is hidden instead, and only explicit quit paths run full shutdown.
- Fixed intermittent `npm run tauri dev` startup failure on Linux (`scripts/build-espeak-static.sh` exit `141`): replaced a SIGPIPE-prone `ar -t ... | head -1` cache-check pipeline (with `set -o pipefail`) by a safe `mapfile`-based first-object read, preventing false build-script aborts on valid archives.

### LLM Catalog

- Added Qwen3.5 27B Claude 4.6 Opus Reasoning Distilled model family (`eugenehp/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-GGUF`) to the LLM catalog with 17 quant variants (Q2_K through BF16/F16).
- Added OmniCoder 9B model family (`Tesslate/OmniCoder-9B-GGUF`) to the LLM catalog with 13 quant variants (Q2_K through BF16) — a coding-focused 9B model.

## [0.0.36] — 2026-03-12
### CI Runtime

- Fixed macOS updater 404: renamed the macOS updater tarball from `NeuroSkill™.app.tar.gz` (non-ASCII URL) to `NeuroSkill_<version>_aarch64.app.tar.gz` so the URL stored in `latest.json` is pure ASCII and resolves correctly in the Tauri updater HTTP client.

## [0.0.35] — 2026-03-12
### CI Runtime

- Fixed cross-platform `latest.json` merge encoding in release workflows: Windows now writes `latest.json` as UTF-8 without BOM, and Linux/macOS loaders read with `utf-8-sig` to tolerate BOM-prefixed manifests and avoid `JSONDecodeError: Unexpected UTF-8 BOM`.

## [0.0.34] — 2026-03-12

### CI Runtime

- Fixed Windows release CI `Update latest.json` step crashing with "The property 'windows-x86_64' cannot be found" when `latest.json` already exists: `ConvertFrom-Json` returns a `PSCustomObject` whose properties cannot be set by dot-notation for new hyphenated names; the workflow now uses bracket-notation for hashtable/ordered-dict platforms and `Add-Member -Force` for PSCustomObject platforms.

## [0.0.33] — 2026-03-12
### CI Runtime

- Fixed Windows release CI PowerShell parser failures in `.github/workflows/release-windows.yml` by switching `latest.json` fallback `notes` text to ASCII-safe content and removing backtick-escaped tag/version string literals in the Discord webhook payload fields.

## [0.0.32] — 2026-03-12

### CI Runtime

- Windows release workflow now auto-detects NSIS artifacts across both valid output layouts (`src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis` and `src-tauri/target/release/bundle/nsis`) in the build/sign step and artifact collection step, preventing false "bundle dir not found" / "installer not found" failures when Rust emits host-layout release outputs.

## [0.0.31] — 2026-03-12

### CI Runtime

- Release CI contributor attribution now comes only from git commit authors in the tagged release range (`previous_tag..current_tag`), and release workflows no longer append GitHub auto-generated release-note contributors.
- Windows release CI fallback for post-compile Tauri crash: `.github/workflows/release-windows.yml` now detects `npx tauri build` failure after the Rust binary is already produced (for example exit `-1073741571`/stack-overflow path), then recovers by running `npx tauri bundle --bundles nsis --no-sign`, recreating the updater zip (`*.nsis.zip`) from the generated installer, and signing that updater artifact via `npx tauri signer sign` so release publishing can continue.
- Windows release CI now uses the same primary packaging path that works locally (`npm run tauri:build:win:nsis`) instead of direct `npx tauri build` bundling, then signs the generated installer (when a cert is present) and creates/signs updater artifacts (`*.nsis.zip` + `*.nsis.zip.sig`) in workflow.
- Added npm script alias `taur:build:win:nsis` and switched Windows release CI to run `npm run taur:build:win:nsis` exactly.
- Windows release CI now installs NSIS explicitly before packaging (`choco install nsis` when `makensis` is missing), validates `makensis.exe` discovery, and exports `NSIS_DIR`/PATH so `scripts/create-windows-nsis.ps1` runs reliably on `windows-latest` runners.

## [0.0.30] — 2026-03-12

### Build / Tooling

- **Fix macOS release CI Pillow install**: added `--break-system-packages` to the `pip3 install Pillow` command in `.github/workflows/release-mac.yml` to resolve PEP 668 externally-managed-environment error on the `macos-26` runner.

## [0.0.29] — 2026-03-12

### Refactor

- macOS titlebar button order: switched the close and minimize button positions in the shared custom titlebar component so all macOS windows now use the requested control order.

- **macOS quit-time Metal teardown ordering**: added a one-time blocking shutdown helper in `src-tauri/src/lib.rs` and invoke it on explicit `RunEvent::ExitRequested` (`code = Some(_)`) before process exit continues. This now tears down LLM actor state and TTS backends earlier than `RunEvent::Exit`, reducing late `ggml-metal` static-destruction assertions (`GGML_ASSERT([rsets->data count] == 0)`) on macOS quit.

- **macOS shutdown abort in Metal teardown (`GGML_ASSERT([rsets->data count] == 0)`)**: added a blocking `Shutdown` command to the `tts/kitten.rs` worker and wired it into `tts_shutdown()` so `RunEvent::Exit` now waits for KittenTTS resources to drop before process exit/static destructor cleanup. This prevents late Metal/ggml teardown asserts when quitting after KittenTTS and LLM were active.

- **Heap-allocate AppState**: changed `Mutex<AppState>` → `Mutex<Box<AppState>>` across all Rust source files (`lib.rs`, `tray.rs`, `shortcut_cmds.rs`, `muse_session.rs`, `ws_commands.rs`, `openbci_session.rs`, `active_window.rs`, `label_cmds.rs`, `session_csv.rs`, `session_analysis.rs`, `llm/cmds.rs`, `session_dsp.rs`, `ble_scanner.rs`, `window_cmds.rs`, `history_cmds.rs`, `settings_cmds.rs`, `api.rs`, `commands.rs`, `global_eeg_index.rs`) to move the large `AppState` struct onto the heap, reducing main-thread stack frame size and mitigating stack overflow risk on platforms with smaller default stacks.
- **Extract LLM state into `Box<LlmState>`**: moved all LLM-related fields (`llm_config`, `llm_catalog`, `llm_downloads`, `llm_logs`, `llm_state_cell`, `llm_loading`, `llm_start_error`, `chat_store`) out of `AppState` into a dedicated `LlmState` sub-struct stored as `Box<LlmState>`, accessed via `s.llm.config`, `s.llm.catalog`, etc.  This further reduces `AppState`'s on-stack footprint and groups all LLM concerns behind a single heap-allocated pointer.
- **Construct AppState on a dedicated thread**: added `AppState::new_boxed()` that spawns a 32 MiB-stack thread to run `Box::new(AppState::default())`, avoiding the main-thread stack overflow that occurred on macOS when the large struct + `generate_handler!` frame exceeded the default stack limit.
- **Add macOS/Linux 32 MB main-thread stack size**: emit `-Wl,-stack_size,0x2000000` (macOS) and `-Wl,-z,stacksize=33554432` (Linux) via `cargo:rustc-link-arg-bins` in `build.rs`.  Using `rustc-link-arg-bins` instead of target-wide `rustflags` in `.cargo/config.toml` ensures the flag applies only to the final executable — ld64 rejects `-stack_size` when linking dylibs/cdylibs (the lib crate), which caused `ld: -stack_size option can only be used when linking a main executable`.
- **Extract `setup_app()` / `setup_background_tasks()`**: moved the ~650-line `.setup()` closure body and the updater/DND poll loops into separate `#[inline(never)]` top-level functions so LLVM cannot merge their stack frames with the already-huge `run()` frame produced by `generate_handler!` with ~150 commands.
- **Dynamic stack growth via `stacker`**: added `stacker = "0.1"` dependency and wrapped the `skill_lib::run()` call in `main()` with `stacker::maybe_grow(32 MiB, 64 MiB, ...)`.  This dynamically extends the main-thread stack using `mmap` + inline-asm stack-pointer swap (via `psm`) without changing the thread identity, which is required on macOS where Cocoa/AppKit mandates the event loop runs on the original main thread.  Linker flags (`-Wl,-stack_size`) were unreliable because macOS ld64 rejects them on dylibs and Tauri's mixed `crate-type = ["staticlib", "cdylib", "rlib"]` build triggers both lib and bin linking.

### Build / Tooling

- **Release notes now include changelog section**: all tagged release workflows (`.github/workflows/release-linux.yml`, `.github/workflows/release-mac.yml`, `.github/workflows/release-windows.yml`) now extract the matching `## [x.y.z]` block from `CHANGELOG.md` and pass it to `softprops/action-gh-release` via `body_path`, so GitHub Release information includes the version-specific changelog alongside generated release notes.

- **Preview artifacts now include changelog notes**: `.github/workflows/pr-build.yml` now generates `preview-notes.md` from the matching `CHANGELOG.md` `## [x.y.z]` section (based on `tauri.conf.json` version) and uploads it with the preview DMG/updater artifacts so pre-release testers get version-specific notes with each build.

- **Windows NSIS discovery false-negative fix**: corrected `scripts/create-windows-nsis.ps1` NSIS lookup when `makensis` is already on PATH. The script previously used `Split-Path` twice on `Get-Command makensis`, which resolved to the parent of the NSIS directory and could incorrectly fail `makensis.exe` checks. It now uses the direct parent folder and also accepts `NSIS_DIR` set to either the NSIS directory or a full `makensis.exe` path.

- **Windows NSIS PowerShell argument parsing fix**: corrected `scripts/create-windows-nsis.ps1` candidate-path construction to precompute `$TargetBinary`/`$HostBinary` and then build `$BinaryCandidates` from variables. This avoids a PowerShell parse/invocation edge case where comma-separated inline `Join-Path` calls inside `@(...)` could be interpreted as an array passed to `-ChildPath`, causing `Cannot convert 'System.Object[]' to the type 'System.String'`.

- **Windows NSIS standalone packaging path fallback**: `scripts/create-windows-nsis.ps1` now auto-detects the prebuilt release binary from either `src-tauri/target/x86_64-pc-windows-msvc/release/skill.exe` (explicit target build) or `src-tauri/target/release/skill.exe` (default host-target build). This fixes `npm run tauri:build:win:nsis` failing after a successful `tauri build --no-bundle` when Rust outputs the host-layout path; the script now also places NSIS output under the detected release directory's `bundle/nsis` folder.

- **macOS `.app` manual assembly fallback**: when the Tauri CLI bundler process itself stack-overflows (exit 134 SIGABRT or 139 SIGSEGV) during the `--bundles app` phase — which is a Tauri CLI issue, not the app binary — `scripts/tauri-build.js` now detects the crash, verifies the release binary was already built, and assembles the `.app` bundle manually using `ditto`, `codesign --force --deep --sign -`, the project's `Info.plist`, icons, entitlements, and resources from `tauri.conf.json`.  This makes `npm run tauri:build:mac -- --bundles app` reliable even when the Tauri CLI has stack issues.
- **Standalone macOS `.app` assembler**: added `scripts/assemble-macos-app.sh` that builds the `.app` directory structure from a pre-built release binary without invoking the Tauri CLI bundler at all.  New npm script `npm run tauri:build:mac:app` compiles with `--no-bundle` then runs the assembler.  Copies binary, merges `Info.plist` with required `CFBundle*` keys (including `CFBundleIconFile` and `NSHighResolutionCapable`), copies `icon.icns` + resources via `ditto`, and ad-hoc codesigns.
- **macOS DMG creator**: replaced the custom 800-line `scripts/create-macos-dmg.sh` with a single-pass [`appdmg`](https://github.com/LinusU/node-appdmg) approach.  Generates a branded background image (app icon + product name + version, 660×520 @1x + @2x Retina, dark/light mode adaptive) and a version-badged volume icon (`.icns` with "v0.0.28" pill overlay) via Pillow, then calls `appdmg` with a full spec: app + Applications symlink (top row), README.md + LICENSE + CHANGELOG.md (bottom row), icon positions, window size, ULFO+APFS format.  `appdmg` handles Finder view setup via AppleScript in one pass — no `hdiutil convert` round-trips that corrupt APFS volumes, no Python `ds_store`/`mac_alias` that crash Finder, no `hdiutil udifrez` SLA that corrupts DMGs on macOS 14+.  Both `release-mac.yml` and `pr-build.yml` CI workflows install `appdmg` + Pillow and use the shared script.  Also fixed `assemble-macos-app.sh` Info.plist generation: replaced regex string injection with `plistlib` so `NSHighResolutionCapable` is a proper boolean `<true/>` (not `<string>true</string>`) and added `CFBundleName`, `CFBundleDisplayName`, `LSMinimumSystemVersion`, `NSRequiresAquaSystemAppearance` keys.
- **Windows NSIS installer script**: added `scripts/create-windows-nsis.ps1` and `npm run tauri:build:win:nsis` for standalone Windows NSIS installer creation that bypasses the Tauri CLI bundler.  Generates branded installer images (header 150×57, welcome panel 164×314 with app icon + version via Pillow), bundles `README.md`, `CHANGELOG.md`, `LICENSE`, resources (espeak-ng-data, neutts-samples), creates Start Menu + Desktop shortcuts, registers in Add/Remove Programs, and optionally signs with `signtool.exe` via `CERTIFICATE_THUMBPRINT`.  The GPL-3.0 `LICENSE` is shown as a license agreement page during installation.

## [0.0.27]

### Bug Fixes

- **Feature-gated compilation for `--no-default-features` builds**: added `#[cfg(feature = "llm")]` / `#[cfg(not(feature = "llm"))]` guards in `lib.rs` (stub `llm_state_cell` field + Default impl), `api.rs` (conditional `Mutex` import), `settings_cmds.rs` (split `set_llm_config` into feature-gated paths), `tray.rs` (`ellipsize_middle` helper), and `llm/mod.rs` (`allowed_tools` field); added `#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]` guards in `tts/mod.rs` for imports, helpers, constants, and `impl` blocks so the crate compiles cleanly when built with `--no-default-features` or any subset of feature flags.

### Dependencies

- **Auto-enable GPU backend for LLM by platform**: `llama-cpp-4` now receives `metal` on macOS and `vulkan` on Linux/Windows via target-specific dependency feature merging in `Cargo.toml`, so the `llm` feature automatically uses the correct GPU backend without requiring manual `--features llm-metal` / `--features llm-vulkan` flags.
- **Bump `llama-cpp-4` from 0.2.9 → 0.2.10** (and `llama-cpp-sys-4` 0.2.9 → 0.2.10).

### Build / Tooling

- **Fix CI release binaries missing embedded frontend**: release and preview workflows that bypass the Tauri CLI (`cargo build --release` directly) were producing dev-mode binaries that attempted to load the UI from `localhost:1420` instead of serving the embedded SvelteKit build output.  Root cause: the Tauri crate gates frontend embedding behind its `custom-protocol` Cargo feature, which `npx tauri build` activates automatically but raw `cargo build` does not.  Added a `custom-protocol` feature to `src-tauri/Cargo.toml` forwarding to `tauri/custom-protocol` and pass `--features custom-protocol` in `release-linux.yml`, `release-mac.yml`, `pr-build.yml`, and `release-windows.ps1`.
- `npm run bump` now also rotates the changelog release header automatically: it preserves a fresh `## [Unreleased]` section and inserts `## [x.y.z] — YYYY-MM-DD` for the newly bumped version.
- macOS local Tauri build stability: `scripts/tauri-build.js` now injects `--no-bundle` by default for `build` runs (while still forcing `--target aarch64-apple-darwin --no-sign`), unless the caller explicitly passes `--bundle`/`--bundles`/`--no-bundle`; this avoids post-compile bundle-phase crashes where `npx tauri build --target aarch64-apple-darwin --no-sign` fails but `--no-bundle` succeeds.

### Features

- **Hooks lifecycle completeness pass**: hook triggers now surface full runtime context (last-trigger time, matched label, and one-click session open), emit both in-app toast + native OS notification payloads, and persist immutable trigger snapshots into dedicated `hooks.sqlite` JSON audit rows; the trigger path runs in the background embedding worker with panic isolation and dedicated `hooks` logger toggles, while docs/tests/examples/locales were updated together (`SKILL.md`, `cli.ts`, `test.ts`, Help/FAQ + flow diagram, and `en`/`de`/`fr`/`he`/`uk` translations).
- **Proactive Hooks rename + scenarios**: renamed user-facing Hooks copy from “Automation Hooks” to **Proactive Hooks** and added per-hook scenario modes (`any`, `cognitive`, `emotional`, `physical`) so triggers can be gated by live state metrics in the background worker.
- **Hooks keyword picker keyboard UX**: keyword suggestions now support keyboard navigation (`↑` / `↓` / `Enter` / `Esc`) in addition to click-to-apply.
- **Hooks quick examples**: added one-click starter scenarios (cognitive deep-work guard, emotional calm recovery, physical body-break) to speed up hook creation.
- **Hooks keyword suggestions while typing**: Settings → Hooks now shows live keyword suggestions in the add-keyword flow by combining fuzzy matches from `labels.sqlite` with semantic nearest-label hits from the label text HNSW index; suggestion chips include source tags (`fuzzy`, `semantic`, or `fuzzy+semantic`) and can be clicked to add quickly.
- **Hooks button text-fit polish**: small action buttons in Hooks now use wrap-safe sizing (`h-auto` + multiline text) so localized labels fit without clipping.
- **Hooks scenario dropdown theming polish**: scenario selector now uses themed custom select styling (`appearance-none`, semantic border/ring tokens, custom chevron) for consistent dark/light appearance.
- **Hooks heading naming tweak**: Hooks tab card heading now uses the concise localized tab label ("Hooks") instead of longer variant text.
- **Settings sidebar resize**: Settings tab navigation sidebar is now mouse-resizable with a drag handle, bounded min/max width, and persisted width between opens.
- **Settings titlebar clarity**: settings window title now always includes localized “Settings” plus the active tab name (for example “Settings — Hooks”).

- **Hook distance suggestion**: new "Suggest threshold" button in Settings → Hooks that analyses real HNSW and SQLite data — finds labels matching the hook's keywords, computes cosine-distance distribution of recent EEG embeddings against those label references, and presents a percentile bar (min/p25/p50/p75/max) with a one-click "Apply" action to set the recommended threshold.
- **Hooks WS/CLI observability expansion**: added websocket commands `hooks_suggest` and `hooks_log`, plus CLI subcommands `hooks suggest` and `hooks log` (limit/offset pagination) for scriptable threshold recommendations and audit-log inspection over either WebSocket or HTTP tunnel transport.
- **Hook fire history viewer**: expandable "Hook fire history" section in Settings → Hooks with paginated (20/page) collapsible event rows showing timestamp, label, distance, command, and threshold-at-fire metadata.
- **Last-trigger relative age**: the last-trigger display in Settings → Hooks now shows a live relative-time label (e.g. "12s ago", "3m ago") that updates every second alongside the absolute timestamp.
- Added a new **Settings → Hooks** tab for user-defined automation hooks: each hook supports name, enabled flag, multiple keywords, command payload, custom text payload, configurable EEG distance threshold, and configurable recent-reference count (clamped to 10–20).
- Added backend hook persistence and runtime matching pipeline: hook rules are saved in `settings.json`, hook keyword queries use fuzzy matching plus text-embedding/HNSW nearest-label expansion, then map to recent label-window EEG references; incoming EEG embeddings now trigger websocket broadcasts when close enough, with payload `{ hook, context: "labels", command, text }`.

### Documentation

- **Proactive Hooks docs/examples refresh**: updated `SKILL.md` hooks scenarios and jq examples, refreshed CLI help/output text in `cli.ts` to include scenario metadata, and extended `test.ts` hook status smoke checks to validate `hook.scenario` when hooks exist.
- Added hooks explainers in Help/FAQ including a compact hook flow diagram and a dedicated trigger-mechanics FAQ entry.

### Bug Fixes

- **Single-instance runtime enforcement**: app startup now initializes `tauri-plugin-single-instance`, so opening NeuroSkill while it is already running no longer starts a second process; the existing `main` window is restored/focused instead.

- **Windows CI Rust warning cleanup (`dead_code`)**: removed the non-Linux `linux_has_appindicator_runtime()` stub from `src-tauri/src/lib.rs` so only the Linux implementation is compiled; this eliminates the Windows-only `function is never used` warning while preserving the Linux tray-runtime guard behavior.

### Documentation

- **README Linux packaging quickstart added**: added a concise Development-section command block in `README.md` for Linux release-style local packaging (`tauri:build:linux:x64:native` for AppImage, then `package:linux:system:x64:native -- --skip-build` for manual `.deb`/`.rpm`), including an explicit `ALLOW_LINUX_CROSS=1` cross-target example.
- **Linux setup docs now include tray runtime dependency guidance**: updated `LINUX.md` with a dedicated runtime prerequisite for `tauri dev` (`libayatana-appindicator3-1`, with `libappindicator3-1` fallback) and added troubleshooting steps for the startup error `Failed to load ayatana-appindicator3 or appindicator3 dynamic library`.
- **Linux docs cross-link clarity pass**: added a reciprocal pointer in `LINUX.md` back to `README.md` Development prerequisites and explicit wording that missing appindicator runtime packages can break `npm run tauri dev` at startup.
- **Linux packaging command docs aligned with workflows**: updated the `LINUX.md` build section to recommend the canonical local flow (`npm run tauri:build:linux:x64:native` for AppImage, then `npm run package:linux:system:x64:native -- --skip-build` for `.deb`/`.rpm` via `dpkg-deb`/`rpmbuild`), with cross-target examples when `ALLOW_LINUX_CROSS=1` is intentional.

### Bug Fixes

- **Rust clippy warning cleanup (embeddings/settings)**: marked argument-heavy constructor/spawn entry points in `src-tauri/src/eeg_embeddings.rs` with targeted `#[allow(clippy::too_many_arguments)]` (matching the existing worker rationale), and replaced the manual `Default` implementation for `HookStatus` with `#[derive(Default)]` in `src-tauri/src/settings.rs`.
- **Rust hooks settings compile fix (`E0596`)**: fixed `set_hooks` in `src-tauri/src/settings_cmds.rs` by binding the locked app state as mutable before assigning `s.hooks`, resolving `cannot borrow 's' as mutable, as it is not declared as mutable` during `cargo clippy`/build.
- **Linux tray is now mandatory with fail-fast startup guard**: before tray initialization, startup probes for loadable appindicator shared objects; when `libayatana-appindicator3` / `libappindicator3` is missing, startup aborts immediately with a clear prerequisite error instead of panicking inside `libappindicator-sys` or running without tray.
- **Linux `tauri dev` tray-runtime preflight**: `scripts/tauri-build.js` now checks for a loadable appindicator runtime (`libayatana-appindicator3.so*` or `libappindicator3.so*`) before launching `npx tauri dev`; when missing, it exits early with distro-aware install guidance (`apt`/`dnf`/`pacman`/`zypper`) instead of letting the app crash at startup with a `libappindicator-sys` panic.
- **`npm run bump` Linux preflight dependency clarity**: added an explicit `pkg-config` guard before `cargo clippy` in `scripts/bump.js` that checks `webkit2gtk-4.1`, `javascriptcoregtk-4.1`, and `libsoup-3.0`; when missing, bump now fails fast with actionable `apt install` guidance instead of surfacing a lower-level `webkit2gtk-sys` build-script crash.
- **Strictest non-status accent normalization (UMAP/Embeddings)**: removed remaining category-only orange/sky/emerald/violet highlight styling in UMAP and Embeddings controls (preset chips, pipeline badges, slider thumb/focus affordance, and dimension legend badges) in favor of semantic `primary` / `ring` tokens so generic interactive emphasis consistently follows Appearance accent settings.
- **Strict accent policy completion for generic selectors**: updated the remaining non-status selected controls in Calibration profile editing (break-duration and iterations chips) to use semantic `primary` tokens instead of hardcoded `amber`/`emerald`, and clarified `AGENTS.md` guidance that semantic status colors remain allowed only for true status signaling.
- **Follow-up accent normalization for non-status highlights**: converted remaining generic hardcoded `rose`/`emerald` selection and focus styles (UMAP timeout/cooldown controls, EEG overlap selector summary badges, and interactive search query focus ring) to semantic `primary` / `ring` tokens, while leaving semantic success/warning/error colors unchanged.
- **Broader accent-token consistency sweep**: replaced numerous hardcoded interactive blue states (selected chips/buttons, focus rings, and status badges) with semantic `primary` / `ring` tokens across Appearance, Settings, Focus Timer, History, Labels, Calibration, API, Search, and related tabs so accent-like UI feedback consistently follows the Appearance accent mapping.
- **Accent setting now applies to native form controls and remaining interactive toggles**: added a global `accent-color` base rule tied to the remapped accent palette so checkboxes/radios/ranges/progress controls follow the selected Appearance accent, and replaced remaining hardcoded non-remapped accent classes in interactive Search/UMAP controls.
- **Updater fallback on install failure**: when automatic update download/install fails in the Updates tab, the UI now gives an explicit "download online" fallback and automatically opens the latest GitHub releases page (`https://github.com/NeuroSkill-com/skill/releases/latest`) so users can immediately fetch the newest installer manually.
- **macOS white screen on first launch**: `win.show()` was called in Tauri's `setup` closure before WKWebView had loaded any content, producing a solid white frame until the next compositor cycle.  Fixed by removing the eager `setup` show and adding a new `show_main_window` Tauri command that is invoked from `+layout.svelte` `onMount`; the window now becomes visible only after the page has fully rendered.  Secondary windows (settings, help, calibration, etc.) and the new-user onboarding flow are unaffected — `show_main_window` is a no-op for any window whose label isn't `"main"` or whose onboarding flag is unset.
- **What's New version picker theme mismatch**: the navigation dropdown in `/whats-new` used transparent/native select styling that could ignore app theme colors in the standalone window. The picker now uses explicit themed control styles (`appearance-none`, theme-aware background/border/text) plus a custom caret so light/dark appearance matches the rest of the UI.
- **Appearance accent color not applied consistently across UI**: accent selection previously remapped only Tailwind `violet-*` variables, while many controls and gradients used `blue-*`, `indigo-*`, or `sky-*` classes and stayed on default hues. Accent application now remaps those accent-like families together so interactive highlights, rings, sliders, and accent gradients consistently follow the selected Appearance accent.

### CI Runtime

- Windows release workflow reliability fix: `.github/workflows/release-windows.yml` now uses ASCII-safe Discord title strings in the notify step to avoid Windows PowerShell parser/encoding failures, and the Tauri build step now runs with `--verbose` plus bundle-directory diagnostics when `npx tauri build` exits non-zero (so packaging failures surface actionable logs instead of a bare exit code).
- Linux release workflow now bypasses Tauri bundling entirely (macOS-style): it compiles frontend + Rust only, builds `.deb`/`.rpm` via `scripts/package-linux-system-bundles.sh`, builds the portable Linux tarball via `scripts/package-linux-dist.sh`, signs those outputs with `tauri signer`, and publishes updater metadata from the signed portable tarball instead of AppImage bundle artifacts.
- CI Linux packaging scope reduced to tarball-only in `.github/workflows/ci.yml`: removed the `linux-release` job that produced `.deb`/`.rpm`/`.AppImage`, so Linux CI now only runs the portable package flow and publishes `.tar.gz` artifacts.
- Tauri frontend bundling contract guard: added `scripts/verify-tauri-frontend-structure.js` and wired it into `npm run build` (`package.json`) so `tauri build` (via `beforeBuildCommand`) now fails fast unless the configured `src-tauri/tauri.conf.json` `build.frontendDist` path contains valid built assets (`index.html` + `_app/immutable` JS/CSS) rather than raw source files.
- Linux/macOS/Windows bundling workflows now run an explicit `npm run -s verify:tauri:frontend` step before packaging (`.github/workflows/ci.yml`, `.github/workflows/release-linux.yml`, `.github/workflows/release-mac.yml`, `.github/workflows/release-windows.yml`) to enforce the same Tauri asset layout contract in CI.
- Windows release Discord notifier fix: `.github/workflows/release-windows.yml` now sends the Discord payload from a PowerShell object serialized via `ConvertTo-Json` (instead of shell-escaped inline JSON), eliminating Discord API `50109` (`The request body contains invalid JSON`) failures after successful Windows builds.
- Windows release post-build hardening: `.github/workflows/release-windows.yml` now updates `latest.json` with native PowerShell (no `python3` dependency in Git Bash on `windows-latest`) and skips the Discord notification step when `DISCORD_WEBHOOK_URL` is unset, avoiding non-build-related exit failures after successful Windows artifact compilation.
- macOS release bundle frontend integrity: `.github/workflows/release-mac.yml` now copies the generated SvelteKit `build/` output into `Contents/Resources/app` with `ditto` during manual `.app` assembly and fails fast if `build/index.html`, copied `index.html`, copied `_app/immutable`, or copied JS/CSS assets are missing, preventing release artifacts that omit frontend HTML/JS/CSS/static files.
- Linux CI + release packaging now avoids Tauri for `.deb`/`.rpm`: both `.github/workflows/ci.yml` and `.github/workflows/release-linux.yml` build only AppImage via `tauri-build.js --bundles appimage`, then run `scripts/package-linux-system-bundles.sh` to generate `.deb` with `dpkg-deb` and `.rpm` with `rpmbuild`; this removes Tauri Linux deb/rpm bundler segfaults from automated Linux build paths while keeping artifact outputs unchanged.
- Linux workflow/script consistency pass: `package.json` Linux Tauri scripts (`tauri:build:linux:arm64`, `tauri:build:linux:x64:native`, `tauri:build:linux:x64`) now target AppImage-only bundling, and both Linux workflows call the npm script entrypoint for the AppImage build before running manual system-tool `.deb`/`.rpm` packaging.
- Linux CI/release workflow hardening: added native Linux x86_64 npm scripts (`tauri:build:linux:x64:native`, `package:linux:portable:x64:native`) and switched `.github/workflows/ci.yml` + `.github/workflows/release-linux.yml` to those scripts so hosted x86_64 runners no longer depend on `ALLOW_LINUX_CROSS` cross-mode execution paths.
- Linux CI execution policy refinement: in `.github/workflows/ci.yml`, heavy Linux bundling jobs (`linux-release` and `linux-portable-package`) now run by default on `push`, and can be explicitly enabled for manual `workflow_dispatch` runs via `run_linux_bundles=true`, keeping pull-request CI focused on faster validation.
- Updated GitHub Actions workflows to Node 24-ready action versions across CI and release workflows: `actions/checkout` → `v6`, `actions/setup-node` → `v6`, `actions/cache` → `v5`, and `Swatinem/rust-cache` → `v2.9.0`, removing the GitHub deprecation warnings about Node 20-based actions.
- Removed the Linux Rust job's apt archive cache from `.github/workflows/ci.yml`; that cache was low-value on hosted runners and was the most likely source of the `/usr/bin/tar` post-job save failure that was making the Rust CI job noisy or red despite successful build steps.
- Reintroduced Linux Tauri system dependency caching in CI and Linux release workflows via `awalsh128/cache-apt-pkgs-action` (`.github/workflows/ci.yml`, `.github/workflows/release-linux.yml`) so WebKit/GTK build dependencies are restored from cache instead of re-downloaded on every run.

### UI / Type Safety

- **Settings window width bump**: increased the default Settings window width from `680` to `760` (height unchanged) so tabs and controls have more horizontal room; applied consistently to Settings/Model/Updates entry paths that create the shared `settings` window.

### What's New window

- **Full changelog navigation**: the What's New window now parses the entire bundled `CHANGELOG.md` (via Vite `?raw` import) into individual version sections and renders each one with `MarkdownRenderer`; a compact navigation bar between the header and body provides "Newer ←" / "Older →" arrow buttons and a version-picker `<select>` dropdown so users can browse every release entry from a single window; scroll position resets to the top on each navigation step; a `1 / N` counter in the footer shows the current position; new i18n keys (`whatsNew.older`, `whatsNew.newer`, `whatsNew.unreleased`) added to all five locales (en, de, fr, he, uk)


- Reduced the untyped `any` surface in the Three.js-heavy UI components by introducing explicit typed scene/object wrappers in `src/lib/UmapViewer3D.svelte` and `src/lib/InteractiveGraph3D.svelte`; removed broad `any` refs and `@ts-ignore`, and kept behavior unchanged while making future refactors compile-time safer.

### i18n (0.0.4)

- Localized updater fallback messaging across all shipped locales (`en`, `de`, `fr`, `he`, `uk`) by adding translated keys for: (1) automatic-update install failure with online download guidance, and (2) failure to auto-open the download page; `UpdatesTab.svelte` now uses i18n keys instead of hardcoded English strings for both paths.
- Fixed a locale key-sync detection edge case for `de`, `fr`, `he`, and `uk`: normalized `llm.tools.locationDesc`, `llm.tools.webSearchDesc`, and `llm.tools.webFetchDesc` entries to standard `"key": "value"` formatting so `scripts/sync-i18n.ts --check` correctly counts them
- Ran `scripts/sync-i18n.ts --fix` to auto-backfill 138 missing keys in `src/lib/i18n/he.ts` with English fallbacks, restoring locale key-count parity (`2237` keys) so `npm run sync:i18n:check` passes.
- Completed German fallback translation coverage in [src/lib/i18n/de.ts](src/lib/i18n/de.ts) for the auto-synced OpenBCI/LLM/chat/help/downloads blocks and removed stale in-file TODO translation markers in that locale.
- Completed French/Hebrew/Ukrainian fallback translation coverage in [src/lib/i18n/fr.ts](src/lib/i18n/fr.ts), [src/lib/i18n/he.ts](src/lib/i18n/he.ts), and [src/lib/i18n/uk.ts](src/lib/i18n/uk.ts) for the same auto-synced OpenBCI/LLM/chat/help/downloads blocks, and removed stale in-file TODO translation markers.
- Fixed French placeholder consistency regression in [src/lib/i18n/fr.ts](src/lib/i18n/fr.ts) by restoring `llm.size` interpolation token to `{gb}` so runtime formatting and placeholder-consistency tests align.

### Focus / DND

- Linux Do Not Disturb automation support: implemented real Linux backend behavior in `src-tauri/src/dnd.rs` instead of non-macOS no-ops, with GNOME integration via `gsettings org.gnome.desktop.notifications show-banners` and KDE integration via `qdbus(6)` `org.kde.osdService.setDoNotDisturb`; OS-state polling now reports Linux DND state when detectable
- Linux DND fallback path: when GNOME and KDE DND APIs are unavailable, the backend now falls back to `xdg-desktop-portal` inhibit requests (`gdbus` to `org.freedesktop.portal.Inhibit`) with tracked request-handle lifecycle so disable calls close previously created portal requests
- Windows Do Not Disturb automation support: implemented a Windows backend in `src-tauri/src/dnd.rs` using the per-user notification banner toggle (`HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\PushNotifications\\ToastEnabled`) for enable/disable and OS-state query so focus automation works on Windows as well

### Repo hygiene

- Cleaned editor hygiene warnings by switching release workflows away from fragile cross-step `${{ env.* }}` references in expression contexts, and by normalizing historical changelog markdown structure so repo diagnostics stay quiet.
- Fixed a Rust docs lint warning (`clippy::doc_lazy_continuation`) in [src-tauri/src/dnd.rs](src-tauri/src/dnd.rs) by splitting the Linux bullet list and the Windows support note into separate rustdoc paragraphs.

### Build / CI (Unreleased)

- macOS `aarch64-apple-darwin` Tauri build fix: moved `MACOSX_DEPLOYMENT_TARGET` and `CMAKE_OSX_DEPLOYMENT_TARGET` into top-level Cargo `[env]` scope in `src-tauri/.cargo/config.toml` (they were accidentally nested under `[target.i686-pc-windows-gnu.env]`), so `llama-cpp-sys` now receives a 10.15 deployment target and avoids `std::filesystem` availability errors (`'path' is unavailable: introduced in macOS 10.15`) during CMake/C++ compilation.
- `npm run bump` now runs mandatory preflight gates before mutating versions: `npm run check`, `cargo clippy --manifest-path src-tauri/Cargo.toml`, then `npm run sync:i18n:check`; if any step fails, bump exits immediately and does not update version fields.
- Linux CI bundle stability: `scripts/tauri-build.js` now detects a Tauri CLI segfault (`exit 139`) during explicit multi-target bundle runs (for example `--bundles deb,appimage`) and automatically retries bundling sequentially per target so release jobs can still produce both `.deb` and `.AppImage` artifacts
- Linux CI single-target bundle stability: when an explicit Linux bundle run (for example `--bundles deb`) exits with `139`, `scripts/tauri-build.js` now verifies the expected bundle output for that target and treats the run as successful only if artifacts are present; the same artifact-aware tolerance is also applied per-target during sequential retry after a multi-target segfault.
- Linux CI per-target recovery hardened: when a Linux `tauri build --bundles <target>` run exits `139` before writing bundle artifacts, `scripts/tauri-build.js` now retries that target with `tauri bundle --bundles <target>` and only fails if expected artifacts are still missing after the fallback path.
- Linux CI release-bundle smoke test now fails if no `.deb` package is produced: `.github/workflows/ci.yml` verifies at least one `.deb` exists after bundling and checks both the explicit target-triple bundle path and fallback non-target path to catch segfault-recovery path regressions.
- Linux ARM64 build fallback (macOS-style crash isolation): for explicit bundle builds where Tauri crashes with `139`/`134` but the release binary already exists, `scripts/tauri-build.js` now exits successfully in compile-only mode and prints guidance; set `DISABLE_LINUX_CRASH_COMPILE_FALLBACK=1` to force hard failure.
- Added standalone Linux distribution packaging script `scripts/package-linux-dist.sh` to avoid Tauri bundling: it builds with `--no-bundle`, assembles `NeuroSkill/` (binary, bundled resources, launcher, icon, desktop entry, docs), and emits a portable `tar.gz` archive under `dist/linux/<target>/`.
- Added CI portable-package job in `.github/workflows/ci.yml`: `linux-portable-package` now runs `npm run package:linux:portable:x64`, verifies the generated `dist/linux/x86_64-unknown-linux-gnu/*.tar.gz`, and uploads it as a GitHub Actions artifact (`linux-portable-x86_64`).
- Added Linux `.deb` artifact upload in CI: the `linux-release` job in `.github/workflows/ci.yml` now resolves the generated package from the target/fallback bundle paths and uploads it as `linux-deb-x86_64` for direct download from Actions runs.
- Linux package matrix expanded to include `rpm`: Linux build scripts now request `--bundles deb,appimage,rpm`, and both CI/release workflows were updated to validate and publish `.rpm` alongside `.deb` and `.AppImage` artifacts.
- Added Linux integrity sidecars: workflows now generate `SHA256SUMS` files for Linux bundle outputs and portable tarball outputs, and `release-linux` now also generates detached `.sig` signatures for Linux release artifacts.
- Linux release stale-artifact guard: `.github/workflows/release-linux.yml` now removes cached `src-tauri/target/x86_64-unknown-linux-gnu/release/{bundle,skill}` and `dist/linux/x86_64-unknown-linux-gnu` before compile/package steps so rust-cache leftovers cannot be mistaken for fresh artifacts when assembling release outputs.
- Linux CI parity stale-artifact guard: `.github/workflows/ci.yml` now performs the same pre-build cleanup in the `linux-portable-package` job, clearing cached `target`/`dist` Linux output paths before packaging so uploaded CI tarballs always come from the current run.

## [0.0.24] — 2026-03-12

### UI

- Label window titlebar spacing + vertical fit: moved the add-label window title back to the side, rendered the EEG timer as a padded centered capsule in the shared titlebar, and changed `/label` from `h-screen` to `h-full min-h-0` so the bottom action row no longer gets clipped under the custom titlebar layout
- What's New window vertical fit fix: changed `/whats-new` root container from `h-screen` to `h-full min-h-0` and marked the changelog body as `min-h-0` so the shared custom titlebar no longer pushes the footer off-screen and the bottom `Got it` button remains visible
- Window vertical-fit sweep: switched the remaining titlebar-hosted route roots (`/`, `/about`, `/api`, `/calibration`, `/chat`, `/compare`, `/downloads`, `/focus-timer`, `/help`, `/history`, `/labels`, `/onboarding`, `/session`, `/settings`) from viewport height to parent-constrained height, adding `min-h-0` to the key scroll containers where needed so shared custom-titlebar layouts no longer clip bottom content or footers
- Search window titlebar center alignment: moved the mode segmented control to a true centered position in the shared titlebar (absolute center anchoring), increased control width budget, and tuned spacing/typography so all mode buttons render fully and stay visually aligned
- Label window titlebar timer: moved the live EEG-window elapsed timer from the add-label page header into the shared `CustomTitleBar` center area via a new `label-titlebar.svelte.ts` reactive store, removing the duplicate in-content strip while keeping the timer live
- Search window vertical fit fix: changed `/search` root container from `h-screen` to `h-full min-h-0` so it honors the `#main-content` constrained height under the custom 30px titlebar and no longer overflows/clips at the bottom
- Search window titlebar button rendering fix: updated the shared `CustomTitleBar` search layout to be shrink-safe (`search-window-head` + `search-mode-switch` now flex responsively, title truncates with ellipsis, and mode buttons use equal-width flex sizing) so all search mode buttons render reliably instead of clipping on narrower windows/locales
- History window titlebar consolidation: moved clock icon, title text, day pagination (prev/next + label + position indicator), compare toggle, labels toggle, and reload button from the in-page header into the shared custom titlebar via a new `history-titlebar.svelte.ts` reactive store and callbacks; the in-page header strip is removed and the history page retains only the labels browser panel and scroll content
- Help window titlebar consolidation: moved the search input, version badge, license label, ThemeToggle, and LanguagePicker from the in-page top bar into the shared custom titlebar via a new `help-search-state.svelte.ts` reactive store; the redundant in-page header strip is removed and the search state is shared between the help page and the titlebar seamlessly
- Fixed all windows being clipped at the bottom by exactly the custom titlebar height (30 px): `#main-content` now uses `box-sizing: border-box; height: 100vh` so the `padding-top: 30px` offset is contained within the viewport height rather than overflowing beneath the body's `overflow: hidden` boundary
- Settings window titlebar consolidation: moved the Settings title label, Help button, ThemeToggle, and LanguagePicker from the in-page top bar into the shared custom titlebar; the redundant in-page header strip is removed and the Help button is shown in the titlebar actions whenever the settings window is active
- API Status window: moved title and Refresh button from the in-page header into the shared custom titlebar; the title bar now shows the window title on all platforms, with a refresh icon button next to ThemeToggle and LanguagePicker; the in-page header section is removed
- Search window titlebar alignment: moved Search title and mode toggle buttons (EEG/Text/Interactive) from the in-content header into the shared custom titlebar, with mode switching synchronized between the titlebar and `/search` content
- Updated the shared custom titlebar to show each non-main window title in the titlebar itself and to scope main-only titlebar actions (label/history) to the main window; non-main windows now keep lightweight titlebar controls (theme/language + window controls)
- Removed duplicate in-content title bars from all secondary windows (about, compare, whats-new, focus-timer, session, labels, search, history, calibration, label, onboarding, chat); functional header controls (mode buttons, day pagination, compare toggle, recording badge, elapsed timer, TTS indicator) are preserved in-place while redundant title text, drag regions, and theme/language buttons are removed
- Added global themed scrollbar styling for app scroll containers so Windows windows no longer show default system scrollbars; includes light/dark variants and automatic fallback to system colors in forced-colors mode

### LLM

- Moved per-session LLM transcript files into a dedicated `~/.skill/llm_logs` directory (`skill_dir/llm_logs/llm_<unix-seconds>.txt`) so all LLM logs live in a standalone folder instead of the `skill_dir` root.
- Added i18n translations for all LLM built-in tool toggle labels and descriptions across all five supported locales (en, de, fr, he, uk); `TOOL_ROWS` in `LlmTab.svelte` is now a reactive `$derived` so labels update instantly on language change
- Added per-tool allow-list settings for LLM chat in Settings → LLM; `date`, `location`, `web_search`, and `web_fetch` can now be enabled or disabled individually, and running chat requests pick up the updated tool allow-list immediately
- Multimodal projector selection now stays attached to a compatible downloaded text model instead of behaving like a standalone model; selecting an `mmproj` can auto-pair to a matching downloaded LLM, incompatible projector selections are cleared when the base model changes, and startup now honors the resolved projector path when autoload is enabled
- Added simple built-in tool-calling support in `POST /v1/chat/completions` with a bounded execution loop for `date`, `location`, `web_search`, and `web_fetch`
- Wired Tauri IPC chat streaming (`chat_completions_ipc`) to the same tool-calling loop so the in-app chat window supports the same built-in tools
- IPC chat now emits incremental visible `delta` chunks while tool-calling runs, using a stream sanitizer that suppresses `[TOOL_CALL]...[/TOOL_CALL]` blocks from the UI
- Added tool schema injection and `[TOOL_CALL]...[/TOOL_CALL]` handling so models can call tools and continue generation with tool results
- Added basic external fetch/search integrations (`ipwho.is`, DuckDuckGo instant answer API, and HTTP(S) page fetch) with bounded payload truncation for safe prompt context

### Dependencies (0.0.17)

- `llama-cpp-4` `0.2.7` → `0.2.9` (with matching `llama-cpp-sys-4` lockfile update)

### Build / CI

- Windows release workflow stability fix: `.github/workflows/release-windows.yml` now generates the temporary Tauri `--config` JSON via PowerShell (`ConvertTo-Json`) instead of `bash` + `python3`, removing a fragile command-path dependency that could fail the post-compile build step with exit `127` on `windows-latest`
- Linux release artifact generation fixed: `scripts/tauri-build.js` now treats both `--bundle` and `--bundles` (including `--flag=value`) as explicit bundling requests, preventing accidental `--no-bundle` injection that skipped `.deb`/`.AppImage` outputs in CI
- Added explicit Linux bundle-flag guard steps in CI and release workflows to fail fast if `tauri:build:linux:x64` drops `--bundles deb,appimage` or if `scripts/tauri-build.js` stops recognizing `--bundles`
- Added post-build Linux bundle directory sanity checks in CI and release workflows to fail early when `bundle/deb` or `bundle/appimage` is missing

## [0.0.23] — 2026-03-12

### UI / Build (0.0.23)

- **Custom titlebar for all windows** — replaced native window decorations with a custom titlebar component (minimize, maximize, close buttons) for consistent cross-platform appearance on all windows including main, settings, help, search, history, calibration, chat, downloads, and more
- **Unified window close behavior across all platforms** — on all platforms including Linux, closing the main window now hides it instead of exiting. Users must select "Quit" from the tray menu to exit, which shows a confirmation dialog
- **Downloads window total size footer** — the standalone Downloads window now shows the combined size of all listed downloads in a bottom footer for quick storage visibility
- **Downloads footer visibility improved** — clarified the footer label to “Total download size”, added item count, and increased footer emphasis so the summary is easier to notice
- **Downloads status bar placement** — moved the total-size summary from the bottom footer to an always-visible status bar directly under the Downloads header
- **Custom titlebar controls centralized** — titlebar minimize/maximize/close now use a single shared Svelte handler path (no per-window DOM-id listener wiring), improving consistency across windows
- **All windows aligned to shared custom titlebar path** — added missing window-capability labels (`history`, `compare`, `downloads`, `whats-new`), routed shortcut-created Chat/History windows through shared open-window commands, and ensured recreated main window remains undecorated so custom drag/control behavior is uniform
- **Main window titlebar consolidation** — moved language picker, theme toggle, label, and history buttons from the main card header to the titlebar for a cleaner, more accessible layout; buttons remain icon-only and responsive
- **Titlebar spacing refinement** — action buttons (label, history, theme, language) now live on the left side with window controls (minimize, maximize, close) on the right side, utilizing flex layout for proper visual separation
- **Linux cross-target preflight guard** — `scripts/tauri-build.js` now fails fast when a Linux host attempts a non-native `*-unknown-linux-gnu` target (for example ARM host → x86_64) without explicit opt-in, and prints actionable guidance; this avoids long builds ending in `glib-sys` / `gobject-sys` `pkg-config` cross-compilation failures
- **Linux build docs updated for ARM hosts** — added `pkg-config` cross-compilation troubleshooting to `LINUX.md`, including native ARM build command guidance and recommended x86_64 release build strategy
- **Native ARM64 Linux build shortcut** — added `npm run tauri:build:linux:arm64` to run the correct local aarch64 target build (`deb` + `AppImage`, `llm-vulkan`) in one command
- **Explicit Linux x64 cross-build shortcut** — added `npm run tauri:build:linux:x64`, which sets `ALLOW_LINUX_CROSS=1` and then runs the x86_64 target build path; this keeps accidental cross-target builds blocked by default while allowing intentional ones
- **CI Linux build command aligned with npm scripts** — `.github/workflows/ci.yml` now runs `npm run tauri:build:linux:x64` for the Linux release bundle smoke test instead of an inline `npx tauri build ...` command, keeping CI and local build entrypoints consistent
- **Tagged Linux release workflow aligned with npm scripts** — `.github/workflows/release-linux.yml` now also runs `npm run tauri:build:linux:x64` (with existing signing/env vars), replacing the inline `npx tauri build ...` command so both CI and release workflows share the same build entrypoint
- **Workflow intent comments added** — both `.github/workflows/ci.yml` and `.github/workflows/release-linux.yml` now include inline comments noting that `tauri:build:linux:x64` intentionally sets `ALLOW_LINUX_CROSS=1`, reducing accidental regressions to implicit cross-build behavior

### Bug fixes (Linux)

- **Main window close/minimize/maximize buttons unresponsive** — on Linux
  (Wayland + GNOME/Mutter/KWin), window decoration buttons did nothing
  after the window was created with `visible(false)` and later shown;
  this is a known upstream issue (tauri-apps/tauri#11856); worked around
  by toggling fullscreen briefly after every `show()` call on the main
  window (`linux_fix_decorations()`), which forces the Wayland compositor
  to re-evaluate decoration state; applied in initial setup show,
  `show_and_recover_main()`, and `complete_onboarding()`
- **Window event diagnostic logging** — added `[window-event]` and
  `[run-event]` stderr logging for `CloseRequested`, `Destroyed`,
  `Focused`, `Moved`, `Resized`, `ScaleFactorChanged`, and
  `ExitRequested` events across all windows

### Onboarding (0.0.23)

- **Downloads complete success screen** — when all recommended models
  (Qwen3.5 4B, ZUNA encoder, NeuTTS, Kitten TTS) are downloaded, the
  onboarding done step now displays a prominent **green checkmark** with
  a success message and a clickable link to **settings** where users can
  download additional models or switch to alternatives
- **Downloads complete i18n** — added `onboarding.downloadsComplete`,
  `onboarding.downloadsCompleteBody`, and `onboarding.downloadMoreSettings`
  keys to all five locales (en, de, fr, he, uk)

## [0.0.17] — 2026-03-11

### UI / Build (0.0.17)

- **Tailwind Vite parser crash in MarkdownRenderer fixed** — resolved
  `[plugin:@tailwindcss/vite:generate:serve] Invalid declaration: Marked`
  by refactoring `src/lib/MarkdownRenderer.svelte` to use `marked.parse(...)`
  with a local renderer object and removing an empty local `<style>` block
- **MarkdownRenderer regression guard** — added
  `scripts/check-markdown-renderer.js` and wired it into `npm run check`
  so CI/local checks fail if `MarkdownRenderer.svelte` reintroduces
  `new Marked(...)` or a local `<style>` block
- **MarkdownRenderer guard now runs before dev startup** — `npm run dev`,
  `npm run build`, `npm run check:watch`, and `npm run tauri dev` now execute
  the MarkdownRenderer guard before Vite / SvelteKit startup so Tailwind
  parser regressions fail immediately instead of surfacing later from the
  Tailwind Vite pipeline

### Settings

- **Open `skill_dir` from Settings** — Data Directory now includes an
  **Open Folder** action that opens the fixed `~/.skill` directory in the
  system file manager

### Onboarding (0.0.17)

- **Recommended models quick setup** — onboarding now includes a one-click
  **Download Recommended Set** action that pulls the default local stack:
  **Qwen3.5 4B (Q4_K_M)**, **ZUNA encoder**, **NeuTTS**, and **Kitten TTS**
- **Qwen quant preference tightened** — when selecting the onboarding LLM
  target, the wizard now explicitly prefers **Q4_K_M** for Qwen3.5 4B
- **Staged background downloads** — onboarding now starts the recommended
  model downloads in sequence while the user continues setup: ZUNA →
  KittenTTS → NeuTTS → Qwen3.5 4B (`Q4_K_M` target)
- **Persistent footer model status** — all onboarding views now show a subtle
  footer line with staged model setup progress, and the onboarding window was
  enlarged slightly to keep spacing readable
- **Download order configured in Rust constants** — the onboarding queue no
  longer hardcodes download order in Svelte; it now reads the canonical
  sequence from `src-tauri/src/constants.rs`
- **Onboarding models i18n complete** — added the 16 missing
  `onboarding.step.models`, `onboarding.modelsHint`, `onboarding.modelsTitle`,
  `onboarding.modelsBody`, and `onboarding.models.*` keys to all four
  non-English locales (de, fr, he, uk)

### Tray / Downloads

- **LLM download progress in tray icon + menu** — while model files are
  downloading, the system tray now shows progress in the icon itself (a
  prominent circular ring around the tray icon) and in the tray menu
  (active download rows with filename, percent and live status text)
- **Standalone Downloads window** — added a dedicated downloads manager
  window (`/downloads`) that lists all model downloads at any time with
  per-item actions: pause, resume, cancel, and delete
- **Download initiated timestamp** — each download row now includes when it
  was started so long-running and resumed transfers are easier to track
- **Downloads i18n** — new downloads-window labels/status strings added to
  all shipped locales
- **Tray menu shortcut to Downloads** — added a direct **Downloads…** menu
  action in the tray, opening the standalone downloads window in one click

### Dependencies

- `llama-cpp-4` `0.2.6` → `0.2.7`

### CI / Build

- **Linux local `tauri build` segfault avoided** — `scripts/tauri-build.js`
  now injects `--no-bundle` by default for Linux `build` runs when the caller
  does not explicitly pass `--bundle` / `--no-bundle`; this avoids a native
  post-compile crash (status 139) in the Tauri CLI bundling/updater phase
  while still producing the release binary at
  `src-tauri/target/release/skill`

- **Windows release — wrong `link.exe`** — the GitHub-hosted `windows-latest`
  runner places `C:\Program Files\Git\usr\bin` (Git for Windows' Unix `link`
  utility) before the MSVC toolchain in `PATH`; Rust's MSVC backend resolved
  `link.exe` to that Unix binary, which rejected all MSVC linker flags with
  an "extra operand" error; fixed by adding a PowerShell step immediately
  after `ilammy/msvc-dev-cmd` in `release-windows.yml` that strips every
  `Git\usr\bin`-like entry from `PATH` via `$GITHUB_ENV`, ensuring the MSVC
  `link.exe` wins for all subsequent steps

---

## [0.0.16] — 2026-03-11

### EEG / Embeddings

- **Cross-day HNSW index** — similarity search is no longer scoped per-day;
  a persistent cross-day index (rolling 30-day merged index) is maintained
  under `~/.skill`; near-neighbours across months can be found in a single
  query
- **Label fuzzy semantic search** — label search now uses the vendored
  `fast-hnsw` label index for semantic matching in addition to plain-text
  filtering; queries like "find sessions where I felt anxious" surface
  nearest-neighbour label clusters rather than exact string hits

### LLM — Chat

- **Chat history persisted** — conversations are stored in SQLite at
  `~/.skill/chat_history.sqlite`; messages survive closing and reopening
  the chat window
- **Multi-conversation sidebar** — the chat window now has a sidebar listing
  named conversation threads; threads are persisted to disk and can be
  renamed or deleted
- **System prompt editor** — the system prompt is exposed as a text area in
  the chat settings panel so users can bias the model (e.g. "you are a
  neurofeedback coach") without recompiling
- **EEG context injection** — the current `eeg-bands` WebSocket event is
  automatically wired into the system prompt: "User's current focus: 72,
  relaxation: 58, SNR: 14 dB…" so the model can give contextualised advice
- **Prompt library** — a built-in set of neurofeedback prompt templates
  (e.g. "Summarise today's session", "Suggest a relaxation technique",
  "Explain what high theta means") is accessible from a `+` button in the
  chat input

### LLM — Downloads

- **Model download resumption** — interrupted downloads no longer restart
  from zero; the downloader uses `Content-Range` byte-range requests to
  resume from the last received byte

### UMAP Viewer

- **Export PNG / JSON** — "Export PNG" and "Export JSON" buttons added to
  the 3D scatter plot toolbar; PNG captures the current WebGL viewport,
  JSON exports the full point cloud with labels and timestamps

### Focus Timer

- **Session log** — a summary panel shows today's completed Pomodoro cycles,
  total focus time, and total break time; entries are labelled and persisted
  across restarts

### Onboarding (0.0.16)

- **Extended checklist** — onboarding now includes four additional steps:
  download an LLM model, run a similarity search, set a DND threshold, and
  try the REST API; previous four steps preserved

### UI / UX

- **Command Palette — fuzzy scoring** — the palette filter now uses an
  fzf-style scored fuzzy algorithm; partial matches are ranked by relevance
  instead of simple `includes()` containment
- **Theme — custom accent colour** — a primary-hue colour picker has been
  added to Settings → Appearance; the chosen accent is stored in
  `~/.skill/theme.json` and applied app-wide; dark / light / system mode
  selection is unaffected

---

## [0.0.15] — 2026-03-11

### Windows support

- **espeak-ng static build — Windows MSVC** — `scripts/build-espeak-static.ps1`
  builds `espeak-ng.lib` from source using CMake + MSVC on first run, then
  links it statically into the binary; subsequent builds are a no-op.
  Produces a single merged `.lib` (CMake + `lib.exe /OUT`) so that the linker
  sees no duplicate symbol conflicts
- **espeak-ng static build — Windows MinGW/GNU** — `scripts/build-espeak-static-mingw.sh`
  handles the `*-windows-gnu` target ABI (MSYS2 / cross-compile from
  Linux/macOS); output goes to `espeak-static-mingw/` to avoid collisions
  with the MSVC archive
- **`build.rs` — multi-platform espeak dispatch** — selects the correct
  build script and archive name based on `CARGO_CFG_TARGET_ENV`:
  `msvc` → PowerShell `.ps1`, `gnu` on Windows → MinGW `.sh`,
  macOS/Linux → Unix `.sh`; links `-lc++` on macOS, `-lstdc++` on Linux/MinGW,
  and omits the C++ flag on MSVC (runtime linked automatically)
- **`build.rs` — espeak data copy deferred to release** — the espeak-ng data
  directory is no longer copied during `cargo build` / `tauri dev`; the copy
  is skipped in debug builds to break the infinite rebuild loop where
  `build.rs` copies → Tauri watcher detects the change → `cargo run` → repeat.
  An empty placeholder directory is still created so Tauri's resource-path
  validation does not error at startup
- **`fast-hnsw` — vendored with Windows fix** — `memmap2::Mmap::advise()` and
  `memmap2::Advice` are `#[cfg(unix)]` and not available on Windows; the
  `fast_hnsw` crate's unconditional `advise(Advice::Random)` call caused a
  compile error on the MSVC target; patched locally via
  `src-tauri/vendor/fast-hnsw` with the `advise` call wrapped in
  `#[cfg(unix)]`; vendored until upstream releases a fix
- **`WINDOWS.md`** — updated prerequisites: Visual Studio Build Tools 2022
  (**Desktop development with C++** workload, provides `cl.exe`, `lib.exe`,
  Windows SDK) now listed as step 1; CMake doc note updated to cover
  espeak-ng's build system in addition to llama.cpp; added Git as a
  prerequisite for cloning the espeak-ng source; renumbered all steps

### Build / tooling

- **`scripts/tauri-build.js` refactored** — now a general Tauri wrapper that
  handles `dev`, `build`, and any other subcommand; non-`dev`/`build`
  subcommands (e.g. `tauri info`, `tauri signer`) pass straight through without
  triggering an espeak pre-build; platform detection now also covers
  `*-windows-gnu` (MinGW)
- **npm `tauri` script** — `"tauri": "node scripts/tauri-build.js"` routes all
  `npm run tauri …` invocations through the wrapper, so `npm run tauri info`,
  `npm run tauri dev`, `npm run tauri build -- --debug`, etc. all work
  consistently across platforms

### LLM — WebSocket / REST API

- **WebSocket commands** — `llm_status`, `llm_start`, `llm_stop`,
  `llm_catalog`, `llm_download`, `llm_cancel_download`, `llm_delete`,
  `llm_logs` added to the WebSocket command dispatcher (all behind the `llm`
  Cargo feature flag)
- **REST endpoints** — matching HTTP shortcuts added to the axum router in
  `api.rs`:
  - `GET  /llm/status` — running state, active model name, context size, vision flag
  - `POST /llm/start` — load the active model and start the inference server
  - `POST /llm/stop` — stop the inference server and free GPU/CPU resources
  - `GET  /llm/catalog` — model catalog with per-entry download states
  - `POST /llm/download` — start a background model download `{ "filename": "…" }`
  - `POST /llm/cancel_download` — cancel an in-progress download
  - `POST /llm/delete` — delete a locally-cached model file
  - `GET  /llm/logs` — last 500 LLM server log lines
  - `POST /llm/chat` — non-streaming chat completion; body: `{ message, images?, system?, temperature?, max_tokens? }`; returns `{ text, finish_reason, tokens }`
- **`LlmServerState::chat()`** — new method on the server-state actor handle;
  submits a generate request to the actor's channel and returns an
  `UnboundedReceiver<InferToken>` for streaming; returns `Err` immediately if
  the model is still loading or the actor has exited
- **`extract_images_from_messages()`** — helper that decodes all
  `data:<mime>;base64,…` data-URL image parts from an OpenAI-style messages
  array into raw `Vec<u8>` bytes; plain `https://…` URLs are silently skipped;
  call before passing messages to the actor so it receives pre-decoded bytes

### LLM — verbose logging

- **`LlmConfig.verbose`** (`bool`, default `false`) — when `false` (default),
  all internal llama.cpp / ggml and clip logs are silenced; set `true` to
  see raw tensor-load progress and other low-level detail
- **`mtmd_log_set` silence** — `clip_model_loader` uses a separate logger
  (`mtmd_log_set`) that is not affected by `llama_log_set`; the clip logger is
  now silenced via a no-op `extern "C"` callback when `verbose = false`,
  eliminating the tensor-load spam when loading a multimodal projector

### CLI (`cli.ts`)

- **`llm` subcommand group** added:
  - `llm status` — print LLM server status (stopped / loading / running)
  - `llm start` — load the active model and start the inference server
  - `llm stop` — stop the inference server, free GPU memory
  - `llm catalog` — list all catalog models with download states
  - `llm download <filename>` — start a background model download
  - `llm cancel <filename>` — cancel an in-progress download
  - `llm delete <filename>` — delete a cached model file
  - `llm logs` — print the last 500 LLM server log lines
  - `llm chat` — interactive multi-turn chat REPL (WebSocket streaming)
  - `llm chat "message"` — single-shot: send one message and stream the reply
  - `llm chat "message" --image a.jpg --image b.png` — vision: attach images
    (files are base64-encoded and embedded as `image_url` content parts;
    requires a vision-capable model with mmproj loaded)
- **`--image <path>`** — new flag (repeatable) for attaching image files to
  `llm chat` turns
- **`--system <prompt>`** — system prompt prepended as a `{ role: "system" }`
  message for `llm chat`
- **`--max-tokens <n>`** — maximum tokens to generate per turn
- **`--temperature <f>`** — sampling temperature (0 = deterministic, 1 = creative)

### Settings UI

- **Sidebar navigation** — the tab bar in Settings has been replaced with a
  persistent sidebar; each tab entry shows a 24 × 24 stroked SVG icon alongside
  the label and an active-indicator bar
- **Keyboard shortcuts** — `Cmd/Ctrl + 1–9` switch between the first nine
  settings tabs; tooltips on each sidebar item show the shortcut hint

### Help UI

- **Sidebar navigation + search** — the Help window now uses the same sidebar
  layout as Settings; a search box in the top bar filters across all help
  sections with keyboard-navigable results

### Internals

- **`SKILL_DIR` constant** — `src-tauri/src/constants.rs` now exports
  `pub const SKILL_DIR: &str = ".skill"` so the directory name is defined
  in one place; `default_skill_dir()` in `settings.rs` uses it
- **Data directory hardcoded** — the `data_dir` field has been removed from
  persisted settings; the skill directory is always `~/.skill` and is never
  configurable at runtime; `expand_tilde` helper and its tests removed

### Dependencies (0.0.15)

- `kittentts` `0.2.4` → `0.2.5`

---

## [0.0.13] — 2026-03-10

### Onboarding (0.0.13)

- **Recommended models quick setup** — onboarding now starts staged
  background downloads automatically while the user proceeds through steps,
  in this order: ZUNA → KittenTTS → NeuTTS → Qwen 3.5 4B (`Q4_K_M` target)
- **Persistent footer download status** — all onboarding views now show a
  subtle footer line with staged model setup progress (ZUNA, Kitten, NeuTTS,
  LLM), and the onboarding window size was increased to keep spacing readable
  with the always-visible footer indicator

### Dependencies (0.0.13)

- `llama-cpp-4` `0.2.3` → `0.2.5`
- `kittentts` `0.2.2` → `0.2.4`
- `neutts` `0.0.5` → `0.0.7`

### Bug fixes

- **Blank main window after long idle** — after a full day in the system
  tray with the window hidden, macOS can silently terminate WKWebView's
  web-content process under memory pressure, leaving a blank white page
  that only a full app restart could recover from
  - `+layout.svelte` sets `window.__skill_loaded = true` in `onMount` as
    a renderer-liveness sentinel
  - New `show_and_recover_main()` Rust helper checks the sentinel on every
    show via `eval()`; if the flag is absent it triggers `location.reload()`
    (renderer alive but content cleared), and falls back to `navigate()` if
    `eval()` itself returns `Err` (renderer process fully dead, WKWebView
    needs a fresh process spawned)
  - `RunEvent::Reopen` handler added — clicking the macOS Dock icon while
    all windows are hidden now shows the main window and runs the same
    two-layer recovery (previously a silent no-op)

- **Update loop — first check delayed by full interval** — the background
  updater slept `interval_secs` *before* the first check, so with the
  default 1-hour interval the first background check fired ~61 minutes after
  launch; pattern changed to check-then-sleep so the first check fires 30
  seconds after startup as intended

- **Update loop — update silently dropped on CDN race** — when the Rust
  background task emitted `update-available`, the frontend had to re-run
  `check()` to obtain a downloadable `Update` object; if `check()` returned
  `null` (latest.json not yet propagated to all CDN edge nodes), `available`
  was wiped and `phase` reverted to `"idle"` with no user feedback; fixed
  by threading the event payload as a `hint` through `checkAndDownload()` —
  the known version stays visible in the UI during the re-check, and a CDN
  race surfaces an actionable "Retry" error instead of a silent reset

- **What's New — dismiss race with uninitialised version** — `appVersion`
  started as the string `"…"` and was populated asynchronously via IPC;
  clicking "Got it" before the call resolved stored `"…"` in
  `last_seen_whats_new_version`, causing the window to reopen on every
  subsequent launch; fixed by seeding `appVersion` synchronously from the
  CHANGELOG version embedded at build time

- **What's New — markdown not rendered** — changelog entries containing
  `**bold**`, `` `code` `` spans, multi-line bullet continuations, and
  numbered sub-lists were all rendered as plain text; replaced the
  hand-rolled `parseChangelog` parser (which dropped any line not starting
  with `-` plus a trailing space) and the manual `{#each sections}` template with
  `MarkdownRenderer` (existing component backed by `marked` + GFM); scoped
  CSS overrides inside `.wn-body` preserve the compact window style without
  affecting the chat renderer

### Build / CI (0.0.13)

- **CI `cargo check --locked` failing on Linux** — `Cargo.lock` generated
  on macOS caused the Linux CI job to fail with "cannot update the lock file
  because --locked was passed"; added `cargo fetch --target
  x86_64-unknown-linux-gnu` before `cargo check --locked` to resolve
  platform-specific dependencies for Linux without touching the network
  during the check itself

- **Release — single notarization round trip** — the release workflow
  previously issued two separate `xcrun notarytool submit --wait` calls
  (one for the `.app` as a ZIP, one for the DMG), each waiting up to 20+
  minutes; consolidated to a single DMG submission — Apple's service
  registers notarization tickets for all signed content inside the container,
  so `xcrun stapler staple` succeeds on both the DMG and the `.app`
  afterward without a second submission; the updater tarball step is
  reordered to run after the DMG step so it always packages a stapled `.app`

---

## [0.0.11] — 2026-03-10

### LLM / Chat

- **LLM engine** — full on-device inference via `llama-cpp-4` (llama.cpp
  bindings). Runs text and multimodal (vision) models locally with no cloud
  dependency
- **Model catalog** (`llm_catalog.json`) — curated list of GGUF models
  (Qwen3.5 4B/27B, Llama-3.2-Vision, Gemma3, etc.) with per-entry metadata:
  repo, filename, quantisation, size, family description, tags, recommended
  flag. Bundled into the app at compile time
- **Tauri commands**: `get_llm_catalog`, `set_llm_active_model`,
  `set_llm_active_mmproj`, `download_llm_model`, `cancel_llm_download`,
  `delete_llm_model`, `refresh_llm_catalog`, `get_llm_logs`,
  `start_llm_server`, `stop_llm_server`, `get_llm_server_status`,
  `open_chat_window`
- **HTTP inference server** (`axum` router) — OpenAI-compatible endpoints
  (`/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`) served
  locally so third-party tools can connect to the on-device model
- **Vision / multimodal** — image inputs decoded from data-URL or base64
  and fed through a clip mmproj; `autoload_mmproj` setting automatically
  selects the best downloaded projector for the active model
- **Thinking-model support** — forced `</think>` injection after a budget
  cap; orphaned tail tokens are discarded (decoded into KV cache for
  coherence, suppressed from output) until the next clean line boundary
- **File upload** in chat — images attachable to messages; previewed in
  the UI before sending
- **Markdown renderer** (`MarkdownRenderer.svelte`) — renders streamed
  assistant output with code blocks, tables, and inline formatting
- **Chat window** (`src/routes/chat/+page.svelte`) — full chat UI with
  message history, streaming tokens, stop button, model/mmproj selectors,
  generation parameter controls
- **Global chat shortcut** — configurable keyboard shortcut (stored in
  settings) focuses the existing chat window or opens a new one
- **i18n** — `llm.*` keys added to all five language files (en, de, fr,
  he, uk)

### Build / CI (0.0.11)

- **Bypass Tauri's built-in signing pipeline** in both `release.yml` and
  `pr-build.yml` — Tauri's `create-dmg` subprocess crashes with `SIGILL`
  on macOS 26 (hdiutil API change); replaced with explicit steps:
  1. `npx tauri build --bundles app --no-sign` — compile only
  2. `codesign` — deep-sign with `--options runtime` + `--entitlements`
  3. `xcrun notarytool submit … --wait` — notarize
  4. `xcrun stapler staple` — staple ticket to bundle
  5. Recreate `.app.tar.gz` from the signed bundle, then
     `npx tauri signer sign` — re-sign the updater artifact with Ed25519
- `release.sh` — minor fix to `TAURI_TARGET` default propagation

---

## [0.0.9] — 2026-03-10

### Dependencies (0.0.9)

- Migrated `llama-cpp-4` and `llama-cpp-sys-4` to local path via
  `[patch.crates-io]` (`../../../llama-cpp-rs/llama-cpp-4` and
  `../../../llama-cpp-rs/llama-cpp-sys-4`) — ensures the SIGILL fix
  (correct `CMAKE_OSX_ARCHITECTURES` / `CMAKE_CROSSCOMPILING` for Apple
  cross-arch builds) is always active; both the `llm` feature and neutts's
  backbone resolve to the same local crate, preserving the `links = "llama"`
  deduplication

### Build / CI (0.0.9)

- macOS builds now target `aarch64-apple-darwin` (arm64) only — x86_64
  is no longer compiled
  - `tauri:build:mac` npm script passes `--target aarch64-apple-darwin`
  - `release.sh` defaults `TAURI_TARGET` to `aarch64-apple-darwin` (still
    overridable via env var for universal or x86_64 builds)
  - `build-espeak-static.sh` defaults `CMAKE_OSX_ARCHITECTURES` to `arm64`
    instead of the host architecture (still overridable via `ESPEAK_ARCHS`)
  - `.cargo/config.toml` sets `[build] target = "aarch64-apple-darwin"` so
    plain `cargo build` / `cargo check` / `npx tauri build` all default to
    arm64 without requiring an explicit `--target` flag
  - `ci.yml` Linux `cargo check` / `cargo clippy` steps now pass
    `--target x86_64-unknown-linux-gnu` to override the config.toml default;
    espeak build step passes `ESPEAK_ARCHS=x86_64` explicitly
  - `pr-build.yml` and `release.yml` were already correct (`--target
    aarch64-apple-darwin`, `ESPEAK_ARCHS=arm64`)
- Fixed SIGILL crash after successful compile on macOS 26.3 in both local
  and CI builds; root cause traced via lldb + macOS crash report:
  - Tauri's bundled `create-dmg` script spawns `bundle_dmg.sh` as a child
    process which fails on macOS 26 (hdiutil API change); Node.js propagates
    the child's fatal exit as `process.kill(pid, SIGILL)` via
    `ProcessWrap::OnExit` → promise rejection chain
  - Local dev (`tauri:build:mac`): added `--no-sign` — no certificate on dev
    machines, codesign would have failed at the same stage
  - CI (`release.yml`, `pr-build.yml`): replaced `--bundles app,dmg` with
    `--bundles app`; added an explicit "Create DMG" step that uses `hdiutil`
    directly, stamps the version badge, then signs and notarizes — identical
    end result with no dependency on Tauri's create-dmg script
- Fixed pre-commit hook failing on macOS when CUDA Toolkit is absent
  - `cargo clippy --all-features` activated `llm-cuda` and `llm-vulkan`,
    causing `llama-cpp-sys` to pass `-DGGML_CUDA=ON -DGGML_VULKAN=ON` to
    CMake, which hard-errors if no CUDA Toolkit is found
  - Hook now selects platform-appropriate features: `--features llm-metal`
    on macOS, default features on Linux/Windows — CUDA/Vulkan features are
    never activated where their native toolkits are unavailable

---

## [0.0.6] — 2026-03-06

### Do Not Disturb / Focus Mode

- Replaced hand-rolled ObjC FFI + XPC DND implementation with the
  [`macos-focus`](https://crates.io/crates/macos-focus) crate — pure Rust,
  no private frameworks, no special entitlements
- DND now works reliably on macOS 12–15; the XPC path that consistently
  returned "operation failed" errors has been removed
- Added **Focus mode picker** in Settings → Goals: choose any Focus mode
  configured in System Settings (Do Not Disturb, Work, Personal, Sleep,
  Driving, …) rather than always activating Do Not Disturb
- `focus_mode_identifier` persisted in settings; defaults to Do Not Disturb
  for backwards compatibility with existing configs
- Added `list_focus_modes` Tauri command backed by
  `FocusManager::available_modes()`; falls back to the full first-party mode
  list if `ModeConfigurations.json` is unreadable
- Added TODO stubs for Linux (D-Bus / xdg-portal) and Windows
  (WinRT / IQuietHoursSettings) DND support

### Quit Dialog

- macOS quit confirmation dialog now uses `NSAlert` via `objc2-app-kit`
  dispatched through `dispatch2::DispatchQueue::main().exec_sync()`,
  eliminating the `CFUserNotificationDisplayAlert: called from main
  application thread` log warning that `rfd` triggered

### Bug fixes and warnings

- Fixed `CFStringCreateWithCString` / `CFRelease` clashing `extern "C"`
  signatures between `dnd.rs` and `gpu_stats.rs`
- Removed three unnecessary `unsafe {}` blocks around safe `iimp()` closure
  calls in the (now-deleted) ObjC FFI path
- Removed unused `vm_deallocate` extern declaration in `gpu_stats.rs`
- Removed unnecessary `unsafe {}` block wrapping safe `NSAlert` method calls
- Fixed unescaped ASCII `"` inside German DND strings in `de.ts` that caused
  587 cascading TypeScript parse errors
- Replaced `map_or(false, |v| v == 1)` with `== Some(1)` in `gpu_stats.rs`
- Replaced manual `div_ceil` closure with `u64::div_ceil` in `job_queue.rs`
- Replaced `&&` range assertions with `.contains()` in `ppg_analysis.rs`
- Replaced `vec![…]` with array literals in test push calls
- Replaced `for ch in 0..N` index loops with `enumerate()` iterators in
  `eeg_bands.rs` and `eeg_filter.rs`
- Moved constant-value `assert!` calls into `const { }` blocks in
  `constants.rs`
- Fixed doc comment continuation indent warnings in `gpu_stats.rs`

### i18n

- Added `dnd.focusMode`, `dnd.focusModeDesc`, `dnd.focusModeLoading` keys
  to all five language files (en, de, fr, he, uk)

---

## [0.0.3] — 2026-03-06

- Added NeuTTS engine support alongside KittenTTS, with seamless switching between engines
- TTS engine switching now works reliably in both directions
- Graceful shutdown for NeuTTS on engine change or app exit
- TTS caching and quality improvements
- UI updates for TTS tab including progress/error state display
- Fixed TypeScript type for TTS error phase
- Added translations
- Better updater configuration

---

## [0.0.2] — 2026-03-04

- Improved EEG, Band, and GPU charts
- UI polish for main page
- Dependency and version bumps

---

## [0.0.1] — 2026-03-01

- Initial release
- CI/CD pipeline with signing, notarization, and auto-updater
- EEG visualisation, metrics, and GPU monitoring
- TTS foundation with KittenTTS
