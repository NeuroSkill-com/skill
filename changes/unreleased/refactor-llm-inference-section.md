### UI

- **Extracted LLM advanced inference settings into a dedicated component**: moved the collapsible inference/settings panel from `src/lib/LlmTab.svelte` into `src/lib/llm/LlmInferenceSection.svelte`.

### Refactor

- **Simplified `LlmTab` state surface**: removed local `showAdvanced`, `apiKeyVisible`, and `ctxSizeInput` ownership from `LlmTab` and delegated these UI concerns to `LlmInferenceSection`, while keeping config persistence centralized via `saveConfig()`.
