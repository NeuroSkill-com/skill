### Features

- **Epoch-aligned screenshot interval**: Screenshot capture interval is now aligned with EEG embedding epochs (5 s). The slider offers multipliers from 1× (every 5 s) to 12× (every 60 s) in 5-second steps, replacing the old 1–30 second free-form slider. Legacy config values are automatically snapped to the nearest epoch boundary.

### UI

- **Screenshot interval slider**: Updated to show epoch-aligned steps (5 s, 10 s, …, 60 s) with multiplier badge (e.g. "10s (2× epoch)").

### i18n

- **Screenshot interval strings**: Updated English, German, French, Hebrew, and Ukrainian translations for the epoch-aligned interval description and added `screenshots.intervalEpoch` key.
