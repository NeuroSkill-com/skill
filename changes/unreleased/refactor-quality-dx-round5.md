### Refactor

- **LlmTab logic extraction**: extracted `llm-tab-logic.ts` with hardware-fit badge styling, icons, and labels — 11 unit tests.

- **GoalsTab logic extraction**: extracted `goals-logic.ts` with progress bar coloring and minute formatting — 8 unit tests.

- **HooksTab logic extraction**: extracted `hooks-logic.ts` with timestamp conversion (microsecond/millisecond/second auto-detection) and relative-age formatting — 10 unit tests.

### Docs

- **Fix cargo doc warnings**: resolved unresolved link to `set_bash_edit_hook` in skill-tools and `gpu_fft::psd::psd` in skill-eeg.
