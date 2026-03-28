// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Save user-provided images from LLM chat (or phone uploads) into the
//! screenshot store so they are indexed and searchable alongside automatic
//! screenshots.
//!
//! Images are:
//!   1. Decoded from base64 data URLs
//!   2. Saved to `~/.skill/screenshots/<YYYYMMDD>/` as WebP
//!   3. Inserted into `screenshots.sqlite` with `source = "llm_chat"` (or `"phone_upload"`)
//!   4. Picked up by the existing embed pipeline for vision embedding + OCR

use std::path::Path;

use skill_data::screenshot_store::{ScreenshotRow, ScreenshotStore};

/// Result of saving a chat image.
pub struct SavedChatImage {
    /// Row ID in the screenshots table.
    pub row_id: i64,
    /// Relative filename (e.g. `"20260328/20260328143025_chat.webp"`).
    pub filename: String,
}

/// Save a base64 data-URL image to the screenshot store.
///
/// `data_url` should be `"data:image/jpeg;base64,..."` or `"data:image/png;base64,..."`.
/// `source` should be `"llm_chat"` or `"phone_upload"`.
/// `caption` is the user's chat message / prompt that accompanied this image.
/// `chat_session_id` is the LLM chat session this image belongs to (0 if none).
///
/// Returns `None` if the image can't be decoded.
pub fn save_chat_image(
    skill_dir: &Path,
    data_url: &str,
    source: &str,
    caption: &str,
    chat_session_id: i64,
) -> Option<SavedChatImage> {
    // Parse data URL: "data:image/jpeg;base64,<data>"
    let (_mime, b64) = parse_data_url(data_url)?;

    // Decode base64
    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;

    // Decode image to get dimensions
    let img = image::load_from_memory(&raw).ok()?;
    let (w, h) = (img.width(), img.height());

    // Generate timestamp-based filename
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let unix_ts = now.as_secs();
    let ts = skill_data::util::unix_to_ts(unix_ts);
    let date_str = &format!("{ts}")[..8]; // YYYYMMDD

    let screenshots_dir = skill_dir.join("screenshots");
    let date_dir = screenshots_dir.join(date_str);
    let _ = std::fs::create_dir_all(&date_dir);

    // Save as WebP (or original format if WebP encoding fails)
    let suffix = match source {
        "llm_chat" => "_chat",
        "phone_upload" => "_phone",
        _ => "_ext",
    };
    let filename = format!("{date_str}/{ts}{suffix}.webp");
    let disk_path = screenshots_dir.join(&filename);

    let file_size = if let Ok(webp_data) = encode_to_webp(&img) {
        std::fs::write(&disk_path, &webp_data).ok()?;
        webp_data.len() as u64
    } else {
        let png_filename = format!("{date_str}/{ts}{suffix}.png");
        let png_path = screenshots_dir.join(&png_filename);
        img.save(&png_path).ok()?;
        let meta = std::fs::metadata(&png_path).ok()?;
        return save_to_store(skill_dir, &png_filename, w, h, meta.len(), ts, unix_ts, source, caption, chat_session_id);
    };

    save_to_store(skill_dir, &filename, w, h, file_size, ts, unix_ts, source, caption, chat_session_id)
}

#[allow(clippy::too_many_arguments)]
fn save_to_store(
    skill_dir: &Path,
    filename: &str,
    w: u32,
    h: u32,
    file_size: u64,
    ts: i64,
    unix_ts: u64,
    source: &str,
    caption: &str,
    chat_session_id: i64,
) -> Option<SavedChatImage> {
    let store = ScreenshotStore::open(skill_dir)?;
    let row_id = store.insert(&ScreenshotRow {
        timestamp: ts,
        unix_ts,
        filename: filename.to_string(),
        width: w,
        height: h,
        file_size,
        hnsw_id: None,
        embedding: None,
        embedding_dim: 0,
        model_backend: String::new(),
        model_id: String::new(),
        image_size: w.max(h),
        quality: 90,
        app_name: format!("LLM Chat ({source})"),
        window_title: caption.chars().take(120).collect(),
        ocr_text: String::new(),
        ocr_embedding: None,
        ocr_embedding_dim: 0,
        ocr_hnsw_id: None,
        source: source.to_string(),
        chat_session_id: if chat_session_id > 0 { Some(chat_session_id) } else { None },
        caption: caption.to_string(),
    })?;

    eprintln!(
        "[chat-image] saved {source} image: {filename} ({w}×{h}, {} bytes) row_id={row_id}",
        file_size
    );

    Some(SavedChatImage { row_id, filename: filename.to_string() })
}

fn parse_data_url(url: &str) -> Option<(&str, &str)> {
    // "data:image/jpeg;base64,/9j/4AAQ..."
    let rest = url.strip_prefix("data:")?;
    let semi = rest.find(';')?;
    let mime = &rest[..semi];
    let after = &rest[semi + 1..];
    let data = after.strip_prefix("base64,")?;
    Some((mime, data))
}

fn encode_to_webp(img: &image::DynamicImage) -> Result<Vec<u8>, String> {
    use std::io::Cursor;
    let rgba = img.to_rgba8();
    let mut buf = Cursor::new(Vec::new());
    // Use PNG as intermediate since the `image` crate's WebP encoder
    // may not be available in all builds.  The embed pipeline will
    // re-encode if needed.
    rgba.write_to(&mut buf, image::ImageFormat::WebP)
        .map_err(|e| format!("webp encode: {e}"))?;
    Ok(buf.into_inner())
}
