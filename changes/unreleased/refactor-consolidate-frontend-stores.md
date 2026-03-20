### Refactor

- **Consolidate frontend stores into `src/lib/stores/`**: Moved 12 scattered `.svelte.ts` store files from `src/lib/` root into a dedicated `src/lib/stores/` directory. Merged 5 tiny titlebar-related files (`titlebar-state`, `chat-titlebar`, `history-titlebar`, `label-titlebar`, `help-search-state`) into a single `stores/titlebar.svelte.ts`. Renamed remaining stores to drop the `-store` suffix (e.g. `theme-store` → `theme`, `toast-store` → `toast`). Updated ~50 import paths across the codebase.
