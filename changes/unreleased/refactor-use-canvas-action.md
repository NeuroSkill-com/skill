### Refactor

- **Shared canvas lifecycle action**: added `src/lib/use-canvas.ts` (`animatedCanvas` Svelte action) to DRY the ResizeObserver + requestAnimationFrame + DPR scaling boilerplate duplicated across EegChart, BandChart, PpgChart, GpuChart, and ImuChart. Existing charts are not yet migrated (tracked as a TODO).

### Docs

- **Constants sync guard**: improved doc comment in `src/tests/constants.test.ts` explaining how the test file guards Rust↔TypeScript constant drift.
