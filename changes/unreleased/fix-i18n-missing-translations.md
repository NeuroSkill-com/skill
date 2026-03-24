### i18n

- **Translate all untranslated strings**: Translated 388 strings across 4 locales (de, fr, he, uk) that were still in English. Covers common UI (errors, dismiss, zoom reset, goal reached), supported devices and setup instructions, device API settings, API authentication, screenshot/OCR pipeline labels, screen recording permissions, history streaks, LLM tool settings, onboarding, and search screenshot tab. Added brand/product names and cross-language cognates to the exemption list.

### Bugfixes

- **Fix i18n test import shadowing**: The `extractKeysFromDir` import from `i18n-utils.ts` was shadowing the local test helper of the same name, causing 8 pre-existing key-sync test failures. Renamed to `extractKeysWithValues` in the test import.
