### Bugfixes

- **skill-skills submodule test**: `discover_real_skills_submodule` now skips correctly in CI where the git submodule directory exists but is not populated, by checking for `skills/SKILL.md` instead of just directory existence.
