// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Animated GIF encoding from a sequence of captured frames.
//!
//! These functions are not used in the periodic capture loop (GIF capture is
//! script-only) but are kept available for the script/tool API.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, GenericImageView, ImageReader};

/// Encode a sequence of PNG/raw image frames into an animated GIF.
///
/// - `frames`: raw image bytes (PNG, WebP, etc.) for each frame
/// - `target_size`: resize each frame to fit this square (aspect-preserving + pad)
/// - `frame_delay_cs`: delay between frames in centiseconds (e.g. 10 = 100 ms)
/// - `out_path`: destination `.gif` file
///
/// Returns the file size in bytes, or `None` on failure.
pub(crate) fn encode_gif(frames: &[Vec<u8>], target_size: u32, frame_delay_cs: u16, out_path: &Path) -> Option<u64> {
    if frames.is_empty() {
        return None;
    }

    // Decode and resize all frames to RGBA at target_size x target_size.
    let resized: Vec<image::RgbaImage> = frames
        .iter()
        .filter_map(|raw| {
            let img = ImageReader::new(Cursor::new(raw))
                .with_guessed_format()
                .ok()?
                .decode()
                .ok()?;

            let (w, h) = img.dimensions();
            if w == 0 || h == 0 {
                return None;
            }
            let scale = (target_size as f64 / w as f64).min(target_size as f64 / h as f64);
            let nw = (w as f64 * scale).round() as u32;
            let nh = (h as f64 * scale).round() as u32;

            let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Triangle);
            let mut canvas = DynamicImage::new_rgba8(target_size, target_size);
            let ox = (target_size - nw) / 2;
            let oy = (target_size - nh) / 2;
            image::imageops::overlay(&mut canvas, &resized, ox as i64, oy as i64);
            Some(canvas.into_rgba8())
        })
        .collect();

    if resized.is_empty() {
        return None;
    }

    let w = resized[0].width() as u16;
    let h = resized[0].height() as u16;

    let mut buf: Vec<u8> = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut buf, w, h, &[]).ok()?;
        encoder.set_repeat(gif::Repeat::Infinite).ok()?;

        for rgba in &resized {
            let mut pixels = rgba.as_raw().to_vec();
            // Speed 10 = fast quantisation (lower quality, much faster)
            let mut frame = gif::Frame::from_rgba_speed(w, h, &mut pixels, 10);
            frame.delay = frame_delay_cs;
            encoder.write_frame(&frame).ok()?;
        }
    }

    if let Some(parent) = out_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out_path, &buf).ok()?;
    Some(buf.len() as u64)
}

/// Extract the middle frame from a list of raw image buffers, resize it,
/// and return it as PNG bytes suitable for CLIP embedding.
///
/// The "representative frame" captures the mid-point of the animation,
/// which is more informative than the first or last frame for scrolling
/// content.
pub(crate) fn representative_frame_png(frames: &[Vec<u8>], target_size: u32) -> Option<Vec<u8>> {
    if frames.is_empty() {
        return None;
    }
    let mid = frames.len() / 2;
    let img = ImageReader::new(Cursor::new(&frames[mid]))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;

    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (target_size as f64 / w as f64).min(target_size as f64 / h as f64);
    let nw = (w as f64 * scale).round() as u32;
    let nh = (h as f64 * scale).round() as u32;
    let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Triangle);

    let mut canvas = DynamicImage::new_rgb8(target_size, target_size);
    let ox = (target_size - nw) / 2;
    let oy = (target_size - nh) / 2;
    image::imageops::overlay(&mut canvas, &resized, ox as i64, oy as i64);

    let mut png_bytes = Vec::new();
    canvas
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .ok()?;
    Some(png_bytes)
}
