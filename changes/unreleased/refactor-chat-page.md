### Refactor

- **Chat page component extraction**: Split the monolithic `chat/+page.svelte` (2720 lines) into 9 focused modules — `ChatHeader`, `ChatSettingsPanel`, `ChatToolsPanel`, `ChatMessageList`, `ChatInputBar`, `ChatToolCard` components plus `chat-types.ts` and `chat-eeg.ts` utility modules. The main page is now 897 lines (67% reduction) with each extracted component under 400 lines.
