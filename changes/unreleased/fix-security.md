### Bugfixes

- **Path traversal in `delete_session`**: `csv_path` parameter was passed unsanitized to `fs::remove_file`. Now validates the path is within `skill_dir` via canonicalize + starts_with.
- **Timing-vulnerable token comparison**: default bearer token checks in auth middleware used `==` (variable-time). Switched to `constant_time_eq` for both in-memory and on-disk token paths.
- **Unbounded CSV memory in EXG loader**: loading a CSV with millions of rows would allocate unlimited RAM. Added a 4M row cap (~4.3 hours at 256 Hz).
