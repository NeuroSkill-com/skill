# apple-ocr

Apple Vision framework OCR via compiled Objective-C FFI.

## Overview

Provides on-device text recognition on macOS by bridging to Apple's Vision framework through a thin Objective-C layer compiled at build time via the `cc` crate.

## Public API

| Function | Description |
|---|---|
| `recognize_text(rgba_pixels, width, height)` | Run OCR on raw RGBA pixel data, returns recognized text |
| `recognize_text_from_png(png_bytes)` | Run OCR on an in-memory PNG image |

## Dependencies

- `image` — decoding PNG/WebP/JPEG for the `from_png` path
- `cc` (build) — compiles the bundled `.m` Objective-C source

## Platform

macOS only. The build script links against Apple's Vision and Foundation frameworks.
