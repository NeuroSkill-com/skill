### Bugfixes

- **Fix all 21 TypeScript type errors**: Resolved every `svelte-check` error across the codebase (was 21, now 0):
  - `ChatToolCard`: Added typed `arg()` helper for `Record<string, unknown>` tool args, typed `SourceEntry` interface for web search sources, cast `tu.result` through `Record` instead of direct `unknown` access.
  - `compare/+page.svelte`: Fixed `UmapProgress` cast through `unknown`.
  - `history-helpers.test.ts`: Fixed `LabelRow` field names (`wall_start` → `label_start`).
  - `umap-helpers.test.ts`: Fixed `UmapPoint` construction to match current interface.
