### UI

- **Extract `HistoryCalendar` component**: Moved the 208-line calendar heatmap (year/month/week views) from `history/+page.svelte` into a reusable `$lib/HistoryCalendar.svelte` component. History page reduced from 1924 to 1735 lines.

- **Extract `OnboardingChecklist` component**: Moved the 34-line onboarding checklist from the dashboard `+page.svelte` into `$lib/OnboardingChecklist.svelte`. Fixed hardcoded "Dismiss" button → `t("common.dismiss")`. Dashboard reduced from 1678 to 1644 lines.

### i18n

- **Add `common.dismiss` key**: New i18n key for all 5 languages, replacing the hardcoded "Dismiss" string in the onboarding checklist.
