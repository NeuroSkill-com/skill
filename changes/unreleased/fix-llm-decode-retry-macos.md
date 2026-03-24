### Bugfixes

- **LLM decode retry on transient Metal failure**: When the initial prompt decode fails (commonly seen on macOS as "decode error on prompt (batch at token 0)"), the engine now clears the KV cache, waits briefly, and retries the full prompt once before reporting an error. This handles transient Metal GPU failures (busy command buffer, timeout) that previously required restarting the LLM. The same retry logic applies to multimodal (mtmd) eval.
