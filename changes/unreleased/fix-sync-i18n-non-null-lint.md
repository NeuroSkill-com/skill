### i18n

- **sync-i18n lint fix**: removed a non-null assertion in `scripts/sync-i18n.ts` when collecting extra locale keys, using an explicit `undefined` guard instead.
