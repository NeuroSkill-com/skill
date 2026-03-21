### Bugfixes

- **LLM skill tool args coercion**: When an LLM flattens command arguments to the top level (e.g. `{"command":"search_screenshots","query":"today"}` instead of `{"command":"search_screenshots","args":{"query":"today"}}`), the coercion layer now automatically wraps stray properties into the `args` object before validation. This prevents validation errors for all `skill` tool commands.

### Docs

- **LLM tool call examples in skills**: Added `## LLM Tool Calls` sections with concrete JSON examples to all skill SKILL.md files (screenshots, labels, search, sessions, sleep, hooks, DND, streaming). This helps the LLM use the correct `{"command": "...", "args": {...}}` format.

- **Improved skill tool description**: Updated the `skill` tool description and `args` field description with explicit examples showing the `command` + `args` nesting pattern.
