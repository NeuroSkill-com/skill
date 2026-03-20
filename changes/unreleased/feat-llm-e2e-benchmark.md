### Features

- **LLM E2E integration test with benchmarking**: Added a full end-to-end Rust integration test (`crates/skill-llm/tests/llm_e2e.rs`) that downloads the smallest catalog model, starts the LLM server, runs a plain chat completion, then runs a tool-calling chat (date tool), and verifies the entire pipeline. Every step is benchmarked with timing, throughput (tok/s), and all LLM responses and tool events are captured. A detailed summary report is printed both during execution (live progress) and as a formatted table at the end. Runnable via `npm run test:llm:e2e`.
