# skill-skills

Agent Skills discovery, parsing, and prompt injection for NeuroSkill LLM chat.

Discovers `SKILL.md` files from multiple locations and makes them available
to the LLM chat so it can load specialised instructions on demand using the
`read_file` tool.

## Discovery Locations (priority order)

| Priority | Directory | Source tag | Description |
|---|---|---|---|
| 1 | `~/.skill/skills/` | `user` | User-global skills |
| 2 | `<cwd>/.skill/skills/` | `project` | Project-local skills |
| 3 | `<app_root>/skills/` | `bundled` | Bundled / dev (git submodule) |
| 4 | Explicit paths | `path` | Passed via `skill_paths` |

## Discovery Algorithm

For each scanned directory:

1. **Check for `SKILL.md`** — if found and valid (has `description` in frontmatter),
   load it as a skill and **stop recursing** (the directory is a skill root).
2. **If `SKILL.md` is invalid** (e.g. an index file without frontmatter), continue
   recursing — this supports git submodule roots with a top-level index.
3. **If no `SKILL.md`**: at the root level only, load direct `.md` children as skills.
   Recurse into subdirectories (skipping `.`-prefixed dirs, `node_modules`, `target`).
4. Respects `.gitignore`, `.ignore`, `.fdignore` for filtering.
5. Symlinks are followed (broken symlinks are skipped).

## Skill File Format

Each `.md` file may have YAML frontmatter:

```yaml
---
name: my-skill
description: What this skill does (required, max 1024 chars)
disable-model-invocation: false
---
# Full instructions here...
```

- **`name`** — Optional; defaults to parent directory name. Max 64 chars.
- **`description`** — **Required**. Skills without a description are silently dropped.
- **`disable-model-invocation`** — If `true`, excluded from the system prompt
  (only usable via explicit invocation).

## Deduplication

- **Symlink dedup**: files resolving to the same real path are silently skipped.
- **Name collisions**: first-loaded skill wins (user > project > bundled > explicit).
  Collisions produce a diagnostic.

## Prompt Integration

`format_skills_for_prompt()` emits an XML block:

```xml
<available_skills>
  <skill>
    <name>my-skill</name>
    <description>What this skill does</description>
    <location>/path/to/SKILL.md</location>
  </skill>
</available_skills>
```

This is appended to the system prompt, instructing the LLM to use `read_file`
to load a skill's full content when the user's task matches its description.

## API

```rust
use skill_skills::{load_skills, LoadSkillsOptions, format_skills_for_prompt};

let result = load_skills(LoadSkillsOptions {
    cwd: std::env::current_dir().unwrap(),
    skill_dir: dirs::home_dir().unwrap().join(".skill"),
    bundled_dir: Some("/app/skills".into()),
    skill_paths: vec![],
    include_defaults: true,
});

for skill in &result.skills {
    println!("{}: {}", skill.name, skill.description);
}

let prompt_block = format_skills_for_prompt(&result.skills);
```

## Community Skills Auto-Sync (feature `sync`)

When the `sync` feature is enabled, the crate can periodically download the
latest community skills from the public GitHub repository and extract them
into `<skill_dir>/skills/`.

```rust
use skill_skills::sync::{sync_skills, SyncOutcome, DEFAULT_SKILLS_REFRESH_SECS};

// Download if last sync was >24 h ago (blocking I/O — call from a background thread):
match sync_skills(&skill_dir, DEFAULT_SKILLS_REFRESH_SECS, None) {
    SyncOutcome::Updated { elapsed_ms, .. } => println!("updated in {elapsed_ms} ms"),
    SyncOutcome::Fresh { next_sync_in_secs } => println!("fresh, next in {next_sync_in_secs} s"),
    SyncOutcome::Failed(e) => eprintln!("sync failed: {e}"),
}

// Force re-download (interval = 0):
sync_skills(&skill_dir, 0, None);

// Check last sync timestamp:
if let Some(ts) = skill_skills::sync::last_sync_ts(&skill_dir) {
    println!("last synced at Unix {ts}");
}
```

A `.skills_last_sync` JSON sidecar is written next to the extracted directory
to track the last successful sync timestamp and URL.
