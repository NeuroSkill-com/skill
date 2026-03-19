### Bugfixes

- **LLM skill sub-command auto-redirect**: When the LLM mistakenly calls a Skill API sub-command (e.g. `status`, `sessions`) as a top-level tool, the orchestrator now silently rewrites the call to `skill` with the correct `{"command": "..."}` payload instead of returning an error. This eliminates wasted round-trips entirely.

### LLM

- **Skill tool description clarified**: The `skill` tool description now explicitly states "do NOT call the command names as separate tools" to reduce the chance of misdirected calls.
- **Targeted error for unknown skill commands**: If an unknown tool name matches a known Skill API command in exec.rs, the error message now points the LLM to the correct `skill` tool usage.
