# deepmerge 4.3.1, patched

Upstream `deepmerge` 4.3.1 (`dist/cjs.js`, MIT, see `license.txt`) with one change:
`mergeObject` skips the key `__proto__` when copying target and source keys.

**Why:** GHSA-gcqx-w48m-w2h4 ("mergeObject() allows prototype poisoning of returned
objects", all of 4.2.0–4.3.1) has no patched release — deepmerge has not been published
since 2023. Merging a source with an own `__proto__` key (`JSON.parse` produces one) into
a null-prototype target replaced the result's prototype:
`deepmerge(Object.create(null), JSON.parse('{"__proto__":{"isAdmin":true}}')).isAdmin === true`.

**Used by:** `@sveltejs/vite-plugin-svelte` (build time, merging its own config). Wired in
via `overrides` in the root `package.json`.

**Remove** this directory and the override once deepmerge publishes a fix or
vite-plugin-svelte drops the dependency (`npm ls deepmerge`, then `npm audit`).
