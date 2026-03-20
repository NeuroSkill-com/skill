### i18n

- **Pre-commit hook enforces i18n key synchronisation**: Added `.githooks/pre-commit` that runs `npm run sync:i18n:check` when any file under `src/lib/i18n/` is staged. Blocks commits with missing translation keys and guides the developer to run `npm run sync:i18n:fix`. The hook is automatically activated via `postinstall` (`git config core.hooksPath .githooks`) and skips entirely when no i18n files are changed (~0 ms overhead on normal commits).
