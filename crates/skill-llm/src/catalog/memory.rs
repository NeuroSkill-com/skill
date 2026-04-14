// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Context-size recommendation and memory estimation.

use super::types::LlmModelEntry;

/// Estimate the KV-cache + model memory for a given context length.
///
/// Uses the same formula as `llmfit_core::models::LlmModel::estimate_memory_gb`:
///   model_weights + KV_cache + runtime_overhead
///
/// * `params_b` — parameter count in billions
/// * `quant`    — quantization tag (e.g. `"Q4_K_M"`)
/// * `ctx`      — context length in tokens
pub fn estimate_memory_gb(params_b: f64, quant: &str, ctx: u32) -> f64 {
    let bpp: f64 = match quant {
        "F32" => 4.0,
        "F16" | "BF16" => 2.0,
        "Q8_0" => 1.05,
        "Q6_K" | "Q6_K_L" => 0.80,
        "Q5_K_M" | "Q5_K_S" | "Q5_K_L" => 0.68,
        "Q4_K_M" | "Q4_K_S" | "Q4_K_L" | "Q4_0" | "Q4_1" => 0.58,
        "Q3_K_M" | "Q3_K_S" | "Q3_K_L" | "Q3_K_XL" => 0.48,
        "Q2_K" | "Q2_K_L" => 0.37,
        "IQ4_XS" | "IQ4_NL" => 0.55,
        "IQ3_M" | "IQ3_XS" | "IQ3_XXS" => 0.43,
        "IQ2_M" | "IQ2_S" | "IQ2_XS" | "IQ2_XXS" => 0.30,
        _ => 0.58, // default ~ Q4_K_M
    };
    let model_mem = params_b * bpp;
    // KV cache: ~0.000008 GB per billion params per context token
    let kv_cache = 0.000008 * params_b * ctx as f64;
    // Runtime overhead (CUDA/Metal context, buffers)
    let overhead = 0.5;
    model_mem + kv_cache + overhead
}

/// Recommend a context size for `entry` given the system's available memory.
///
/// The recommendation picks the **largest power-of-two context** (from the
/// standard set 2K, 4K, 8K, 16K, 32K, 64K, 128K) that:
///
/// 1. Does not exceed the model's `max_context_length`.
/// 2. Fits within the available GPU / unified memory with at least 15%
///    headroom (so the OS and other apps still have breathing room).
///
/// Falls back to **4096** if nothing larger fits, or to **4096** if the
/// model entry has no `params_b` / `max_context_length` metadata.
pub fn recommend_ctx_size(entry: &LlmModelEntry) -> u32 {
    // Need model metadata to make an intelligent recommendation.
    if entry.params_b <= 0.0 || entry.max_context_length == 0 {
        return 8192; // legacy fallback (4096 is too small for tool-augmented prompts)
    }

    let gpu = skill_data::gpu_stats::read();
    let available_gb: f64 = gpu
        .as_ref()
        .and_then(|g| {
            if g.is_unified_memory {
                g.free_memory_bytes.map(|b| b as f64 / (1024.0 * 1024.0 * 1024.0))
            } else {
                // Discrete GPU: prefer total VRAM (free VRAM may be None).
                g.total_memory_bytes.map(|b| b as f64 / (1024.0 * 1024.0 * 1024.0))
            }
        })
        .unwrap_or(8.0); // conservative fallback when GPU info is unavailable

    // Budget = available memory x 0.85 (keep 15% headroom for OS + apps).
    let budget = available_gb * 0.85;

    // Standard context sizes to try, largest first.
    const CANDIDATES: &[u32] = &[131072, 65536, 32768, 16384, 8192, 4096];

    for &ctx in CANDIDATES {
        if ctx > entry.max_context_length {
            continue;
        }
        let mem = estimate_memory_gb(entry.params_b, &entry.quant, ctx);
        if mem <= budget {
            return ctx;
        }
    }

    8192 // absolute minimum (tool-augmented prompts need >4K)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_memory_q4_k_m_7b() {
        let mem = estimate_memory_gb(7.0, "Q4_K_M", 4096);
        // 7.0 * 0.58 + 0.000008 * 7.0 * 4096 + 0.5
        // = 4.06 + 0.2293 + 0.5 ≈ 4.79
        assert!(mem > 4.0 && mem < 6.0, "got {mem}");
    }

    #[test]
    fn estimate_memory_f16_is_larger() {
        let q4 = estimate_memory_gb(7.0, "Q4_K_M", 8192);
        let f16 = estimate_memory_gb(7.0, "F16", 8192);
        assert!(f16 > q4, "F16 ({f16}) should need more memory than Q4_K_M ({q4})");
    }

    #[test]
    fn estimate_memory_larger_ctx_needs_more() {
        let small = estimate_memory_gb(7.0, "Q4_K_M", 4096);
        let large = estimate_memory_gb(7.0, "Q4_K_M", 32768);
        assert!(large > small, "32K ctx ({large}) should need more than 4K ({small})");
    }

    #[test]
    fn estimate_memory_unknown_quant_uses_default() {
        let known = estimate_memory_gb(7.0, "Q4_K_M", 4096);
        let unknown = estimate_memory_gb(7.0, "MYSTERY_QUANT", 4096);
        // Both use 0.58 bpp, so should be equal
        assert!((known - unknown).abs() < 0.001);
    }

    #[test]
    fn recommend_ctx_fallback_without_metadata() {
        let entry = LlmModelEntry {
            repo: "test/repo".into(),
            filename: "model.gguf".into(),
            quant: "Q4_K_M".into(),
            size_gb: 4.0,
            description: String::new(),
            family_id: String::new(),
            family_name: String::new(),
            family_desc: String::new(),
            tags: vec![],
            is_mmproj: false,
            recommended: false,
            advanced: false,
            params_b: 0.0, // no metadata
            max_context_length: 0,
            shard_files: vec![],
            local_path: None,
            state: super::super::types::DownloadState::NotDownloaded,
            status_msg: None,
            progress: 0.0,
            initiated_at_unix: None,
        };
        assert_eq!(recommend_ctx_size(&entry), 4096);
    }

    #[test]
    fn recommend_ctx_respects_max_context() {
        let entry = LlmModelEntry {
            repo: "test/repo".into(),
            filename: "model.gguf".into(),
            quant: "Q4_K_M".into(),
            size_gb: 4.0,
            description: String::new(),
            family_id: String::new(),
            family_name: String::new(),
            family_desc: String::new(),
            tags: vec![],
            is_mmproj: false,
            recommended: false,
            advanced: false,
            params_b: 7.0,
            max_context_length: 8192, // capped at 8K
            shard_files: vec![],
            local_path: None,
            state: super::super::types::DownloadState::NotDownloaded,
            status_msg: None,
            progress: 0.0,
            initiated_at_unix: None,
        };
        let ctx = recommend_ctx_size(&entry);
        assert!(ctx <= 8192, "ctx {ctx} should not exceed max_context_length 8192");
        assert!(ctx >= 4096, "ctx {ctx} should be at least 4096");
    }
}
