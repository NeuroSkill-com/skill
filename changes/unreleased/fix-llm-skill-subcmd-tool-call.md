### Bugfixes

- **LLM skill sub-command auto-redirect**: When the LLM calls a Skill API sub-command (e.g. `status`, `say`) or a `neuroskill` alias (e.g. `neuroskill`, `neuroskill-status`, `neuroskill-hooks`) as a top-level tool, the call is silently rewritten to `skill` with the correct `{"command": "..."}` at three layers: extraction (parse.rs), validation (tool_orchestration.rs), and execution (exec.rs).
- **LLM dedup loop fix**: When the model re-emits the same tool call on round 2 (all calls deduped), instead of returning empty text, the orchestrator now injects a nudge message telling the model to summarize the existing results, then continues to a new inference round.
- **Tool result not misdetected as tool call**: JSON objects with `"ok"` or `"command"` keys (tool results) are no longer falsely extracted as tool calls when the model quotes them in its response.

### LLM

- **`neuroskill` tool alias**: `neuroskill` is recognized as an alias for `skill`. Hyphenated forms like `neuroskill-status`, `neuroskill-sessions`, `neuroskill-hooks` map to the corresponding API command.
- **Skill tool description restructured**: Compact grouped categories instead of a flat list of 30+ commands. The `command` parameter includes a JSON Schema `enum` constraint.
- **Skill tool call examples added**: Both full and compact tool prompts include an explicit `skill` calling example.
