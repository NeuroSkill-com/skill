### Bugfixes

- **Add keys to critical `{#each}` loops**: Added `(key)` expressions to Svelte `{#each}` loops for session lists, device lists, day lists, settings tabs, search presets, and score keys across dashboard, history, search, compare, and settings pages. Prevents DOM thrashing when lists update.

- **Remove debug console.log**: Replaced 7 `console.log` calls in `UmapScene.svelte` with `console.debug` (filtered in production DevTools by default).
