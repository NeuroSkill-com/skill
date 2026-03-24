### Bugfixes

- **skill-skills submodule test**: Added `submodules: true` to the `rust-check` CI checkout step so the `skills/` git submodule is fully populated and `discover_real_skills_submodule` runs (and passes) in CI instead of seeing an empty directory.
