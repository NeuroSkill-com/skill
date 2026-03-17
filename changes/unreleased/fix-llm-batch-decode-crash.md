### Bugfixes

- **Fix LLM crash when prompt exceeds n_batch**: Long prompts triggered a fatal `GGML_ASSERT(n_tokens_all <= cparams.n_batch)` abort in llama.cpp, killing the entire process. The prompt is now decoded in chunks of `n_batch` tokens, preventing the native assertion failure.

### CLI

- **LLM error diagnostics in CLI**: The `llm chat` command (both single-shot and REPL modes) now classifies known LLM error patterns (batch overflow, context window exceeded, decode failures, template errors, native panics, tokenization failures) and prints actionable hints alongside the error message.
