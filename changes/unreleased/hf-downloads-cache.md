### LLM

- **HF downloads cache for model ranking**: Add `scripts/update-hf-downloads-cache.mjs` to crawl Hub download counts for every catalog repo plus top mlx-community models, writing both `src-tauri/hf_downloads_cache.json` and `src/lib/generated/hf-downloads-cache.json`.
- **Family sort by popularity**: Sort the LLM family picker by Hub downloads descending by default and show `↓ N` download hints in the dropdown.
- **MLX search UX**: Add an MLX tab in HF search (also sorted by downloads) and support importing an entire mlx-community snapshot via "Add pack".

### Build

- **Cache freshness automation**: Add a weekly GitHub Action to refresh HF download counts and open a PR; add CI validation that the committed cache covers the current catalog.
