### Refactor

- **Split i18n into namespace files**: Replaced monolithic ~3000-line locale files (`en.ts`, `de.ts`, `fr.ts`, `he.ts`, `uk.ts`) with 15 namespace-based files per locale under `src/lib/i18n/<locale>/` (`common`, `dashboard`, `settings`, `search`, `calibration`, `history`, `hooks`, `llm`, `onboarding`, `screenshots`, `tts`, `perm`, `help`, `help-ref`, `ui`). Each locale folder has a barrel `index.ts` that merges all namespaces.
- **Added `TranslationKey` type safety**: Generated `keys.ts` with a union type of all 2731 valid translation keys. The `t()` function now accepts `TranslationKey` for compile-time checking on static keys, with a `string` overload for dynamic/computed keys.
- **Extracted shared `i18n-utils.ts`**: Moved duplicated `extractKeys()` logic from `sync-i18n.ts` and `audit-i18n.ts` into a shared `src/lib/i18n/i18n-utils.ts` module.
- **Updated i18n tests**: Test suite now validates per-namespace file consistency (74 tests, all passing).
- **Updated scripts**: `sync-i18n.ts` and `audit-i18n.ts` now operate on the new directory structure and use shared utilities.
