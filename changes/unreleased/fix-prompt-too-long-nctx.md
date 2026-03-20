### Bugfixes

- **Fix "prompt too long" error when prompt exceeds n_ctx**: The LLM engine now automatically trims older chat history messages (keeping system prompt and latest user turn) when the tokenized prompt exceeds the context window budget. Previously, conversations would fail with "prompt too long (N ≥ n_ctx M)" once history grew past the context limit.

### Performance

- **Raise minimum auto context size from 2048 to 4096**: The `recommend_ctx_size` heuristic no longer returns 2048 as a minimum, which was too small for most conversations with system prompts and tool results. The new floor is 4096 tokens.
