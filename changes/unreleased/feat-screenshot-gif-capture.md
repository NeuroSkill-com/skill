### Features

- **Animated GIF capture for scrolling/animated windows**: The screenshot capture worker now detects motion between consecutive frames using pixel-difference scoring. When the change exceeds a configurable threshold (default 5%), a rapid burst of frames is captured and encoded as an animated GIF alongside the still WebP screenshot. New config fields: `gif_enabled`, `gif_frame_count`, `gif_frame_delay_ms`, `gif_motion_threshold`, `gif_max_size_kb`. The middle frame of the burst is used as the representative image for CLIP embedding and OCR. GIFs exceeding the size limit are automatically discarded.

### Refactor

- **New `gif_encode` module in `skill-screenshots`**: Extracted GIF encoding and representative-frame extraction into a dedicated module (`gif_encode.rs`) with `encode_gif()` and `representative_frame_png()` helpers.
- **Motion detection and burst capture in `platform.rs`**: Added `motion_score()` for pixel-diff comparison and `capture_burst()` for rapid multi-frame capture.
- **`gif_filename` column in screenshot store**: Added SQLite migration and `update_gif_filename()` method to `ScreenshotStore`. All query result types (`ScreenshotResult`) now include the `gif_filename` field.
