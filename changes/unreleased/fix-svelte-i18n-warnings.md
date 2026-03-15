### Bugfixes

- **Fix all svelte-check warnings and i18n gaps**: Resolved 5 Svelte compiler warnings — fixed `propColors` initial-value capture in `EegChart.svelte` by using `EEG_COLOR` fallback, fixed `device` prop capture in `ElectrodeGuide.svelte` by deferring initial tab selection to `$effect`, and fixed non-interactive element warning in `history/+page.svelte` by using `role="toolbar"` with `tabindex`. Added 215 missing English-fallback i18n keys across de, fr, he, and uk locales.
