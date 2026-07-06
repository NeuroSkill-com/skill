// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Screenshot capture + vision-encoder embedding system.
//!
//! Every ~5 seconds (aligned with EEG embedding epoch cadence), captures the
//! active application window, encodes it through a vision embedding model, and
//! stores the raw embedding alongside metadata in SQLite + HNSW.  The shared
//! `YYYYMMDDHHmmss` timestamp is the cross-modal join key to EEG embeddings.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fast_hnsw::{distance::Cosine, labeled::LabeledIndex, Builder};
use image::{DynamicImage, GenericImageView, ImageReader};
use serde::Serialize;

use crate::config::ScreenshotConfig;
use crate::platform::capture_active_window;
use skill_constants::{
    HNSW_EF_CONSTRUCTION, HNSW_M, SCREENSHOTS_DIR, SCREENSHOTS_HNSW, SCREENSHOTS_OCR_HNSW, SCREENSHOT_HNSW_SAVE_EVERY,
};
use skill_data::screenshot_store::{ReembedEstimate, ReembedResult, ScreenshotResult, ScreenshotRow, ScreenshotStore};

// ── Image resize + pad ────────────────────────────────────────────────────────

/// Resize with aspect-ratio-preserving fit, then center-pad to
/// `target × target` with black pixels.  Returns the padded `DynamicImage`
/// directly — callers that need encoded bytes (PNG for the vision encoder)
/// should use [`encode_png`] separately.
pub(crate) fn resize_fit_pad_image(raw_bytes: &[u8], target: u32) -> Option<DynamicImage> {
    let img = ImageReader::new(Cursor::new(raw_bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(resize_fit_pad_dyn(&img, target))
}

/// Resize + pad from a pre-decoded `CapturedImage`.  Uses the decoded image
/// if available, otherwise decodes from `raw_bytes`.  Avoids the
/// encode→decode round-trip on Linux/Windows where xcap gives us RGBA directly.
fn resize_fit_pad_captured(captured: &crate::platform::CapturedImage, target: u32) -> Option<DynamicImage> {
    if let Some(ref img) = captured.decoded {
        Some(resize_fit_pad_dyn(img, target))
    } else {
        resize_fit_pad_image(&captured.raw_bytes, target)
    }
}

/// Core resize + pad operating on an already-decoded `DynamicImage`.
fn resize_fit_pad_dyn(img: &DynamicImage, target: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    let scale = (target as f64 / w as f64).min(target as f64 / h as f64);
    let nw = (w as f64 * scale).round() as u32;
    let nh = (h as f64 * scale).round() as u32;

    // Triangle (bilinear) is ~10× faster than Lanczos3 on large images
    // and visually indistinguishable at the target sizes used here.
    let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Triangle);

    // Center-pad to target × target
    let mut canvas = DynamicImage::new_rgb8(target, target);
    let offset_x = (target - nw) / 2;
    let offset_y = (target - nh) / 2;
    image::imageops::overlay(&mut canvas, &resized, offset_x as i64, offset_y as i64);
    canvas
}

/// Legacy wrapper: resize + pad → PNG bytes.
#[allow(dead_code)]
fn resize_fit_pad(raw_bytes: &[u8], target: u32) -> Option<(Vec<u8>, u32, u32)> {
    let canvas = resize_fit_pad_image(raw_bytes, target)?;
    let png = encode_png(&canvas)?;
    Some((png, target, target))
}

/// Encode a `DynamicImage` as PNG bytes.
#[allow(dead_code)]
fn encode_png(img: &DynamicImage) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png).ok()?;
    Some(buf)
}

/// Encode an already-decoded image as WebP with the given quality.
pub(crate) fn encode_webp(img: &DynamicImage, _quality: u8, out_path: &Path) -> Option<u64> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::WebP)
        .ok()?;
    std::fs::write(out_path, &buf).ok()?;
    Some(buf.len() as u64)
}

// ── Timestamp helpers ─────────────────────────────────────────────────────────

/// Generate `YYYYMMDDHHmmss` timestamp (UTC) from current time.
///
/// All timestamps in the screenshot system are **UTC** — matching the EEG
/// embedding pipeline's `YYYYMMDDHHmmss` convention.  `chrono::DateTime::from_timestamp`
/// Returns `("YYYYMMDDHHmmss", unix_secs)` — always UTC.
///
/// Delegates to [`skill_data::util::yyyymmddhhmmss_utc_str`].
#[inline]
fn yyyymmddhhmmss_utc() -> (String, u64) {
    skill_data::util::yyyymmddhhmmss_utc_str()
}

// ── HNSW helpers ──────────────────────────────────────────────────────────────

fn fresh_hnsw() -> LabeledIndex<Cosine, i64> {
    Builder::new()
        .m(HNSW_M)
        .ef_construction(HNSW_EF_CONSTRUCTION)
        .build_labeled(Cosine)
}

/// Reset `hnsw` to a fresh index when its stored vector dimension no longer
/// matches `emb_len` (e.g. the embedding model changed). No-op when the index
/// is empty or already matches. `label` is used only for the log line.
fn reset_hnsw_if_dim_mismatch(hnsw: &mut LabeledIndex<Cosine, i64>, emb_len: usize, label: &str) {
    if hnsw.is_empty() {
        return;
    }
    if let Some(dim) = hnsw.inner.dim() {
        if dim != emb_len {
            eprintln!(
                "[screenshot] {label} HNSW dimension mismatch (index={dim}, new={emb_len}); \
                 resetting index — run re-embed to backfill"
            );
            *hnsw = fresh_hnsw();
        }
    }
}

/// Generic load-or-rebuild for any HNSW index backed by an embedding-fetch closure.
fn load_or_rebuild_hnsw_generic(
    skill_dir: &Path,
    hnsw_file: &str,
    label: &str,
    fetch_rows: impl FnOnce() -> Vec<(i64, Vec<f32>)>,
) -> LabeledIndex<Cosine, i64> {
    let hnsw_path = skill_dir.join(hnsw_file);
    if hnsw_path.exists() {
        match LabeledIndex::<Cosine, i64>::load(&hnsw_path, Cosine) {
            Ok(idx) => {
                eprintln!("[screenshot] loaded {label} HNSW from {}", hnsw_path.display());
                return idx;
            }
            Err(e) => {
                eprintln!("[screenshot] {label} HNSW load error: {e} — rebuilding");
            }
        }
    }
    let mut idx = fresh_hnsw();
    let rows = fetch_rows();
    eprintln!("[screenshot] rebuilding {label} HNSW from {} embeddings", rows.len());
    for (ts, emb) in rows {
        idx.insert(emb, ts);
    }
    if let Err(e) = idx.save(&hnsw_path) {
        eprintln!("[screenshot] {label} HNSW save error: {e}");
    }
    idx
}

