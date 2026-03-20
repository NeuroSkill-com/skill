### Refactor

- **Split AppState into domain sub-states**: Extracted `ShortcutState`, `UiPrefsState`, `InputTrackingState`, and `EmbeddingModelState` from the monolithic 50+ field `AppState` struct. Fields are now accessed via `state.shortcuts.*`, `state.ui.*`, `state.input.*`, and `state.embedding.*`. This improves code organization and prepares for future independent locking to reduce mutex contention.

### Bugfixes

- **Fix missing `format` field in LlmModel**: Added the required `format: ModelFormat::Gguf` field to the `LlmModel` constructor in `hardware_fit.rs`, fixing a compilation error introduced by an upstream `llmfit-core` dependency update.
