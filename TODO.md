# TODO

- [x] Discord link in Help & About — added Discord community invite link to the About window Links section and Help window Dashboard tab Community section; `APP_DISCORD_URL` constant, `discordUrl` on `AboutInfo`, Discord SVG icon, i18n keys in all 5 locales

- [x] Configurable OCR engine, text embedding model, GPU/CPU toggle — `ScreenshotConfig` extended with `ocr_enabled` (bool, default true), `ocr_engine` (string, default "ocrs"), `ocr_text_model` (string, default "bge-small-en-v1.5", also supports "all-minilm-l6-v2" and "bge-base-en-v1.5"), `use_gpu` (bool, default true); OCR is now fully optional via toggle; GPU/CPU toggle applies to both image embeddings and OCR inference; Settings UI updated with OCR on/off toggle, GPU acceleration toggle, OCR engine selector (ocrs), text embedding model dropdown (BGE-Small/MiniLM/BGE-Base), Active Models info grid showing current engine/models/inference mode; OCR section only shown when OCR is enabled; i18n keys for en/de (14 new keys)

- [x] OCR text extraction + text embedding for screenshots — added `ocrs` (0.12.1) and `rten` (0.24.0) crates for on-device OCR; OCR runs on the **full-resolution** captured image before any downsizing for maximum text recognition quality; extracted text is embedded via fastembed BGE-Small-EN-v1.5 text model and stored in a separate HNSW index (`screenshots_ocr.hnsw`) for semantic text search; SQLite schema extended with `ocr_text`, `ocr_embedding`, `ocr_embedding_dim`, `ocr_hnsw_id` columns (with automatic migration for existing databases); OCR models (~10 MB each) auto-downloaded from S3 on first use to `~/.skill/ocr_models/`; new `search_screenshots_by_text` Tauri command supporting both `"semantic"` (embedding HNSW search) and `"substring"` (SQL LIKE) modes; OCR engine loaded once at worker startup and reused for all captures; `embed_ocr_text` helper uses fastembed text embedder for OCR text; constants added for `SCREENSHOTS_OCR_HNSW`, OCR model URLs and filenames

- [x] macOS screenshot capture via CoreGraphics FFI — replaced osascript/Swift/Python subprocess approach with pure Rust FFI: gets frontmost PID via `NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier` (objc2 msg_send), enumerates windows via `CGWindowListCopyWindowInfo` (CoreGraphics.framework C FFI), finds first layer-0 window matching PID, passes CGWindowID to `screencapture -x -l <wid>` for completely silent capture; no cursor change, no user interaction, sub-millisecond PID+window lookup, no subprocess for window enumeration

- [x] Screenshots always-visible Re-embed & Reindex button — the re-embed section is now always visible (not hidden behind a conditional), with a "Re-embed & Reindex" button that re-embeds all screenshots with the current model and rebuilds the HNSW index; model-changed amber banner shows a separate "Re-embed now" call-to-action with stale count and time estimate; stale/unembedded badge pills and progress bar with ETA shown inline; i18n keys `screenshots.reembedBtn` renamed to "Re-embed & Reindex", new `screenshots.reembedNowBtn` added across all 5 locales

- [x] Screenshots Settings UI tab — new `ScreenshotsTab.svelte` added to Settings window with: master enable toggle (accent-colored), session-only toggle, capture interval slider (1–30s), image size slider (128–512px) with recommended-size hint, WebP quality slider (10–100), embedding backend select (fastembed/mmproj), fastembed model select (CLIP/Nomic), apply button, model-changed amber banner with re-embed prompt and time estimate, re-embed progress bar with ETA, statistics grid (embedded/unembedded/stale counts), privacy note callout; full i18n across all 5 locales (en/de/fr/uk/he); tab icon (image/landscape); integrated into settings page tab list between Embeddings and Hooks

- [x] Screenshot capture + vision-encoder embedding system — new `screenshot.rs` and `screenshot_store.rs` modules implement periodic active-window capture (~5 s interval), aspect-ratio-preserving resize + center-pad, WebP saving to `~/.skill/screenshots/YYYYMMDD/`, fastembed CLIP ViT-B/32 (512-dim) or Nomic Embed Vision v1.5 (768-dim) embedding, SQLite metadata store (`screenshots.sqlite`) with full provenance columns (model_backend, model_id, image_size, quality, app_name, window_title), HNSW visual-similarity index (`screenshots.hnsw`), and background worker thread; `ScreenshotConfig` added to `settings.rs` (opt-in, session-gated by default); 7 new Tauri commands: `get_screenshot_config`, `set_screenshot_config`, `estimate_screenshot_reembed`, `rebuild_screenshot_embeddings`, `get_screenshots_around`, `search_screenshots_by_vector`, `search_screenshots_by_image`; cross-modal join via `YYYYMMDDHHmmss` timestamp key to EEG embeddings; `image` crate added to Cargo.toml; constants for `SCREENSHOTS_DIR`, `SCREENSHOTS_SQLITE`, `SCREENSHOTS_HNSW`, `SCREENSHOT_HNSW_SAVE_EVERY` added to constants.rs; macOS captures active window via CGWindowListCopyWindowInfo window-ID + `screencapture -l`; Linux captures via `xdotool`/`import -window` (X11) or `swaymsg`/`grim -g` (Wayland) with `scrot -u` fallback; Windows captures foreground window via `GetForegroundWindow`/`GetWindowRect`/`CopyFromScreen`; `InferRequest::EmbedImage` variant added to `llm/mod.rs` for mmproj vision-projector embedding via LLM actor channel; `screenshot_store` shared via `AppState` as `Option<Arc<ScreenshotStore>>`

- [x] Calibration profile CRUD via CLI — added `calibrations create`, `calibrations update`, and `calibrations delete` subcommands to cli.ts; `create` requires `--actions "Label:secs,..."` and name, supports `--loops`, `--break`, `--auto-start`; `update` resolves by UUID or name substring, supports `--name`/`--actions`/`--loops`/`--break`/`--auto-start`; `delete` resolves by UUID or name substring; helper functions `parseCalActions` (compact format parser) and `resolveProfileId` (UUID/name resolver); SKILL.md updated with full subcommand table, flag reference, CLI examples, HTTP/REST examples, and example output

- [x] Status command shows latest hook trigger — added `hooks` object to `status` WS response containing `total`, `enabled`, and `latest_trigger` (hook name, triggered_at_utc, distance, label_id, label_text); cli.ts `cmdStatus` renders a Hooks section showing hook count, latest trigger name, timestamp, time-ago, distance, and matched label; SKILL.md updated with hooks field in status JSON example, --full reveals entry, and jq query examples

- [x] Hook trigger broadcast enrichment + CLI listen display — enhanced the `hook` WebSocket broadcast event to include full trigger context (scenario, distance, label_id, label_text, triggered_at_utc) alongside the existing hook/command/text fields; cli.ts `listen` command now renders a dedicated 🪝 Hook Triggers section showing hook name, scenario, cosine distance, matched label, and configured command/text; SKILL.md updated with comprehensive "How Proactive Hooks Work" section explaining the end-to-end flow (label → reference embeddings → live comparison → scenario gating → cooldown → broadcast → audit log), hook trigger event JSON shape, and automation recipes (shell script, Python WebSocket listener, end-to-end workflow)

- [x] SKILL.md comprehensive coverage — updated SKILL.md to cover all cli.ts functionality: added `say` command (TTS with `--voice`), `calibrations` command (list/get profiles), `dnd` command (status/on/off), full `hooks` CRUD subcommands (list/add/remove/enable/disable/update with `--keywords`/`--scenario`/`--command`/`--hook-text`/`--threshold`/`--recent` flags), all new LLM subcommands (`add`/`select`/`mmproj`/`autoload-mmproj`/`pause`/`resume`/`downloads`/`refresh`/`fit`), `--poll <n>` for status, `--context`/`--at` for label, `--no-color`/`--version` global options; added corresponding WebSocket command table entries, HTTP examples, `--full` reveals sections, and updated Global Options table and Table of Contents

