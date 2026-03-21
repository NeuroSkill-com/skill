### Bugfixes

- **Fix label_store tests failing with "readonly database"**: The `TempDir` was dropped at the end of the helper function, deleting the temporary directory before the SQLite connection could write to it. The `TempDir` handle is now kept alive for the duration of each test.
