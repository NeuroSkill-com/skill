### Bugfixes

- **Add tests to previously untested crates**: Added 35 new unit tests across 4 crates that had zero test coverage:
  - `skill-exg` (17 tests): cosine distance (identical, opposite, orthogonal, edge cases), fuzzy matching (exact, case-insensitive, substring, typo, empty), Levenshtein distance.
  - `skill-commands` (13 tests): DOT escaping, SVG escaping, text truncation, turbo colormap, graph generation.
  - `skill-eeg/band_metrics` (10 tests): spectral edge frequency, spectral centroid, Hjorth parameters, permutation entropy, sample entropy, DFA exponent, Higuchi fractal dimension.
  - `skill-history/cache` (5 tests): timeseries downsampling (noop, exact count, endpoint preservation, min-2), sleep stage analysis.
