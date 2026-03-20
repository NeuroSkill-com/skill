### Bugfixes

- **Replace `any` types with proper TypeScript types**: Replaced `any` with `unknown` in error catch callbacks (`LlmTab`, `SettingsTab`), added `instanceof Error` checks for error message extraction, typed Tauri invoke results with `Record<string, unknown>` instead of `any[]`, and replaced `as any` event payload casts with typed `Record` casts.
