// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Screenshot configuration types.
//!
//! Extracted into `skill-settings` so that the heavy `skill-screenshots`
//! crate (which pulls in `xcap` → `pipewire` on Linux) is not required
//! just to read/write the configuration.

use serde::{Deserialize, Serialize};

use skill_constants::{EMBEDDING_EPOCH_SECS, SCREENSHOT_INTERVAL_MAX_MULT, SCREENSHOT_INTERVAL_MIN_MULT};

pub fn default_screenshot_interval() -> u32 {
    5
}
pub fn default_screenshot_image_size() -> u32 {
    768
}
pub fn default_screenshot_quality() -> u8 {
    60
}
pub fn default_screenshot_session_only() -> bool {
    true
}
pub fn default_screenshot_embed_backend() -> String {
    "fastembed".into()
}
pub fn default_screenshot_fastembed_model() -> String {
    "nomic-embed-vision-v1.5".into()
}
pub fn default_screenshot_ocr_enabled() -> bool {
    true
}
pub fn default_screenshot_ocr_engine() -> String {
    #[cfg(target_os = "macos")]
    {
        "apple-vision".into()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "ocrs".into()
    }
}
pub fn default_screenshot_use_gpu() -> bool {
    true
}
pub fn default_screenshot_gif_enabled() -> bool {
    false
}
pub fn default_screenshot_gif_frame_count() -> u32 {
    15
}
pub fn default_screenshot_gif_frame_delay() -> u32 {
    100
}
pub fn default_screenshot_gif_motion_thr() -> f32 {
    0.05
}
pub fn default_screenshot_gif_max_size_kb() -> u64 {
    2048
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenshotConfig {
    pub enabled: bool,

    #[serde(default = "default_screenshot_interval")]
    pub interval_secs: u32,

    #[serde(default = "default_screenshot_image_size")]
    pub image_size: u32,

    #[serde(default = "default_screenshot_quality")]
    pub quality: u8,

    #[serde(default = "default_screenshot_session_only")]
    pub session_only: bool,

    #[serde(default = "default_screenshot_embed_backend")]
    pub embed_backend: String,

    #[serde(default = "default_screenshot_fastembed_model")]
    pub fastembed_model: String,

    #[serde(default = "default_screenshot_ocr_enabled")]
    pub ocr_enabled: bool,

    #[serde(default = "default_screenshot_ocr_engine")]
    pub ocr_engine: String,

    #[serde(default = "default_screenshot_use_gpu")]
    pub use_gpu: bool,

    /// Enable animated GIF capture when window motion is detected.
    #[serde(default = "default_screenshot_gif_enabled")]
    pub gif_enabled: bool,

    /// Number of frames to capture in a GIF burst.
    #[serde(default = "default_screenshot_gif_frame_count")]
    pub gif_frame_count: u32,

    /// Delay between GIF frames in milliseconds.
    #[serde(default = "default_screenshot_gif_frame_delay")]
    pub gif_frame_delay_ms: u32,

    /// Pixel-change fraction (0.0-1.0) to trigger GIF capture.
    #[serde(default = "default_screenshot_gif_motion_thr")]
    pub gif_motion_threshold: f32,

    /// Maximum GIF file size in KB; discard if exceeded.
    #[serde(default = "default_screenshot_gif_max_size_kb")]
    pub gif_max_size_kb: u64,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: default_screenshot_interval(),
            image_size: default_screenshot_image_size(),
            quality: default_screenshot_quality(),
            session_only: default_screenshot_session_only(),
            embed_backend: default_screenshot_embed_backend(),
            fastembed_model: default_screenshot_fastembed_model(),
            ocr_enabled: default_screenshot_ocr_enabled(),
            ocr_engine: default_screenshot_ocr_engine(),
            use_gpu: default_screenshot_use_gpu(),
            gif_enabled: default_screenshot_gif_enabled(),
            gif_frame_count: default_screenshot_gif_frame_count(),
            gif_frame_delay_ms: default_screenshot_gif_frame_delay(),
            gif_motion_threshold: default_screenshot_gif_motion_thr(),
            gif_max_size_kb: default_screenshot_gif_max_size_kb(),
        }
    }
}

impl ScreenshotConfig {
    /// The epoch-aligned capture interval in seconds.
    ///
    /// `interval_secs` must be a multiple of `EMBEDDING_EPOCH_SECS` (5 s)
    /// in the range 5–60 s (1×–12× the EEG embedding epoch).  Legacy or
    /// out-of-range values are snapped to the nearest valid multiple.
    pub fn effective_interval_secs(&self) -> u64 {
        let epoch = EMBEDDING_EPOCH_SECS as u64;
        let mult = self.interval_multiplier() as u64;
        epoch * mult
    }

