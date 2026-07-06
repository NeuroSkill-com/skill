//! CPU-side construction of Brain-JEPA 2D positional embeddings.
//!
//! Builds a "height" sincos grid and a "width" embedding that is either:
//! - `origin`: sincos over temporal patch index
//! - `mapping`: learned linear projection of brain gradient coordinates,
//!   repeated across the temporal axis and min/max normalised to [-1, 1]
//!
//! RLX graphs don't currently expose reduction-min/max helpers in the
//! inferred builder API, so we build the full `[1, N, D]` positional
//! embedding on the CPU and feed it as a graph input.

use crate::error::BrainJepaError;

pub fn build_pos_embed(
    mode: &str,
    grid_h: usize,
    grid_w: usize,
    embed_dim: usize,
    gradient_xyz: &[f32], // [grid_h, grad_dim] row-major
    grad_dim: usize,
    grad_proj_w: Option<&[f32]>, // [grad_dim, embed_dim/2] row-major
    grad_proj_b: Option<&[f32]>, // [embed_dim/2]
) -> crate::error::Result<Vec<f32>> {
    let n = grid_h * grid_w;
    let half = embed_dim / 2;

    let emb_h = sincos_1d_grid(half, grid_h, grid_w); // [n, half]

    let emb_w = match mode {
        "origin" => sincos_1d_width(half, grid_h, grid_w),
        "mapping" => {
            let w = grad_proj_w.ok_or_else(|| BrainJepaError::InvalidPosMode {
                mode: "mapping (missing grad_proj_w)".into(),
            })?;
            let b = grad_proj_b.ok_or_else(|| BrainJepaError::InvalidPosMode {
                mode: "mapping (missing grad_proj_b)".into(),
            })?;
            if grad_dim == 0 {
                return Err(BrainJepaError::InvalidPosMode {
                    mode: "mapping (grad_dim=0)".into(),
                });
            }
            if gradient_xyz.len() != grid_h * grad_dim {
                return Err(BrainJepaError::InvalidPosMode {
                    mode: format!(
                        "mapping (gradient_xyz len {}, expected {})",
                        gradient_xyz.len(),
                        grid_h * grad_dim
                    ),
                });
            }
            if w.len() != grad_dim * half || b.len() != half {
                return Err(BrainJepaError::InvalidPosMode {
                    mode: format!(
                        "mapping (grad_proj shapes w={}, b={}, expected w={}, b={})",
                        w.len(),
                        b.len(),
                        grad_dim * half,
                        half
                    ),
                });
            }

            // Project: [grid_h, grad_dim] × [grad_dim, half] + b -> [grid_h, half]
            let mut proj = vec![0f32; grid_h * half];
            for i in 0..grid_h {
                for j in 0..half {
                    let mut acc = b[j];
                    for k in 0..grad_dim {
                        acc += gradient_xyz[i * grad_dim + k] * w[k * half + j];
                    }
                    proj[i * half + j] = acc;
                }
            }

            // Repeat each ROI row across time patches -> [n, half]
            let mut rep = vec![0f32; n * half];
            for h in 0..grid_h {
                for tw in 0..grid_w {
                    let dst_row = h * grid_w + tw;
                    let src = &proj[h * half..(h + 1) * half];
                    let dst = &mut rep[dst_row * half..(dst_row + 1) * half];
                    dst.copy_from_slice(src);
                }
            }

            // Min/max normalise to [-1, 1].
            let mut min_v = f32::INFINITY;
            let mut max_v = f32::NEG_INFINITY;
            for &v in &rep {
                if v < min_v {
                    min_v = v;
                }
                if v > max_v {
                    max_v = v;
                }
            }
            let range = (max_v - min_v).max(1e-12);
            for v in &mut rep {
                *v = ((*v - min_v) / range) * 2.0 - 1.0;
            }
            rep
        }
        _ => {
            return Err(BrainJepaError::InvalidPosMode {
                mode: mode.to_string(),
            })
        }
    };

    // Concat along last dim: [n, half] + [n, half] -> [n, embed_dim]
    let mut out = vec![0f32; n * embed_dim];
    for row in 0..n {
        let a = &emb_h[row * half..(row + 1) * half];
        let b = &emb_w[row * half..(row + 1) * half];
        out[row * embed_dim..row * embed_dim + half].copy_from_slice(a);
        out[row * embed_dim + half..(row + 1) * embed_dim].copy_from_slice(b);
    }
    Ok(out)
}

fn sincos_1d_grid(half_dim: usize, grid_h: usize, grid_w: usize) -> Vec<f32> {
    let n = grid_h * grid_w;
    let quarter = half_dim / 2;
    let mut data = vec![0.0f32; n * half_dim];
    for h in 0..grid_h {
        for w in 0..grid_w {
            let pos = h as f64;
            let idx = h * grid_w + w;
            for k in 0..quarter {
                let omega = 1.0 / 10000.0_f64.powf(k as f64 / quarter as f64);
                let angle = pos * omega;
                data[idx * half_dim + k] = angle.sin() as f32;
                data[idx * half_dim + quarter + k] = angle.cos() as f32;
            }
        }
    }
    data
}

fn sincos_1d_width(half_dim: usize, grid_h: usize, grid_w: usize) -> Vec<f32> {
    let n = grid_h * grid_w;
    let quarter = half_dim / 2;
    let mut data = vec![0.0f32; n * half_dim];
    for h in 0..grid_h {
        for w in 0..grid_w {
            let pos = w as f64;
            let idx = h * grid_w + w;
            for k in 0..quarter {
                let omega = 1.0 / 10000.0_f64.powf(k as f64 / quarter as f64);
                let angle = pos * omega;
                data[idx * half_dim + k] = angle.sin() as f32;
                data[idx * half_dim + quarter + k] = angle.cos() as f32;
            }
        }
    }
    data
}
