### Bugfixes

- **Skills sync discovers all community skills**: The skill discovery algorithm stopped recursing into subdirectories when the repository root contained a valid `SKILL.md` with a `description` in frontmatter, causing only one skill (the index) to be loaded. Added support for an `index: true` frontmatter flag that marks a `SKILL.md` as an index file — the skill is loaded but the scanner continues recursing into child directories. The community skills repo root `SKILL.md` now uses this flag. Also fixed Phase 2 to skip re-processing `SKILL.md` files already handled in Phase 1.
