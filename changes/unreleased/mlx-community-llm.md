### LLM

- **MLX community model path**: Wire huggingface.co/mlx-community packs through `rlx-models` (`MlxLoader` / `Qwen3Runner::from_mlx_packed`).
- **Catalog entries for MLX Qwen3**: Add Qwen3 0.6B / 1.7B / 4B 4-bit entries and snapshot download handling for `tags: ["mlx"]`.
- **Discovery + loader behavior**: Surface safetensors snapshot directories (not GGUF-only) in local discovery and prefer the MLX device when loading compatible packs.
- **Feature-gated rollout**: Keep discovery and `from_mlx_packed` behind `llm-model-discovery` until the `rlx-models` pin is bumped.
