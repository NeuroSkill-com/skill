// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Pure-logic helpers for the system tray: progress-ring icon overlay,
//! shortcut label formatting, and download-progress bucketing.
//!
//! Everything in this crate is platform-agnostic and has **zero** Tauri
//! dependencies.  The Tauri-specific menu building, icon loading, and
//! refresh logic remain in `src-tauri/src/tray.rs`.

// ── Progress helpers ──────────────────────────────────────────────────────────

/// Quantise a 0.0–1.0 progress value into 5 %-point buckets (0..=20).
/// Used as a deduplication key so the tray icon is only re-rendered when
/// the visible progress ring actually changes.
pub fn progress_bucket(progress: f32) -> u8 {
    ((progress.clamp(0.0, 1.0) * 20.0).round() as u8).min(20)
}

/// Convert a 0.0–1.0 progress value into a display percentage (0..=100).
pub fn progress_percent(progress: f32) -> u8 {
    ((progress.clamp(0.0, 1.0) * 100.0).round() as u8).min(100)
}

// ── Text helpers ──────────────────────────────────────────────────────────────

/// Truncate `text` in the middle to at most `max_chars`, inserting `...`.
///
/// ```
/// # use skill_tray::ellipsize_middle;
/// assert_eq!(ellipsize_middle("abcdefghij", 7), "ab...ij");
/// assert_eq!(ellipsize_middle("short", 10), "short");
/// ```
pub fn ellipsize_middle(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return "...".to_string();
    }

    let head = (max_chars - 3) / 2;
    let tail = max_chars - 3 - head;
    format!(
        "{}...{}",
        chars[..head].iter().collect::<String>(),
        chars[chars.len() - tail..].iter().collect::<String>(),
    )
}

/// Format a keyboard shortcut string for display in a menu label.
///
/// On macOS, modifier keys are replaced with their standard symbols
/// (⌘, ⇧, ⌥, ⌃) and the `+` separators between modifiers are removed,
/// matching native macOS menu conventions.
///
/// On Linux/Windows, modifiers are spelled out (`Ctrl`, `Shift`, `Alt`)
/// and joined with `+`.
///
/// Returns an empty string if `shortcut` is blank.
///
/// ```
/// # use skill_tray::shortcut_suffix;
/// // On Linux: "  Ctrl+Shift+O"
/// // On macOS: "  ⌃⇧O"
/// let s = shortcut_suffix("CmdOrCtrl+Shift+O");
/// assert!(!s.is_empty());
/// ```
pub fn shortcut_suffix(shortcut: &str) -> String {
    if shortcut.trim().is_empty() {
        return String::new();
    }

    let parts: Vec<&str> = shortcut.trim().split('+').collect();

    if cfg!(target_os = "macos") {
        // macOS: render modifiers as symbols, no separators between them,
        // then append the key character.
        let mut modifiers = String::new();
        let mut key = String::new();
        for part in &parts {
            let p = part.trim();
            match p {
                "CmdOrCtrl" | "Command" | "Cmd" | "Meta" => modifiers.push('\u{2318}'),
                "Ctrl" | "Control" => modifiers.push('\u{2303}'),
                "Shift" => modifiers.push('\u{21E7}'),
                "Alt" | "Option" => modifiers.push('\u{2325}'),
                "Plus" => key = "+".into(),
                other => {
                    let cleaned = other.replace("Arrow", "");
                    key = cleaned;
                }
            }
        }
        format!("  {modifiers}{key}")
    } else {
        // Linux / Windows: human-readable text joined with +.
        let mut tokens: Vec<String> = Vec::new();
        for part in &parts {
            let p = part.trim();
            match p {
                "CmdOrCtrl" | "Command" | "Meta" => tokens.push("Ctrl".into()),
                "Cmd" => tokens.push("Ctrl".into()),
                "Option" => tokens.push("Alt".into()),
                "Plus" => tokens.push("+".into()),
                other => {
                    let cleaned = other.replace("Arrow", "");
                    tokens.push(cleaned);
                }
            }
        }
        format!("  {}", tokens.join("+"))
    }
}

/// Append the formatted shortcut suffix to a menu label.
///
/// ```
/// # use skill_tray::with_shortcut;
/// let label = with_shortcut("Open", "CmdOrCtrl+O");
/// // On macOS: "Open  ⌘O"
/// // On Linux: "Open  Ctrl+O"
/// ```
pub fn with_shortcut(label: &str, shortcut: &str) -> String {
    format!("{label}{}", shortcut_suffix(shortcut))
}

// ── Icon progress-ring overlay ────────────────────────────────────────────────

