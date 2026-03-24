### i18n

- **Untranslated value detection**: Added a vitest check (`i18n untranslated value detection`) that fails when any non-English locale contains values identical to English that are not in the exemption list. The exemption logic (brand names, technical acronyms, formulas, academic citations, etc.) is now shared between `i18n-utils.ts`, the `audit-i18n.ts` script, and the test suite. A new `check:i18n` npm script runs the audit with `--check` for CI gating.
