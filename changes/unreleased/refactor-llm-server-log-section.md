### UI

- **Extracted LLM server log viewer into a dedicated component**: moved filtering/search/auto-scroll log UI from `src/lib/LlmTab.svelte` into `src/lib/llm/LlmServerLogSection.svelte`.

### Refactor

- **Reduced `LlmTab` UI state surface**: removed log-view-local state (`logFilter`, `logSearch`, `logAutoScroll`, scroll element management) from `LlmTab` and delegated it to `LlmServerLogSection` while keeping event-driven log ingestion in the parent.
