### LLM

- **LLM catalog: 41 new quantizations of catalogued models.** Synced from Hugging Face. Nothing was removed and the default model (Qwen3.5-0.8B Q4_K_M) is unchanged. New entries:
  - Qwen3-0.6B: unsloth BF16 and UD Q2–Q8 K_XL quants.
  - LFM2.5-VL-1.6B: Q4_K_M, Q5_K_M and Q6_K.
  - LFM2.5-1.2B-Instruct: QAD Q4_0.
  - Bonsai-27B and Ternary-Bonsai-27B: dspark Q4_1, F16, dspark BF16 and Ternary PQ2_0 / Q2_g64, plus BF16 and Q8_0 vision projectors.
- Hugging Face download counts refreshed (140 repos).

### Build

- **The LLM-catalog sync script now records real file sizes.** It asked Hugging Face for each repo's file list without `blobs=true`, which leaves sizes out, so every added entry silently copied an existing entry's size (a 0.59 GB projector was listed at 3.8 GB). It now fetches sizes. It also names quantized projectors by their quant (`Q8_0`, not `MMPROJ`), and no longer marks a new Q4_K_M or BF16 projector "recommended" in a family that already has a curated pick (LFM2.5-VL would have shown two recommended models).
- **Every release now publishes its LLM catalog.** A new workflow (`release-catalog.yml`) runs on each release tag and attaches `llm_catalog.json` + `llm_catalog.json.sha256` to the release. It also mirrors them to `rc-latest`, the same way as `latest.json`. Stable: `releases/latest/download/llm_catalog.json`; RC channel: `releases/download/rc-latest/llm_catalog.json`. It needs no build, so it publishes in about a minute and survives a failed platform build. The catalog is validated first (`node scripts/ci.mjs publish-llm-catalog`), and the CI self-test in pre-push runs the same validation: every model has a known family and a size, the default model exists and isn't a vision projector, no duplicates.
- **The published catalog says what it is compatible with.** A `compatibility` block comes first: `schema` (catalog layout, now 1), `min_app_version` (oldest app that reads this layout: 0.0.131-rc.10), `bundled_in` (the release version, tag, commit and platforms it shipped in) and `runtime.rlx` (the rlx revision those models were validated against). Everything after it is byte-for-byte the catalog compiled into the app, which ignores the extra block.
