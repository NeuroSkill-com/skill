### UI

- **Remove 2K context size option from LLM settings**: The 2K (2048 tokens) context size option was removed from the LLM inference settings UI. The backend auto-recommend (`recommend_ctx_size`) already treats 2048 as "too small for practical use" and never selects it — the minimum auto-recommended context is 4K. The default remains "auto", which intelligently picks the largest context size that fits in available GPU/unified memory. Users can still manually select 4K, 8K, 16K, 32K, 64K, or 128K.
