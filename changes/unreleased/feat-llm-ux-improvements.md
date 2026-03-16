### Features

- **LLM auto-start on launch**: Added `autostart` field to `LlmConfig`. When enabled + a model is downloaded and selected, the LLM server starts automatically during app setup with a 500ms delay to let the UI render first. Toggle added to the LLM settings tab.
- **Atomic model switch**: New `switch_llm_model` Tauri command that atomically stops the running server, waits for full shutdown, sets the new active model, and starts the new one — eliminating the fragile 150ms sleep race in the frontend.
- **Abort feedback in chat**: The stop/abort button now shows a spinner and "Aborting…" state while the abort is in flight, and is disabled to prevent double-clicks.
- **Context window warning**: A warning banner appears above the chat input when context usage exceeds 85% (amber) or 95% (red), showing the current usage percentage.
- **Per-session generation params**: Temperature, max tokens, top-k, top-p, and thinking level are now saved per chat session (new `params` column in `chat_sessions` table). Params auto-save on change (debounced 500ms) and restore when switching sessions.
- **Regenerate button**: Hover over the last assistant message to see a "Regenerate" button that removes the response and re-sends the last user message with current params.
- **Edit & resend on user messages**: Hover over any user message to see "Edit & resend" which populates the input with that message's text and removes all subsequent messages.
- **Live tok/s indicator**: During streaming, a live tokens-per-second counter is shown below the assistant message. After completion, the final tok/s is included in the timing line alongside TTFT and token counts.
- **Open LLM settings from chat**: The empty chat state (when server is stopped) now includes an "Open LLM settings" button alongside "Start server".
- **Reduced settings panel height**: Chat settings and tools panels reduced from 50vh to 40vh max to leave more room for the message list on small screens.

### UI

- **Log viewer filtering**: The LLM server log in the settings tab now has level filter tabs (All / Info / Warn / Error) and a text search box. The line count shows filtered/total when a filter is active, and "No matching lines" is shown instead of "No log output yet" when filtering produces no results.

### i18n

- Added `llm.autostart`, `llm.autostartDesc` keys to all 5 locales (en, de, fr, uk, he).
- Added `chat.btn.aborting`, `chat.btn.regenerate`, `chat.btn.editResend`, `chat.tokSec`, `chat.ctxWarning`, `chat.noModelBtn`, `chat.logFilter.*` keys to en locale.

### Bugfixes

- Fixed trailing garbage bytes in `crates/skill-tools/src/types.rs` that could cause a compilation failure.
