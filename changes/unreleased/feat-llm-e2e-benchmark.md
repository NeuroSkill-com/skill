### Features

- **LLM E2E integration test with benchmarking and mock EEG data**: Full end-to-end Rust integration test (`crates/skill-llm/tests/llm_e2e.rs`) that: downloads a capable model (>=1.5B), starts the LLM server, runs a plain chat, a date tool-calling chat, and a NeuroSkill status tool-calling chat with a mock EEG API server returning realistic brain-state data (device info, signal quality, meditation/focus scores, session history, labels). Every step is benchmarked with timing and throughput (tok/s). All responses and tool events are captured and displayed in a formatted report. Runnable via `npm run test:llm:e2e`.

### Build

- **LLM E2E test in CI**: Added `llm-e2e` job to the CI workflow that runs the full LLM integration test on main pushes and manual triggers. The HuggingFace model cache is persisted across runs. The full report is saved as a build artifact (`llm-e2e-report`) with 30-day retention, and the formatted report table is rendered in the GitHub Actions Job Summary. The Discord notification now includes the LLM E2E result.

### UI

- **Smarter onboarding LLM model selection**: The onboarding model picker now uses a priority chain: already-downloaded model → Qwen3.5 4B Q4_K_M → LFM2.5-VL 1.6B Q8_0 (ultra-compact fallback) → any recommended model (smallest first). This ensures low-memory devices get a working LLM out of the box.
