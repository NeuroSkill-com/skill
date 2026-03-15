# Changelog Fragments

Fragment-based changelog system. Each change gets its own `.md` file instead of editing a shared `CHANGELOG.md`.

## Adding a changelog entry

Create a `.md` file in `changes/unreleased/`:

```bash
# Name it descriptively
changes/unreleased/feat-screenshot-ocr.md
changes/unreleased/fix-copy-paste-macos.md
changes/unreleased/refactor-extract-skill-eeg.md
```

### Fragment format

Each file contains one or more `### Category` sections:

```markdown
### Features

- **Short title**: description of the change.

### Bugfixes

- **Fix X**: what was wrong and how it was fixed.
```

### Valid categories (in display order)

`Features` · `Performance` · `Bugfixes` · `Refactor` · `Build` · `CLI` · `UI` · `LLM` · `Server` · `i18n` · `Docs` · `Dependencies`

## Compiling a release

Happens automatically during `npm run bump`:

1. Reads all `.md` files from `changes/unreleased/`
2. Groups entries by category in canonical order
3. Writes `changes/releases/<version>.md`
4. Deletes consumed fragment files
5. Rebuilds `CHANGELOG.md` from all release files (newest first)

Can also be run standalone:

```bash
npm run compile:changelog -- 0.0.38            # compile + rebuild
node scripts/compile-changelog.js 0.0.38 2026-03-15  # explicit date
node scripts/compile-changelog.js --rebuild    # rebuild from archives only
```

## Directory structure

```
changes/
├── README.md              ← this file
├── unreleased/            ← pending fragments (one per change)
│   ├── feat-new-thing.md
│   └── fix-bug.md
└── releases/              ← one file per version
    ├── 0.0.37.md
    └── 0.0.38.md
```

`CHANGELOG.md` at the project root is **generated** — it is rebuilt from `changes/releases/*.md`. Do not edit it directly.
