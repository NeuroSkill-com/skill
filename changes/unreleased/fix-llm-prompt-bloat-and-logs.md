### Performance

- **The Agent Skills block is now relevance-capped, like tools already were.** Turning on `read_file` injects a catalogue of every discovered skill so the model knows what it can load — 33 skills measured at ~3,900 prompt tokens, on top of ~1,800 for the tool block, so a one-word "hi" arrived as ~5,600 prompt tokens. Tools were already cut to the top-N most relevant per prompt; skills were not, even though they are the larger half. They now go through the same `intelligent_selection` setting and the same top-N, which more than halves the block at 33 skills. This is paid on every turn, three times over: prefill latency before the first byte, a prefill graph sized for the larger prompt, and small models answering the system prompt instead of the user.

### Bugfixes

- **Settings → LLM → Logs stopped losing lines.** That view reads only an in-memory ring buffer — never the on-disk `llm_logs/` file — and the ring held 500 entries. One session with tools on overruns that easily, because a single orchestration round logs a line per tool call, per result and per round, so the `chat request — …` line for an earlier turn was already evicted by the time anyone looked for it. It read as "the request was never logged". The ring now holds 5,000 entries; they are short strings, so the cost is a few MB.