    /// Derive the interval multiplier (1–12) from `interval_secs`.
    ///
    /// Legacy configs may store non-multiples (e.g. 7); those are rounded to
    /// the nearest epoch boundary and clamped to 1–12.
    pub fn interval_multiplier(&self) -> u32 {
        let epoch = EMBEDDING_EPOCH_SECS as u32;
        let raw = self.interval_secs.max(1);
        let mult = (raw + epoch / 2) / epoch;
        mult.clamp(SCREENSHOT_INTERVAL_MIN_MULT, SCREENSHOT_INTERVAL_MAX_MULT)
    }

    pub fn model_id(&self) -> String {
        match self.embed_backend.as_str() {
            "fastembed" => match self.fastembed_model.as_str() {
                "clip-vit-b-32" => "Qdrant/clip-ViT-B-32-vision".into(),
                "nomic-embed-vision-v1.5" => "nomic-ai/nomic-embed-text-v1.5".into(),
                other => other.into(),
            },
            "mmproj" => "mmproj".into(),
            "llm-vlm" => "llm-vlm".into(),
            other => other.into(),
        }
    }

    pub fn recommended_image_size(&self) -> u32 {
        match self.embed_backend.as_str() {
            "fastembed" => match self.fastembed_model.as_str() {
                "nomic-embed-vision-v1.5" => 768,
                _ => 768,
            },
            "mmproj" | "llm-vlm" => 768,
            _ => 768,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let cfg = ScreenshotConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.interval_secs, 5);
        assert_eq!(cfg.image_size, 768);
        assert_eq!(cfg.quality, 60);
        assert!(cfg.session_only);
        assert_eq!(cfg.embed_backend, "fastembed");
        assert!(cfg.ocr_enabled);
        assert!(cfg.use_gpu);
        assert!(!cfg.gif_enabled);
    }

    #[test]
    fn effective_interval_at_default() {
        let cfg = ScreenshotConfig::default();
        assert_eq!(cfg.effective_interval_secs(), 5);
    }

    #[test]
    fn interval_multiplier_rounds_to_nearest_epoch() {
        let mut cfg = ScreenshotConfig::default();
        cfg.interval_secs = 7; // Between 5 and 10, rounds to 1× (7/5 = 1.4, rounds to 1)
        let mult = cfg.interval_multiplier();
        assert!(mult >= 1 && mult <= 12, "multiplier should be 1-12, got {mult}");
    }

    #[test]
    fn interval_multiplier_clamps_high() {
        let mut cfg = ScreenshotConfig::default();
        cfg.interval_secs = 999;
        assert_eq!(cfg.interval_multiplier(), SCREENSHOT_INTERVAL_MAX_MULT);
    }

    #[test]
    fn interval_multiplier_clamps_low() {
        let mut cfg = ScreenshotConfig::default();
        cfg.interval_secs = 0;
        assert_eq!(cfg.interval_multiplier(), SCREENSHOT_INTERVAL_MIN_MULT);
    }

    #[test]
    fn model_id_fastembed_nomic() {
        let cfg = ScreenshotConfig::default();
        assert_eq!(cfg.model_id(), "nomic-ai/nomic-embed-text-v1.5");
    }

    #[test]
    fn model_id_fastembed_clip() {
        let mut cfg = ScreenshotConfig::default();
        cfg.fastembed_model = "clip-vit-b-32".into();
        assert_eq!(cfg.model_id(), "Qdrant/clip-ViT-B-32-vision");
    }

    #[test]
    fn model_id_mmproj() {
        let mut cfg = ScreenshotConfig::default();
        cfg.embed_backend = "mmproj".into();
        assert_eq!(cfg.model_id(), "mmproj");
    }

    #[test]
    fn model_id_llm_vlm() {
        let mut cfg = ScreenshotConfig::default();
        cfg.embed_backend = "llm-vlm".into();
        assert_eq!(cfg.model_id(), "llm-vlm");
    }

    #[test]
    fn model_id_custom_passthrough() {
        let mut cfg = ScreenshotConfig::default();
        cfg.embed_backend = "custom-backend".into();
        assert_eq!(cfg.model_id(), "custom-backend");
    }

    #[test]
    fn model_id_custom_fastembed_model_passthrough() {
        let mut cfg = ScreenshotConfig::default();
        cfg.fastembed_model = "custom-model".into();
        assert_eq!(cfg.model_id(), "custom-model");
    }

    #[test]
    fn recommended_image_size_always_768() {
        let cfg = ScreenshotConfig::default();
        assert_eq!(cfg.recommended_image_size(), 768);

        let mut cfg2 = ScreenshotConfig::default();
        cfg2.embed_backend = "mmproj".into();
        assert_eq!(cfg2.recommended_image_size(), 768);
    }

    #[test]
    fn serde_roundtrip() {
        let cfg = ScreenshotConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: ScreenshotConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg2.interval_secs, cfg.interval_secs);
        assert_eq!(cfg2.quality, cfg.quality);
        assert_eq!(cfg2.embed_backend, cfg.embed_backend);
    }

    #[test]
    fn serde_defaults_on_empty_json() {
        let cfg: ScreenshotConfig = serde_json::from_str("{}").unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.interval_secs, 5);
        assert_eq!(cfg.quality, 60);
    }
}
