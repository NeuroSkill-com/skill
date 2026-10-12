### Bugfixes

- **Local chat no longer dies silently mid-request**: a panic inside a model kernel unwound the inference actor, so the request's stream just ended — no error to the caller, no status event, and a log that stopped dead after `chat request — …`. Each request now runs behind a panic boundary: the failure is reported on the request and in the LLM log, and the server keeps serving.
- **Replies containing an accent, CJK or an emoji could kill the actor**: the token streamer tracked how much it had sent as a byte offset into the previous decode. Byte-level BPE revises earlier bytes when a codepoint spans tokens — a 3-byte replacement char becomes a 2-byte `é` — so that offset could land mid-character and the slice panicked. English-only output never tripped it. The cursor is now resynchronised on whole characters.
- **Qwen3 models failed every chat with `weight not found: model.embed_tokens.weight`**: the runner's context was left at the library default of 128 tokens, which sized its decode-bucket ladder to 256. One token past that, decoding fell back to a path that needs F32 weights a packed GGUF does not have. The error named a tensor and looked like a GGUF/safetensors mismatch; it was a context cap. Context is now passed through, as it already was for Qwen3-VL and Qwen3.5.
- **Small models answered with the same phrase over and over**: the stop signal a caller returns from its token callback was discarded on the Qwen3 path, so chat EOS, user stop strings and the repetition guard were all ignored and generation always ran to the token cap. Qwen3-0.6B asked for the capital of France produced 303 tokens of "The capital of France's capital of France's …". It also made `finish_reason` permanently `length`.
- **Loading a model the catalogue does not describe budgeted memory for the wrong one**: context was sized from the active catalogue entry even when a different file was being loaded (a path set directly, a discovered model, `--model`). Loading a 1-bit 8B while the catalogue still described a 0.8B picked a 16K context for the smaller model and drove the daemon to 16.5 GB resident and 24.6 GB of swap. Mismatches are now detected, warned about, and fall back to a conservative context.
- **`max_tokens` sent by an OpenAI-compatible client had no effect**: it was forwarded under a key nothing reads, so every such request silently ran to the 2048 default.
- **Web search left a panic in the log on every call**: the headless renderer tried to create its event loop on a background thread, which macOS forbids, then reported an unrelated timeout before falling back to a plain fetch. It now declines up front with the reason and the supported alternative.

### Performance

- **Local chat uses far less memory**: the Qwen3.5 predict graph was compiled twice — once into a cache nothing reads, then again to return — and the chat UI's typing-time warm pass built that graph even though generation never uses it. Both are gone. Metal buffer allocation for the default 0.8B model drops from 25.9 GB to 3.97 GB and peak process memory from ~31 GB to ~11.7 GB.
- **Prefill width is configurable (`llm.rlx_prefill_seq`)**: Qwen3.5 runs every prompt through one static prefill graph whose width defaulted to the whole decode context, so a one-line question was prefilled as if it were 8192 tokens. Setting this decouples the two — measured on the 0.8B with a 25-token prompt, time to first token falls from 10.2 s to 1.2 s and decode rises from 5.6 to 35.6 tokens/s. A prompt longer than the configured width is rejected outright, so raise it when tools are on.
- **Agent Skills are no longer injected when they cannot be used**: the skills block tells the model to load files with the `read_file` tool, which is off by default, so 33 discovered skills added roughly 3,900 tokens to every prompt for nothing — a one-line question arrived as 3,970 tokens. It is now sent only when that tool is actually offered, and the daemon says once at startup when skills are being withheld for that reason.

### UI

- **Progress is shown while a tool runs**: a tool round-trip sent the client nothing at all — 21 of 22 seconds silent on a search — because only generation-phase markers were forwarded. Tool start and finish now appear as phases, so the chat shows "Running tools…" instead of looking stalled.
- **The onboarding model card names the model it will actually download**: the title was a fixed string reading "Qwen3.5 4B (Q4_K_M)" in every language while the download was Qwen3.5 0.8B, with the quant badge beside it already showing the truth.
- **Start honours a model picked elsewhere**: onboarding ignored the selected model and fetched its own default instead.
- **The LLM panel shows the port the API is really on**: it displayed a configurable advertisement value (8375) rather than the port the daemon bound (18444), so the endpoint list and the `curl` example were both wrong.

### Build

- **`npm run daemon` builds a GPU-capable daemon by default** and takes `--model <path>` to load a GGUF directly; previously it built without the platform's GPU features, so models ran on CPU, and pointing it at a local file meant four API calls by hand.
