### Features

- Add deep-link search for individual settings in the Command Palette (Cmd-K). 154 settings across all 20 tabs are now searchable by name, description, and synonyms — typing "DND", "dark mode", "GPU", "OCR", etc. instantly finds the right setting.
- Auto-generate per-locale search indexes from i18n translation files and Tab components. Indexes are built for all 9 languages (EN, DE, ES, FR, HE, JA, KO, UK, ZH) and stay in sync automatically via a Vite plugin that watches for changes during development.
- Add synonym expansion to Cmd-K fuzzy search. Common abbreviations like DND, BT, GPU, OCR, LLM, and 30+ others are expanded before matching so users can search using shorthand.
- Add Command Palette button to the title bar of every window. Clicking the command-key icon opens Cmd-K, matching the keyboard shortcut behavior.
- Selecting a setting in Cmd-K now opens the Settings window, switches to the correct tab, scrolls to the exact setting, and flashes a blue highlight to draw attention to it.
- Wire settings index generation into `npm run dev`, `npm run build`, and `npm run bump` so indexes are always up to date before commits and builds.
- Add Chat and Compare shortcuts to the Shortcuts settings tab, making all 11 global shortcuts user-configurable. The Compare shortcut (previously hardcoded to Cmd+Shift+M) is now customizable like all others.
- Tray menu now reflects user-customized shortcuts for all actions including Chat and Compare.
- The keyboard shortcuts overlay (? key) now shows all 11 global shortcuts, matching the Shortcuts settings tab and tray menu.
- Global keyboard shortcuts no longer steal keystrokes from other apps. Shortcuts are now only registered when a Skill window is focused and unregistered when all windows lose focus, so the focused app always gets the keystroke first. The "Open NeuroSkill" shortcut (Cmd+Shift+O) remains always-on so users can bring the app to the foreground from anywhere.

### Bugfixes

- Fix Chat shortcut not being loaded from saved settings on app startup.
