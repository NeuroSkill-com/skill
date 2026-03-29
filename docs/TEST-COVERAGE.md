# Test Coverage Report

> Auto-generated analysis. Last updated: 2026-03-28

## Summary

| Suite | Test files | Tests | Time (warm) |
|-------|-----------|-------|-------------|
| Rust (20 crates) | ~50 | ~840 | ~15 s |
| Frontend (Vitest) | 28 | 748 | ~8 s |
| LLM E2E | 1 | 1 | ~12 s |
| **Total** | ~79 | **~1 589** | ~35 s |

---

## Rust Crates — Coverage by Crate

### ✅ Well-Tested

| Crate | Tests | Inline tests | Notes |
|-------|-------|-------------|-------|
| skill-tools | 189 | 10/27 src files | Tool parsing, extraction, schemas |
| skill-data | 109 | 5/16 src files | Chat store, hooks, embeddings |
| skill-llm | 48 + E2E | 4/23 src files | Catalog types, chat store, think tracker |
| skill-devices | 45 | 9/11 src files | Session, scores, adapters |
| skill-eeg | 5 + proptests | 8/10 src files | DSP pipeline, band analysis, filters |
| skill-constants | 18 | 1/1 | Channel maps, sample rates |
| skill-exg | 28 | 1/1 | ExG signal processing |
| skill-settings | 11 | 3/4 | Settings API, keychain |
| skill-health | 9 | 1/2 | Health checks |

### ⚠️ Partially Tested

| Crate | Tests | Gap |
|-------|-------|-----|
| skill-history | 2 | `lib.rs` (997 lines) has session listing/loading logic untested |
| skill-screenshots | 5 | Only integration tests; `capture.rs` (1357 lines), `platform.rs` (550 lines) untested |
| skill-commands | 3 | SVG/DOT graph renderers (`svg.rs` 927 lines, `svg_3d.rs` 377 lines) untested |
| skill-tts | 2 | `neutts.rs` (705 lines), `lib.rs` (511 lines), `kitten.rs` (205 lines) untested |
| skill-headless | ~5 | `engine.rs` (1193 lines) — the core headless inference engine — untested |

### ❌ Missing or Minimal Tests

| Crate | Src files | Lines (est.) | What needs testing |
|-------|-----------|-------------|-------------------|
| **skill-calendar** | 6 | ~500 | Platform calendar integrations (macOS/Linux/Windows) |
| **skill-lsl** | 4 | ~310 | LSL + Iroh adapters (existing tests have unused variables) |
| **skill-iroh** | 8 | ~205 untested | `device_receiver.rs` — Iroh device data receiver |
| **skill-tray** | 1 | small | System tray (platform-specific, hard to unit test) |
| **skill-vision** | 1 | Obj-C | macOS Vision OCR bridge (can't unit test easily) |

---

## Rust — Specific Gaps to Address

### High Priority (core logic, easy to test)

1. **`skill-history/src/lib.rs`** (997 lines)
   Session discovery, listing, date grouping, cache invalidation.
   Only 2 tests exist (path construction). Missing: listing with filters,
   date range queries, cache hit/miss, corrupt session handling.

2. **`skill-data/src/screenshot_store.rs`** (682 lines)
   Has 4 basic insert/count tests. Missing: search queries, embedding
   updates, pagination, HNSW ID assignment, migration paths.

3. **`skill-data/src/session_csv.rs`** (637 lines) / **`session_parquet.rs`** (581 lines)
   Session data export. No tests. Should test: round-trip write/read,
   column types, empty sessions, large sessions, corrupt file handling.

4. **`skill-data/src/dnd.rs`** (643 lines)
   Drag-and-drop data handling. No tests.

5. **`skill-tools/src/parse/coerce.rs`** (300 lines) / **`extract.rs`** (474 lines)
   Tool argument coercion and extraction from LLM output.
   Some tests exist in adjacent files but these specific modules lack coverage.

6. **`skill-tools/src/exec/`** — Tool execution layer
   - `tools_fs.rs` (365 lines) — file read/write/edit tools
   - `tools_system.rs` (381 lines) — bash, date tools
   - `tools_web.rs` (416 lines) — web search/fetch tools
   - `safety.rs` (166 lines) — path validation, sandboxing
   - `truncate.rs` (98 lines) — output truncation

### Medium Priority

7. **`skill-llm/src/engine/tool_orchestration.rs`** (891 lines)
   Multi-round tool-calling loop. Tested indirectly via E2E but no unit tests
   for edge cases: max rounds, error recovery, parallel tool calls.

8. **`skill-llm/src/engine/sampling.rs`** (229 lines)
   Temperature, top-p, repetition penalty. No unit tests.

9. **`skill-llm/src/catalog/download.rs`** (518 lines)
   Model download, resume, shard assembly. No unit tests (only E2E).

10. **`skill-commands/src/graph/svg.rs`** (927 lines) + **`svg_3d.rs`** (377 lines)
    EEG graph SVG rendering. Complex math, no tests.

11. **`skill-settings/src/screenshot_config.rs`** (184 lines)
    Screenshot configuration. No tests.

### Low Priority (platform-specific / hard to unit test)

12. **`skill-screenshots/src/capture.rs`** (1357 lines) / **`platform.rs`** (550 lines)
    Screen capture — requires display server, platform APIs.

13. **`skill-headless/src/engine.rs`** (1193 lines)
    Headless inference engine — needs mock LLM server.

14. **`skill-calendar`** — All platform backends need integration tests.

15. **`skill-tts`** — TTS engines need audio hardware or mocking.

---

## Frontend — Coverage

### ✅ Well-Tested (28 test files, 748 tests)

Tested modules: calibration, chat-types, chat-utils, compare-logic, compare-types,
constants, dashboard-logic, devices-logic, electrodes, format, goals-logic,
graph3d-logic, history-helpers, hooks-logic, i18n, llm-helpers, llm-tab-logic,
markdown-normalize, onboarding-logic, screenshots-logic, search-interactive-logic,
search-logic, search-types, sleep-analysis, theme, umap-helpers, umap-viewer-logic,
utils.

### ⚠️ Untested Frontend Modules (>50 lines)

| File | Lines | What it does |
|------|-------|-------------|
| `compare-canvas.ts` | 493 | EEG comparison canvas rendering |
| `history-canvas.ts` | 350 | Session history canvas rendering |
| `stores/theme.svelte.ts` | 338 | Theme store (partial — `theme.test.ts` exists) |
| `stores/chart-colors.svelte.ts` | 205 | Chart color palette logic |
| `stores/titlebar.svelte.ts` | 106 | Titlebar state management |
| `types.ts` | 185 | Shared type definitions |
| `use-canvas.ts` | 130 | Canvas rendering hook |

---

## Recommendations

### Quick Wins (highest value per effort)

1. **`skill-history` lib.rs** — Add session listing/filtering tests (pure logic, no I/O)
2. **`skill-data` session_csv/parquet** — Round-trip tests with tempdir
3. **`skill-tools` exec/ safety.rs** — Path validation tests (security-critical)
4. **`skill-tools` parse/ coerce+extract** — More edge cases for LLM output parsing
5. **`skill-data` screenshot_store** — Search, pagination, embedding update tests

### Test Infrastructure Improvements

- **`cargo-llvm-cov`** — Add code coverage reporting to CI
- **Property testing** — Expand `proptest` usage (currently only in skill-eeg and skill-tools)
- **Snapshot tests** — For SVG graph rendering (skill-commands)
- **Mock LLM** — Lightweight mock for tool_orchestration unit tests
