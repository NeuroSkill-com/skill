### Build

- **Added i18n guard steps to PR build workflow**: `pr-build.yml` now runs `sync:i18n:check`, `audit:i18n:check`, and `check:i18n:critical` (de/he) after dependency install so translation drift is caught before expensive packaging/signing steps.
