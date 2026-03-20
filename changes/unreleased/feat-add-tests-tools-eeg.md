### Features

- **Add tests for untested modules**: Added 61 new unit tests across 4 previously untested modules:
  - `skill-eeg/eeg_model_config` (13 tests): config defaults, JSON round-trip, persistence save/load, corrupt file handling.
  - `skill-tools/types` (18 tests): `LlmToolConfig` defaults, dangerous tool safety, serialization, `ToolContextCompression` levels.
  - `skill-tools/defs` (21 tests): builtin tool definitions integrity, `is_builtin_tool_enabled` toggle logic, `resolve_skill_alias` routing.
  - `skill-tools/context` (9 tests): token estimation, context trimming, compression levels, system/user message preservation.
