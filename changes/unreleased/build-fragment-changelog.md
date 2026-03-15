### Build

- **Fragment-based changelog system**: replaced single-file `CHANGELOG.md` editing with `changes/unreleased/` fragments. Each change gets its own `.md` file; `npm run bump` compiles fragments into `changes/releases/<version>.md`, deletes consumed fragments, and rebuilds `CHANGELOG.md` from all release files. All 20 historical releases migrated. Supports `--rebuild` to regenerate from archives.
