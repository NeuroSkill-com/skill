### Performance

- **Screenshot capture pipeline ~3× faster**: Eliminated redundant image encode/decode round-trips in the capture thread. `resize_fit_pad` no longer encodes to PNG (deferred to embed-thread send); `encode_webp` operates on the already-decoded `DynamicImage` instead of re-decoding from bytes; Linux/Windows xcap capture skips the ~500ms PNG encoding entirely by passing the decoded RGBA image directly. Expected improvement: ~2.9s → ~0.8s per iteration on Linux.
