### Bugfixes

- **Fix CI compilation and clippy errors**: Added missing `std::io::Cursor` import for Linux in `skill-screenshots`, replaced `.err().expect()` with `.expect_err()` in `skill-llm` tool orchestration, used `.is_multiple_of()` instead of manual modulo check, replaced `.map_or(true, …)` with `.is_none_or(…)`, and fixed `needless_range_loop` clippy warnings in generation and actor modules.
