### Bugfixes

- **LLM skill sub-command auto-redirect**: When the LLM mistakenly calls a Skill API sub-command (e.g. `status`, `sessions`) as a top-level tool, the call is now silently rewritten to `skill` with `{"command": "..."}` at the validation layer. This covers both sequential and parallel execution paths and eliminates wasted error round-trips.

### LLM

- **Skill tool description clarified**: The `skill` tool description now explicitly states "do NOT call the command names as separate tools" to reduce the chance of misdirected calls.
