### Refactor

- **Fix clippy warnings in main crate**: Replaced `match` with `if let` in `worker.rs`, derived `Default` for `DndRuntimeState`, and used struct initializer for `InputTrackingState` in `state.rs`.