- [x] CLI full hook CRUD — added `hooks list`, `hooks add`, `hooks remove`, `hooks enable`, `hooks disable`, `hooks update` subcommands to cli.ts; new `hooks_get` and `hooks_set` WebSocket commands in ws_commands.rs reuse `sanitize_hook` for validation; all hook management previously only available through the Settings UI is now fully accessible from the command line; comprehensive test coverage in test.ts: 15+ test cases covering add, multi-hook, enable/disable toggle, update all fields, remove by omission, sanitization (empty name, invalid scenario, threshold clamping, recent_limit clamping, keyword trimming), clear-all, missing-field defaults, and hooks_status interplay

- [x] add external HF models to catalog via CLI/API — new `llm add <repo> <filename>` CLI command and `llm_add_model` WebSocket/Tauri command lets users download any GGUF model from HuggingFace that isn't in the bundled catalog; supports `llm add <repo> <filename>` and `llm add <hf-url>` forms; optional `--mmproj <file>` flag downloads a vision projector from the same repo alongside the model; metadata (quant, mmproj, family) is auto-inferred from the filename; the entry is persisted to `llm_catalog.json` and download starts immediately; duplicates are detected and skipped

- [x] full LLM management via CLI — added 9 new WebSocket commands (`llm_select_model`, `llm_select_mmproj`, `llm_pause_download`, `llm_resume_download`, `llm_refresh_catalog`, `llm_downloads`, `llm_set_autoload_mmproj`, `llm_hardware_fit`) and corresponding CLI subcommands (`llm select`, `llm mmproj`, `llm autoload-mmproj`, `llm pause`, `llm resume`, `llm downloads`, `llm refresh`, `llm fit`); all LLM management tasks (model selection, vision projector config, download lifecycle, hardware fit check, catalog refresh) can now be performed entirely from the command line

