### Refactor

- **The Headache index is gone.** It is no longer computed (`BandSnapshot.headache_index`), shown on the dashboard's EEG indices card, returned by search neighbour metrics, or used by the `physical` hook gate. That gate now fires on drowsiness ≥ 55, migraine index ≥ 45, or out-of-range heart rate. The Migraine index is unchanged and keeps its citation (Bjørk et al. 2009). The dashboard label and tooltip strings are removed from all nine locales, the metric reference now names only the migraine index, and METRICS.md §11 is now "Migraine EEG Correlate". Skill protocols that keyed on `headache_index` now trigger on `migraine_index`, `bar` or `stillness`, or on the user saying they have a headache.
- Older rows keep `headache_index` inside their stored metrics JSON. Nothing reads it any more.

### Docs

- **Help → FAQ, in all nine languages:** a new "How do I export my recordings as BIDS?" entry, and "Why did my data files change from .sqlite to .duckdb?", which covers the one-time import, the `.sqlite.bak` backups, and quitting the app before opening a database with another tool. All other help text now says DuckDB/`.duckdb` instead of SQLite/`.sqlite`.
