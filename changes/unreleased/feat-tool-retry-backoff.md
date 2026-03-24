### Features

- **Network retry with exponential backoff**: Web tools (`web_search`, `web_fetch`, `location`) now automatically retry on transient errors (HTTP 429/5xx, connection failures) with configurable exponential backoff. The retry policy is exposed in Settings > Tools with controls for max retries (0-3) and base delay (500-3000 ms). Default: 2 retries with 1 s base delay.

- **Per-round usage tracking**: Tool orchestration now emits `ToolEvent::RoundComplete` events with cumulative `prompt_tokens`, `completion_tokens`, and `tool_calls_count` after each inference + tool-execution round, enabling cost/usage observability in agentic loops.

- **Increased default max tool rounds**: Bumped the default maximum tool-calling rounds from 10 to 15, giving the agent more room for complex multi-step tasks. The Settings UI now includes a 15-round option.

### Refactor

- **`retry_with_backoff` helper**: Added a generic `retry_with_backoff(max_retries, base_delay, closure)` utility in `skill-tools::exec::helpers`, reusable across any blocking I/O path. Exported from the crate root.

- **`ToolRetryConfig` struct**: New `ToolRetryConfig { max_retries, base_delay_ms }` in `LlmToolConfig`, serialisable and configurable per-user. Wired through to `exec_location`, `exec_web_fetch_plain`, and `exec_web_search`.

### UI

- **Retry settings in Tools tab**: New "Network retry" section in Settings > Tools with button-group selectors for max retries and base delay, matching the existing visual style.

### i18n

- **Retry setting strings**: Added `llm.tools.retry*` keys to all five locales (en, de, fr, he, uk).
