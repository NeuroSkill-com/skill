### Refactor

- **Extract history page canvas rendering into `history-canvas.ts`**: Moved 3 pure canvas rendering functions (`renderDayDots`, `renderDayGrid`, `renderSparkline`) and the `heatColor` utility from the 2,224-line `history/+page.svelte` into a dedicated `src/lib/history-canvas.ts` module (280 lines). The history page now delegates to these functions via thin wrappers, reducing it to 1,983 lines (-241). All rendering logic is now testable independently of Svelte reactive state.
