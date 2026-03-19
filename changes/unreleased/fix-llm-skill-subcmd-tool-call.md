### Bugfixes

- **LLM skill sub-command auto-redirect**: When the LLM calls a Skill API sub-command (e.g. `status`, `say`) or a `neuroskill` alias (e.g. `neuroskill`, `neuroskill-status`, `neuroskill-hooks`) as a top-level tool, the call is silently rewritten to `skill` with the correct `{"command": "..."}` at three layers: extraction (parse.rs), validation (tool_orchestration.rs), and execution (exec.rs). This eliminates wasted error round-trips regardless of which code path handles the call.

### LLM

- **`neuroskill` tool alias**: `neuroskill` is now recognized as an alias for the `skill` tool. Hyphenated forms like `neuroskill-status`, `neuroskill-sessions`, `neuroskill-hooks` are mapped to the corresponding API command. This prevents errors when the LLM reads community skill files that reference `npx neuroskill <cmd>` and attempts to call those names as tools.
- **Skill tool description restructured**: The `skill` tool description is now compact and grouped by category instead of a flat list of 30+ commands. The `command` parameter includes a JSON Schema `enum` constraint listing all valid command names.
- **Skill tool call examples added**: Both the full and compact tool prompt blocks now include an explicit example showing the correct `skill` calling convention.
