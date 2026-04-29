### Build

- **CI for the VS Code extension** (`extensions/vscode/.github/workflows/`): two workflows live in the submodule repo (`vscode-neuroskill`).
  - **`ci.yml`** — runs on every PR and `main` push. `npm ci` → `tsc` build → `vsce package` → uploads the `.vsix` as a 14-day artifact. No secrets, no publish.
  - **`release.yml`** — fires two ways. *Tag push* (`git push --tags` after `npm version patch`) verifies the tag matches `package.json` and publishes. *Manual dispatch* with a `patch | minor | major | x.y.z` input bumps `package.json`, commits, tags, pushes, then publishes. Both paths run `npx @vscode/vsce publish` (uses `VSCE_PAT`) and `npx ovsx publish` (uses `OVSX_PAT`), then create a GitHub release with the `.vsix` attached and the latest `## ` block from `CHANGELOG.md` as release notes.
- **Skip-on-missing-secret**: if either `VSCE_PAT` or `OVSX_PAT` is unset, the corresponding publish step emits a `::warning::` and is skipped — letting you wire up Open VSX later without breaking the workflow.
- **Releasing section in the extension README**: documents the secret setup (Azure DevOps PAT, Open VSX token), the two release flows, and the one-time namespace claims (Marketplace publisher registration + `ovsx create-namespace`).
