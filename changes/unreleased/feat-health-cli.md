### CLI

- **`health` command**: New CLI command family for Apple HealthKit data. Subcommands: `health` / `health summary` (aggregate counts), `health sleep` / `health workouts` / `health hr` / `health steps` / `health metrics` (typed queries with `--start`, `--end`, `--limit`), `health metric-types` (list stored types), and `health sync` (push data from iOS companion). Human-readable formatting with color-coded output for each data type.

### Docs

- **SKILL.md**: Added full `health` command reference with subcommand table, CLI examples, HTTP equivalents, JSON response shapes, sync payload format, and common metric types.
