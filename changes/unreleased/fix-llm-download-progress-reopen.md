### Bugfixes

- **LLM download progress lost on window reopen**: When starting a model download in LLM settings, closing the window, and reopening it, the download progress bar was not shown. The poll timer only refreshed the catalog when it already knew about an active download, creating a chicken-and-egg problem on fresh mounts. Fixed by always polling the catalog (a cheap in-memory read) so in-flight downloads are detected regardless of initial component state. Also added the missing `"paused"` variant to the frontend `DownloadState` type.

### Refactor

- **Extract LLM helpers into testable module**: Moved all pure functions and types from `LlmTab.svelte` into `$lib/llm-helpers.ts` (vendor labels, quant ranking, family grouping, entry sorting, entry group splitting, family auto-selection, download detection). The component now imports from the shared module.

### Features

- **LLM helpers integration tests**: Added 65 tests in `llm-helpers.test.ts` covering the full download-progress lifecycle — unit tests for each helper, plus end-to-end scenarios simulating download start, window reopen, completion, failure, pause, multi-family downloads, shard progress, and stale-state recovery.
