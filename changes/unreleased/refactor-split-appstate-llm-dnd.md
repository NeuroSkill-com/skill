### Refactor

- **Split AppState into domain sub-states**: Extracted `LlmState` and `DndRuntimeState` behind their own `Arc<Mutex<>>` inside `AppState`, eliminating lock contention between LLM operations (model loading, chat streaming, downloads) and the EEG/device hot path. LLM and DND code now acquires independent locks without blocking device status reads or UI commands. Reduced the `new_boxed()` stack allocation from 32 MB to 8 MB.

### Bugfixes

- **Replace `.lock().expect("lock poisoned")` with `.lock_or_recover()`**: Converted all 37 occurrences of panicking mutex locks across 6 files (`api.rs`, `lib.rs`, `settings_cmds`, `ws_commands`, `llm/cmds/server.rs`, `llm/cmds/streaming.rs`) to use the poison-recovering `MutexExt::lock_or_recover()` trait. The app will now gracefully recover from poisoned locks instead of crashing.
