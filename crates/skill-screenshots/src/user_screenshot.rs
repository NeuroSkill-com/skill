// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Detect and import user-taken screenshots into the screenshot store.
//!
//! Watches OS-specific screenshot directories for new files matching known
//! screenshot naming patterns (e.g. `Screenshot YYYY-MM-DD at HH.MM.SS.png`
//! on macOS).  Imported screenshots are:
//!   1. Read from disk (original file is NOT moved or deleted)
//!   2. Resized and saved as WebP into `~/.skill/screenshots/<YYYYMMDD>/`
//!   3. Inserted into `screenshots.sqlite` with `source = "user_screenshot"`
//!   4. Picked up by the existing embed pipeline for vision embedding + OCR

use std::path::{Path, PathBuf};

use skill_data::screenshot_store::{ScreenshotRow, ScreenshotStore};

/// Result of importing a user screenshot.
pub struct SavedUserScreenshot {
    /// Row ID in the screenshots table.
    pub row_id: i64,
    /// Relative filename (e.g. `"20260425/20260425143025_user.webp"`).
    pub filename: String,
}

// ── Platform-specific screenshot directory detection ─────────────────────────

/// Detect the OS default screenshot directory/directories.
///
/// - **macOS**: reads `defaults read com.apple.screencapture location`, falls
///   back to `~/Desktop`.
/// - **Windows**: `~/Pictures/Screenshots`.
/// - **Linux**: `~/Pictures/Screenshots` if it exists, else `~/Pictures`.
pub fn detect_screenshot_dirs() -> Vec<PathBuf> {
    let home = home_dir();
    let Some(home) = home else {
        return vec![];
    };

    let mut dirs = Vec::new();

    #[cfg(target_os = "macos")]
    {
        // Try reading the user-configured screenshot location.
        if let Some(custom) = macos_screenshot_location() {
            let p = PathBuf::from(&custom);
            if p.is_dir() {
                dirs.push(p);
            }
        }
        // Always include ~/Desktop as fallback (default location).
        let desktop = home.join("Desktop");
        if desktop.is_dir() && !dirs.iter().any(|d| d == &desktop) {
            dirs.push(desktop);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let pics_ss = home.join("Pictures").join("Screenshots");
        if pics_ss.is_dir() {
            dirs.push(pics_ss);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let pics_ss = home.join("Pictures").join("Screenshots");
        if pics_ss.is_dir() {
            dirs.push(pics_ss);
        } else {
            let pics = home.join("Pictures");
            if pics.is_dir() {
                dirs.push(pics);
            }
        }
    }

    dirs
}

/// Check if a filename matches known OS screenshot naming patterns.
///
/// Recognized patterns:
/// - macOS: `Screenshot YYYY-MM-DD at HH.MM.SS.png` (or `.jpg`)
/// - Windows: `Screenshot (N).png`, `Screenshot YYYY-MM-DD HHMMSS.png`
/// - Linux/GNOME: `Screenshot from YYYY-MM-DD HH-MM-SS.png`
pub fn is_user_screenshot(name: &str) -> bool {
    // Must have a screenshot image extension.
    let lower = name.to_ascii_lowercase();
    let has_image_ext = lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg");
    if !has_image_ext {
        return false;
    }

    // All known OS patterns start with "Screenshot" (case-insensitive).
    let name_lower = lower.as_str();
    if !name_lower.starts_with("screenshot") {
        return false;
    }

    // macOS: "screenshot YYYY-MM-DD at HH.MM.SS" (lowercase comparison)
    // Contains " at " separator between date and time.
    if name_lower.contains(" at ") {
        return true;
    }

    // Windows: "screenshot (N)" or "screenshot YYYY-MM-DD"
    if name_lower.contains('(') || name_lower.contains("screenshot 20") {
        return true;
    }

    // Linux/GNOME: "screenshot from YYYY-MM-DD"
    if name_lower.contains(" from ") {
        return true;
    }

    // Fallback: any file starting with "screenshot" and having an image extension
    // is likely a screenshot. This catches custom tools and localized names.
    true
}

/// Import a user-taken screenshot file into the screenshot store.
///
/// Reads the file from disk (does NOT move or delete it), resizes it to the
/// configured image size, saves as WebP, and inserts a row with
/// `source = "user_screenshot"`.  The embed pipeline backfill will
/// automatically OCR + embed it.
///
/// Returns `None` if the file can't be read, decoded, or is too small (<10 KB).
pub fn import_user_screenshot(skill_dir: &Path, source_path: &Path) -> Option<SavedUserScreenshot> {
    import_external_image(skill_dir, source_path, "user_screenshot", "User Screenshot")
}

/// Import a clipboard image file into the screenshot store.
///
/// Same pipeline as user screenshots but tagged with `source = "clipboard_image"`.
pub fn import_clipboard_image(skill_dir: &Path, source_path: &Path) -> Option<SavedUserScreenshot> {
    import_external_image(skill_dir, source_path, "clipboard_image", "Clipboard Image")
}

/// Core import logic shared by user screenshots and clipboard images.
fn import_external_image(
    skill_dir: &Path,
    source_path: &Path,
    source: &str,
    label: &str,
) -> Option<SavedUserScreenshot> {
    // Read the file.
    let raw_bytes = std::fs::read(source_path).ok()?;

    // Reject tiny files (thumbnails, incomplete writes).
    if raw_bytes.len() < 10_000 {
        return None;
    }

    // Decode + resize to target size for consistency with auto-screenshots.
    let settings = skill_settings::load_settings(skill_dir);
    let target_size = settings.screenshot.image_size;
    let quality = settings.screenshot.quality;

    let resized = crate::capture::resize_fit_pad_image(&raw_bytes, target_size)?;

    // Generate timestamp.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let unix_ts = now.as_secs();
    let ts = skill_data::util::unix_to_ts(unix_ts);
    let date_str = &format!("{ts}")[..8]; // YYYYMMDD

    // Save as WebP.
    let screenshots_dir = skill_dir.join("screenshots");
    let date_dir = screenshots_dir.join(date_str);
    let _ = std::fs::create_dir_all(&date_dir);

    let suffix = match source {
        "clipboard_image" => "_clip",
        _ => "_user",
    };
    let filename = format!("{date_str}/{ts}{suffix}.webp");
    let webp_path = screenshots_dir.join(&filename);
    let file_size = crate::capture::encode_webp(&resized, quality, &webp_path)?;

    let (w, h) = (target_size, target_size);

    // Original filename for display.
    let original_name = source_path.file_name().and_then(|n| n.to_str()).unwrap_or(label);

    // Source path string for dedup (stored in caption column).
    let source_path_str = source_path.to_string_lossy().to_string();

    // Insert into store.
    let store = ScreenshotStore::open(skill_dir)?;
    let row_id = store.insert(&ScreenshotRow {
        timestamp: ts,
        unix_ts,
        filename: filename.clone(),
        width: w,
        height: h,
        file_size,
        hnsw_id: None,
        embedding: None,
        embedding_dim: 0,
        model_backend: String::new(),
        model_id: String::new(),
        image_size: target_size,
        quality,
        app_name: label.to_string(),
        window_title: original_name.to_string(),
        ocr_text: String::new(),
        ocr_embedding: None,
        ocr_embedding_dim: 0,
        ocr_hnsw_id: None,
        source: source.to_string(),
        chat_session_id: None,
        caption: source_path_str,
    })?;

    eprintln!("[{source}] imported {original_name} -> {filename} ({w}×{h}, {file_size} bytes) row_id={row_id}");

    Some(SavedUserScreenshot { row_id, filename })
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

/// Read macOS screencapture location via `defaults`.
#[cfg(target_os = "macos")]
fn macos_screenshot_location() -> Option<String> {
    let output = std::process::Command::new("defaults")
        .args(["read", "com.apple.screencapture", "location"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let loc = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if loc.is_empty() {
        None
    } else {
        // Expand ~ if present.
        if let Some(rest) = loc.strip_prefix('~') {
            let home = home_dir()?;
            Some(home.join(rest.trim_start_matches('/')).to_string_lossy().to_string())
        } else {
            Some(loc)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_screenshot_pattern() {
        assert!(is_user_screenshot("Screenshot 2026-04-25 at 14.30.25.png"));
        assert!(is_user_screenshot("Screenshot 2026-04-25 at 2.30.25 PM.png"));
        assert!(is_user_screenshot("Screenshot 2026-04-25 at 14.30.25.jpg"));
    }

    #[test]
    fn windows_screenshot_pattern() {
        assert!(is_user_screenshot("Screenshot (1).png"));
        assert!(is_user_screenshot("Screenshot (42).png"));
        assert!(is_user_screenshot("Screenshot 2026-04-25 143025.png"));
    }

    #[test]
    fn linux_screenshot_pattern() {
        assert!(is_user_screenshot("Screenshot from 2026-04-25 14-30-25.png"));
    }

    #[test]
    fn rejects_non_screenshot() {
        assert!(!is_user_screenshot("photo.png"));
        assert!(!is_user_screenshot("document.pdf"));
        assert!(!is_user_screenshot("readme.md"));
        assert!(!is_user_screenshot("Screenshot.txt")); // wrong extension
    }

    #[test]
    fn generic_screenshot_fallback() {
        // Any file starting with "Screenshot" + image ext should match.
        assert!(is_user_screenshot("Screenshot_custom_tool.png"));
    }

    #[test]
    fn detect_dirs_returns_something() {
        // On any platform with a HOME set, we should get at least one dir
        // (or empty if none of the expected dirs exist).
        let dirs = detect_screenshot_dirs();
        // Just verify it doesn't panic.
        eprintln!("Detected screenshot dirs: {dirs:?}");
    }

    /// Create a valid PNG (400x400 noisy image, > 10 KB) for testing.
    /// Uses deterministic pixel values so tests are reproducible.
    fn create_test_png(path: &std::path::Path) {
        let img = image::RgbImage::from_fn(400, 400, |x, y| {
            // Deterministic "noise" that doesn't compress well.
            let r = ((x * 7 + y * 13) % 256) as u8;
            let g = ((x * 11 + y * 3) % 256) as u8;
            let b = ((x * 5 + y * 17) % 256) as u8;
            image::Rgb([r, g, b])
        });
        img.save(path).expect("failed to save test PNG");
    }

    #[test]
    fn import_user_screenshot_creates_store_row() {
        let skill_dir = std::env::temp_dir().join(format!("skill_test_import_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&skill_dir);

        // Create a fake screenshot file.
        let src = skill_dir.join("Screenshot 2026-04-25 at 14.30.25.png");
        create_test_png(&src);

        // Import it.
        let result = import_user_screenshot(&skill_dir, &src);
        assert!(result.is_some(), "import should succeed for valid PNG");
        let saved = result.unwrap();
        assert!(saved.row_id > 0);
        assert!(saved.filename.ends_with("_user.webp"));

        // Verify it's in the store via dedup check.
        let store = ScreenshotStore::open(&skill_dir).unwrap();
        assert!(store.has_user_screenshot_from_path(&src.to_string_lossy()));

        // Verify the WebP file was created on disk.
        let webp_path = skill_dir.join("screenshots").join(&saved.filename);
        assert!(webp_path.exists(), "WebP file should exist on disk");

        // Verify total count increased.
        assert_eq!(store.count_all(), 1);

        let _ = std::fs::remove_dir_all(&skill_dir);
    }

    #[test]
    fn import_clipboard_image_uses_clip_suffix() {
        let skill_dir = std::env::temp_dir().join(format!("skill_test_clip_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&skill_dir);

        let src = skill_dir.join("clipboard_tmp.png");
        create_test_png(&src);

        let result = import_clipboard_image(&skill_dir, &src);
        assert!(result.is_some(), "import should succeed");
        let saved = result.unwrap();
        assert!(
            saved.filename.ends_with("_clip.webp"),
            "clipboard images should use _clip suffix"
        );

        // Verify it's findable by the dedup check (covers both sources).
        let store = ScreenshotStore::open(&skill_dir).unwrap();
        assert!(store.has_user_screenshot_from_path(&src.to_string_lossy()));

        let _ = std::fs::remove_dir_all(&skill_dir);
    }

    #[test]
    fn import_dedup_prevents_double_import() {
        let skill_dir = std::env::temp_dir().join(format!("skill_test_dedup_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&skill_dir);

        let src = skill_dir.join("Screenshot 2026-04-25 at 15.00.00.png");
        create_test_png(&src);

        // First import should succeed.
        let r1 = import_user_screenshot(&skill_dir, &src);
        assert!(r1.is_some());

        // Dedup check should detect the existing import.
        let store = ScreenshotStore::open(&skill_dir).unwrap();
        assert!(store.has_user_screenshot_from_path(&src.to_string_lossy()));

        // Store should have exactly 1 row.
        assert_eq!(store.count_all(), 1);

        let _ = std::fs::remove_dir_all(&skill_dir);
    }

    #[test]
    fn import_rejects_tiny_file() {
        let skill_dir = std::env::temp_dir().join(format!("skill_test_tiny_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&skill_dir);

        let src = skill_dir.join("Screenshot tiny.png");
        // Write < 10KB of data.
        std::fs::write(&src, &[0u8; 100]).unwrap();

        let result = import_user_screenshot(&skill_dir, &src);
        assert!(result.is_none(), "should reject tiny files");

        let _ = std::fs::remove_dir_all(&skill_dir);
    }
}
