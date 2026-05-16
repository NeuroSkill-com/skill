### Bugfixes

- **Fix `interactive_search` hang on empty query**: with `query=""` the SQL `text LIKE '%' || '' || '%'` matched every label in `labels.sqlite`, then looped `search_embeddings_in_range(±10 minutes)` across every daily DB — 30s+ before the test harness gave up. The daemon now short-circuits to `{"ok":false,"error":"empty query"}` when the query is empty or whitespace-only.
