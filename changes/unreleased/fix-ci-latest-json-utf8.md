### Bugfixes

- **CI: fix latest.json encoding and Python indentation**: Fixed IndentationError in macOS workflow (`if not notes:` was mis-indented inside `except` block) and inconsistent indentation in Linux workflow. Replaced literal `™` with `\u2122` escape in Python scripts and added `ensure_ascii=False` to all `json.dump` calls so `latest.json` is always coherent UTF-8 across all three platform CIs.

### Docs

- **AGENTS.md: add CI shared-artifact encoding rule**: New section documenting that all CI workflows must produce and consume `latest.json` as UTF-8 without BOM, with no literal non-ASCII characters in CI scripts.
