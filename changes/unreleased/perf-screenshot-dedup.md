### Performance

- **Screenshot duplicate detection**: when a new screenshot is identical to the previous one (same resized-PNG hash), the embed thread now copies the vision embedding, OCR text, and OCR text embedding from the previous row instead of re-running the vision encoder, OCR engine, and text embedder. This eliminates redundant GPU/CPU inference when the screen content hasn't changed (e.g. idle desktop, paused video).
