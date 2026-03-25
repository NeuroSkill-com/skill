### i18n

- **Replaced de/he-only fallback-marker guard with all-locale guard**: `scripts/check-critical-i18n-locales.js` now validates every discovered non-English locale directory (currently `de`, `fr`, `he`, `uk`) for `TODO: translate` fallback markers.

### Build

- **Updated CI workflows to check all locales**: both `ci.yml` and `pr-build.yml` now run `check:i18n:locales` (all non-`en` locales) instead of a de/he-only check.
