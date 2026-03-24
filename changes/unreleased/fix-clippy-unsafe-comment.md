### Bugfixes

- **Add missing safety comment for unsafe block**: Added `// SAFETY:` comment on the Linux `RLIMIT_STACK` unsafe block in `main.rs` to satisfy `clippy::undocumented_unsafe_blocks` (Rust 1.94).
