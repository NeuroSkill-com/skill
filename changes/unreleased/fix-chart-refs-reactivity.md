### Bugfixes

- **Chart refs reactivity**: Declared `chartEl` and `bandChartEl` with `$state(...)` in `+page.svelte` to fix `non_reactive_update` warnings.
