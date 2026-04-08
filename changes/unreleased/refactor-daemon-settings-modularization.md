### Refactor

- **Modularize daemon settings routes by domain**: split `crates/skill-daemon/src/routes/settings.rs` into focused modules (`settings_io`, `settings_lsl`, `settings_llm`, `settings_llm_runtime`, `settings_llm_chat`, `settings_exg`, `settings_hooks_activity`) while preserving existing route paths and handler behavior.
- **Compose settings router from subrouters**: introduced grouped route composition (`exg_routes`, `llm_routes`, `lsl_routes`) to reduce `settings.rs` complexity and make route ownership auditable.
- **Simplify command dispatch structure**: refactored `crates/skill-daemon/src/cmd_dispatch.rs` to use grouped family dispatch for hooks/sleep-schedule/dnd/iroh/llm commands, reducing top-level `match` size.
- **Make device connector routing table-driven**: updated `session/connect.rs` to use an explicit `ConnectRoute` selector path and added route-selection coverage for aliases/prefixes.

### Server

- **Keep Tauri daemon proxy behavior stable during cleanup**: reduced boilerplate in `src-tauri/src/daemon_cmds.rs` by adding shared async proxy helpers while preserving existing endpoint contracts.

### Bugfixes

- **Protect EXG route extraction with tests**: added smoke + edge coverage for extracted EXG model routes (config/status/catalog/reembed/rebuild/estimate, duplicate-download rejection, cancel flag handling) to prevent regressions during modularization.
- **Harden runtime/dispatch/connect contracts**: added command-matrix coverage in `cmd_dispatch`, deterministic and overlap checks for connect route selection, and additional LLM runtime state transition tests to improve reliability under refactors.
