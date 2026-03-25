### UI

- **Extracted LLM model picker into a dedicated component**: moved family selection, quant list rendering, hardware-fit badges, and vision-projector controls from `src/lib/LlmTab.svelte` into `src/lib/llm/LlmModelPickerSection.svelte`.

### Refactor

- **Slimmed down `LlmTab` orchestration layer**: `LlmTab` now delegates server, model picker, inference, and log rendering to focused child components and keeps only shared state/event wiring.
