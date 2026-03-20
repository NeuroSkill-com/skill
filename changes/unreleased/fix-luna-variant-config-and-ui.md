### Bugfixes

- **Fix LUNA huge/large variant crash (GroupNorm dimension mismatch)**: The HuggingFace `config.json` for LUNA lacks per-variant hyperparameters, so all variants loaded with the `base` config (embed_dim=64, depth=8). The `huge` variant (embed_dim=128, depth=24) crashed with "Expected 16, got 32" in GroupNorm. Added `LUNA_VARIANT_CONFIGS` constants with correct dimensions for each variant and a `luna_variant_config_path` helper that generates a variant-specific config file before loading the encoder.

### UI

- **Show correct encoder name and loading state in EEG Model settings**: The encoder status section was hardcoded to show "ZUNA Encoder" regardless of the selected backend. Now dynamically shows the correct name (e.g. "LUNA Encoder (huge)") based on the selected backend and variant. The status indicator dot also pulses blue while the encoder is loading on the GPU.
