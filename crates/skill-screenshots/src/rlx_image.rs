// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! RLX-backed image embedder (`rlx-embed`).
//!
//! Loads `nomic-ai/nomic-embed-vision-v1.5` (the one image-embedding family
//! covered by `rlx-embed`) from HuggingFace and forwards a single image
//! through the compiled RLX graph. Returns a 768-dim L2-normalized vector.
//!
//! The whole module is gated on `text-embeddings-rlx` so it disappears
//! cleanly when image embeddings are not compiled in.

#![cfg(feature = "text-embeddings-rlx")]

use anyhow::{anyhow, Result};
use image::{imageops::FilterType, DynamicImage, GenericImageView, ImageReader};
use rlx_models::RlxVisionModel;
use std::io::Cursor;
use std::sync::Mutex;

/// Encoder handle — owns the compiled `RlxVisionModel` + image-size knobs.
pub struct RlxImageEmbedder {
    inner: Mutex<RlxVisionModel>,
    img_size: usize,
}

impl RlxImageEmbedder {
    /// Resolve nomic-embed-vision-v1.5 from HF cache and compile its graph.
    pub fn from_repo(device: &str) -> Result<Self> {
        let device = parse_device(device);
        let repo = hf_hub::api::sync::ApiBuilder::new()
            .with_progress(true)
            .build()?
            .model("nomic-ai/nomic-embed-vision-v1.5".to_string());
        let config_path = repo.get("config.json")?;
        let weights_path = repo.get("model.safetensors")?;
        let weights_path = weights_path
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 weights path"))?
            .to_string();
        let model = RlxVisionModel::load_sized_on(&config_path, &weights_path, 1, device)?;
        let img_size = model.img_size();
        Ok(Self {
            inner: Mutex::new(model),
            img_size,
        })
    }

    /// Embed a single image (PNG / JPEG / WebP bytes). Returns a 768-dim
    /// L2-normalized vector or `None` on decode/forward failure.
    pub fn embed_bytes(&self, bytes: &[u8]) -> Option<Vec<f32>> {
        let img = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?;
        self.embed_image(&img)
    }

    /// Embed an already-decoded `DynamicImage`.
    pub fn embed_image(&self, img: &DynamicImage) -> Option<Vec<f32>> {
        let target = self.img_size as u32;
        let resized = resize_center_crop(img, target);
        let pixels = to_nchw_f32(&resized, self.img_size);
        let mut guard = self.inner.lock().ok()?;
        let mut out = guard.forward(&pixels, 1);
        if out.is_empty() {
            return None;
        }
        l2_normalize(&mut out);
        Some(out)
    }

    pub fn dim(&self) -> usize {
        // NomicEmbedVisionV15: hidden=768.
        self.inner.lock().map(|m| m.hidden_size()).unwrap_or(768)
    }
}

fn parse_device(s: &str) -> rlx::Device {
    use rlx::Device;
    let want = match s.trim().to_ascii_lowercase().as_str() {
        "metal" => Device::Metal,
        "mlx" => Device::Mlx,
        "cuda" => Device::Cuda,
        "rocm" => Device::Rocm,
        "gpu" | "wgpu" => Device::Gpu,
        _ => Device::Cpu,
    };
    // Fall back to CPU when the requested GPU backend isn't compiled in / present,
    // rather than handing rlx an unavailable device (which panics at session setup).
    if matches!(want, Device::Cpu) || rlx::runtime::device_ext::is_available(want) {
        want
    } else {
        Device::Cpu
    }
}

/// Center-crop after aspect-preserving resize to the model input size.
fn resize_center_crop(img: &DynamicImage, target: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    let scale = (target as f64 / w.min(h) as f64).max(1.0);
    let nw = (w as f64 * scale).round() as u32;
    let nh = (h as f64 * scale).round() as u32;
    let resized = img.resize(nw, nh, FilterType::Triangle);
    let cx = (nw.saturating_sub(target)) / 2;
    let cy = (nh.saturating_sub(target)) / 2;
    resized.crop_imm(cx, cy, target, target)
}

/// Convert RGB image → NCHW f32 `[1, 3, H, W]`. Normalises with CLIP-style
/// per-channel mean/std (NomicEmbedVision uses the same constants per its
/// preprocessor_config.json).
fn to_nchw_f32(img: &DynamicImage, target: usize) -> Vec<f32> {
    #[allow(clippy::excessive_precision)]
    const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
    #[allow(clippy::excessive_precision)]
    const STD: [f32; 3] = [0.26862954, 0.26130258, 0.27577711];
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    debug_assert_eq!(w as usize, target);
    debug_assert_eq!(h as usize, target);
    let mut out = vec![0f32; 3 * target * target];
    for y in 0..target {
        for x in 0..target {
            let px = rgb.get_pixel(x as u32, y as u32);
            for c in 0..3 {
                let v = px.0[c] as f32 / 255.0;
                out[c * target * target + y * target + x] = (v - MEAN[c]) / STD[c];
            }
        }
    }
    out
}

fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}
