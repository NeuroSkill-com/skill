//! CPU-side 4D positional embedding (Fourier + MLP + LayerNorm).
//!
//! Precomputing this before the RLX graph run avoids several GPU-sensitive
//! narrow/reshape/activation patterns and improves cross-backend parity.

use super::weights::ParamMap;

const MARGIN: f32 = 0.4;
const INCREMENT_TIME: f32 = 0.1;
const LN_EPS: f32 = 1e-5;

fn gelu(x: f32) -> f32 {
    0.5 * x * (1.0 + ((2.0_f32 / std::f32::consts::PI).sqrt() * (x + 0.044715 * x * x * x)).tanh())
}

fn layer_norm(x: &mut [f32], gamma: &[f32], beta: &[f32], eps: f32) {
    let d = gamma.len();
    let n = x.len() / d;
    for t in 0..n {
        let row = &mut x[t * d..(t + 1) * d];
        let mean = row.iter().sum::<f32>() / d as f32;
        let var = row.iter().map(|v| {
            let d = *v - mean;
            d * d
        }).sum::<f32>() / d as f32;
        let inv = (var + eps).sqrt().recip();
        for j in 0..d {
            row[j] = (row[j] - mean) * inv * gamma[j] + beta[j];
        }
    }
}

fn matmul_vec(a: &[f32], a_cols: usize, w: &[f32], w_cols: usize) -> Vec<f32> {
    let a_rows = a.len() / a_cols;
    debug_assert_eq!(w.len(), a_cols * w_cols);
    let mut out = vec![0f32; a_rows * w_cols];
    for r in 0..a_rows {
        for c in 0..w_cols {
            let mut sum = 0f32;
            for k in 0..a_cols {
                sum += a[r * a_cols + k] * w[k * w_cols + c];
            }
            out[r * w_cols + c] = sum;
        }
    }
    out
}

/// Build `[S, embed_dim]` positional embeddings from `[S, 4]` positions.
pub fn precompute_pos_embed(pos4: &[f32], s: usize, d: usize, params: &ParamMap) -> Vec<f32> {
    let half = d / 2;
    let freq_t = &params["__reve.freq_t"].data; // [4, half]
    let mlp_w = &params["mlp4d.0.weight"].data; // [4, d]
    let mlp_ln_g = &params["mlp4d.2.weight"].data;
    let mlp_ln_b = &params["mlp4d.2.bias"].data;
    let pos_ln_g = &params["ln.weight"].data;
    let pos_ln_b = &params["ln.bias"].data;

    // ── Fourier branch (matches Burn: scale time, then add margin to all) ──
    let mut pos_scaled = vec![0f32; s * 4];
    for t in 0..s {
        let src = t * 4;
        pos_scaled[src + 0] = pos4[src + 0];
        pos_scaled[src + 1] = pos4[src + 1];
        pos_scaled[src + 2] = pos4[src + 2];
        pos_scaled[src + 3] = pos4[src + 3] * INCREMENT_TIME;
    }
    for v in &mut pos_scaled {
        *v += MARGIN;
    }
    let loc = matmul_vec(&pos_scaled, 4, freq_t, half); // [S, half]
    let mut fourier = vec![0f32; s * d];
    for t in 0..s {
        for j in 0..half {
            let v = loc[t * half + j];
            fourier[t * d + j] = v.cos();
            fourier[t * d + half + j] = v.sin();
        }
    }

    // ── MLP branch ──
    let mut mlp = matmul_vec(pos4, 4, mlp_w, d);
    for v in &mut mlp {
        *v = gelu(*v);
    }
    layer_norm(&mut mlp, mlp_ln_g, mlp_ln_b, LN_EPS);

    // ── Combine + final LN ──
    let mut pos = vec![0f32; s * d];
    for i in 0..pos.len() {
        pos[i] = fourier[i] + mlp[i];
    }
    layer_norm(&mut pos, pos_ln_g, pos_ln_b, LN_EPS);
    pos
}
