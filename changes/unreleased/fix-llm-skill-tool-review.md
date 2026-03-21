### Bugfixes

- **Block LLM download/management commands**: Added `llm_download`, `llm_cancel_download`, `llm_pause_download`, `llm_resume_download`, `llm_refresh_catalog`, and `llm_logs` to the BLOCKED list in skill tool execution. These LLM self-management commands should not be callable from the LLM itself.

### Docs

- **Status SKILL.md**: Updated to document the new `apps` (top apps by window switches), `labels.top_*` (most frequent label texts), and `screenshots` (OCR counts, top apps) fields in the status response. Added LLM Tool Calls section with guidance on using `status` for app usage queries. Fixed JSON response example to show correct field names (`switches`, `last_seen`, `last_used`).
