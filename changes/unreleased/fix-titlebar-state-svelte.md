### Bugfixes

- **Fix `$state` invalid placement in titlebar-state**: Moved `$state(initial)` from a `return` statement to a variable declaration initializer, fixing the Svelte 5 `state_invalid_placement` error in `src/lib/titlebar-state.svelte.ts`.