- [x] fix windows not brought forward and focused when opened from tray menu — all window-opening functions (settings, help, history, compare, chat, downloads, search, labels, focus timer, calibration, about, API, onboarding, what's new, session detail, main) now call `win.unminimize()` before `win.show()` + `win.set_focus()` for existing windows, ensuring minimized or behind-other-windows windows are properly raised; newly created windows now call `win.set_focus()` after `.build()` instead of discarding the window handle, ensuring they receive focus on creation

- [x] rounded window corners — all windows now have 10px rounded corners via `transparent(true)` on every Tauri window builder and CSS `border-radius: 10px; overflow: hidden` on the root `<html>` element; on macOS requires `macos-private-api` feature flag (`macOSPrivateApi: true` in tauri.conf.json); applies to main, settings, help, history, chat, about, calibration, downloads, search, session, labels, focus timer, onboarding, What's New, compare, and API windows

- [x] persist tool calls in chat history SQLite — new `chat_tool_calls` table stores tool name, status, args, result, and tool_call_id for each tool invocation; tool calls are saved alongside the assistant message after inference completes; when loading a session, tool calls are joined back onto their parent messages so expandable tool-call cards are fully restored on reload; schema uses `CREATE TABLE IF NOT EXISTS` for seamless migration of existing databases; frontend `storedToMessage` maps persisted rows back to `ToolUseEvent` objects with `expanded: false` default

- [x] chat UI: always-visible context usage, model name in titlebar, deduplicate tools — context usage bar is now permanently visible below the header (not hidden inside the tools panel); model name moved from the status area into the titlebar drag region to free horizontal space; tools allow-list in the parameters panel is hidden when the dedicated tools panel is already open, avoiding duplicate UI

- [x] tools badge toggles dedicated tools panel — clicking the tools badge in the chat header now opens/closes a dedicated tools panel (instead of opening the full settings panel); panel shows tool toggles, execution mode, and context length bar with token usage from the last assistant message; tools badge is always visible when the model supports tools (even with 0 enabled); mutually exclusive with the settings panel

- [x] add LLM accuracy warning banner in chat window — persistent amber warning above the footer hint reminding users that LLM output can be inaccurate and to always verify tool results and generated content; full i18n across all 5 locales (en/de/fr/uk/he)

- [x] README badges — added Discord community link badge, Homebrew cask install badge, and platform download buttons (macOS, Windows, Linux) at the top of README.md pointing to the latest GitHub release assets

- [x] real-time context usage prediction — context bar now updates live as messages accumulate, as the user types, and as tokens stream in; uses ~4 chars/token heuristic with tool prompt overhead estimation; shows `~` prefix when displaying estimates; snaps to real values from llama.cpp when the `done` chunk arrives; completion tokens tracked during streaming

- [x] add Cmd/Ctrl+W to close (or hide) windows — keydown handler in root layout calls `getCurrentWindow().close()` on all windows; main window is hidden (existing CloseRequested intercept), all other windows (chat, settings, help, etc.) are closed

- [x] fix tool results not shown to model — "tool" role messages were passed directly to llama.cpp `apply_chat_template` which only supports system/user/assistant roles; tool results now mapped to "user" role with a `[Tool Result]` wrapper; empty assistant messages replaced with `[Calling tools…]` placeholder to maintain user/assistant alternation and prevent consecutive user messages that break chat templates

- [x] add `search_output` tool and bash output file storage — bash tool now saves full output to `scripts_dir/output_<ts>.txt` and returns a compact summary (first 20 + last 20 lines) with `output_file` path; new `search_output` tool provides regex search with context lines, head/tail retrieval, and line-range slicing on output files, letting the LLM navigate large outputs without loading them into context; auto-enabled when bash is enabled; i18n for all 5 locales

- [x] fix chat settings/tools panel not scrollable — the parameters panel (system prompt, EEG, tool toggles, thinking level, sliders) had no overflow handling and no max-height; when content exceeded the window it pushed the message list off-screen; now capped at `max-h-[50vh]` with `overflow-y-auto` and thin scrollbar styling

- [x] context-aware tool calling — compact tool prompt for small contexts (≤4096 tokens), automatic history trimming (drops oldest messages to fit 75% of n_ctx), and tool result truncation (2 KB cap on tool output in history) to prevent "prompt too long" errors on small-context models

- [x] fix chat history not preserving full assistant responses — `leadIn` text (what the model says before calling tools) was discarded on save, and the save condition required non-empty `content` which skipped messages that only had lead-in or thinking; now combines `leadIn` + `content` into a single persisted string so the complete response is visible when reloading old conversations

- [x] fix Cmd+C / Cmd+V / Cmd+X / Cmd+A not working in chat window on macOS — added Edit submenu with predefined Undo, Redo, Cut, Copy, Paste, Select All menu items to the macOS app menu; without these, the Tauri webview does not route standard keyboard shortcuts to the web content

- [x] fix bash tool "prompt too long" errors for large commands — when a bash command exceeds 8 KB, it is now written to a timestamped shell script file (`cmd_<ts>_<ms>.sh`) under `skill_dir/chats/scripts/<session_ts>/` and executed as `bash <script>` instead of `bash -c <command>`, avoiding OS ARG_MAX limits; script files are preserved per-session for later inspection; the tool result includes a `script_path` field when a script was used; scripts are created with `set -euo pipefail` for safety

- [x] add per-tool-call cancel/stop UI with danger detection — each tool-call card now shows a cancel button while the tool is executing; dangerous operations (bash commands with `rm`/`sudo`/`chmod`/system paths, file writes to `/etc/`/`/boot/`/`/usr/` etc.) display an inline danger warning badge and red-highlighted card border; user can click cancel to stop any individual tool call by its `tool_call_id`; backend `cancel_tool_call` Tauri command adds the ID to a shared `cancelled_tool_calls` set checked before and during both sequential and parallel tool execution; cancelled tools return a clean error result to the LLM; `ToolCancelled` IPC chunk type for real-time cancel feedback; cancelled status shown with amber border, slash-circle icon, and "Cancelled" label; expanded detail panel includes a prominent cancel button for dangerous tools; full i18n across all 5 locales (en/de/fr/he/uk)

- [x] add filesystem and shell coding-agent tools to the LLM tool-calling system — new `bash`, `read_file`, `write_file`, and `edit_file` built-in tools (inspired by pi-mono coding-agent architecture) with per-tool toggles in Settings → LLM and Chat sidebar; `bash` executes shell commands with timeout support and tail-truncated output (2 000 lines / 50 KB); `read_file` reads text files with offset/limit pagination and head-truncated output; `write_file` creates/overwrites files with auto-mkdir; `edit_file` performs exact find-and-replace with CRLF-aware matching; all four default to disabled and show an "Advanced" warning badge; `~` home-dir expansion and relative path resolution; `KNOWN_TOOL_NAMES` updated for tool-call extraction/stripping; full i18n across all 5 locales; 18/18 Rust tests pass, 193/193 frontend tests pass

- [x] add expandable tool-call cards in chat UI — tool pills are now clickable cards (like the thinking expand/collapse) that show structured arguments and results; bash commands show the command in the header, file tools show the path; expanded panel displays formatted JSON args and result with scrollable pre blocks; `tool_execution_start`/`tool_execution_end` IPC events are now captured into `ToolUseEvent` instead of being ignored

- [x] add safety approval flow for dangerous tool operations — bash commands containing `rm`, `sudo`, `chmod`, system paths (`/etc/`, `/usr/`, etc.), and similar destructive patterns trigger an OS-native approval dialog (`rfd::MessageDialog`) before execution; file write/edit to sensitive system paths (`/etc/`, `/boot/`, `/usr/`, `/var/`, `/bin/`, etc.) also require approval; denied operations return a clean error to the LLM; `DANGEROUS_BASH_PATTERNS` and `SENSITIVE_PATH_PREFIXES` constants for easy extension

- [x] fix leaked `[TOOL_CALL]` markup visible in chat bubbles — `stripToolCallFences()` now strips complete `[TOOL_CALL]…[/TOOL_CALL]` blocks and incomplete `[TOOL_C…` prefixes during streaming; prevents raw tool-call tags from appearing in lead-in text and response content

- [x] fix LLM tool calling not executing — the system prompt injected tool JSON schemas (`[TOOL_SCHEMA]`) but gave the model zero instructions on how to emit a tool call, so the model described tool usage in prose instead of actually calling tools; rewrote `inject_tools_into_system_prompt` to include per-tool parameter docs, explicit `[TOOL_CALL]{"name":"…","arguments":{…}}[/TOOL_CALL]` format instructions, rules (valid JSON, stop-and-wait, don't fabricate), and concrete examples; 18/18 Rust tests pass

- [x] complete i18n translation pass — translated all remaining English-fallback keys in Hebrew (he.ts): 138 dashboard/history/help keys, 39 hooks keys, 10 LLM tool keys, 2 help-settings keys; translated 39 hooks keys in German (de.ts), French (fr.ts), and Ukrainian (uk.ts); removed all `// TODO: translate` markers across all 4 non-English locales; 0 missing keys across all locales

- [x] align release workflow Discord notification titles/descriptions to the UI-facing brand `NeuroSkill™` in `.github/workflows/release-linux.yml`, `.github/workflows/release-mac.yml`, and `.github/workflows/release-windows.yml` while preserving plain `NeuroSkill` artifact/file naming

- [x] update remaining release workflow updater-note fallback strings to `NeuroSkill™` in `.github/workflows/release-linux.yml`, `.github/workflows/release-mac.yml`, and `.github/workflows/release-windows.yml` while preserving plain `NeuroSkill` artifact/file naming

- [x] normalize app naming by context: keep generated artifact/file names as `NeuroSkill` (no `™`) while forcing UI/window-facing display strings to `NeuroSkill™`; harden Windows packaging/release PowerShell scripts to run with UTF-8 console/output encoding so `™` renders reliably

- [x] remove `™` from Windows/Linux build artifacts and installer-facing names — Linux desktop entry names in `scripts/package-linux-dist.sh` and `scripts/package-linux-system-bundles.sh` now use `NeuroSkill`; Linux release updater manifest notes in `.github/workflows/release-linux.yml` now use `NeuroSkill`; build fallback default in `scripts/tauri-build.js` and Windows release script synopsis in `release-windows.ps1` were normalized to `NeuroSkill`

- [x] rename Homebrew cask + macOS app bundle naming for install reliability — cask token/file is now `neuroskill` (`Casks/neuroskill.rb`), cask install target is `NeuroSkill.app` (no trademark symbol), `~/.skill` is no longer removed by Homebrew `zap`, and GitHub Actions macOS app-bundle naming (`release-mac.yml`, `pr-build.yml`) plus `src-tauri/tauri.conf.json` now use `NeuroSkill` consistently

- [x] implement pi-mono style tool calling architecture — added JSON Schema argument validation (`jsonschema` crate, `validate_tool_arguments`), configurable parallel/sequential tool execution modes (`ToolExecutionMode` setting), configurable max rounds and max calls per round, rich tool-execution lifecycle events (`ToolExecutionStart`/`ToolExecutionEnd` IPC chunks alongside legacy `ToolUse`), `BeforeToolCallFn`/`AfterToolCallFn` hook type definitions for future extensibility, execution mode UI toggle in both Chat window and Settings → LLM panel, all 5 languages localised; 15/15 Rust tests pass including 4 new validation tests

- [x] harden markdown normalization across chat bubbles — extracted shared `normalizeMarkdown()` utility, applied it to both final-answer and thought rendering paths, protected fenced/inline code from normalization, and broadened emphasis repair to cover stray spaces plus CommonMark flanking-rule failures for both `**...**` and `*...*`; added frontend unit tests for repaired strong/emphasis output
- [x] broaden frontend tool-call fence stripping to match Rust prefix detection — leaked partial fenced JSON like chat9 no longer depends on an exact ````json\n{` start; `stripToolCallFences()` now recognizes malformed/incomplete tool-call prefixes with blank lines or partial bodies by mirroring `looks_like_tool_call_json_prefix` logic from Rust
- [x] strip orphan opening JSON fence preambles from assistant markdown — some thought traces still began with an unmatched ````json` block plus partial JSON fragments (chat10), which caused the rest of the thought markdown to render badly; `normalizeMarkdown()` now drops only that narrow malformed preamble while preserving closed fenced code blocks
- [x] fix dict-style multi-tool call format `{"date":{},"location":{}}` not recognized — models like Qwen3 emit all tool calls as one object with tool names as keys and parameter objects as values; added `KNOWN_TOOL_NAMES`, `is_dict_style_multi_tool()` helper, updated `extract_calls_from_value`, `is_tool_call_value`, `looks_like_tool_call_json_prefix`, and frontend `stripToolCallFences`; 11/11 Rust tests pass
- [x] fix multi-tool / multi-round streaming: freeze rawAcc into frozenLeadIn/frozenThinking on first tool-call event so inter-tool text doesn't flicker in the response bubble; reset rawAcc per round; merge frozen state back into every subsequent parse so thinking and lead-in from earlier rounds are preserved
- [x] replace blinking text cursor during LLM inference with a spinning SVG on the avatar column, and order assistant turn sub-bubbles chronologically (thinking → lead-in → tool uses → response)
- [x] fix leaked partial tool-call JSON and literal `<think>` tags visible in chat response bubble — `ToolCallStreamSanitizer` emitted partial fence before recognising it as a tool call; also models emit two `<think>` blocks per tool-call turn (pre-tool + post-tool) which the old single-pair extractor left raw in `content`; fixed with frontend `stripToolCallFences` + multi-block `parseAssistantOutput`
- [x] rewrite chat-window assistant parsing/rendering so a single assistant turn is split into separate visual bubbles for lead-in text, tool activity, thinking, and final response instead of merging everything into one Markdown blob
- [x] fix chat formatting when a model opens a JSON tool-call fence without closing it before `<think>` by suppressing incomplete trailing tool-call fences/JSON during streaming and extracting think blocks even when they appear after other assistant text
- [x] fix chat tool-call transcript leakage by stripping OpenAI-style JSON tool payloads (inline/fenced) from streamed assistant text/history, and make built-in `date` tool return explicit local timezone metadata (`iso_local`, timezone name, UTC offset) so time answers default to local timezone
- [x] fix follow-up chat tool-calling parser miss for assistant payloads that use `{"tool":"date","parameters":{}}` (without `name`) by accepting `tool` as an alias key during JSON extraction
- [x] fix chat tool-calling when models emit plain OpenAI-style JSON function-call snippets (for example `{"name":"date","parameters":{}}`) instead of `[TOOL_CALL]...[/TOOL_CALL]` blocks by extending Rust extractor fallback parsing for inline/fenced JSON and OpenAI `tool_calls` envelopes

- [x] tint main-window titlebar red when Bluetooth is unavailable (bt_off state) so the connection problem is immediately visible
- [x] chat window: center model name in the titlebar instead of left-aligning it next to the status dot — uses absolute positioning to keep it visually centered between sidebar toggle and right-side controls

- [x] add LLM and Proactive Hooks help sections to the Help window — new HelpLlm.svelte (5 sections: overview, model management, inference settings, built-in tools, chat & logs with 19 help items) and HelpHooks.svelte (3 sections: overview, configuration, advanced with 15 help items); registered as sidebar tabs with icons, search index entries, and full i18n keys in en.ts
- [x] add `audit-i18n.ts` script to detect untranslated keys (values identical to English) across all locale files — supports `--check` (CI exit 1), `--locale <code>` (single locale), `--verbose` (show values); exempt list for legitimately identical keys (technical acronyms, formulas, brand names, URLs, short tokens); npm scripts `audit:i18n` and `audit:i18n:check`
- [x] translate all untranslated i18n keys across de/fr/he/uk — titles, labels, section headings, body text, FAQ answers for LLM help, Hooks help, TTS help, compare, settings, and UI elements; audit-i18n now reports 0 untranslated keys across all 4 locales (1527 legitimately exempt technical/brand terms)

- [ ] when you download first model, it does not get activated by default. Fix the `use` button. And streamline first onboarding with the first model. If user click start the LLM engine, if the model is not used, ask user to go and click `use` which one they want to run.
- [x] change history view to the calendar view (year, month, week, day), default is month. show it as a heatmap — added `HistoryViewMode` (year/month/week/day) segmented control in the custom titlebar, GitHub-style year heatmap with weekly columns, month grid calendar with per-day session counts; week view shows a 24h timeline grid per day with canvas-rendered epoch dots (one dot per 5-second EEG embedding), color-coded by session with per-band Y-mapping (relaxation), label markers (amber triangles + text), session bar fallback while timeseries loads, and a day label sidebar that navigates to day view; day view adds an epoch dot canvas below the 24h bar with session color legend and label count; clicking any day with recordings navigates to the day view; calendar prev/next navigation in the titlebar; emerald-gradient heat coloring with legend; always renders the calendar UI as a preloading skeleton even while data loads or when empty; reworked titlebar with themed segmented view-mode switcher, skeleton loading animation for day labels, context-sensitive nav; i18n across all 5 locales
- [x] reduce repeated title/menu redraw work by deduplicating unchanged `setTitle(...)` calls and skipping no-op titlebar observer updates
- [x] refactor `CustomTitleBar.svelte` — collapsed macOS and Windows/Linux duplicate branches into shared Svelte 5 snippets (`windowControls`, `centerContent`, `actionButtons`, `historyHead`, `tbBtn`, icon snippets); single unified template with platform-aware ordering; extracted repeated SVG icons into reusable snippets; 975 → 533 lines (45% reduction)
- [x] reduce spacing between titlebar close/maximize/minimize controls across all windows by matching shared `.titlebar-controls` button width to other titlebar icon buttons (`30px`) in `CustomTitleBar`

- [x] add tool calling UI to the LLM chat window — date/time, IP geolocation, web search (DuckDuckGo), web fetch — with per-tool toggles in the settings panel, live tool-use indicators on assistant messages, a header badge showing enabled tool count, and `ToolUse` IPC chunk for real-time status feedback; tools section is only shown when the model is running (`supports_tools`)
- [x] fix Tailwind v4 `Invalid declaration: onMount` dev-server errors — `@tailwindcss/vite` v4.2's `enforce:"pre"` transform matched `.svelte?svelte&type=style&lang.css` virtual modules before the Svelte compiler extracted the `<style>` block, causing the CSS parser to choke on JS imports; patched `vite.config.js` to skip `.svelte` style IDs in all Tailwind transform plugins; also removed empty `<style></style>` blocks in `whats-new/+page.svelte` and `UmapViewer3D.svelte`
- [x] fix mmproj crash on missing file — guard `mtmd_init_from_file` with an `exists()` check before calling the C library (which can abort on some platforms); use `resolve_mmproj_path(autoload)` instead of `active_mmproj_path()` so auto-detection works; filter out stale paths where the file has been deleted from disk
- [x] fix app crash after mmproj fails to load — disable clip GPU warmup in `MtmdContextParams` (avoids Vulkan state corruption when the mmproj is incompatible); wire up `no_mmproj_gpu` and `mmproj_n_threads` settings that were defined but never passed to the native library; add file-size sanity check (<1 KB → reject as truncated download); wrap `init_from_file` in `catch_unwind` to survive native panics; log the file path and size for diagnostics
- [x] fix Linux mmproj startup crashes by defaulting projector init to CPU mode (stable path) even when global LLM GPU offload is enabled; add explicit expert override `SKILL_FORCE_MMPROJ_GPU=1` to re-enable mmproj GPU init when the local Vulkan/driver stack is known-good
- [x] fix stale incompatible mmproj fallback on startup — when active model repo is known, reject `config.mmproj` paths that map to a different repo (e.g., 27B projector with 4B model) before mtmd init; log a clear mismatch warning and continue in text-only mode
- [x] fix Linux WebKit startup abort caused by `stacker::maybe_grow` swapping the main-thread stack before JavaScriptCoreGTK initialises; raise `RLIMIT_STACK` to 64 MiB on Linux instead and keep `stacker` only on macOS/Windows
- [x] fix Linux app auto-close after startup by preventing implicit `RunEvent::ExitRequested` exits (code `None`) and hiding main window instead; only explicit quit now runs shutdown
- [x] fix quit confirmation dialog not receiving focus — set the parent window on the `rfd::MessageDialog` so the popup appears focused and modal on Linux/Windows
- [x] fix intermittent `npm run tauri dev` failure (exit 141) in `scripts/build-espeak-static.sh` by removing `ar -t | head -1` SIGPIPE-prone pipeline under `set -o pipefail`; use `mapfile` to read first archive object safely
- [x] add OmniCoder 9B model family to the LLM catalog (`Tesslate/OmniCoder-9B-GGUF`, 13 quant variants)
- [x] add Qwen3.5 27B Claude 4.6 Opus Reasoning Distilled GGUF model family to the LLM catalog (`eugenehp/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-GGUF`, 17 quant variants)

- [x] History view should show calendar view, with each day showing how many sessions were recorded as a span. User can switch to weekly view, or yearly view, to get a better sense of their data. It's sort of heatmap to see how often and how frequently the data was recorded. And from there navigate between the sessions and easily look into any of them or compare any of them.

- [x] fix release CI contributors source to use only git commit authors (per tagged commit range) and stop including GitHub auto-generated contributors
- [x] disable `generate_release_notes` in tagged release workflows and append a `## Contributors` section built from `git log` commit authors only
- [x] fix cross-platform `latest.json` merge encoding in release CI by writing UTF-8 without BOM in Windows workflow and making Linux/macOS manifest reads BOM-tolerant (`utf-8-sig`)
- [x] fix macOS updater 404 caused by non-ASCII `™` in tarball filename by renaming artifact to `NeuroSkill_<version>_aarch64.app.tar.gz` in release CI

- [x] fix Windows release CI post-compile Tauri crash (`tauri build` exit `-1073741571`) by adding a workflow fallback that runs `tauri bundle --bundles nsis` and recreates/signs the updater `.nsis.zip` artifact manually
- [x] align Windows release CI with the local-proven packaging command (`npm run tauri:build:win:nsis`) and keep CI-side installer signing + updater `.nsis.zip` / `.sig` generation
- [x] add npm alias `taur:build:win:nsis` and make Windows release CI call `npm run taur:build:win:nsis` exactly
- [x] fix Windows release CI NSIS bootstrap by installing NSIS in `.github/workflows/release-windows.yml` before `taur:build:win:nsis` and exporting `NSIS_DIR`/PATH for `create-windows-nsis.ps1`
- [x] fix Windows release CI NSIS artifact discovery by resolving both possible bundle output layouts (`src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis` and `src-tauri/target/release/bundle/nsis`) in build diagnostics and artifact collection
- [x] fix Windows release CI PowerShell parser failures in `Update latest.json`/Discord notify steps by using ASCII-safe release notes text and removing unsafe backtick-escaped tag/version field values
- [x] fix Windows release CI `Update latest.json` step `Exception setting "windows-x86_64"` crash when `latest.json` already exists (`ConvertFrom-Json` returns PSCustomObject; use `Add-Member`/bracket-notation based on type)

- [x] fix macOS release CI `pip3 install Pillow` failure on macOS 26 (PEP 668 externally-managed-environment) by adding `--break-system-packages` flag

- [x] include `CHANGELOG.md` version notes in preview CI artifacts by generating `preview-notes.md` in `.github/workflows/pr-build.yml`

- [x] include the matching `CHANGELOG.md` version section in GitHub Release information during release CI (linux/mac/windows workflows)

- [x] fix Windows NSIS locator false-negative: when `makensis` is on PATH, use its direct parent directory (not parent-of-parent), and accept `NSIS_DIR` set to either NSIS folder or full `makensis.exe` path

- [x] fix Windows NSIS script PowerShell parse error by precomputing candidate binary paths before array construction (avoid comma being parsed into `Join-Path` `ChildPath` argument array)

- [x] fix Windows standalone NSIS packager path detection so `tauri:build:win:nsis` accepts both `src-tauri/target/<triple>/release/skill.exe` and host-layout `src-tauri/target/release/skill.exe` outputs from `tauri build --no-bundle`

- [x] macOS titlebar: swap close and minimize button positions in shared `CustomTitleBar` so all windows use the requested order

- [x] fix macOS quit-time Metal assert (`GGML_ASSERT([rsets->data count] == 0)`) by running one-time blocking LLM/TTS shutdown on explicit `RunEvent::ExitRequested` (code `Some(_)`) instead of relying only on `RunEvent::Exit`, so GPU resources are released before late process/static teardown

- [x] fix macOS shutdown abort (`GGML_ASSERT([rsets->data count] == 0)`) by adding blocking `KittenTTS` worker shutdown in `RunEvent::Exit` via `tts_shutdown()` so KittenTTS resources are dropped before process/static Metal teardown

- [x] fix macOS `aarch64-apple-darwin` release build stack overflow by adding `-Wl,-stack_size,0x1000000` (16 MB) linker flag for both macOS targets in `.cargo/config.toml` — the default 8 MB main-thread stack is exceeded by the large `run()` frame (`generate_handler!` with ~150 commands + `AppState::default()` + setup/run closures)

- [x] fix compilation with `--no-default-features` by adding `#[cfg(feature = "llm")]` and `#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]` guards across `lib.rs`, `api.rs`, `settings_cmds.rs`, `tray.rs`, `tts/mod.rs`, and `llm/mod.rs`

- [x] enable `llm-metal` automatically on macOS and `llm-vulkan` automatically on Linux/Windows via target-specific `llama-cpp-4` dependency features

- [x] update `llama-cpp-4` dependency from 0.2.9 to 0.2.10

- [x] fix CI release builds producing dev-mode binaries that try to load frontend from localhost:1420 instead of embedding SvelteKit assets — add `custom-protocol` feature forwarding to `tauri/custom-protocol` and pass `--features custom-protocol` in all workflows and scripts that invoke `cargo build` directly (bypassing the Tauri CLI)

- [x] fix Windows release CI parse/runtime failures by using ASCII-safe Discord notify titles in PowerShell and adding verbose Tauri bundle diagnostics when `tauri build` exits non-zero

- [x] macOS local build stability: default wrapper-driven `tauri build` to `--no-bundle` (unless explicit bundle flags are passed) so `aarch64-apple-darwin --no-sign` compile-only builds stop failing in bundle/updater phase

- [x] add the same stale target/dist cleanup guard to Linux CI portable packaging job so cached outputs cannot mask fresh frontend/package artifacts

- [x] prevent Linux release CI from publishing stale frontend/bundle outputs by clearing cached target/dist release artifact directories before compile/package steps

- [x] fix macOS `aarch64-apple-darwin` Tauri/llama-cpp build failure by moving `MACOSX_DEPLOYMENT_TARGET` / `CMAKE_OSX_DEPLOYMENT_TARGET` out of the Windows target env table and into global Cargo `[env]` scope so `std::filesystem` symbols compile with min macOS 10.15

- [x] harden `release-linux` AppImage step to retry once on Tauri bundling segfault (`exit 139`) so transient Linux AppImage crashes don’t fail the release on first attempt

- [x] release-linux CI follow-up: keep first-attempt non-segfault failures hard-failing while allowing exactly one retry for Linux AppImage segfault exit `139`

- [x] switch `release-linux` to non-bundling packaging flow (compile-only + system `.deb`/`.rpm` + portable tarball), matching manual-packaging style used on other release workflows

- [x] enforce single-instance app runtime so launching NeuroSkill while already running re-focuses the existing main window instead of starting a second process

- [x] in CI, remove Linux `.deb`/`.rpm` bundle generation and keep Linux artifact packaging tar.gz-only

- [x] make `npm run bump` rotate `CHANGELOG.md` by preserving `## [Unreleased]` and inserting a dated `## [x.y.z]` release header automatically

- [x] make Settings window a bit wider by increasing its default width from 680 to 760 across settings/model/updates window open paths

- [x] refresh docs/tests/CLI for Proactive Hooks scenarios by updating `SKILL.md`, `cli.ts`, and `test.ts` with scenario-aware examples and outputs

- [x] upgrade Hooks UX into Proactive Hooks with scenario-aware matching (any/cognitive/emotional/physical), keyboard-driven suggestion selection, and quick-start scenario examples

- [x] fix Windows release Discord notifier JSON encoding by moving notify step to PowerShell `ConvertTo-Json` + `Invoke-RestMethod` (resolves Discord API code `50109` invalid JSON post-build failure)

- [x] harden Windows release post-build steps by replacing `Update latest.json` bash+python3 path with native PowerShell JSON handling and by skipping Discord notify when `DISCORD_WEBHOOK_URL` is unset

- [x] enforce Tauri frontend bundle-structure contract in npm/CI by validating `src-tauri/tauri.conf.json` `build.frontendDist` output (`build/index.html` + `_app/immutable` JS/CSS) before release bundling

- [x] improve Hooks keyword entry UX by making small action button labels wrap/fill correctly and adding live keyword suggestions (fuzzy + semantic/HNSW) from existing local labels while typing

- [x] remove Windows CI `dead_code` warning for `linux_has_appindicator_runtime` by dropping the non-Linux stub and compiling the helper only on Linux

- [x] ensure macOS release `.app` assembly copies generated SvelteKit frontend assets with macOS `ditto` and fails fast when `index.html` or `_app/immutable` JS/CSS assets are missing

- [x] extend hooks with runtime last-trigger metadata, session-open jump, hook-specific logging toggle, trigger toast/OS notification, hooks_status API/CLI/test coverage, and Help/FAQ + i18n updates
- [x] add last-trigger relative age timer, distance-threshold suggestion (from real HNSW/SQLite data), and persistent hook-fire audit-log (hooks.sqlite) with paginated history viewer in HooksTab
- [x] polish Settings/Hooks UX: theme-compliant scenario dropdown, Hooks heading label, mouse-resizable settings tab sidebar, and i18n Settings titlebar text including active tab
- [x] expose hooks suggest/log over websocket + CLI (`hooks suggest`, `hooks log`), and extend smoke tests/docs (`test.ts`, `SKILL.md`)
- [x] fix Rust hooks settings compile error in `set_hooks` by making the state lock guard mutable before assigning `s.hooks`
- [x] add Linux packaging quickstart commands in `README.md` Development section (AppImage via Tauri + `.deb`/`.rpm` via system tools)

- [x] add Settings → Hooks with persisted hook rules (name/keywords/command/text/threshold/recent refs) and runtime websocket hook broadcasts triggered by fuzzy+embedding label proximity
- [x] clear current Rust clippy warnings in embeddings/settings by documenting intended `too_many_arguments` constructors and deriving `Default` for `HookStatus`
- [x] update `LINUX.md` build section to document canonical Linux packaging flow: AppImage via `tauri:build:linux:*` plus manual `.deb`/`.rpm` via `package-linux-system-bundles.sh` (`dpkg-deb` + `rpmbuild`)
- [x] in Linux CI and release workflows, stop using Tauri for `.deb`/`.rpm` and build those packages manually with system tools (`dpkg-deb` + `rpmbuild`), while keeping Tauri only for AppImage output
- [x] align Linux npm build scripts and workflow entrypoints with AppImage-only Tauri bundling (`tauri:build:linux:*` now use `--bundles appimage`; CI/release call the npm script and use system-tool scripts for `.deb`/`.rpm`)

- [x] update `LINUX.md` with reciprocal `README.md` cross-link and explicit startup-failure wording so Linux prerequisite context is discoverable from both docs
- [x] enforce tray-always-required behavior on Linux by hard-failing startup when appindicator runtime (`libayatana-appindicator3`/`libappindicator3`) is missing instead of running in no-tray mode
- [x] add a Linux prerequisites callout in `README.md` linking to `LINUX.md` and the tray runtime dependency section so `npm run tauri dev` requirements are visible from the main docs
- [x] document Linux tray runtime prerequisite in `LINUX.md` (`libayatana-appindicator3-1` / fallback `libappindicator3-1`) and add troubleshooting note for the `Failed to load ayatana-appindicator3 or appindicator3 dynamic library` startup error
- [x] prevent Linux startup panic when appindicator runtime is missing by probing `libayatana-appindicator3`/`libappindicator3` before tray init and running without tray when unavailable
- [x] add Linux `tauri dev` preflight in `scripts/tauri-build.js` that detects missing appindicator runtime libraries (`libayatana-appindicator3` / `libappindicator3`) and fails fast with distro-aware install guidance instead of letting Tauri panic at startup
- [x] make `npm run bump` fail fast on Linux when required Tauri/WebKit pkg-config deps are missing, with explicit apt install guidance before `cargo clippy`
- [x] localize updater online-download fallback messages across en/de/fr/he/uk and remove hardcoded English text from Updates tab
- [x] strictest pass: neutralize remaining non-status category-colored controls in UMAP/Embeddings to semantic `primary`/`ring` tokens
- [x] add `workflow_dispatch` toggle (`run_linux_bundles`) in CI to optionally run heavy Linux bundle/portable jobs during manual checks
- [x] speed up PR CI by running heavy Linux bundle/portable jobs only on push events in `.github/workflows/ci.yml`
- [x] harden Linux CI/release to run native x86_64 build/package scripts on x86_64 runners (remove ALLOW_LINUX_CROSS dependency from workflow execution paths)
- [x] finalize strict accent enforcement in calibration selectors and clarify AGENTS rule that semantic status colors are allowed but generic selected/focus styling must be accent-aware
- [x] normalize remaining non-status hardcoded accent highlights (`rose`/`emerald`) in generic selection/focus controls to semantic `primary`/`ring` tokens
- [x] run broader accent consistency sweep by replacing interactive blue-state/focus classes with semantic `primary`/`ring` tokens in core settings/workflow screens
- [x] enforce global accent consistency so native form controls and remaining checkbox/range accents always follow the Appearance accent setting
- [x] add Linux `rpm` packaging to CI/release bundle workflows and publish checksum artifacts in CI plus detached-signature artifacts in `release-linux`
- [x] normalize appearance accent-color behavior by remapping accent-like Tailwind families (`violet`/`blue`/`indigo`/`sky`) to the selected accent so interactive UI highlights and controls consistently honor the chosen accent
- [x] upload Linux `.deb` package from CI `linux-release` job as a downloadable Actions artifact
- [x] add CI job that builds the standalone Linux portable package (`package:linux:portable:x64`) and uploads the generated tarball as an Actions artifact
- [x] what's new window: make the version dropdown theme-compliant (light/dark) by replacing transparent/native select styling with explicit themed control styles
- [x] when automatic app update install fails, direct users to download the latest release online and open the GitHub releases page for them
- [x] fix macOS white screen on first launch by deferring `win.show()` until the frontend's `onMount` fires instead of calling it in Tauri setup before WKWebView has loaded the page
- [x] add standalone Linux distribution packager (`scripts/package-linux-dist.sh`) that builds with `--no-bundle` and creates a portable tarball containing binary, resources, launcher, desktop file, and docs
- [x] linux arm64 build resilience: when explicit bundle builds crash in Tauri CLI (`139`/`134`) but release binary is produced, treat as compile-only success (with opt-out env) instead of failing the entire build
- [x] add Linux CI bundle smoke-test assertion that requires at least one `.deb` artifact and checks both target-triple and fallback bundle output paths
- [x] harden Linux CI bundle recovery: when per-target `tauri build --bundles <target>` segfaults with exit `139` before writing artifacts, retry the same target via `tauri bundle` and only fail if artifacts are still missing
- [x] route all per-session LLM log files into a standalone `llm_logs` folder under `skill_dir` (`~/.skill/llm_logs`) instead of writing `llm_*.txt` in the `skill_dir` root
- [x] cache Linux Tauri apt system dependencies in CI and release workflows using `awalsh128/cache-apt-pkgs-action` (instead of uncached plain `apt-get`) to reduce repeated dependency download time
- [x] update GitHub Actions workflows to Node 24-ready action versions (`checkout`/`setup-node`/`cache`/`rust-cache`) and remove the Linux apt cache step that was causing tar save failures in CI
- [x] linux CI bundling stability: treat Linux `tauri build` exit `139` as recoverable for explicit single-target bundle runs when expected bundle artifacts already exist (extends the existing sequential multi-target retry path)
- [x] make `npm run bump` run preflight checks in order (`npm run check` → `cargo clippy` in `src-tauri` → `npm run sync:i18n:check`) and abort bump on first failure
- [x] fix clippy `doc_lazy_continuation` warning in `src-tauri/src/dnd.rs` module docs by separating Linux list and Windows paragraph with a blank doc line
- [x] run `npm run sync:i18n:fix` to backfill 138 missing `he.ts` keys with English fallbacks and restore key-count parity (`2237/2237`)
- [x] fix i18n placeholder consistency regression in French locale by restoring `llm.size` token from `{Go}` to `{gb}`
- [x] reduce `any` surface in 3D UI by introducing typed Three.js scene/object wrappers in `UmapViewer3D.svelte` and `InteractiveGraph3D.svelte`
- [x] fix i18n key-sync false-missing report for `llm.tools.*Desc` in de/fr/he/uk by normalizing `"key": "value"` spacing so `scripts/sync-i18n.ts --check` detects all keys
- [x] finish i18n fallback translation pass for German locale (`src/lib/i18n/de.ts`) and remove stale TODO translation markers in that locale
- [x] finish i18n fallback translation pass for French, Hebrew, and Ukrainian locales (`src/lib/i18n/fr.ts`, `src/lib/i18n/he.ts`, `src/lib/i18n/uk.ts`) and remove stale TODO translation markers
- [x] add Windows Do Not Disturb automation backend path using per-user notification toggle (`PushNotifications\\ToastEnabled`) with query + set support
- [x] clean repo hygiene warnings in GitHub workflow diagnostics and changelog markdown structure
- [x] linux CI bundling stability: when `npx tauri build --bundles ...` exits with status 139, retry bundling one target at a time in `scripts/tauri-build.js` to keep `.deb` and `.AppImage` artifact generation reliable
- [x] add Linux xdg-desktop-portal fallback for DND automation when GNOME/KDE DND backends are unavailable
- [x] implement Linux Do Not Disturb automation backend paths (GNOME + KDE) so focus-driven DND activation/deactivation works on Linux too
- [x] update `CHANGELOG.md` for the `0.0.24` release
- [x] add label window: center the EEG countdown without crowding the window title, and switch the page root to parent height so the footer stays visible under the shared titlebar
- [x] search window titlebar: center the mode segmented control in the middle of the titlebar and increase available width so all mode buttons render aligned without clipping
- [x] window vertical-fit sweep: switch remaining titlebar-hosted route roots from viewport height to parent height so shared custom-titlebar windows stop clipping at the bottom
- [x] what's new window: keep the bottom dismiss button visible by switching the page root to parent height under the shared custom titlebar
- [x] add label window: move the live EEG window countdown into the shared titlebar center and remove the duplicate in-page header strip
- [x] search window: ensure full window fits vertically by making the page root parent-height (`h-full`) instead of viewport-height (`h-screen`) under the custom titlebar layout
- [x] search window titlebar: fix segmented mode buttons clipping/truncating so all mode buttons render correctly across narrow widths and longer localized labels
- [x] history window: move title, day pagination, compare toggle, labels toggle, and reload button from the in-page header into the shared custom titlebar
- [x] help window: move search input, version badge, license label, ThemeToggle, and LanguagePicker from the in-page top bar into the shared custom titlebar
- [x] settings window: move title and buttons (Help, ThemeToggle, LanguagePicker) from the in-page top bar into the shared custom titlebar
- [x] api status window: move title and refresh button from in-page header into the shared custom titlebar
- [x] search window: move search title + mode buttons from in-content header into the shared custom titlebar
- [x] add i18n strings for all LLM built-in tool toggle labels and descriptions (de, fr, he, uk + en); convert `TOOL_ROWS` to reactive `$derived` in `LlmTab.svelte`
- [x] add per-tool allow-list settings for LLM chat so `date`, `location`, `web_search`, and `web_fetch` can be enabled or disabled from Settings → LLM
- [x] keep multimodal projector selection paired with a compatible downloaded LLM so choosing an `mmproj` never displaces or breaks the base text model
- [x] custom titlebar: show each non-main window name in the shared titlebar and keep main-only titlebar actions scoped to the main window
- [x] remove duplicate in-content title bars from all secondary windows (about, compare, whats-new, focus-timer, session, labels, search, history, calibration, label, onboarding, chat) so controls live exclusively in the shared titlebar
- [x] style Windows scrollbars globally so all Tauri/Svelte windows use themed scrollbars instead of native system bars
- [x] stream incremental visible assistant deltas in IPC chat while suppressing `[TOOL_CALL]` blocks during built-in LLM tool-calling rounds
- [x] make in-app IPC chat (`chat_completions_ipc`) use the same built-in LLM tool-calling loop as `/v1/chat/completions`
- [x] add simple LLM tool calling for built-in `date`, `location`, `web_search`, and `web_fetch` in `/v1/chat/completions`
- [x] fix Windows release workflow build step exit 127 by replacing bash+python config generation with native PowerShell JSON config for `npx tauri build`
- [x] add post-build Linux sanity check in CI/release workflows for `bundle/deb` and `bundle/appimage` directories
- [x] add Linux CI/release guard step that validates `tauri:build:linux:x64` keeps `--bundles deb,appimage` and wrapper support for `--bundles`
- [x] fix Linux release artifact miss: treat `--bundles` as explicit bundling in `scripts/tauri-build.js` so CI does not inject `--no-bundle`
- [x] update `llama-cpp-4` to `0.2.9` and refresh the paired `llama-cpp-sys-4` lockfile entry
- [x] update `llama-cpp-4` to `0.2.9` and refresh the paired `llama-cpp-sys-4` lockfile entry
- [x] what's new window: bundle CHANGELOG.md via Vite `?raw` import, parse all version sections, and add prev/next navigation with a version-picker dropdown so users can browse the full release history from the What's New window
- [x] update `CHANGELOG.md` for the `0.0.23` release and move the Linux decoration fixes out of the older `0.0.17` entry
- [x] fix all windows being clipped at the bottom by the custom titlebar height: add `box-sizing: border-box; height: 100vh` to `#main-content` so the 30 px padding-top is absorbed into the viewport height instead of overflowing
- [x] add CI/local guard to block `MarkdownRenderer.svelte` regressions (`new Marked(...)` and local `<style>` block) that triggered Tailwind `Invalid declaration: Marked`
- [x] fix Tailwind Vite `Invalid declaration: Marked` crash from `src/lib/MarkdownRenderer.svelte`
- [x] run the MarkdownRenderer guard automatically before `npm run dev`, `npm run build`, `npm run check:watch`, and `npm run tauri dev` so Tailwind parser regressions fail before Vite/SvelteKit startup
- [x] downloads window: show total size of all downloads in a footer at the bottom
- [x] downloads window: make bottom total-size footer more prominent with explicit label + item count
- [x] downloads window: move total-size summary to always-visible status bar under header
- [x] centralize custom titlebar actions (minimize/maximize/close) in one shared component path
- [x] custom titlebar: ensure all app windows use shared undecorated window path + proper Tauri capabilities for controls/drag
- [x] main window: move top card buttons (language, theme, label, history) into titlebar for cleaner layout
- [x] titlebar: add proper spacing (action buttons left, window controls right)
- [x] when app downloads anything, show progress in the tray icon itself (prominent circular ring) and menu (filename, %, live status/size text).
- [x] add previous labels in the label window
- [x] onboarding models step: start staged background downloads while user continues onboarding in order ZUNA → KittenTTS → NeuTTS → Qwen3.5 4B (`Q4_K_M`)
- [x] onboarding footer: show subtle staged model-download status across all onboarding views and enlarge onboarding window to keep layout clean.
- [x] configure onboarding model download order from `src-tauri/src/constants.rs` instead of hardcoding it in the onboarding UI.
- [x] settings: allow opening `skill_dir` (`~/.skill`) directly from the Data Directory section
- [x] add standalone downloads window showing all downloads with pause/resume/cancel/delete and initiated timestamp + i18n
- [x] add Downloads window entry to tray menu
- [x] finish i18n: add 16 missing `onboarding.models.*` / `onboarding.step.models` / `onboarding.modelsHint` keys to de, fr, he, uk
- [x] linux build: avoid post-compile `tauri build` segfault (status 139) by defaulting wrapper builds to `--no-bundle` unless caller explicitly passes bundling flags
- [x] linux build: fail fast with a clear message when local wrapper builds force a non-native `*-unknown-linux-gnu` target without explicit cross-compilation opt-in
- [x] add `tauri:build:linux:arm64` npm script for one-command native ARM64 Linux deb/AppImage builds with `llm-vulkan`
- [x] add explicit `tauri:build:linux:x64` npm script that opts into cross-build mode via `ALLOW_LINUX_CROSS=1`
- [x] CI linux release smoke test: use `npm run tauri:build:linux:x64` instead of inline `npx tauri build ...`
- [x] release-linux workflow: use `npm run tauri:build:linux:x64` instead of inline `npx tauri build ...`
- [x] add workflow comments in CI + release-linux clarifying that `tauri:build:linux:x64` intentionally enables cross-build mode (`ALLOW_LINUX_CROSS=1`)
- [x] linux: main window close/minimize/maximize buttons unresponsive (tauri-apps/tauri#11856) — workaround: fullscreen toggle after every `show()` call forces compositor to re-evaluate decorations
- [x] custom titlebar: implement custom titlebar for all windows with minimize, maximize, and close buttons
- [x] linux window close: on linux, closing main window should hide it instead of exiting; only quit via tray "Quit" with confirmation
- [x] custom titlebar on all windows: extend custom titlebar to settings, help, search, history, calibration, chat, downloads, about, compare, labels, label, api, onboarding, whats-new, focus-timer, and session detail windows
- [x] onboarding: show big green checkmark when all recommended models are downloaded with explanation and clickable link to settings for more options

- [x] change `Mutex<AppState>` to `Mutex<Box<AppState>>` across the entire Rust codebase to heap-allocate AppState and reduce stack frame size
- [x] extract all LLM fields from `AppState` into a `Box<LlmState>` sub-struct (`llm_config`→`llm.config`, `llm_catalog`→`llm.catalog`, `llm_downloads`→`llm.downloads`, `llm_logs`→`llm.logs`, `llm_state_cell`→`llm.state_cell`, `llm_loading`→`llm.loading`, `llm_start_error`→`llm.start_error`, `chat_store`→`llm.chat_store`)
- [x] add `AppState::new_boxed()` that constructs AppState on a dedicated 32 MiB thread to prevent main-thread stack overflow during initialization
- [x] add macOS/Linux main-thread stack size linker flags (32 MB) via `cargo:rustc-link-arg-bins` in `build.rs` — applies only to the final executable (not the lib crate), fixing `ld: -stack_size option can only be used when linking a main executable`
- [x] use `stacker` crate in `main.rs` to dynamically grow the main-thread stack to 64 MiB before calling `run()` — works on macOS (preserves main thread identity for Cocoa), reliable regardless of linker flag support
- [x] add macOS `.app` manual assembly fallback in `scripts/tauri-build.js` — when the Tauri CLI bundler stack-overflows (exit 134/139) during bundling, assemble the `.app` bundle from the already-built release binary, Info.plist, icons, entitlements, and resources using `ditto` + `codesign`
- [x] add standalone `scripts/assemble-macos-app.sh` and `npm run tauri:build:mac:app` for reliable macOS `.app` bundling that bypasses the Tauri CLI bundler entirely
- [x] fix missing `CFBundleIconFile` and `NSHighResolutionCapable` in manual `.app` assembly (`assemble-macos-app.sh` and `tauri-build.js` fallback) so the app icon shows in Dock/Finder/Spotlight
- [x] add `scripts/create-macos-dmg.sh` and `npm run tauri:build:mac:dmg` — single-pass `appdmg` with branded background (Pillow, dark/light mode adaptive), version-badged volume icon, app + Applications + README/CHANGELOG/LICENSE with icon positions, ULFO+APFS, codesign + notarize
- [x] use `scripts/create-macos-dmg.sh` in `release-mac.yml` and `pr-build.yml` — CI installs `appdmg` + Pillow, replaces inline DMG/signing logic
- [x] add `scripts/create-windows-nsis.ps1` and `npm run tauri:build:win:nsis` for standalone Windows NSIS installer creation — generates installer images (header + welcome panel with icon + version via Pillow), includes README.md, CHANGELOG.md, LICENSE, resources, Start Menu + Desktop shortcuts, Add/Remove Programs registry, optional code signing
- [x] fix `assemble-macos-app.sh` Info.plist — use `plistlib` for correct boolean types (`<true/>` not `<string>true</string>`), add `CFBundleName`, `CFBundleDisplayName`, `LSMinimumSystemVersion`, `NSRequiresAquaSystemAppearance`
- [x] extract `setup_app()` and `setup_background_tasks()` as `#[inline(never)]` functions from the `run()` closure to prevent LLVM from merging their locals into the massive `generate_handler!` stack frame
- [x] move LLM chat history database from `skill_dir/chat_history.sqlite` into `skill_dir/chats/chat_history.sqlite` subdirectory with automatic migration of legacy DB and WAL/SHM sidecar files
- [x] fix LLM chat messages not being persisted — `save_chat_message` invoke used snake_case `session_id` key but Tauri v2 command macro expects camelCase `sessionId`; messages were silently dropped because `.catch(() => {})` swallowed the deserialization error

- [x] show model name in the chat window's custom titlebar — added `chat-titlebar.svelte.ts` shared reactive store exposing `modelName` and `status`; chat page syncs both via `$effect`; `CustomTitleBar.svelte` detects the `chat` window label and renders a centered status dot (green/amber/grey) + model name in the OS-level titlebar; removed the duplicate model name from the in-page chat header to avoid redundancy

- [x] replace context usage bar with circular progress next to tools button — removed the separate always-visible context usage bar below the chat header; a compact circular SVG progress ring (green → amber → red) with percentage text now sits next to the tools badge as its own element; full token details in tooltip on hover; tools button restored to its original wrench icon + label + count

- [x] remove tools allow-list from the parameters panel — the tool toggles and execution mode selector were duplicated in both the parameters panel and the dedicated tools panel; removed the copy from parameters so tools are only configured via the dedicated tools panel opened by the tools badge button

- [x] ensure bash command is always visible in tool cards — include the `command` field in the bash tool result JSON so even when `args` from `ToolExecutionStart` is empty, the UI can extract the command from the result; header summary and expanded detail view now check both `tu.args.command` and `tu.result.command` as fallback

- [x] fix bash tool calls with empty arguments — when the model emits `[TOOL_CALL]{"name":"bash","arguments":{}}[/TOOL_CALL]` alongside a `` ```bash `` code fence, the command is now extracted from the fence and injected into the empty args; also accept `"tool"` as alias for `"name"` and `"parameters"` as alias for `"arguments"` in `[TOOL_CALL]` blocks; 24/24 Rust tests pass including 4 new tests

- [x] fix scripts storage: stop creating empty per-server-start directories — `scripts_dir` is now the base `chats/scripts/` path; `run_<ts>/` subdirectories are created lazily only when a tool actually writes a script or output file, preventing empty directories from accumulating

- [x] fix tool call UI showing empty `{}` arguments and duplicate tool executions — tool cards no longer show "ARGUMENTS: {}" for tools called with empty args (empty objects are no longer considered "has details"); bash tool calls with missing `command` argument are now silently filtered out instead of executing and erroring; cross-round dedup prevents the model from re-executing the exact same tool call with identical arguments in subsequent rounds; if all extracted tool calls are filtered out, the model's text response is returned directly instead of entering an empty tool-execution phase

- [x] fix model not calling tools on 4096 context — compact tool prompt threshold lowered from ≤4096 to ≤2048 so models with 4096 context get the full prompt with parameter docs and examples instead of the terse compact version; compact prompt also improved with inline examples so even very small contexts (≤2048) give the model enough format guidance to emit `[TOOL_CALL]` blocks

- [x] improve expandable tool-call cards with per-tool detail views — bash shows the full command under a "Command" label, file tools show the path plus find/replace or content sections, web_search shows the query, web_fetch shows the URL; cards are now expandable whenever any detail is available (args, result, or detail text); all pre blocks have `select-text` for easy copying; 6 new i18n keys across all 5 locales

- [x] fix LLM not calling tools (showing commands in code blocks instead) — strengthened tool-calling system prompt instructions to explicitly forbid showing commands in code blocks and require `[TOOL_CALL]` usage; added bash/sh/shell/zsh code fence fallback extraction so if the model still emits a bare ````bash` fence, it's automatically converted into a bash tool call; fallback only fires when no proper `[TOOL_CALL]` blocks are present to avoid duplicates; added more examples (desktop files, location); 21/21 Rust tests pass including 3 new fallback tests

- [x] add chat session archive with soft-delete — default action on chat sessions is now "archive" instead of permanent delete; archived sessions are hidden from the main list and shown in a collapsible "Archive" section at the bottom of the sidebar; archive section has restore (unarchive) and permanent delete buttons per session; backend adds `archived` column to `chat_sessions` table with automatic migration, plus `archive_session`, `unarchive_session`, `list_archived_sessions` methods and corresponding Tauri commands; full i18n across all 5 locales (en/de/fr/he/uk)

- [x] fix chat window bottom corners not rounded — added `border-radius: 10px` to `body` and `border-radius: 0 0 10px 10px; overflow: hidden` to `#main-content` in `app.css`, plus `rounded-b-[10px]` on the chat page root container, so the bottom corners properly clip to the window's rounded shape on all platforms

- [x] auto-label typed text in chat window every 5 seconds — when the user types (not copy-paste) in the chat input, a 5-second interval (matching the EXG model's EPOCH_S window size) accumulates only keyboard-originated characters via the `beforeinput` event; at the end of each window, if any words were typed, a label is submitted with the typed words as text and the current chat session (session ID, model name, recent messages) as context; timer starts on mount, flushes remaining text on destroy; paste/drop/autocomplete inputs are excluded; word-boundary aware flushing defers the label submission until the current word is complete (next space/punctuation/Enter) with a 1.5 s safety timeout if the user stops typing mid-word; deleted words are tracked via `beforeinput` delete events and wrapped in `<del>…</del>` tags in the label text so downstream consumers can see which words were typed then removed within the same window
