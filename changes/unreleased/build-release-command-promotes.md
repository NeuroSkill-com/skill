### Build

- **`npm run release` now ships the current RC to stable.** It used to exit with an error unless given `--rc`. It checks that the RC is complete (clean tree, pushed and tagged bump commit, mergeable PR, every release workflow green, every platform in `latest.json`), asks you to type the version, rebase-merges the release PR and waits for `promote.yml`. `--dry-run` runs only the checks, and `--rc` is unchanged.
