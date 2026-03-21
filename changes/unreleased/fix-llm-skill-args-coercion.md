### Bugfixes

- **LLM skill tool args coercion**: When an LLM flattens command arguments to the top level (e.g. `{"command":"search_screenshots","query":"today"}` instead of `{"command":"search_screenshots","args":{"query":"today"}}`), the coercion layer now automatically wraps stray properties into the `args` object before validation. Also handles the common `"arguments"` misspelling as an alias for `"args"`. This prevents validation errors for all `skill` tool commands.

- **Missing skill commands in tool enum**: Added `sleep_schedule`, `sleep_schedule_set`, `health_summary`, `health_query`, `health_metric_types`, `health_sync`, `search_screenshots_vision`, and `search_screenshots_by_image_b64` to the skill tool command enum, description, and `is_skill_api_command()`. These WS commands were functional but invisible to the LLM.

### Docs

- **LLM tool call examples in skills**: Added `## LLM Tool Calls` sections with concrete JSON examples to all skill SKILL.md files (screenshots, labels, search, sessions, sleep, hooks, DND, streaming). This helps the LLM use the correct `{"command": "...", "args": {...}}` format.

- **Improved skill tool description**: Updated the `skill` tool description and `args` field description with explicit examples showing the `command` + `args` nesting pattern. Added SLEEP SCHEDULE and HEALTH command groups to the description.
