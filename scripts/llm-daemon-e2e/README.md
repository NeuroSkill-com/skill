# LLM daemon end-to-end suite

Drives the **real** `skill-daemon` over HTTP — the same OpenAI-compatible
requests the app sends — and asserts the behaviours that unit tests cannot see.

```sh
# build a GPU-capable daemon once (plain `cargo build -p skill-daemon` omits it)
cargo build --release -p skill-daemon --no-default-features --features apple

npm run test:llm:daemon-e2e -- q35 /path/Qwen3.5-0.8B-Q4_K_M.gguf 18470
#                               label  gguf                        port
# optional: release|debug  tools|notools  '{"ENV_VAR":"value"}'
```

Knobs: `TURNS` (default 8), `LONG_CAP` (384), `SOAK_SECS` (120),
`E2E_RUNS` (scratch dir).

## What it covers, and why each check exists

| check | the bug it would have caught |
|---|---|
| single turn, realistic `max_tokens`, non-empty content | an 8-token budget is swallowed whole by the think prefix, so a tiny cap reports `content: ""` and looks like a stripping bug |
| prompt-token count reported | the daemon's tool prompt is ~1.4k–5.2k tokens before the user types, which is what pushed every request past the old 256-token decode-bucket ceiling |
| SSE deltas timed through `curl -N` | `urllib` response iteration buffers, so an in-process client reports every delta arriving at the end and blames the server for buffering it does not do |
| 8 turns, each preceded by the UI's `max_tokens:0` warm pass, history growing | the chat page fires that warm pass while you type; and multi-turn is where a stale/spliced KV cache shows up (`cache invariant violated`), which a single turn never reaches |
| sustained generation + tok/s | throughput regressions |
| 3 concurrent requests | the actor is single-threaded; requests must queue, not drop |
| idle soak, then another turn | distinguishes a transient load-time allocation from a leak |
| per-phase RSS series | attributes memory growth to load / stream / multiturn / soak instead of guessing |
| daemon liveness + `weight not found` + panics in the log | a panic used to kill the inference actor silently, leaving the log dead after `chat request — …` |

Each run uses its own `SKILL_DAEMON_CONFIG_ROOT` / `SKILL_DATA_DIR`, so it never
touches your real profile, and waits for the port to free before binding.
