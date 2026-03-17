### Dependencies

- **Upgrade skill-headless wry to 0.54**: Updated `skill-headless` crate from `wry` 0.49 to 0.54.3 to match the workspace's tauri-runtime-wry dependency, resolving a `kuchikiki` version conflict. The only API change was renaming `WebViewBuilder::with_web_context()` to `WebViewBuilder::new_with_web_context()`.

### Features

- **Headless browser rendering in web_fetch tool**: Added `render` parameter to the `web_fetch` LLM tool. When `render=true`, pages are loaded in a headless browser (via `skill-headless`) that executes JavaScript, enabling content extraction from SPAs and dynamically rendered pages. Supports optional `wait_ms`, `selector` (CSS selector to wait for), and `eval_js` (custom JS to evaluate) parameters.

- **Headless browser rendering in web_search tool**: Added `render` and `render_count` parameters to the `web_search` LLM tool. When `render=true`, the top N search result URLs are visited in a headless browser and their rendered text content is included in the results under a `rendered_text` field, giving the LLM access to full page content including JS-rendered material.
