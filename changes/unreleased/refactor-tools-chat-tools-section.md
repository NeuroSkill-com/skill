### UI

- **Extracted Chat Tools settings panel into a dedicated component**: moved the large tools configuration UI from `src/lib/ToolsTab.svelte` into `src/lib/tools/ChatToolsSection.svelte`, including tool toggles, provider settings, web-cache controls, execution limits, compression, and retry settings.

### Refactor

- **Reduced `ToolsTab` to orchestration composition**: `ToolsTab` now delegates chat-tools rendering to `ChatToolsSection` and keeps only state loading/saving plus skill-refresh/skills orchestration.
