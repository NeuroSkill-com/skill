### Features

- **Screenshot capture + vision embedding system**: periodic active-window capture with CLIP vision embedding (ONNX) and HNSW index. macOS CoreGraphics FFI, Linux X11/Wayland, Windows GDI. Configurable interval, size, quality, and embedding backend.

- **OCR text extraction + text embedding**: on-device OCR via `ocrs` crate. Dual HNSW architecture for visual and text similarity search.

- **Screenshots Settings UI tab**: full configuration with live re-embed progress.

- **Configurable OCR engine, GPU/CPU toggle**: `ScreenshotConfig` extended with OCR engine, model, and GPU settings.
