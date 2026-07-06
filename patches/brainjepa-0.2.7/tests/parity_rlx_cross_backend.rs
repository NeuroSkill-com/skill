//! RLX encoder + JEPA predictor on each backend vs RLX CPU reference.
//!
//! Run with `--test-threads=1` — BHSD/BSNH cases set `BRAINJEPA_ATTN_LAYOUT` and
//! must not run in parallel with other tests.
//!
//! Requires real weights (`data/brainjepa.safetensors` or HF cache).
//!
//! ```text
//! # macOS — CPU + Metal + wgpu (+ MLX if built)
//! cargo test --release --no-default-features \
//!     --features rlx-engine,rlx-metal,rlx-gpu \
//!     --test parity_rlx_cross_backend -- --nocapture
//!
//! cargo test --release --no-default-features \
//!     --features rlx-engine,rlx-mlx \
//!     --test parity_rlx_cross_backend -- --nocapture
//! ```

#![cfg(feature = "rlx")]
#![allow(dead_code, unused_imports)] // helpers used by cfg-gated backend tests (metal, mlx, gpu, …)

mod parity_harness;

use std::path::PathBuf;

use brainjepa::data::load_fmri_safetensors_f32;
use brainjepa::rlx::{ensure_device, BrainJepaEncoder, BrainJepaPredictor};
use parity_harness::{
    default_configs, locate_weights, max_abs_diff, write_fmri_sample, TOL_RLX_GPU_VS_CPU,
    TOL_RLX_METAL_VS_CPU,
};
use rlx::Device;

fn fmri_path() -> PathBuf {
    std::env::temp_dir().join("brainjepa_cross_backend_fmri.safetensors")
}

struct CpuReference {
    encoder_emb: Vec<f32>,
    encoder_shape: Vec<usize>,
    pred_enc: Vec<f32>,
    pred_out: Vec<f32>,
    fmri: brainjepa::data::FmriInputF32,
    enc_indices: Vec<i64>,
    pred_indices: Vec<i64>,
}

fn cpu_reference() -> Option<CpuReference> {
    let (weights, gradient) = locate_weights()?;
    let fmri_path = fmri_path();
    write_fmri_sample(&fmri_path).ok()?;

    let (model_cfg, data_cfg) = default_configs();
    let w = weights.to_str()?;
    let g = gradient.to_str()?;
    let dev = Device::Cpu;
    ensure_device(dev).ok()?;

    let fmri = load_fmri_safetensors_f32(fmri_path.to_str()?).ok()?;

    let (mut enc, _) = BrainJepaEncoder::from_weights(w, g, &model_cfg, &data_cfg, &dev).ok()?;
    let enc_out = enc.encode_safetensors(fmri_path.to_str()?).ok()?;

    let (mut pred, _) = BrainJepaPredictor::from_weights(w, g, &model_cfg, &data_cfg, &dev).ok()?;
    let enc_indices = pred.enc_indices.clone();
    let pred_indices = pred.pred_indices.clone();
    let (pred_enc, pred_out) = pred
        .predict_f32(
            fmri.data.clone(),
            fmri.n_rois,
            fmri.n_time,
            &enc_indices,
            &pred_indices,
        )
        .ok()?;

    Some(CpuReference {
        encoder_emb: enc_out.embeddings,
        encoder_shape: enc_out.shape,
        pred_enc,
        pred_out,
        fmri,
        enc_indices,
        pred_indices,
    })
}

