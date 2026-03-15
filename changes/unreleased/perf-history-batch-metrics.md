### Performance

- **History view: batch metrics loading with disk cache**: Replaced per-session IPC waterfall (4 concurrent `get_csv_metrics` calls) with a single `get_day_metrics_batch` call that loads all sessions' metrics in one roundtrip. Added persistent `_metrics_cache.json` disk cache next to each session CSV — subsequent loads skip CSV re-parsing entirely. Timeseries payloads are downsampled to ≤360 points on the backend, reducing transfer size for sparklines and heatmaps. Week view also uses a single batch call for all 7 days. Adjacent-day prefetching now batch-loads as well.