/// Save an HNSW index to `skill_dir/hnsw_file`.
fn save_hnsw_to(idx: &LabeledIndex<Cosine, i64>, skill_dir: &Path, hnsw_file: &str, label: &str) {
    let path = skill_dir.join(hnsw_file);
    if let Err(e) = idx.save(&path) {
        eprintln!("[screenshot] {label} HNSW save error: {e}");
    }
}

fn load_or_rebuild_hnsw(skill_dir: &Path, store: &ScreenshotStore) -> LabeledIndex<Cosine, i64> {
    load_or_rebuild_hnsw_generic(skill_dir, SCREENSHOTS_HNSW, "vision", || store.all_embeddings())
}

fn save_hnsw(idx: &LabeledIndex<Cosine, i64>, skill_dir: &Path) {
    save_hnsw_to(idx, skill_dir, SCREENSHOTS_HNSW, "vision");
}

// ── Screenshot image embedding ──────────────────────────────────────────────
//
// Vision embeddings run on the RLX runtime. In the default ("rlx") backend the
// screenshot's vision vector IS the embedding of its OCR text (nomic-embed-text
// shares an aligned space with nomic-embed-vision, so query-by-image still
// works — see `rlx_image::RlxImageEmbedder`). The `mmproj` / `llm-vlm` backends
// embed the pixels directly through the local LLM. Both go through the shared
// embedders in `context`/`rlx_image` — no fastembed/ONNX path remains.

/// Encode a `DynamicImage` as JPEG bytes.  JPEG encoding is ~10× faster than
/// PNG and is used for paths that need encoded bytes (LLM vision, OCR) where
/// lossless fidelity is unnecessary.
fn encode_jpeg(img: &DynamicImage, quality: u8) -> Option<Vec<u8>> {
    let rgb = img.to_rgb8();
    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    rgb.write_with_encoder(encoder).ok()?;
    Some(buf)
}

// ── OCR engine ────────────────────────────────────────────────────────────────

/// Download an OCR model file if it doesn't exist yet. Public alias for Tauri commands.
pub fn download_ocr_model_pub(url: &str, dest: &Path) -> bool {
    download_ocr_model(url, dest)
}

/// Download an OCR model file if it doesn't exist yet.
fn download_ocr_model(url: &str, dest: &Path) -> bool {
    if dest.exists() {
        return true;
    }
    eprintln!("[screenshot] downloading OCR model: {url}");
    match ureq::get(url).call() {
        Ok(resp) => {
            let mut body = Vec::new();
            if resp.into_body().into_reader().read_to_end(&mut body).is_ok() && !body.is_empty() {
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::write(dest, &body).is_ok() {
                    eprintln!("[screenshot] OCR model saved: {}", dest.display());
                    return true;
                }
            }
            eprintln!("[screenshot] OCR model download failed (empty body)");
            false
        }
        Err(e) => {
            eprintln!("[screenshot] OCR model download error: {e}");
            false
        }
    }
}

use rlx_models::ocr as rlx_ocr;

/// Load the OCR engine via `rlx-ocr` (rlx-models' drop-in replacement
/// for the legacy `ocrs` crate). Downloads model files on first use.
// OCR weights are BUNDLED as safetensors (~12 MB total), converted offline from
// the ocrs `.rten` checkpoints (text-detection-ssfbcj81 / text-rec-checkpoint-
// s52qdbqt). Drops the runtime download + the `convert-rten`/rten-inference
// stack; OCR works offline on first launch. To refresh: re-run
// `rlx_ocr::weights::export_rten_to_safetensors` on new `.rten` files and replace
// the asset.
static OCR_DETECTION_SAFETENSORS: &[u8] = include_bytes!("../assets/ocr-detection.safetensors");
static OCR_RECOGNITION_SAFETENSORS: &[u8] = include_bytes!("../assets/ocr-recognition.safetensors");

fn load_ocr_engine(skill_dir: &Path) -> Option<rlx_ocr::OcrEngine> {
    // The engine loads weights by path, so unpack the bundled bytes once.
    let ocr_dir = skill_dir.join("ocr_models");
    let det_st = ocr_dir.join("ocr-detection.safetensors");
    let rec_st = ocr_dir.join("ocr-recognition.safetensors");
    if let Err(e) = materialize_bundled(&det_st, OCR_DETECTION_SAFETENSORS)
        .and_then(|_| materialize_bundled(&rec_st, OCR_RECOGNITION_SAFETENSORS))
    {
        eprintln!("[screenshot-embed] OCR weights unpack failed: {e:#}");
        return None;
    }

    rlx_ocr::OcrEngine::new(rlx_ocr::OcrEngineParams {
        detection_model: Some(det_st),
        recognition_model: Some(rec_st),
        device: ocr_device(),
        ..Default::default()
    })
    .map_err(|e| eprintln!("[screenshot-embed] OCR engine init failed: {e:#}"))
    .ok()
}

/// Write bundled weight `bytes` to `path` unless an identically-sized file is
/// already there (so the ~12 MB write happens once).
fn materialize_bundled(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if std::fs::metadata(path)
        .map(|m| m.len() == bytes.len() as u64)
        .unwrap_or(false)
    {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

/// Pick the OCR runtime device. An `OCR_DEVICE` env value (`metal`/`mlx`/`cuda`/
/// `rocm`/`gpu`/`cpu`) wins if its backend is available; otherwise auto-detect
/// the best compiled GPU backend, else CPU.
///
/// Detection is at *runtime* via `validate_device` (Ok only when the backend was
/// compiled in) rather than `cfg!(feature = …)`: on macOS the daemon enables
/// Metal through `[target.macos.dependencies]` with no skill feature flag, so a
/// cfg check would miss it and leave OCR on CPU. All GPU backends reached parity
/// + per-image multi-shape in rlx 0.2.x, so OCR runs wherever the rest of the
/// screenshots pipeline does.
fn ocr_device() -> rlx_runtime::Device {
    use rlx_runtime::Device;
    let ok = |d: Device| rlx_ocr::validate_device(d).is_ok();
    if let Ok(s) = std::env::var("OCR_DEVICE") {
        let d = match s.trim().to_ascii_lowercase().as_str() {
            "metal" => Device::Metal,
            "mlx" => Device::Mlx,
            "cuda" => Device::Cuda,
            "rocm" => Device::Rocm,
            "gpu" | "wgpu" => Device::Gpu,
            _ => Device::Cpu,
        };
        return if ok(d) { d } else { Device::Cpu };
    }
    [Device::Metal, Device::Mlx, Device::Cuda, Device::Rocm, Device::Gpu]
        .into_iter()
        .find(|&d| ok(d))
        .unwrap_or(Device::Cpu)
}

/// Run OCR on an already-resized PNG image. Returns the extracted text.
///
/// On macOS: tries `skill-vision` (Apple Vision, GPU / Neural Engine, <50 ms).
/// Falls back to `rlx-ocr` everywhere else.
fn run_ocr(engine: &rlx_ocr::OcrEngine, png_bytes: &[u8]) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(text) = skill_vision::recognize_text_from_png(png_bytes) {
            return Some(text);
        }
    }

    run_ocr_rten(engine, png_bytes)
}

