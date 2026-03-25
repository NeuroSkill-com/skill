### Bugfixes

- **Cleared all Biome lint errors** across `src/` and `scripts/`: fixed `noNonNullAssertion`, `noExplicitAny`, `useIterableCallbackReturn`, `noAssignInExpressions`, `useImportType`, and `noUnusedFunctionParameters` in ~35 files using safe rewrites or targeted `biome-ignore` suppressions with justification comments.
- **Restored blocking Biome lint in CI**: `Biome lint` step in `.github/workflows/ci.yml` is now a hard failure again.
