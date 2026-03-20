### Refactor

- **Move hooks CRUD + keyword suggestions into `hook_cmds.rs`**: Extracted `sanitize_hook`, `get_hooks`, `set_hooks`, `get_hook_statuses`, `suggest_hook_keywords` (with helper functions `norm_keyword`, `fuzzy_score`, `merge_suggestion`) from `settings_cmds/mod.rs` into the existing `hook_cmds.rs` sub-module. `mod.rs` reduced from 959 to 761 lines.
