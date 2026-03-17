// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Image decoding helpers for base64 data-URLs in chat messages.

use serde_json::Value;

/// Decode a base64 data-URL (`data:<mime>;base64,<data>`) or return `None`
/// for plain HTTP/S URLs (which we cannot fetch synchronously from the actor).
fn decode_image_url(url: &str) -> Option<Vec<u8>> {
    let data = url.strip_prefix("data:")?;
    // data:<mime>;base64,<payload>
    let payload = data.split(';').nth(1)?.strip_prefix("base64,")?;
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(payload).ok()
}

/// Decode all base64-embedded images across an entire messages array.
///
/// Iterates every message's `content` field (which may be a string or an
/// OpenAI-style parts array) and collects raw JPEG/PNG bytes in document
/// order.  Plain HTTP/S image URLs are silently skipped — only
/// `data:<mime>;base64,<…>` data-URLs are supported.
///
/// Call this before passing `messages` to [`LlmServerState::chat`] so the
/// actor receives pre-decoded bytes alongside the text context.
pub fn extract_images_from_messages(messages: &[Value]) -> Vec<Vec<u8>> {
    messages.iter()
        .flat_map(|m| {
            m.get("content")
                .map(extract_images_from_content)
                .unwrap_or_default()
        })
        .collect()
}

/// Extract all raw image bytes from a single `content` value (string or parts array).
/// Returns images in document order.
fn extract_images_from_content(content: &Value) -> Vec<Vec<u8>> {
    let Value::Array(parts) = content else { return Vec::new() };
    parts.iter()
        .filter_map(|p| {
            if p.get("type")?.as_str() != Some("image_url") { return None; }
            let url = p.get("image_url")?.get("url")?.as_str()?;
            decode_image_url(url)
        })
        .collect()
}
