### Features

- **LLM VLM screenshot backend**: Added a new `"llm-vlm"` screenshot embed backend that uses the LLM vision model for both image embeddings (mean-pooled vision tokens via mmproj) and OCR (VLM-based text extraction via chat completion). This allows benchmarking VLM-based OCR against traditional OCR engines (ocrs / Apple Vision). Selectable in Settings → Screenshots → Embed backend. Also added `ocr_via_llm()` to the `ScreenshotContext` trait.

- **VLM image embedding benchmark in E2E test**: Added step 9 to the LLM E2E test that benchmarks VLM image embedding via the `EmbedImage` request path. Reports embedding dimensions and timing. Skipped with a warning when no mmproj is loaded.

### Refactor

- **skill-headless is now optional in skill-tools**: The `skill-headless` dependency (wry/tao headless browser) is now behind an optional `"headless"` feature (on by default). `skill-llm` depends on `skill-tools` with `default-features = false`, so the LLM E2E test no longer pulls in wry/tao (fixes macOS compile error). When headless is disabled, web_fetch/web_search gracefully fall back to plain HTTP.
