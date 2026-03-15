### Bugfixes

- **Fix LLM catalog crash on Windows**: Replaced the `llm_catalog.json` symlink in `skill-llm` with a direct `include_str!` path to the source file. Git on Windows checks out symlinks as plain-text files (containing the target path), which caused an invalid-JSON panic at startup.
