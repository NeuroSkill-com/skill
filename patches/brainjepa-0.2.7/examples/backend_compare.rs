//! Encode one fMRI sample on every RLX backend compiled into this binary.
//!
//! ```sh
//! cargo run --example backend_compare --release --features rlx-engine
//!
//! # macOS — Metal + MLX + wgpu:
//! cargo run --example backend_compare --release --no-default-features \
//!     --features rlx-engine,rlx-metal,rlx-mlx,rlx-gpu
//! ```

use std::path::PathBuf;
use std::time::Instant;

use brainjepa::rlx::{ensure_device, resolve_attn_layout, AttnLayout, BrainJepaEncoder};
use brainjepa::{DataConfig, ModelConfig};
use rlx::Device;

struct Row {
    backend: String,
    attn: String,
    ms: f64,
    max_abs_vs_cpu: Option<f32>,
}

fn try_encode(
    label: &str,
    device: Device,
    weights: &str,
    gradient: &str,
    fmri: &str,
    model_cfg: &ModelConfig,
    data_cfg: &DataConfig,
    cpu_ref: Option<&[f32]>,
) -> Option<Row> {
    if !rlx::is_available(device) {
        eprintln!("  {label:<22} SKIP (runtime unavailable)");
        return None;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<_> {
        ensure_device(device)?;
        let layout = resolve_attn_layout(device)?;
        let (mut enc, _) =
            BrainJepaEncoder::from_weights(weights, gradient, model_cfg, data_cfg, &device)?;
        let t = Instant::now();
        let out = enc.encode_safetensors(fmri)?;
        Ok((out.embeddings, t.elapsed().as_secs_f64() * 1000.0, layout))
    }));
    match result {
        Ok(Ok((emb, ms, layout))) => {
            let attn = match layout {
                AttnLayout::Bsnh => "bsnh",
                AttnLayout::Bhsd => "bhsd",
            };
            let diff = cpu_ref.map(|r| {
                emb.iter()
                    .zip(r.iter())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0f32, f32::max)
            });
            eprintln!("  {label:<22} {ms:8.1} ms  attn={attn}");
            Some(Row {
                backend: label.to_string(),
                attn: attn.to_string(),
                ms,
                max_abs_vs_cpu: diff,
            })
        }
        Ok(Err(e)) => {
            eprintln!("  {label:<22} ERR  {e:#}");
            None
        }
        Err(_) => {
            eprintln!("  {label:<22} SKIP (panic)");
            None
        }
    }
}

fn main() -> anyhow::Result<()> {
    brainjepa::init_threads(None);

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let weights = std::env::var("BRAINJEPA_WEIGHTS")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let p = manifest.join("data/brainjepa.safetensors");
            p.exists().then_some(p)
        })
        .ok_or_else(|| anyhow::anyhow!("missing weights — set BRAINJEPA_WEIGHTS or add data/"))?;
    let gradient = std::env::var("BRAINJEPA_GRADIENT")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("data/gradient_mapping_450.csv"));
    let fmri = std::env::var("BRAINJEPA_INPUT")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let p = manifest.join("data/test_fmri.safetensors");
            p.exists().then_some(p)
        })
        .ok_or_else(|| {
            anyhow::anyhow!("missing input — set BRAINJEPA_INPUT or add data/test_fmri.safetensors")
        })?;

    let w = weights.to_str().unwrap();
    let g = gradient.to_str().unwrap();
    let f = fmri.to_str().unwrap();

    let model_cfg = ModelConfig::from_variant("vit_base")?;
    let data_cfg = DataConfig::default();

    println!("Brain-JEPA RLX backend compare");
    println!("  weights : {w}");
    println!("  fmri    : {f}");
    println!();

    let cpu_ref: Vec<f32> = {
        ensure_device(Device::Cpu)?;
        let (mut enc, _) =
            BrainJepaEncoder::from_weights(w, g, &model_cfg, &data_cfg, &Device::Cpu)?;
        enc.encode_safetensors(f)?.embeddings
    };

    let mut rows = Vec::new();

    if let Some(r) = try_encode(
        "RLX/CPU",
        Device::Cpu,
        w,
        g,
        f,
        &model_cfg,
        &data_cfg,
        Some(&cpu_ref),
    ) {
        rows.push(r);
    }

    #[cfg(feature = "rlx-blas-accelerate")]
    if let Some(r) = try_encode(
        "RLX/CPU+Accelerate",
        Device::Cpu,
        w,
        g,
        f,
        &model_cfg,
        &data_cfg,
        Some(&cpu_ref),
    ) {
        rows.push(r);
    }

    #[cfg(feature = "rlx-metal")]
    if let Some(r) = try_encode(
        "RLX/Metal",
        Device::Metal,
        w,
        g,
        f,
        &model_cfg,
        &data_cfg,
        Some(&cpu_ref),
    ) {
        rows.push(r);
    }

    #[cfg(feature = "rlx-mlx")]
    if let Some(r) = try_encode(
        "RLX/MLX",
        Device::Mlx,
        w,
        g,
        f,
        &model_cfg,
        &data_cfg,
        Some(&cpu_ref),
    ) {
        rows.push(r);
    }

    #[cfg(feature = "rlx-gpu")]
    if let Some(r) = try_encode(
        "RLX/wgpu",
        Device::Gpu,
        w,
        g,
        f,
        &model_cfg,
        &data_cfg,
        Some(&cpu_ref),
    ) {
        rows.push(r);
    }

    #[cfg(feature = "rlx-cuda")]
    if let Some(r) = try_encode(
        "RLX/CUDA",
        Device::Cuda,
        w,
        g,
        f,
        &model_cfg,
        &data_cfg,
        Some(&cpu_ref),
    ) {
        rows.push(r);
    }

    #[cfg(feature = "rlx-rocm")]
    if let Some(r) = try_encode(
        "RLX/ROCm",
        Device::Rocm,
        w,
        g,
        f,
        &model_cfg,
        &data_cfg,
        Some(&cpu_ref),
    ) {
        rows.push(r);
    }

    println!();
    println!(
        "{:<22} {:>10}  {:>6}  {:>12}",
        "backend", "encode_ms", "attn", "max|Δ| vs CPU"
    );
    println!("{}", "-".repeat(58));
    for r in &rows {
        let diff = r
            .max_abs_vs_cpu
            .map(|d| format!("{d:.2e}"))
            .unwrap_or_else(|| "—".into());
        println!(
            "{:<22} {:>9.1}ms  {:>6}  {:>12}",
            r.backend, r.ms, r.attn, diff
        );
    }

    Ok(())
}
