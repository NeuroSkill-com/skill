### UI

- **History view accent consistency**: migrated all interactive/highlight colors in the history view from hardcoded `emerald-*` to accent-aware `violet-*` (remapped by the Appearance accent setting). Affected: year/month heatmap cells and legend swatches, month calendar day-count text, screenshot canvas diamond indicator (now reads `--color-violet-500` CSS property), and screenshot tooltip dot. Semantic status colors (`emerald-500` for positive trend, `red-*` for destructive actions) remain unchanged.
