### Features

- **Separate tool-call logger**: Added a standalone pluggable logger (`skill-tools::log`) for tool-call tracing, following the same pattern as `skill-llm::log`. Logs tool invocations (name + args), completion times, safety approval events, and errors. Use `set_log_callback` to route output to the app logger and `set_log_enabled` to toggle at runtime. The `tool_log!` macro short-circuits formatting when logging is disabled.
