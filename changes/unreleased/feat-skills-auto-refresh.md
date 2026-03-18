### Features

- **Skills auto-refresh**: Community skills are now periodically downloaded from GitHub to `~/.skill/skills/`. Users can configure the refresh interval (off / 12 h / 24 h / 7 d) or trigger a manual sync from the Tools settings tab. A background task checks freshness and downloads the latest tarball when stale. The new `sync` feature in `skill-skills` handles download, extraction, and metadata tracking via a `.skills_last_sync` sidecar file.
