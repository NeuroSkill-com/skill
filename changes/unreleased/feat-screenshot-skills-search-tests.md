### Docs

- **Screenshot & skills search tests**: Added comprehensive smoke tests for all 6 screenshot WS commands (`search_screenshots`, `screenshots_around`, `search_screenshots_vision`, `search_screenshots_by_image_b64`, `screenshots_for_eeg`, `eeg_for_screenshots`) covering semantic/substring modes, cross-modal EEG↔screenshot bridging, CLIP vision search, base64 image upload, error handling for missing/invalid fields, and result structure validation. Also added skills command rejection tests confirming Tauri-only commands are correctly rejected over WS/HTTP.
