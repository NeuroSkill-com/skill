### i18n

- **Added critical-locale translation guard for German/Hebrew**: introduced `scripts/check-critical-i18n-locales.js` and `npm run check:i18n:critical` to fail when de/he locale files contain auto-sync TODO fallback markers.

### Build

- **Wired critical-locale i18n guard into CI**: frontend CI now runs `check:i18n:critical` after sync/audit checks to catch untranslated fallback markers early.
