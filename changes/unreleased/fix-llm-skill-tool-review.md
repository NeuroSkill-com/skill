### Bugfixes

- **Coerced tool arguments now reach execution**: The tool orchestrator validated and coerced LLM arguments (e.g. wrapping flat `query` into `args`) but discarded the coerced value before calling `execute_builtin_tool_call`. The executor re-parsed the original un-coerced string, so flattened skill args like `{"command":"search_screenshots","query":"today"}` passed validation but still failed at runtime because `args.get("args")` found nothing. Fixed both sequential and parallel execution paths to write coerced arguments back to `tc.function.arguments` before execution.

- **Generic hyphenated CLI name resolution**: Added a generic fallback in `resolve_skill_alias` that converts any hyphenated tool name to underscored form and checks if it's a valid skill API command. This catches LLMs copying CLI names from docs (e.g. `search-labels`, `session-metrics`, `sleep-schedule`, `dnd-set`) without needing to enumerate every variant.

- **Block LLM download/management commands**: Added `llm_download`, `llm_cancel_download`, `llm_pause_download`, `llm_resume_download`, `llm_refresh_catalog`, and `llm_logs` to the BLOCKED list in skill tool execution. These LLM self-management commands should not be callable from the LLM itself.

### Docs

- **Status SKILL.md**: Updated to document the new `apps` (top apps by window switches), `labels.top_*` (most frequent label texts), and `screenshots` (OCR counts, top apps) fields in the status response. Added LLM Tool Calls section with guidance on using `status` for app usage queries. Fixed JSON response example to show correct field names (`switches`, `last_seen`, `last_used`).
