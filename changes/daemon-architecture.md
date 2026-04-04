### Added
- **Thin Tauri client + daemon HTTP architecture**: all business logic, persistence, and background workers now run in a standalone `skill-daemon` process. The Tauri frontend communicates via localhost HTTP API (173 endpoints) and WebSocket event stream.
- **Virtual EEG Device** (Settings → Virtual EEG): configurable synthetic EEG generator with 5 signal templates, adjustable channels/quality/noise, live waveform + band power preview, file replay support.
- **Multi-token API authentication** (Settings → API Tokens): create named tokens with ACL levels (admin/read-only/data/stream) and expiration (week/month/quarter/never). Tokens persist in `~/.skill/daemon/tokens.json`.
- **EEG streaming via daemon WebSocket**: replaces Tauri IPC Channels with auto-reconnecting WS client for real-time EEG/PPG/IMU/band-power events.
- **LLM inference settings UI**: n_batch, n_ubatch, flash attention, offload KQV controls in Settings → LLM.
- **Prebuilt llama.cpp support**: `npm run setup:llama-prebuilt` downloads platform-specific static libraries for ~85× faster dev builds.

### Changed
- **Default LLM inference**: temperature 0.0 (greedy/deterministic), n_batch=2048 for faster prefill.
- **llama-cpp-4** updated from 0.2.24 to 0.2.26.
- **Tauri `generate_handler!`** pruned from 303 to ~105 entries.
- Phone pairing (iroh) now creates a scoped API token embedded in the QR invite payload.
- Pre-commit hook now runs `cargo deny check` for license/advisory validation.

### Removed
- 1,649+ lines of dead Rust proxy code after daemon migration.
- Legacy Tauri IPC Channel subscriptions for EEG/PPG/IMU.
- `PLAN.md` and `implementation-backlog.md` (migration complete).
