### Bugfixes

- **Calendar Linux/Windows dedup**: `linux.rs` `parse_ics_file` had a two-pass deduplication bug where the first loop inserted all UIDs into the shared `seen` set before the second loop could check them, causing every event with a non-empty UID to be silently dropped on Linux. Both `linux.rs` and `windows.rs` now use a single-pass atomic check-and-insert. Added two regression tests (`dedup_across_files_via_seen_set`, `dedup_anonymous_events`).

- **LLM e2e test**: `llm_e2e.rs` match on `ToolEvent` was non-exhaustive after `RoundComplete { .. }` was added to the enum; added the missing arm.
