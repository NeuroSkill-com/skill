### Bugfixes

- **LLM skill sub-command mismatch**: When the LLM mistakenly calls a Skill API sub-command (e.g. `status`) as a top-level tool, the error now tells it to use `skill` with `{"command": "status"}` instead of the generic "unsupported tool" message, eliminating a wasted round-trip.

### LLM

- **Skill tool description clarified**: The `skill` tool description now explicitly states "do NOT call the command names as separate tools" to reduce LLM confusion.
