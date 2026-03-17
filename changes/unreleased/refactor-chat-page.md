### Refactor

- **Chat page component extraction**: Split the monolithic `chat/+page.svelte` (2720 lines) into 9 focused modules — `ChatHeader`, `ChatSettingsPanel`, `ChatToolsPanel`, `ChatMessageList`, `ChatInputBar`, `ChatToolCard` components plus `chat-types.ts` and `chat-eeg.ts` utility modules. The main page is now 897 lines (67% reduction) with each extracted component under 400 lines.
- **Compare page logic extraction**: Extracted types, constants, helpers, insight computation, UMAP analysis, and all canvas drawing functions from `compare/+page.svelte` (2582 lines) into `compare-types.ts` and `compare-canvas.ts`. The main page is now 1924 lines (25% reduction) with 663 lines of pure logic in reusable modules.
