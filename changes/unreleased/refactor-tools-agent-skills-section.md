### UI

- **Extracted Agent Skills UI into a dedicated component**: moved the large Agent Skills block out of `src/lib/ToolsTab.svelte` into `src/lib/tools/AgentSkillsSection.svelte`, including markdown skill description rendering and license panel behavior.

### Refactor

- **Simplified ToolsTab composition**: `ToolsTab` now composes `SuggestSkillCta`, `AgentSkillsSection`, and `SkillsRefreshSection`, reducing in-file template complexity and local state surface.