macro_rules! backend_matches_cpu {
    ($name:ident, $feat:meta, $dev:expr, $label:literal, $tol:expr $(, $attn_layout:expr)?) => {
        #[test]
        #[$feat]
        fn $name() {
            $(
                unsafe { std::env::set_var("BRAINJEPA_ATTN_LAYOUT", $attn_layout); }
            )?
            let reference = match cpu_reference() {
                Some(r) => r,
                None => {
                    eprintln!("[SKIP] {} — missing weights", $label);
                    return;
                }
            };

            let dev = $dev;
            if !rlx::is_available(dev) {
                eprintln!("[SKIP] {} — {:?} unavailable", $label, dev);
                return;
            }
            ensure_device(dev).expect("device");

            let (weights, gradient) = locate_weights().expect("weights");
            let fmri_path = fmri_path();
            write_fmri_sample(&fmri_path).expect("fmri");

            let (model_cfg, data_cfg) = default_configs();
            let w = weights.to_str().unwrap();
            let g = gradient.to_str().unwrap();
            let tol: f32 = $tol;

            eprintln!("→ parity {} encoder vs RLX CPU", $label);
            let (mut enc, _) =
                BrainJepaEncoder::from_weights(w, g, &model_cfg, &data_cfg, &dev).expect("load");
            let out = enc
                .encode_safetensors(fmri_path.to_str().unwrap())
                .expect("encode");

            assert_eq!(out.shape, reference.encoder_shape);
            let enc_diff = max_abs_diff(&reference.encoder_emb, &out.embeddings);
            eprintln!("  encoder max_abs = {enc_diff:.6} (tol {tol})");
            assert!(
                enc_diff < tol,
                "{} encoder vs RLX CPU: max_abs {enc_diff:.6} >= {tol}",
                $label,
            );

            eprintln!("→ parity {} predictor vs RLX CPU", $label);
            let (mut pred, _) =
                BrainJepaPredictor::from_weights(w, g, &model_cfg, &data_cfg, &dev)
                    .expect("predictor load");
            let (pred_enc, pred_out) = pred
                .predict_f32(
                    reference.fmri.data.clone(),
                    reference.fmri.n_rois,
                    reference.fmri.n_time,
                    &reference.enc_indices,
                    &reference.pred_indices,
                )
                .expect("predict");

            let pred_enc_diff = max_abs_diff(&reference.pred_enc, &pred_enc);
            let pred_out_diff = max_abs_diff(&reference.pred_out, &pred_out);
            eprintln!("  predictor ctx max_abs = {pred_enc_diff:.6} (tol {tol})");
            eprintln!("  predictor out max_abs = {pred_out_diff:.6} (tol {tol})");
            assert!(
                pred_enc_diff < tol,
                "{} predictor ctx vs RLX CPU: max_abs {pred_enc_diff:.6} >= {tol}",
                $label,
            );
            assert!(
                pred_out_diff < tol,
                "{} predictor out vs RLX CPU: max_abs {pred_out_diff:.6} >= {tol}",
                $label,
            );
        }
    };
}

backend_matches_cpu!(
    metal_matches_cpu,
    cfg(all(feature = "rlx-metal", target_os = "macos")),
    Device::Metal,
    "metal",
    TOL_RLX_METAL_VS_CPU
);

backend_matches_cpu!(
    metal_bhsd_matches_cpu,
    cfg(all(feature = "rlx-metal", target_os = "macos")),
    Device::Metal,
    "metal-bhsd",
    TOL_RLX_METAL_VS_CPU,
    "bhsd"
);

backend_matches_cpu!(
    metal_bsnh_matches_cpu,
    cfg(all(feature = "rlx-metal", target_os = "macos")),
    Device::Metal,
    "metal-bsnh",
    TOL_RLX_METAL_VS_CPU,
    "bsnh"
);

backend_matches_cpu!(
    mlx_matches_cpu,
    cfg(all(feature = "rlx-mlx", target_os = "macos")),
    Device::Mlx,
    "mlx",
    TOL_RLX_GPU_VS_CPU
);

backend_matches_cpu!(
    wgpu_matches_cpu,
    cfg(feature = "rlx-gpu"),
    Device::Gpu,
    "wgpu",
    TOL_RLX_GPU_VS_CPU
);

backend_matches_cpu!(
    cuda_matches_cpu,
    cfg(feature = "rlx-cuda"),
    Device::Cuda,
    "cuda",
    TOL_RLX_GPU_VS_CPU
);

backend_matches_cpu!(
    rocm_matches_cpu,
    cfg(feature = "rlx-rocm"),
    Device::Rocm,
    "rocm",
    TOL_RLX_GPU_VS_CPU
);
