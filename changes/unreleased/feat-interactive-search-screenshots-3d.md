### Features

- **Interactive search: screenshot discovery and 3D visualization**: The interactive cross-modal search now discovers screenshots near EEG neighbor timestamps, ranking them by window-title and OCR-text proximity to the query. Screenshot nodes appear as a new layer in the graph with dedicated styling (pink `#ec4899`). Two new edge kinds (`screenshot_prox` for temporal proximity and `ocr_sim` for text-based matches) connect EEG points to relevant screenshots. A new 3D perspective-projected SVG (`svg_3d`) is generated alongside the existing 2D layouts using 3-component PCA across all text embeddings. The `InteractiveGraphNode` struct gains `proj_z`, `filename`, `app_name`, `window_title`, `ocr_text`, and `ocr_similarity` fields. The `InteractiveSearchResult` includes the new `svg_3d` field. Both the Tauri command and WebSocket `interactive_search` handler are updated. DOT and flat SVG exports also render screenshot nodes.

### Refactor

- **3D PCA utility**: Added `pca_3d()` function to `skill-commands` for 3-component power-iteration PCA, complementing the existing `pca_2d()`.
- **SVG 3D generator**: Added `generate_svg_3d()` to `skill-commands::graph` — renders a dark-themed perspective-projected SVG with depth cues (scale, opacity, drop shadows) and a grid floor.
