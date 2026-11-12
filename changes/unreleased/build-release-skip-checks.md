### Build

- **`npm run release -- --rc --skip-checks`** (and `npm run bump -- --skip-checks`): cut a release candidate without any local testing or compilation. `bump` skips its preflight (frontend checks, the i18n check, vitest, clippy and cargo test), and `npm run tag` pushes the tag with `--no-verify` so the pre-push hook does not re-run them either. The version and tag safety checks and the pre-commit hook still run, and CI is the first to see the commit.
