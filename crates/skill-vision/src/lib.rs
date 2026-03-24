// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Safe Rust wrapper around the Apple Vision framework OCR FFI.
// On non-macOS platforms, `recognize_text` is a no-op that returns `None`.

/// Recognize text in an RGBA image using Apple Vision framework.
///
/// Runs on GPU / Apple Neural Engine via `VNRecognizeTextRequest`.
/// Typically completes in 20–50 ms for a 768×768 image.
///
/// # Arguments
/// * `rgba_pixels` — Raw RGBA pixel data (4 bytes per pixel, row-major).
/// * `width`       — Image width in pixels.
/// * `height`      — Image height in pixels.
///
/// # Returns
/// `Some(text)` with extracted text (lines separated by `\n`), or `None`
/// if no text was found or the platform doesn't support it.
pub fn recognize_text(rgba_pixels: &[u8], width: u32, height: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        recognize_text_macos(rgba_pixels, width, height)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (rgba_pixels, width, height);
        None
    }
}

/// Convenience: run OCR on raw PNG/JPEG/WebP bytes.
///
/// Decodes the image, converts to RGBA, and calls [`recognize_text`].
/// Requires the `image` crate (which is already a dependency of the
/// consuming crate).
pub fn recognize_text_from_png(png_bytes: &[u8]) -> Option<String> {
    let img = image::load_from_memory(png_bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    recognize_text(img.as_raw(), w, h)
}

#[cfg(target_os = "macos")]
fn recognize_text_macos(rgba_pixels: &[u8], width: u32, height: u32) -> Option<String> {
    if rgba_pixels.len() < (width as usize * height as usize * 4) {
        return None;
    }

    extern "C" {
        fn apple_vision_ocr(
            rgba_pixels: *const u8,
            width: u32,
            height: u32,
            out_len: *mut u32,
        ) -> *mut u8;
    }

    // SAFETY: `apple_vision_ocr` is our compiled Objective-C FFI that:
    //   1. Takes a valid RGBA pixel pointer (we verified length above).
    //   2. Returns a malloc'd UTF-8 buffer (or null) with length in `out_len`.
    //   3. The returned pointer must be freed with `free()` after use.
    // `from_raw_parts` is safe because `ptr` is non-null and `len` bytes
    // were allocated by the FFI. `libc_free` releases the malloc'd buffer.
    unsafe {
        let mut len: u32 = 0;
        let ptr = apple_vision_ocr(rgba_pixels.as_ptr(), width, height, &mut len);
        if ptr.is_null() {
            return None;
        }

        let slice = std::slice::from_raw_parts(ptr, len as usize);
        let text = String::from_utf8_lossy(slice).into_owned();
        libc_free(ptr as *mut std::ffi::c_void);

        let text = text.trim().to_string();
        if text.is_empty() { None } else { Some(text) }
    }
}

#[cfg(target_os = "macos")]
extern "C" {
    fn free(ptr: *mut std::ffi::c_void);
}

#[cfg(target_os = "macos")]
/// SAFETY: Caller must pass a pointer originally returned by `malloc` / the
/// Apple Vision OCR FFI.  The pointer is invalidated after this call.
unsafe fn libc_free(ptr: *mut std::ffi::c_void) {
    // SAFETY: `ptr` was allocated by the C side via `malloc`.
    unsafe { free(ptr) };
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_image_returns_none() {
        assert!(super::recognize_text(&[], 0, 0).is_none());
    }

    #[test]
    fn too_small_buffer_returns_none() {
        assert!(super::recognize_text(&[0; 4], 2, 2).is_none());
    }
}
