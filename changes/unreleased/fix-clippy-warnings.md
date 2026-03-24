### Bugfixes

- **Fixed 2 broken tests in `src-tauri/src/constants.rs`**: The `embedding_overlap_samples_correct` and `embedding_hop_samples_correct` tests were asserting stale values (640) after `EMBEDDING_OVERLAP_SECS` changed from 2.5 to 0.0. Tests now derive expected values from the actual constants.

- **Fixed compilation error in `skill-headless`**: Two `eval_fire()` calls passed `reply` by value instead of by reference.

### Refactor

- **Eliminated all clippy warnings across the workspace**: Resolved every warning from `cargo clippy --workspace` — from 500+ warnings to zero.

  - Converted 21 `match` → `let...else` patterns across 10 files (api.rs, screenshot_cmds.rs, ws_commands, device_scanner, session_runner, session_connect, eeg_embeddings, label_cmds, skill-skills/sync, skill-llm/actor).
  - Replaced 17 `lock().expect("lock poisoned")` calls with `lock_or_recover()` across skill-llm (handlers, logging, state, tool_orchestration) and src-tauri/api.
  - Applied `cargo clippy --fix` auto-fixes: redundant closures, method call simplifications, let-else conversions.
  - Added `// SAFETY:` comments to remaining undocumented `unsafe` blocks (skill_log, quit, active_window, window_cmds).
  - Added `#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]` to `build.rs` (build-time panics are standard practice).
  - Added `#![allow(clippy::unwrap_used)]` to `image_encode_bench.rs` (benchmark binary).
  - Disabled `needless_pass_by_value` lint (280 false positives from Tauri `#[command]` handlers).
  - Disabled `expect_used` lint (50 legitimate uses in thread spawning, NonZero constructors, Tauri app builder — all unrecoverable).
