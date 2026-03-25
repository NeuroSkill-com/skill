### Build

- **Faster pre-commit checks**: updated `.githooks/pre-commit` to run frontend checks only on changed files (`biome check` + `vitest related`) instead of full project-wide frontend checks on every commit.
