### Bugfixes

- **Fix clippy warnings**: Removed unused `std::io::Cursor` import in `skill-screenshots`, changed doc comment to plain comment in `session_runner.rs`, replaced `map_or(true, …)` with `is_none_or(…)` in LLM download/server commands, and used `matches!` macro in `session_connect.rs`.