/// Render a circular progress ring around an RGBA icon image.
///
/// `base_rgba` is the raw RGBA pixel buffer; `width` / `height` are the
/// image dimensions.  `progress` is clamped to `0.0..=1.0`.
///
/// Returns a new owned RGBA buffer with the ring composited on top.
/// The caller is responsible for wrapping it in the platform's `Image` type.
pub fn overlay_progress_bar(base_rgba: &[u8], width: u32, height: u32, progress: f32) -> Vec<u8> {
    let mut rgba = base_rgba.to_vec();
    let progress = progress.clamp(0.0, 1.0);

    if width < 8 || height < 8 {
        return rgba;
    }

    let cx = (width as f32 - 1.0) * 0.5;
    let cy = (height as f32 - 1.0) * 0.5;
    let outer = (width.min(height) as f32 * 0.5) - 0.75;
    let thickness = ((width.min(height) as f32) * 0.24).clamp(2.0, 5.0);
    let inner = (outer - thickness).max(0.0);
    let start_angle = -std::f32::consts::FRAC_PI_2;
    let end_angle = start_angle + progress * std::f32::consts::TAU;

    fn blend(rgba: &mut [u8], idx: usize, color: [u8; 4]) {
        let alpha = color[3] as u16;
        let inv = 255u16.saturating_sub(alpha);
        rgba[idx] = (((rgba[idx] as u16 * inv) + (color[0] as u16 * alpha)) / 255) as u8;
        rgba[idx + 1] = (((rgba[idx + 1] as u16 * inv) + (color[1] as u16 * alpha)) / 255) as u8;
        rgba[idx + 2] = (((rgba[idx + 2] as u16 * inv) + (color[2] as u16 * alpha)) / 255) as u8;
        rgba[idx + 3] = rgba[idx + 3].max(color[3]);
    }

    fn angle_in_arc(angle: f32, start: f32, end: f32) -> bool {
        if end >= std::f32::consts::TAU + start {
            return true;
        }
        if end <= start {
            return false;
        }
        if angle >= start {
            angle <= end
        } else {
            angle + std::f32::consts::TAU <= end
        }
    }

    // Draw high-contrast circular progress ring: dark track + bright filled arc.
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < inner || dist > outer {
                continue;
            }

            let mut angle = dy.atan2(dx);
            if angle < start_angle {
                angle += std::f32::consts::TAU;
            }
            let in_progress_arc = angle_in_arc(angle, start_angle, end_angle);

            let idx = ((y * width + x) * 4) as usize;

            // Ring track.
            blend(&mut rgba, idx, [12, 16, 22, 220]);

            // Bright progress arc.
            if in_progress_arc && progress > 0.0 {
                blend(&mut rgba, idx, [255, 255, 255, 245]);
            }

            // Outer halo.
            if dist > outer - 0.8 {
                if in_progress_arc && progress > 0.0 {
                    blend(&mut rgba, idx, [255, 255, 255, 210]);
                } else {
                    blend(&mut rgba, idx, [0, 0, 0, 190]);
                }
            }
        }
    }

    // Dim unfinished interior sector.
    if progress < 1.0 {
        let interior_radius = (inner - 0.8).max(0.0);
        for y in 0..height {
            for x in 0..width {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > interior_radius {
                    continue;
                }
                let mut angle = dy.atan2(dx);
                if angle < start_angle {
                    angle += std::f32::consts::TAU;
                }
                if !angle_in_arc(angle, start_angle, end_angle) {
                    let idx = ((y * width + x) * 4) as usize;
                    rgba[idx] = ((rgba[idx] as u16 * 72) / 100) as u8;
                    rgba[idx + 1] = ((rgba[idx + 1] as u16 * 72) / 100) as u8;
                    rgba[idx + 2] = ((rgba[idx + 2] as u16 * 72) / 100) as u8;
                }
            }
        }
    }

    rgba
}

// ── Minimum rebuild interval ──────────────────────────────────────────────────

