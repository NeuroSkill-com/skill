### Dependencies

- **Align Arrow stack to v58**: bumped `parquet`, `arrow-array`, and `arrow-schema` in `skill-data` all to version 58, and updated Parquet EEG column casting in `src-tauri` to use `downcast_ref::<Float64Array>()` (replacing the removed `as_primitive_opt`).
