### Features

- **Validation & Research settings tab** (`src/lib/settings/ValidationTab.svelte`): new top-level settings tab gathering five `SettingsCard` blocks — global gates (`respect_flow`, quiet hours), KSS, NASA-TLX, PVT, and EEG fatigue index — plus a Calibration Week button and a recent-results card.
  - Every toggle / numeric input PATCHes one nested field via `daemonPatch("/v1/validation/config", …)`; the daemon's recursive JSON merge keeps the rest of the config untouched.
  - EEG fatigue card shows the live `(α + θ) / β` value polled every 5 s from `/v1/validation/fatigue-index`.
  - Calibration Week: single button that batch-updates all four channels to higher-frequency presets (KSS 8/day, TLX after every flow block ≥ 20 min, PVT mid-week) in one PATCH.
- **PVT panel** (`src/lib/settings/PvtPanel.svelte`): full 3-minute Psychomotor Vigilance Task. Random ITIs (2–10 s), green-dot stimulus, `performance.now()` for sub-millisecond RT measurement, tracks responses + false starts. On finish computes mean RT, median RT, slowest-10% mean RT (anticipation-resistant), and lapse count (RT > 500 ms per Dinges & Powell 1985), POSTs to `/v1/validation/pvt`. State machine: intro → running → done.
- **NASA-TLX form** (`src/lib/settings/TlxForm.svelte`): six-slider modal for the raw (un-weighted) NASA-TLX. Performance scale uses inverted endpoints ("Failure" → "Perfect") per Hart 2006. POSTs to `/v1/validation/tlx` with `task_kind`, `task_duration_secs`, optional `prompt_id` echo-back.
- **Pure stats helper** (`src/lib/settings/pvt-stats.ts`): extracted `mean`, `median`, `slowest10Mean`, `lapseCount`, `computeStats` from the PVT panel so the math is unit-testable without a Svelte renderer.
- **`daemonPatch<T>(path, body)`** in `src/lib/daemon/http.ts` — needed for the validation config endpoint.

### UI

- **Spacing pass on the validation tab and modals**: every card uses `flex flex-col gap-5 p-6` instead of the old `space-y-4` (which provided no padding override). Conditional sub-settings under a channel toggle are now indented under a left border (`ml-1 border-l-2 border-border pl-5`) so the parent/child relationship reads visually. Modals padded to `p-8` with `gap-3` button rows. TLX sliders gained `mt-1` lift off the description and `uppercase tracking-wide` low/high legend labels.

### i18n

- **New `validation` namespace** in all 9 languages (`src/lib/i18n/{de,en,es,fr,he,ja,ko,uk,zh}/validation.ts`): ~95 keys in English (full coverage), ~55 in each other language for the user-visible strings (long-tail descriptions fall back via the runtime `t()` chain). `index.ts` barrel exports updated for every language.