/// Minimum interval between full native menu rebuilds (ms).
/// Prevents multiple rapid state changes from blocking the main thread.
pub use skill_constants::MENU_REBUILD_MIN_MS;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bucket_boundaries() {
        assert_eq!(progress_bucket(0.0), 0);
        assert_eq!(progress_bucket(0.5), 10);
        assert_eq!(progress_bucket(1.0), 20);
        assert_eq!(progress_bucket(1.5), 20); // clamped
    }

    #[test]
    fn progress_percent_boundaries() {
        assert_eq!(progress_percent(0.0), 0);
        assert_eq!(progress_percent(0.5), 50);
        assert_eq!(progress_percent(1.0), 100);
    }

    #[test]
    fn ellipsize_short_string_unchanged() {
        assert_eq!(ellipsize_middle("hello", 10), "hello");
    }

    #[test]
    fn ellipsize_long_string() {
        assert_eq!(ellipsize_middle("abcdefghij", 7), "ab...ij");
    }

    #[test]
    fn ellipsize_tiny_max() {
        assert_eq!(ellipsize_middle("abcdefghij", 3), "...");
    }

    #[test]
    fn shortcut_suffix_empty() {
        assert_eq!(shortcut_suffix(""), "");
        assert_eq!(shortcut_suffix("   "), "");
    }

    #[test]
    fn shortcut_suffix_formats_modifiers() {
        // On Linux this should produce "  Ctrl+Shift+O"
        // On macOS this should produce "  ⌘⇧O"
        let s = shortcut_suffix("CmdOrCtrl+Shift+O");
        assert!(!s.is_empty());
        if cfg!(target_os = "macos") {
            assert_eq!(s, "  \u{2318}\u{21E7}O");
        } else {
            assert_eq!(s, "  Ctrl+Shift+O");
        }
    }

    #[test]
    fn shortcut_suffix_single_key() {
        let s = shortcut_suffix("CmdOrCtrl+,");
        if cfg!(target_os = "macos") {
            assert_eq!(s, "  \u{2318},");
        } else {
            assert_eq!(s, "  Ctrl+,");
        }
    }

    #[test]
    fn with_shortcut_appends() {
        let s = with_shortcut("Open", "");
        assert_eq!(s, "Open");
    }

    #[test]
    fn with_shortcut_formats_nicely() {
        let s = with_shortcut("Settings…", "CmdOrCtrl+,");
        if cfg!(target_os = "macos") {
            assert_eq!(s, "Settings…  \u{2318},");
        } else {
            assert_eq!(s, "Settings…  Ctrl+,");
        }
    }

    #[test]
    fn overlay_tiny_image_passthrough() {
        let rgba = vec![0u8; 4 * 4 * 4]; // 4×4
        let out = overlay_progress_bar(&rgba, 4, 4, 0.5);
        assert_eq!(out.len(), rgba.len()); // no panic, returns unchanged
    }

    #[test]
    fn overlay_normal_image_modifies_pixels() {
        let size = 32u32;
        let rgba = vec![128u8; (size * size * 4) as usize];
        let out = overlay_progress_bar(&rgba, size, size, 0.5);
        assert_eq!(out.len(), rgba.len());
        // At 50% progress some pixels should be modified (ring drawn)
        assert_ne!(out, rgba, "overlay should modify some pixels");
    }

    #[test]
    fn overlay_zero_progress_still_draws_track() {
        let size = 32u32;
        let rgba = vec![200u8; (size * size * 4) as usize];
        let out = overlay_progress_bar(&rgba, size, size, 0.0);
        assert_ne!(out, rgba, "even 0% should draw the dark track ring");
    }

    #[test]
    fn overlay_full_progress() {
        let size = 32u32;
        let rgba = vec![0u8; (size * size * 4) as usize];
        let out = overlay_progress_bar(&rgba, size, size, 1.0);
        assert_eq!(out.len(), rgba.len());
        // Should not panic, should draw full ring
        assert_ne!(out, rgba);
    }

    #[test]
    fn overlay_clamped_above_one() {
        let size = 16u32;
        let rgba = vec![0u8; (size * size * 4) as usize];
        // progress > 1.0 clamped to 1.0
        let out = overlay_progress_bar(&rgba, size, size, 5.0);
        assert_eq!(out.len(), rgba.len());
    }

    #[test]
    fn progress_bucket_negative_clamped() {
        assert_eq!(progress_bucket(-1.0), 0);
    }

    #[test]
    fn progress_percent_negative_clamped() {
        assert_eq!(progress_percent(-0.5), 0);
    }

    #[test]
    fn progress_percent_above_one_clamped() {
        assert_eq!(progress_percent(2.0), 100);
    }

    #[test]
    fn ellipsize_exact_length() {
        assert_eq!(ellipsize_middle("abcde", 5), "abcde");
    }

    #[test]
    fn ellipsize_one_over() {
        assert_eq!(ellipsize_middle("abcdef", 5), "a...f");
    }

    #[test]
    fn shortcut_suffix_plus_key() {
        let s = shortcut_suffix("CmdOrCtrl+Plus");
        assert!(s.contains("+"), "Plus should render as +: {s}");
    }

    #[test]
    fn shortcut_suffix_arrow_key() {
        let s = shortcut_suffix("CmdOrCtrl+ArrowUp");
        // "Arrow" prefix stripped
        assert!(s.contains("Up"));
        assert!(!s.contains("Arrow"));
    }
}
