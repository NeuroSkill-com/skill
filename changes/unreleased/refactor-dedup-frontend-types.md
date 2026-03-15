### Refactor

- **Deduplicate frontend types and formatting helpers**: extracted shared TypeScript interfaces into `$lib/types.ts`; extracted 12 formatting functions into `$lib/format.ts`; extracted `SleepAnalysis` into `$lib/sleep-analysis.ts`; updated 15 consumer files; eliminates ~30 duplicate interface definitions and ~20 duplicate utility functions.
