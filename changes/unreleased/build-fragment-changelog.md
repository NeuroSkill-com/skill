### Build

- **Fragment-based changelog system**: replaced single-file `CHANGELOG.md` editing with `changes/unreleased/` fragments. Each change gets its own `.md` file; `npm run bump` compiles fragments into `CHANGELOG.md` and archives them to `changes/releases/<version>/`. Eliminates merge conflicts and unbounded file growth. Added `scripts/compile-changelog.js` and `npm run compile:changelog`.
