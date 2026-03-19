# skill-vision

Apple Vision framework OCR via compiled Objective-C FFI.

## Overview

Thin safe-Rust wrapper around the Apple Vision framework's `VNRecognizeTextRequest` for on-device optical character recognition. Runs on GPU / Apple Neural Engine and typically completes in 20–50 ms for a 768×768 image. On non-macOS platforms, all functions are no-ops that return `None`.

The Objective-C implementation (`vision_ocr.m`) is compiled at build time via the `cc` build script.

## Public API

| Function | Description |
|---|---|
| `recognize_text(rgba_pixels, width, height)` | Run OCR on raw RGBA pixel data. Returns `Some(text)` with `\n`-separated lines, or `None` if no text found or unsupported platform. |
| `recognize_text_from_png(png_bytes)` | Convenience wrapper — decodes PNG/JPEG/WebP bytes via the `image` crate, converts to RGBA, and calls `recognize_text`. |

## Platform support

| Platform | Behavior |
|---|---|
| **macOS** | Full OCR via `VNRecognizeTextRequest` (Vision.framework) |
| **Linux / Windows** | No-op — returns `None` |

## Dependencies

- `image` — image decoding (PNG, WebP, JPEG)
- `cc` (build) — compiles the Objective-C FFI source
