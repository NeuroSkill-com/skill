### Bugfixes

- **LLM skill sub-command auto-redirect**: When the LLM calls a Skill API sub-command (e.g. `status`, `say`, `sessions`) as a top-level tool, the call is now silently rewritten to `skill` with `{"command": "..."}` at three layers: extraction (parse.rs), validation (tool_orchestration.rs), and execution (exec.rs). This eliminates wasted error round-trips regardless of which code path handles the call.

### LLM

- **Skill tool description restructured**: The `skill` tool description is now compact and grouped by category instead of a flat list of 30+ commands. The `command` parameter now includes a JSON Schema `enum` constraint listing all valid command names, helping models with structured output select from the correct set.
- **Skill tool call examples added**: Both the full and compact tool prompt blocks now include an explicit example showing the correct `skill` calling convention (`{"name":"skill","arguments":{"command":"status"}}`), with a warning not to call command names directly.
