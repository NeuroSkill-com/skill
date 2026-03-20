### Refactor

- **Split ws_commands/mod.rs into sub-modules**: Extracted calibration (7 commands), health (4 commands), and screenshot (2 commands) handlers into dedicated `calibration.rs`, `health.rs`, and `screenshots.rs` sub-modules. Reduced `mod.rs` from 1168 to 873 lines while keeping the dispatch table as the single routing point.

- **Clean up TODO.md**: Removed 4 completed items, keeping only active/open tasks.
