### Features

- **Add tests for `skill-autostart`**: 3 tests covering Linux XDG autostart enable/disable lifecycle, non-existent app safety, and disable idempotency.
- **Add tests for `skill-tts`**: 9 tests across config (defaults, JSON round-trip, empty JSON deserialization) and logging (enable toggle, write without callback, disabled noop).

### UI

- **Extract `HistoryStatsBar` component**: Moved the 75-line recording streak / stats bar from `history/+page.svelte` into a reusable `$lib/HistoryStatsBar.svelte` component with proper i18n (7 new keys: streak messages, days/hours/sessions labels, week trend). History page reduced from 1985 to 1924 lines.
