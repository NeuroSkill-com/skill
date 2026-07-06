//! CPU gather helpers for JEPA masks on row-major `[B, N, D]` tensors.

/// Gather along sequence dim `1` for each mask in `masks`, concatenate on batch.
///
/// `x`: row-major `[b, n, d]`  
/// `masks`: each `[k]` patch index into `n`  
/// Returns row-major `[b * masks.len(), k_max, d]` — all masks must share the same `k` for now.
pub fn apply_masks_f32(x: &[f32], b: usize, n: usize, d: usize, masks: &[Vec<i64>]) -> Vec<f32> {
    if masks.is_empty() {
        return x.to_vec();
    }
    let k = masks[0].len();
    let out_b = b * masks.len();
    let mut out = vec![0f32; out_b * k * d];
    for (mi, mask) in masks.iter().enumerate() {
        assert_eq!(mask.len(), k, "all masks must have the same length");
        for bi in 0..b {
            let out_row = (mi * b + bi) * k * d;
            for (ki, &idx) in mask.iter().enumerate() {
                let idx = idx as usize;
                assert!(idx < n, "mask index {idx} >= n={n}");
                let in_off = (bi * n + idx) * d;
                let out_off = out_row + ki * d;
                out[out_off..out_off + d].copy_from_slice(&x[in_off..in_off + d]);
            }
        }
    }
    out
}

/// Gather positions from `[1, n, d]` using indices `[k]`.
pub fn gather_positions_f32(pos: &[f32], n: usize, d: usize, indices: &[i64]) -> Vec<f32> {
    let k = indices.len();
    let mut out = vec![0f32; k * d];
    for (ki, &idx) in indices.iter().enumerate() {
        let idx = idx as usize;
        assert!(idx < n);
        let off = idx * d;
        out[ki * d..(ki + 1) * d].copy_from_slice(&pos[off..off + d]);
    }
    out
}
