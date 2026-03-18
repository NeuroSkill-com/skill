### Features

- **Skills auto-refresh**: Community skills are now periodically downloaded from GitHub to `~/.skill/skills/`. Users can configure the refresh interval (off / 12 h / 24 h / 7 d) or trigger a manual sync from the Tools settings tab. A background task checks freshness and downloads the latest tarball when stale. The new `sync` feature in `skill-skills` handles download, extraction, and metadata tracking via a `.skills_last_sync` sidecar file.
- **Skills download on onboarding**: Community skills are automatically downloaded when onboarding completes, so fresh installs have the latest skills available immediately.
- **Agent Skills settings card**: Separate card in the Tools tab listing all discovered skills with their descriptions (pulled from SKILL.md frontmatter). Individual skills can be toggled on/off, with bulk Enable All / Disable All actions. Disabled skills are excluded from the LLM system prompt. Changes are live-applied to the running LLM server without restart.
