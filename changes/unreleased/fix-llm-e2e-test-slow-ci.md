### Performance

- **LLM E2E test: reduce CI time from ~15 min to ~2-3 min**: Lowered `max_tokens` from 512 to 128 and `max_rounds` from 2 to 1 for tool-chat test steps. The 1.6B model on CPU was generating max tokens every round at ~1.6 tok/s, and hallucinating calls to disabled tools caused extra rounds. Also added "Do NOT call any other tool" to system prompts to reduce hallucinated tool calls.