/// OCR via `rlx-ocr` (rten-inference backend, CPU). Used on Linux/Windows
/// and as a fallback on macOS if Vision framework is unavailable.
fn run_ocr_rten(engine: &rlx_ocr::OcrEngine, png_bytes: &[u8]) -> Option<String> {
    let img = image::load_from_memory(png_bytes).ok()?.into_rgb8();
    run_ocr_rten_rgb(engine, &img)
}

/// OCR from an already-decoded `DynamicImage` — avoids the encode→decode
/// round-trip when the caller already has pixel data.
fn run_ocr_from_image(engine: &rlx_ocr::OcrEngine, img: &DynamicImage) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(jpg) = encode_jpeg(img, 85) {
            if let Some(text) = skill_vision::recognize_text_from_png(&jpg) {
                return Some(text);
            }
        }
    }

    let rgb = img.to_rgb8();
    run_ocr_rten_rgb(engine, &rgb)
}

/// Core OCR on an already-decoded RGB8 image buffer.
fn run_ocr_rten_rgb(engine: &rlx_ocr::OcrEngine, img: &image::RgbImage) -> Option<String> {
    let (w, h) = img.dimensions();
    let source = rlx_ocr::ImageSource::from_bytes(img.as_raw(), (w, h)).ok()?;
    let input = engine.prepare_input(source).ok()?;
    let text = engine.get_text(&input).ok()?;
    let text = text.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Embed OCR text using an externally-provided embed function.
///
/// The caller passes `embed_fn` — typically bound to the shared app-wide
/// text embedder (`EmbedderState`).  This avoids loading a second copy of
/// the ONNX model (~130 MB) inside the screenshots crate.
fn embed_ocr_text(text: &str, embed_fn: &dyn Fn(&str) -> Option<Vec<f32>>) -> Option<Vec<f32>> {
    embed_fn(text)
}

// ── OCR HNSW helpers ──────────────────────────────────────────────────────────

fn load_or_rebuild_ocr_hnsw(skill_dir: &Path, store: &ScreenshotStore) -> LabeledIndex<Cosine, i64> {
    load_or_rebuild_hnsw_generic(skill_dir, SCREENSHOTS_OCR_HNSW, "OCR", || store.all_ocr_embeddings())
}

fn save_ocr_hnsw(idx: &LabeledIndex<Cosine, i64>, skill_dir: &Path) {
    save_hnsw_to(idx, skill_dir, SCREENSHOTS_OCR_HNSW, "OCR");
}

// ── Screenshot event payload ──────────────────────────────────────────────────

#[derive(Clone, Serialize)]
struct ScreenshotCapturedEvent {
    ts: String,
    filename: String,
}

// ── Pipeline metrics (lock-free atomics) ──────────────────────────────────────

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Shared metrics updated by both capture and embed threads.
/// All times are in microseconds.  All counters are monotonic.
pub struct ScreenshotMetrics {
    // ── Capture thread ──
    pub captures: AtomicU64,
    pub capture_errors: AtomicU64,
    pub drops: AtomicU64,            // try_send failures
    pub capture_us: AtomicU64,       // last window-capture time
    pub ocr_us: AtomicU64,           // last OCR time
    pub resize_us: AtomicU64,        // last resize+pad time
    pub save_us: AtomicU64,          // last WebP save + SQLite insert
    pub capture_total_us: AtomicU64, // last full capture-thread iteration

    // ── Embed thread ──
    pub embeds: AtomicU64,
    pub embed_errors: AtomicU64,
    pub vision_embed_us: AtomicU64, // last vision embedding time
    pub text_embed_us: AtomicU64,   // last OCR text embedding time
    pub embed_total_us: AtomicU64,  // last full embed iteration
    pub queue_depth: AtomicI64,     // current channel occupancy (inc on send, dec on recv)

    // ── Throughput (rolling) ──
    pub last_capture_unix: AtomicU64, // unix-ms of last capture
    pub last_embed_unix: AtomicU64,   // unix-ms of last embed completion

    // ── Adaptive backoff ──
    pub backoff_multiplier: AtomicU64, // current interval multiplier (1 = no backoff)
}

impl Default for ScreenshotMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenshotMetrics {
    pub fn new() -> Self {
        Self {
            captures: AtomicU64::new(0),
            capture_errors: AtomicU64::new(0),
            drops: AtomicU64::new(0),
            capture_us: AtomicU64::new(0),
            ocr_us: AtomicU64::new(0),
            resize_us: AtomicU64::new(0),
            save_us: AtomicU64::new(0),
            capture_total_us: AtomicU64::new(0),
            embeds: AtomicU64::new(0),
            embed_errors: AtomicU64::new(0),
            vision_embed_us: AtomicU64::new(0),
            text_embed_us: AtomicU64::new(0),
            embed_total_us: AtomicU64::new(0),
            queue_depth: AtomicI64::new(0),
            last_capture_unix: AtomicU64::new(0),
            last_embed_unix: AtomicU64::new(0),
            backoff_multiplier: AtomicU64::new(1),
        }
    }

    /// Snapshot all metrics into a serializable struct.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            captures: self.captures.load(Ordering::Relaxed),
            capture_errors: self.capture_errors.load(Ordering::Relaxed),
            drops: self.drops.load(Ordering::Relaxed),
            capture_us: self.capture_us.load(Ordering::Relaxed),
            ocr_us: self.ocr_us.load(Ordering::Relaxed),
            resize_us: self.resize_us.load(Ordering::Relaxed),
            save_us: self.save_us.load(Ordering::Relaxed),
            capture_total_us: self.capture_total_us.load(Ordering::Relaxed),
            embeds: self.embeds.load(Ordering::Relaxed),
            embed_errors: self.embed_errors.load(Ordering::Relaxed),
            vision_embed_us: self.vision_embed_us.load(Ordering::Relaxed),
            text_embed_us: self.text_embed_us.load(Ordering::Relaxed),
            embed_total_us: self.embed_total_us.load(Ordering::Relaxed),
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            last_capture_unix: self.last_capture_unix.load(Ordering::Relaxed),
            last_embed_unix: self.last_embed_unix.load(Ordering::Relaxed),
            backoff_multiplier: self.backoff_multiplier.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MetricsSnapshot {
    pub captures: u64,
    pub capture_errors: u64,
    pub drops: u64,
    pub capture_us: u64,
    pub ocr_us: u64,
    pub resize_us: u64,
    pub save_us: u64,
    pub capture_total_us: u64,
    pub embeds: u64,
    pub embed_errors: u64,
    pub vision_embed_us: u64,
    pub text_embed_us: u64,
    pub embed_total_us: u64,
    pub queue_depth: i64,
    pub last_capture_unix: u64,
    pub last_embed_unix: u64,
    pub backoff_multiplier: u64,
}

/// Convenience: current time in milliseconds since epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── Embed job sent from capture thread → embed thread ─────────────────────────

struct EmbedJob {
    row_id: i64,
    ts_i64: i64,
    /// Pre-decoded resized image.  Passed directly to fastembed's
    /// `embed_images()` to avoid the CPU-heavy PNG encode→decode
    /// round-trip.  For LLM/OCR paths that need encoded bytes, JPEG
    /// is produced lazily in the embed thread (~10× faster than PNG).
    resized_img: Option<DynamicImage>,
    /// Whether OCR should run on the resized image in the embed thread.
    run_ocr: bool,
    config: ScreenshotConfig,
    /// When set, the screenshot is identical to a previous one — copy
    /// embedding + OCR from this row instead of running ML inference.
    copy_from_row: Option<i64>,
}

// ── Background worker ─────────────────────────────────────────────────────────

/// Run the screenshot capture worker in a dedicated thread.
/// Called from `lib.rs :: setup_app`.
///
/// Architecture: two threads connected by a bounded channel.
///
/// **Capture thread** (this function) — fast, never blocks on ML:
///   capture → OCR → resize → save WebP → insert SQLite → notify → send job
///
/// **Embed thread** (spawned below) — slow, GPU-bound:
///   receive job → vision embed → HNSW insert → text embed → HNSW insert → UPDATE SQLite
///
/// This ensures the capture cadence is never delayed by slow embedding work
/// and screenshots are always persisted immediately.
#[allow(clippy::needless_pass_by_value)] // thread entry point — takes ownership of Arcs and PathBuf
pub fn run_screenshot_worker(
    ctx: Arc<dyn crate::context::ScreenshotContext>,
    skill_dir: PathBuf,
    shared_store: Option<Arc<ScreenshotStore>>,
    metrics: Arc<ScreenshotMetrics>,
) {
    let Some(store) = shared_store.or_else(|| ScreenshotStore::open(&skill_dir).map(Arc::new)) else {
        eprintln!("[screenshot] failed to open store — worker exiting");
        return;
    };

    // Read initial config
    let config = ctx.config();

    // ── Spawn the embed thread ──
    // Bounded channel (capacity 4) provides backpressure: if the embed
    // thread falls behind, the capture thread blocks on send rather than
    // accumulating unbounded memory.
    let (embed_tx, embed_rx) = crossbeam_channel::bounded::<EmbedJob>(4);
    let embed_store = Arc::clone(&store);
    let embed_dir = skill_dir.clone();
    let embed_ctx = Arc::clone(&ctx);
    let embed_config = config.clone();
    let embed_metrics = Arc::clone(&metrics);

    std::thread::Builder::new()
        .name("screenshot-embed".into())
        .spawn(move || {
            run_embed_thread(embed_ctx, embed_dir, embed_store, embed_rx, embed_config, embed_metrics);
        })
        .unwrap_or_else(|e| {
            eprintln!("[screenshot] failed to spawn embed thread: {e}");
            std::process::abort();
        });

    // ── Capture loop ──
    let screenshots_dir = skill_dir.join(SCREENSHOTS_DIR);
    let _ = std::fs::create_dir_all(&screenshots_dir);

    // Adaptive backoff: when drops occur, double the effective interval
    // (up to 4× the configured value).  When the queue drains, recover
    // back to the configured interval over 3 successful sends.
    let mut backoff_multiplier: u64 = 1;
    let mut consecutive_ok: u32 = 0;
    const MAX_BACKOFF: u64 = 4;
    const BACKOFF_STEPS: [u64; 4] = [1, 2, 3, 4];

    // Duplicate detection: hash the resized PNG and compare with the
    // previous capture.  When identical, the embed thread can copy
    // OCR + embeddings from the previous row instead of re-running ML.
    let mut prev_screenshot_hash: u64 = 0;
    let mut prev_row_id: Option<i64> = None;

    // Previous resized PNG — kept for motion detection between captures.
    // prev_resized_png was used for GIF motion detection — removed since
    // GIF capture is now script-only.  Kept as a comment for context.

    loop {
        // Re-read config + session state
        let config = ctx.config();
        let session_active = ctx.is_session_active();

        // Gate checks BEFORE sleep — don't waste time sleeping when
        // capture is disabled or gated by session.
        if !config.enabled || (config.session_only && !session_active) {
            // Sleep a short interval and re-check (responsive to config changes)
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }

        let base_secs = config.effective_interval_secs();
        let effective_secs = base_secs * backoff_multiplier;
        std::thread::sleep(Duration::from_secs(effective_secs));

        let iter_start = Instant::now();

        // ── Capture active window ──
        let t0 = Instant::now();
        let Some(captured) = capture_active_window() else {
            metrics.capture_errors.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        metrics
            .capture_us
            .store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        // ── Resize + pad (no PNG encoding — just pixel ops) ──
        let t0 = Instant::now();
        let Some(resized_img) = resize_fit_pad_captured(&captured, config.image_size) else {
            continue;
        };
        let (w, h) = (config.image_size, config.image_size);
        metrics
            .resize_us
            .store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        drop(captured); // free full-res capture immediately

        // ── Duplicate detection via fast hash on raw pixels ──
        let mut hasher = DefaultHasher::new();
        resized_img.as_bytes().hash(&mut hasher);
        let current_hash = hasher.finish();
        let copy_from = if current_hash == prev_screenshot_hash && prev_row_id.is_some() {
            eprintln!("[screenshot] duplicate detected — will copy OCR + embeddings from previous row");
            prev_row_id
        } else {
            None
        };

        // GIF burst capture is intentionally disabled in the periodic capture
        // loop — only scripts may produce animated GIFs.  The gif_encode module
        // and config fields are preserved for the script-level API.

        // ── Save to disk as WebP + SQLite + context ──
        let t0 = Instant::now();
        let (ts_str, unix_ts) = yyyymmddhhmmss_utc();
        let date_str = &ts_str[..8];
        let date_dir = screenshots_dir.join(date_str);
        let _ = std::fs::create_dir_all(&date_dir);
        let webp_name = format!("{date_str}/{ts_str}.webp");
        let webp_path = screenshots_dir.join(&webp_name);
        let Some(file_size) = encode_webp(&resized_img, config.quality, &webp_path) else {
            metrics.capture_errors.fetch_add(1, Ordering::Relaxed);
            continue;
        };

        let aw = ctx.active_window();
        let (app_name, window_title) = (aw.app_name, aw.window_title);

        let ts_i64: i64 = ts_str.parse().unwrap_or(0);

        let row_id = store.insert(&ScreenshotRow {
            timestamp: ts_i64,
            unix_ts,
            filename: webp_name.clone(),
            width: w,
            height: h,
            file_size,
            hnsw_id: None,
            embedding: None,
            embedding_dim: 0,
            model_backend: String::new(),
            model_id: String::new(),
            image_size: config.image_size,
            quality: config.quality,
            app_name,
            window_title,
            ocr_text: String::new(), // backfilled by embed thread after OCR
            ocr_embedding: None,
            ocr_embedding_dim: 0,
            ocr_hnsw_id: None,
            source: "auto".into(),
            chat_session_id: None,
            caption: String::new(),
        });

        metrics
            .save_us
            .store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        // ── Notify frontend ──
        ctx.emit_event(
            "screenshot-captured",
            serde_json::to_value(&ScreenshotCapturedEvent {
                ts: ts_str,
                filename: webp_name,
            })
            .unwrap_or_default(),
        );

        // ── Prepare image for the embed thread ──
        // Pass the decoded DynamicImage directly — the embed thread uses
        // fastembed's `embed_images()` which accepts DynamicImage, avoiding
        // the CPU-heavy PNG encode→decode round-trip.  For LLM/OCR paths
        // that need encoded bytes, JPEG is produced lazily in the embed
        // thread (~10× faster than PNG).
        let resized_for_embed = if copy_from.is_some() {
            // Duplicate — embed thread will copy from previous row, no image needed.
            drop(resized_img);
            None
        } else {
            Some(resized_img)
        };

        // ── Send to embed thread (non-blocking if capacity available) ──
        if let Some(row_id) = row_id {
            match embed_tx.try_send(EmbedJob {
                row_id,
                ts_i64,
                resized_img: resized_for_embed,
                run_ocr: config.ocr_enabled,
                config: config.clone(),
                copy_from_row: copy_from,
            }) {
                Ok(()) => {
                    metrics.queue_depth.fetch_add(1, Ordering::Relaxed);
                    // Track for duplicate detection on next iteration
                    prev_screenshot_hash = current_hash;
                    prev_row_id = Some(row_id);
                    // Successful send — recover toward base interval
                    consecutive_ok += 1;
                    if consecutive_ok >= 3 && backoff_multiplier > 1 {
                        // Step down: 4→3→2→1
                        let cur_idx = BACKOFF_STEPS
                            .iter()
                            .position(|&s| s == backoff_multiplier)
                            .unwrap_or(BACKOFF_STEPS.len() - 1);
                        backoff_multiplier = if cur_idx > 0 { BACKOFF_STEPS[cur_idx - 1] } else { 1 };
                        consecutive_ok = 0;
                        eprintln!("[screenshot] backoff recovered → {}× base interval", backoff_multiplier);
                    }
                }
                Err(_) => {
                    metrics.drops.fetch_add(1, Ordering::Relaxed);
                    // Drop — embed thread can't keep up.  Step up the interval
                    // to release pressure (1→2→3→4 × base).
                    consecutive_ok = 0;
                    if backoff_multiplier < MAX_BACKOFF {
                        let cur_idx = BACKOFF_STEPS.iter().position(|&s| s == backoff_multiplier).unwrap_or(0);
                        backoff_multiplier = BACKOFF_STEPS[(cur_idx + 1).min(BACKOFF_STEPS.len() - 1)];
                        eprintln!(
                            "[screenshot] embed queue full — backing off to {}× base interval ({}s)",
                            backoff_multiplier,
                            config.effective_interval_secs() * backoff_multiplier
                        );
                    }
                }
            }
        }

        metrics.captures.fetch_add(1, Ordering::Relaxed);
        metrics
            .capture_total_us
            .store(iter_start.elapsed().as_micros() as u64, Ordering::Relaxed);
        metrics.last_capture_unix.store(now_ms(), Ordering::Relaxed);
        metrics.backoff_multiplier.store(backoff_multiplier, Ordering::Relaxed);
    }
}

/// Device string for the RLX image embedder, derived from the screenshot
/// config. Apple Silicon: Metal is the fastest backend for nomic-vision —
/// benchmarked ~110 img/s vs MLX ~49 and CPU ~25 (see bench_image_embed).
#[cfg(feature = "text-embeddings-rlx")]
fn rlx_image_device(cfg: &ScreenshotConfig) -> String {
    if cfg.use_gpu && cfg!(target_os = "macos") {
        "metal".into()
    } else {
        "cpu".into()
    }
}

/// Lazily load (once) and return the RLX vision embedder
/// (nomic-embed-vision-v1.5). Its output shares an aligned 768-d space with
/// nomic-embed-text, so text queries (`search_query:`) match these vectors
/// cross-modally. Returns `None` if the model can't be loaded.
#[cfg(feature = "text-embeddings-rlx")]
fn ensure_image_embedder<'a>(
    cache: &'a mut Option<crate::rlx_image::RlxImageEmbedder>,
    device: &str,
) -> Option<&'a crate::rlx_image::RlxImageEmbedder> {
    if cache.is_none() {
        match crate::rlx_image::RlxImageEmbedder::from_repo(device) {
            Ok(e) => {
                eprintln!("[screenshot-embed] rlx image embedder (nomic-embed-vision-v1.5) loaded on {device}");
                *cache = Some(e);
            }
            Err(e) => {
                eprintln!("[screenshot-embed] rlx image embedder load failed: {e:#}");
                return None;
            }
        }
    }
    cache.as_ref()
}

/// Embedding thread — processes jobs from the capture thread.
/// Runs vision embedding + OCR text embedding on GPU (when available)
/// and backfills results into SQLite + HNSW.
#[allow(clippy::needless_pass_by_value)] // thread entry point — takes ownership of Arcs, PathBuf, config
fn run_embed_thread(
    ctx: Arc<dyn crate::context::ScreenshotContext>,
    skill_dir: PathBuf,
    store: Arc<ScreenshotStore>,
    rx: crossbeam_channel::Receiver<EmbedJob>,
    initial_config: ScreenshotConfig,
    metrics: Arc<ScreenshotMetrics>,
) {
    // Load HNSW indexes
    let mut hnsw = load_or_rebuild_hnsw(&skill_dir, &store);
    let mut ocr_hnsw = load_or_rebuild_ocr_hnsw(&skill_dir, &store);

    let mut last_backend = initial_config.embed_backend.clone();
    let mut last_model = initial_config.fastembed_model.clone();

    // Load OCR engine (downloads models on first use)
    let ocr_engine = if initial_config.ocr_enabled {
        let engine = load_ocr_engine(&skill_dir);
        if engine.is_some() {
            eprintln!("[screenshot-embed] OCR engine ({}) loaded", initial_config.ocr_engine);
        } else {
            eprintln!("[screenshot-embed] OCR engine not available");
        }
        engine
    } else {
        eprintln!("[screenshot-embed] OCR disabled by config");
        None
    };

    // OCR text embedding: reuse the app-wide shared text embedder via ctx.embed_text().
    // No local TextEmbedding instance needed — saves ~130 MB of RAM.
    eprintln!("[screenshot-embed] OCR text embedding: using shared app-wide embedder via ctx.embed_text()");

    // RLX image embedder (nomic-embed-vision-v1.5) — lazily loaded on first
    // use. Shares the aligned 768-d space with nomic-embed-text.
    #[cfg(feature = "text-embeddings-rlx")]
    let mut image_embedder: Option<crate::rlx_image::RlxImageEmbedder> = None;
    #[cfg(feature = "text-embeddings-rlx")]
    let image_device = rlx_image_device(&initial_config);

    let mut inserts_since_save: usize = 0;
    let mut ocr_inserts_since_save: usize = 0;

    // ── Startup backfill: process any screenshots that were saved but
    // not yet embedded (e.g. app crashed mid-embed, or features were
    // disabled when the screenshot was captured).
    // Only runs when screenshots are enabled and not session-gated
    // (or a session is active).
    let should_backfill = {
        let cfg = ctx.config();
        cfg.enabled && (!cfg.session_only || ctx.is_session_active())
    };

    if should_backfill {
        let screenshots_dir = skill_dir.join(SCREENSHOTS_DIR);

        // Backfill: vision HNSW gets the rlx nomic-vision *image* embedding;
        // OCR HNSW gets the nomic-text embedding of the OCR'd content. Both
        // live in the same aligned 768-d space.
        let ocr_rows = store.rows_without_ocr();
        let embed_rows = store.rows_without_embedding();
        // Rows that already have OCR text but no OCR embedding (e.g. after an
        // embedding-scheme migration) — re-embed the existing text, no re-OCR.
        let ocr_embed_rows = store.rows_without_ocr_embedding();
        let needs_ocr: std::collections::HashSet<i64> = ocr_rows.iter().map(|r| r.id).collect();
        let needs_embed: std::collections::HashSet<i64> = embed_rows.iter().map(|r| r.id).collect();
        let needs_ocr_embed: std::collections::HashSet<i64> = ocr_embed_rows.iter().map(|r| r.id).collect();

        // Merge into a deduplicated list with filenames
        let mut all_rows: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
        for r in ocr_rows.iter().chain(embed_rows.iter()).chain(ocr_embed_rows.iter()) {
            all_rows.entry(r.id).or_insert_with(|| r.filename.clone());
        }

        // Don't gate the whole backfill on the OCR engine: vision and OCR-text
        // re-embed rows don't need it (only fresh OCR does, which is handled
        // per-row below). So a failed OCR-engine load can't block re-embedding.
        if !all_rows.is_empty() {
            eprintln!(
                "[screenshot-embed] backfill: {} screenshots need OCR/embedding",
                all_rows.len()
            );
            for (&row_id, filename) in &all_rows {
                let webp_path = screenshots_dir.join(filename);
                if !webp_path.exists() {
                    continue;
                }
                let Ok(raw) = std::fs::read(&webp_path) else {
                    continue;
                };

                // OCR if needed, otherwise fetch existing OCR text for embedding
                let ocr_text = if needs_ocr.contains(&row_id) {
                    if let Some(ref engine) = ocr_engine {
                        run_ocr(engine, &raw).unwrap_or_default()
                    } else {
                        String::new()
                    }
                } else {
                    // Already has OCR — fetch existing text for the embedding
                    store
                        .get_embedding_and_ocr(row_id)
                        .map(|e| e.ocr_text)
                        .unwrap_or_default()
                };

                let ts = store.get_timestamp(row_id).unwrap_or(0);

                // Vision HNSW: embed the actual image via rlx nomic-vision
                // (falls back to OCR-text-as-vision when rlx isn't compiled in).
                if needs_embed.contains(&row_id) {
                    #[cfg(feature = "text-embeddings-rlx")]
                    let (vision_emb, vision_backend, vision_model) = (
                        ensure_image_embedder(&mut image_embedder, &image_device).and_then(|e| e.embed_bytes(&raw)),
                        "rlx".to_string(),
                        "nomic-ai/nomic-embed-vision-v1.5".to_string(),
                    );
                    #[cfg(not(feature = "text-embeddings-rlx"))]
                    let (vision_emb, vision_backend, vision_model) = (
                        if ocr_text.is_empty() {
                            None
                        } else {
                            ctx.embed_text(&ocr_text)
                        },
                        initial_config.embed_backend.clone(),
                        initial_config.model_id(),
                    );
                    if let Some(emb) = vision_emb {
                        let id = hnsw.len() as u64;
                        hnsw.insert(emb.clone(), ts);
                        inserts_since_save += 1;
                        store.update_embedding(
                            row_id,
                            &emb,
                            Some(id),
                            &vision_backend,
                            &vision_model,
                            initial_config.image_size,
                        );
                    }
                }

                // OCR HNSW: embed the OCR text (text search over screenshot
                // content) — for freshly-OCR'd rows and re-embed-only rows.
                if needs_ocr.contains(&row_id) || needs_ocr_embed.contains(&row_id) {
                    let ocr_emb = if ocr_text.is_empty() {
                        None
                    } else {
                        ctx.embed_text(&ocr_text)
                    };
                    if let Some(emb) = ocr_emb {
                        let id = ocr_hnsw.len() as u64;
                        ocr_hnsw.insert(emb.clone(), ts);
                        ocr_inserts_since_save += 1;
                        store.update_ocr(row_id, &ocr_text, Some(&emb), Some(id));
                    } else {
                        store.update_ocr(row_id, &ocr_text, None, None);
                    }
                }
            }
            if inserts_since_save > 0 {
                save_hnsw(&hnsw, &skill_dir);
                inserts_since_save = 0;
            }
            if ocr_inserts_since_save > 0 {
                save_ocr_hnsw(&ocr_hnsw, &skill_dir);
                ocr_inserts_since_save = 0;
            }
            eprintln!("[screenshot-embed] backfill: OCR + embeddings done");
        }
    } else if !should_backfill {
        eprintln!("[screenshot-embed] skipping backfill (disabled or session-gated with no active session)");
    }

    while let Ok(job) = rx.recv() {
        metrics.queue_depth.fetch_sub(1, Ordering::Relaxed);

        // Check the LIVE config — skip stale jobs.
        {
            let live = ctx.config();
            if !live.enabled {
                continue;
            }
            if live.session_only && !ctx.is_session_active() {
                continue;
            }
        }

        let embed_start = Instant::now();
        let config = &job.config;

        // ── Fast path: duplicate screenshot — copy from previous row ──
        if let Some(src_id) = job.copy_from_row {
            if let Some(prev) = store.get_embedding_and_ocr(src_id) {
                // Copy vision embedding
                if let Some(ref emb) = prev.embedding {
                    let id = hnsw.len() as u64;
                    hnsw.insert(emb.clone(), job.ts_i64);
                    inserts_since_save += 1;
                    if inserts_since_save >= SCREENSHOT_HNSW_SAVE_EVERY {
                        save_hnsw(&hnsw, &skill_dir);
                        inserts_since_save = 0;
                    }
                    store.update_embedding(
                        job.row_id,
                        emb,
                        Some(id),
                        &prev.model_backend,
                        &prev.model_id,
                        prev.image_size,
                    );
                }
                // Copy OCR text + OCR embedding
                if !prev.ocr_text.is_empty() {
                    if let Some(ref ocr_emb) = prev.ocr_embedding {
                        let id = ocr_hnsw.len() as u64;
                        ocr_hnsw.insert(ocr_emb.clone(), job.ts_i64);
                        ocr_inserts_since_save += 1;
                        if ocr_inserts_since_save >= SCREENSHOT_HNSW_SAVE_EVERY {
                            save_ocr_hnsw(&ocr_hnsw, &skill_dir);
                            ocr_inserts_since_save = 0;
                        }
                        store.update_ocr(job.row_id, &prev.ocr_text, Some(ocr_emb), Some(id));
                    } else {
                        store.update_ocr(job.row_id, &prev.ocr_text, None, None);
                    }
                }
                eprintln!(
                    "[screenshot-embed] copied OCR + embeddings from row {src_id} → {}",
                    job.row_id
                );
                metrics.embeds.fetch_add(1, Ordering::Relaxed);
                metrics
                    .embed_total_us
                    .store(embed_start.elapsed().as_micros() as u64, Ordering::Relaxed);
                metrics.last_embed_unix.store(now_ms(), Ordering::Relaxed);
                continue;
            }
            // Fallback: source row missing/corrupted — proceed with normal embedding
            eprintln!("[screenshot-embed] copy source row {src_id} not found — running full pipeline");
        }

        // Track model changes for metadata
        if config.embed_backend != last_backend || config.fastembed_model != last_model {
            eprintln!(
                "[screenshot-embed] model changed: {} / {}",
                config.embed_backend, config.fastembed_model
            );
            last_backend = config.embed_backend.clone();
            last_model = config.fastembed_model.clone();
        }

        // ── Lazily encode JPEG for paths that need encoded bytes ──
        // JPEG encoding is ~10× faster than PNG.  Only produced when the
        // LLM vision or OCR paths actually need it.
        let encoded_bytes_lazy = |img: &DynamicImage| -> Vec<u8> { encode_jpeg(img, 85).unwrap_or_default() };

        // ── OCR extraction (on the resized image — typically 768px) ──
        // OCR runs first so that the "fastembed" backend can use the OCR text
        // as the image's embedding via nomic-embed-text-v1.5.
        let t_ocr = Instant::now();
        let ocr_text = if job.run_ocr {
            if config.embed_backend == "llm-vlm" || config.ocr_engine == "llm-vlm" {
                // VLM-based OCR — encode to JPEG for the LLM vision endpoint.
                let encoded = job.resized_img.as_ref().map(encoded_bytes_lazy).unwrap_or_default();
                if !encoded.is_empty() {
                    ctx.ocr_via_llm(&encoded).unwrap_or_default()
                } else {
                    String::new()
                }
            } else if let Some(ref engine) = ocr_engine {
                // ocrs works on decoded pixel data — encode to PNG/JPEG for
                // its image loading, or call run_ocr_rten with raw pixels.
                if let Some(ref img) = job.resized_img {
                    run_ocr_from_image(engine, img).unwrap_or_default()
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        metrics
            .ocr_us
            .store(t_ocr.elapsed().as_micros() as u64, Ordering::Relaxed);

        // ── Embed OCR text → OCR HNSW (text search over screenshot content) ──
        let t0 = Instant::now();
        let ocr_embedding = if !ocr_text.is_empty() {
            ctx.embed_text(&ocr_text)
        } else {
            None
        };

        // ── Vision embedding → vision HNSW ──
        // The default "rlx" backend embeds the actual image via
        // nomic-embed-vision-v1.5, which shares an aligned 768-d space with
        // nomic-embed-text — so text queries (search_query:) match images
        // cross-modally. "mmproj"/"llm-vlm" embed pixels via the local LLM.
        let (embedding, model_backend, model_id) = match config.embed_backend.as_str() {
            "rlx" | "fastembed" => {
                #[cfg(feature = "text-embeddings-rlx")]
                {
                    match job.resized_img.as_ref().and_then(|img| {
                        ensure_image_embedder(&mut image_embedder, &image_device).and_then(|e| e.embed_image(img))
                    }) {
                        Some(v) => (
                            Some(v),
                            "rlx".to_string(),
                            "nomic-ai/nomic-embed-vision-v1.5".to_string(),
                        ),
                        None => (None, String::new(), String::new()),
                    }
                }
                #[cfg(not(feature = "text-embeddings-rlx"))]
                {
                    // No RLX vision compiled in — fall back to OCR-text-as-vision.
                    (ocr_embedding.clone(), "rlx".to_string(), config.model_id())
                }
            }
            "mmproj" | "llm-vlm" => {
                let encoded = job.resized_img.as_ref().map(encoded_bytes_lazy).unwrap_or_default();
                let result = if !encoded.is_empty() {
                    ctx.embed_image_via_llm(&encoded)
                } else {
                    None
                };
                let backend = config.embed_backend.clone();
                let mid = config.model_id();
                if result.is_some() {
                    (result, backend, mid)
                } else {
                    (None, String::new(), String::new())
                }
            }
            _ => (None, String::new(), String::new()),
        };

        metrics
            .vision_embed_us
            .store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        // ── Store vision embedding in HNSW ──
        if let Some(ref emb) = embedding {
            // If the model changed and produces a different embedding dimension,
            // the existing HNSW is incompatible — reset rather than panic.
            reset_hnsw_if_dim_mismatch(&mut hnsw, emb.len(), "vision");
            let id = hnsw.len() as u64;
            hnsw.insert(emb.clone(), job.ts_i64);
            inserts_since_save += 1;
            if inserts_since_save >= SCREENSHOT_HNSW_SAVE_EVERY {
                save_hnsw(&hnsw, &skill_dir);
                inserts_since_save = 0;
            }
            store.update_embedding(job.row_id, emb, Some(id), &model_backend, &model_id, config.image_size);
        }

        // ── OCR text + OCR HNSW backfill ──
        if !ocr_text.is_empty() {
            if let Some(ref emb) = ocr_embedding {
                reset_hnsw_if_dim_mismatch(&mut ocr_hnsw, emb.len(), "OCR");
                let id = ocr_hnsw.len() as u64;
                ocr_hnsw.insert(emb.clone(), job.ts_i64);
                ocr_inserts_since_save += 1;
                if ocr_inserts_since_save >= SCREENSHOT_HNSW_SAVE_EVERY {
                    save_ocr_hnsw(&ocr_hnsw, &skill_dir);
                    ocr_inserts_since_save = 0;
                }
                store.update_ocr(job.row_id, &ocr_text, Some(emb), Some(id));
            } else {
                // Shared embedder not available — still save the OCR text
                store.update_ocr(job.row_id, &ocr_text, None, None);
            }
        }
        metrics
            .text_embed_us
            .store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        metrics.embeds.fetch_add(1, Ordering::Relaxed);
        metrics
            .embed_total_us
            .store(embed_start.elapsed().as_micros() as u64, Ordering::Relaxed);
        metrics.last_embed_unix.store(now_ms(), Ordering::Relaxed);
    }

    // Channel closed — save indexes before exit
    save_hnsw(&hnsw, &skill_dir);
    save_ocr_hnsw(&ocr_hnsw, &skill_dir);
    eprintln!("[screenshot-embed] thread exiting — indexes saved");
}

// ── Public query functions (called from Tauri commands) ───────────────────────

/// Search screenshots by embedding vector using the HNSW index.
pub fn search_by_vector(
    hnsw: &LabeledIndex<Cosine, i64>,
    store: &ScreenshotStore,
    query: &[f32],
    k: usize,
) -> Vec<ScreenshotResult> {
    let ef = k.max(100); // ef >= k for good recall
    let results = hnsw.search(query, k, ef);
    results
        .iter()
        .filter_map(|r| {
            let ts = *r.payload; // YYYYMMDDHHmmss
            let mut sr = store.find_by_timestamp(ts)?;
            sr.similarity = 1.0 - r.distance; // cosine distance → similarity
            Some(sr)
        })
        .collect()
}

/// Search screenshots by OCR text similarity using the OCR HNSW index.
///
/// `embed_fn` embeds the query text into a vector using the app-wide shared
/// text embedder.  Pass a closure that delegates to `EmbedderState` (or
/// `ScreenshotContext::embed_text`) so we don't need a local ONNX model.
pub fn search_by_ocr_text_embedding(
    skill_dir: &Path,
    store: &ScreenshotStore,
    query: &str,
    k: usize,
    embed_fn: &dyn Fn(&str) -> Option<Vec<f32>>,
) -> Vec<ScreenshotResult> {
    // Embed the query text via the shared embedder
    let query_emb = embed_ocr_text(query, embed_fn);
    let Some(query_emb) = query_emb else {
        return vec![];
    };

    // Load OCR HNSW
    let hnsw_path = skill_dir.join(SCREENSHOTS_OCR_HNSW);
    let Ok(hnsw) = LabeledIndex::<Cosine, i64>::load(&hnsw_path, Cosine) else {
        return vec![];
    };

    search_by_vector(&hnsw, store, &query_emb, k)
}

/// Search screenshots by OCR text substring (SQL LIKE).
pub fn search_by_ocr_text_like(store: &ScreenshotStore, query: &str, limit: usize) -> Vec<ScreenshotResult> {
    store.search_by_ocr_text(query, limit)
}

/// Get screenshots around a given unix timestamp.
pub fn get_around(store: &ScreenshotStore, timestamp: i64, window_secs: i32) -> Vec<ScreenshotResult> {
    store.around_timestamp(timestamp, window_secs)
}

/// Estimate re-embedding work.
pub fn estimate_reembed(store: &ScreenshotStore, config: &ScreenshotConfig, skill_dir: &Path) -> ReembedEstimate {
    let backend = &config.embed_backend;
    let mid = config.model_id();
    let total = store.count_embedded();
    let stale = store.count_stale(backend, &mid);
    let unembedded = store.count_unembedded();

    // Estimate per-image time: OCR (~200ms) + text embedding (~50ms)
    let _ = skill_dir; // used by previous vision-encoder benchmark
    let per_image_ms = 250u64;

    let total_to_embed = stale + unembedded;
    let eta_secs = (total_to_embed as u64 * per_image_ms) / 1000;

    ReembedEstimate {
        total,
        stale,
        unembedded,
        per_image_ms,
        eta_secs,
    }
}

/// Re-embed all screenshots with the current model.
pub fn rebuild_embeddings(
    store: &ScreenshotStore,
    config: &ScreenshotConfig,
    skill_dir: &Path,
    ctx: &dyn crate::context::ScreenshotContext,
) -> ReembedResult {
    let backend = &config.embed_backend;
    let mid = config.model_id();

    let rows = store.rows_needing_embed(backend, &mid);
    let total = rows.len();

    // Load OCR engine for re-extracting text from images
    let ocr_engine = load_ocr_engine(skill_dir);

    let screenshots_dir = skill_dir.join(SCREENSHOTS_DIR);
    let start = Instant::now();
    let mut embedded = 0usize;
    let mut skipped = 0usize;

    for (i, row) in rows.iter().enumerate() {
        let webp_path = screenshots_dir.join(&row.filename);
        if !webp_path.exists() {
            skipped += 1;
            continue;
        }

        // Try existing OCR text first, otherwise run OCR
        let existing_ocr = store
            .get_embedding_and_ocr(row.id)
            .map(|e| e.ocr_text)
            .unwrap_or_default();

        let ocr_text = if !existing_ocr.is_empty() {
            existing_ocr
        } else if let Some(ref engine) = ocr_engine {
            let Ok(raw) = std::fs::read(&webp_path) else {
                skipped += 1;
                continue;
            };
            run_ocr(engine, &raw).unwrap_or_default()
        } else {
            skipped += 1;
            continue;
        };

        if ocr_text.is_empty() {
            skipped += 1;
            continue;
        }

        // Embed OCR text via shared text embedder
        let emb = ctx.embed_text(&ocr_text);

        if let Some(emb) = emb {
            store.update_embedding(row.id, &emb, None, backend, &mid, config.image_size);
            embedded += 1;
        } else {
            skipped += 1;
        }

        // Progress event every 10 rows
        if (i + 1) % 10 == 0 || i + 1 == total {
            let elapsed = start.elapsed().as_secs_f64();
            let rate = if embedded > 0 { elapsed / embedded as f64 } else { 0.25 };
            let remaining = total - i - 1;
            let eta = remaining as f64 * rate;
            ctx.emit_event(
                "screenshot-reembed-progress",
                serde_json::json!({
                    "done": i + 1,
                    "total": total,
                    "elapsed_secs": elapsed,
                    "eta_secs": eta,
                }),
            );
        }
    }

    // Rebuild HNSW
    load_or_rebuild_hnsw_generic(skill_dir, SCREENSHOTS_HNSW, "vision", || store.all_embeddings());

    ReembedResult {
        embedded,
        skipped,
        elapsed_secs: start.elapsed().as_secs_f64(),
    }
}
