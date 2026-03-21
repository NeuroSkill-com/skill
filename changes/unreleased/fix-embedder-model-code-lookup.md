### Bugfixes

- **Fix text embedder model resolution**: `build_embedder` used `EmbeddingModel::from_str` which only matches debug variant names (e.g. `BGESmallENV15`), not the `model_code` strings persisted in settings (e.g. `Xenova/bge-small-en-v1.5`). Added `resolve_embedding_model()` that first looks up by `model_code` from the supported-models list, falling back to the variant-name parser. Applied to both model init and `set_embedding_model` validation.
