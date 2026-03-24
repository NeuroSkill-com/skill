# TODO

- [ ] caching for crawled domains, or queries, with TTL and when to refresh. Maybe a unified list of domains and rules on when to refresh. Optimize for speed and precision.

- [ ] in the tool calling, allow to edit bash commands before they are executed.

- [ ] record SNR too, so we can filter out by it later.

- [ ] send raw messages to the LLM actor and let it apply the model's built-in chat template via `model.apply_chat_template()` (see `skill-llm/src/handlers.rs`).

- [x] tool-call self-healing: re-prompt the model when it emits a garbled tool call that cannot be parsed, injecting a corrective message with the raw output and asking it to re-emit in the correct format (use the existing multi-round loop).

- [x] parse `<function=name><parameter=key>value</parameter></function>` XML tool-call format (Llama-family models) in `extract_tool_calls()`.

- [x] workspace-wide lint configuration (clippy::unwrap_used, undocumented_unsafe_blocks, etc.)

- [x] split `skill-tools/src/parse.rs` (2441 lines) into focused sub-modules (types, coerce, validate, extract, strip, inject, json_scan).

- [x] add SAFETY comments to all `unsafe` blocks in skill-vision, skill-gpu, skill-llm, skill-screenshots.

- [x] add `cargo audit` to CI pipeline.

- [x] add integration tests for DSP pipeline (skill-eeg), headless browser (skill-headless), and screenshots (skill-screenshots).

- [x] extract search page business logic into `search-logic.ts` with tests.
