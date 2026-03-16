### Features

- **Tool-call logging toggle in Settings**: Added a `tools` flag to the logging configuration so users can enable/disable tool-call logging from Settings. The toggle controls the `skill-tools::log` subsystem which traces tool invocations, safety approvals, completion times, and errors. Wired into the central `SkillLogger` with `init_tool_logger` callback and `set_tool_logging` runtime toggle. Added i18n strings for en, de, fr, uk, he.
