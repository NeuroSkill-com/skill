### Build

- **Broadened integration-test targeting in CI**: added `skill-router` and `skill-label-index` to the integration-test crate detection list.

### Bugfixes

- **Added router integration tests**: new `skill-router` integration tests cover UMAP cache path/store/load round-trips and inclusive epoch-label matching behavior.
- **Added label-index integration tests**: new `skill-label-index` integration tests verify empty-window EEG mean behavior and startup index initialization via `LabelIndexState::load()`.
