### Features

- **Split/sharded GGUF support**: The LLM catalog and downloader now support multi-part (split) GGUF models. Added `shard_files` field to `LlmModelEntry` listing all shard filenames in order. The new `download_model()` function downloads shards sequentially with overall progress tracking, pause/resume per-shard, and cancellation between shards. Delete properly removes all shard files. The frontend shows shard count on download buttons and current shard progress during download.

- **MiniMax M2.5 full catalog**: Added 11 quant variants of MiniMax M2.5 (456B MoE, 46B active) to the LLM catalog via `unsloth/MiniMax-M2.5-GGUF` — from TQ1_0 (52 GB single file) through Q8_0 (226 GB, 6 shards). The Q4_K_M quant is marked as recommended.

### Bugfixes

- **MoE detection for hardware fit**: The hardware-fit analyzer now detects MoE models from the `"moe"` tag in addition to inferring from family name patterns, improving fit predictions for MiniMax M2.5 and similar models.
