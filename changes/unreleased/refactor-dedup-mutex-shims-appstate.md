### Refactor

- **Deduplicate `MutexExt` trait**: Moved the poison-recovering `MutexExt` trait to `skill-constants` (zero-dependency crate). `skill-data` and `skill-jobs` now re-export from the single canonical definition instead of maintaining independent copies.

- **Add `AppStateExt` helper trait**: Introduced a blanket `AppStateExt` trait on `Manager<Wry>` that replaces the verbose `app.state::<Mutex<Box<AppState>>>()` pattern (137 call sites) with `app.app_state()`. Cleaned up newly-unused `Mutex` and `AppState` imports across 13 files.

- **Remove 14 re-export shim modules**: Eliminated one-line facade modules in `lib.rs` (e.g. `mod eeg_bands { pub use skill_eeg::eeg_bands::*; }`) that only proxied upstream crate items. All 59 call sites now reference the source crates directly (`skill_eeg::`, `skill_data::`) making dependencies explicit and reducing indirection.
