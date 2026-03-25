### Bugfixes

- **Cleared all Biome lint errors** across `src/` and `scripts/`: fixed `noNonNullAssertion`, `noExplicitAny`, `useIterableCallbackReturn`, `noAssignInExpressions`, `useImportType`, and `noUnusedFunctionParameters` in ~50 files using safe rewrites or targeted `biome-ignore` suppressions with justification comments.
- **Restored blocking Biome lint in CI**: `Biome lint` step in `.github/workflows/ci.yml` is now a hard failure again (0 errors, 0 warnings).
- **Suppressed false-positive `useImportType` warnings for `.svelte` files**: added a `biome.json` override so Biome no longer flags Svelte component runtime imports as type-only — converting them to `import type` breaks `svelte-check`.
- **Fixed incorrect auto-conversion of Svelte component imports**: reverted `import type` from bits-ui and Svelte component imports that must remain runtime imports (`PromptLibrary`, `ChatInputBar`, `ChatMessageList`, `ChatSidebar`, `DialogPrimitive`, `ProgressPrimitive`, `SeparatorPrimitive`, `DialogPortal`).
