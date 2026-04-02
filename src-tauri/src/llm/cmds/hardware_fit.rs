// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Per-model hardware fit prediction.

use serde::Serialize;
use std::sync::{Mutex, OnceLock};

use crate::AppState;
use crate::MutexExt;

/// Cached `SystemSpecs` detection — detect once, reuse forever.
///
/// `SystemSpecs::detect()` spawns child processes (nvidia-smi, rocm-smi, …)
/// and reads sysfs/WMI, so we must not call it on every Tauri poll.
static SYSTEM_SPECS: OnceLock<llmfit_core::hardware::SystemSpecs> = OnceLock::new();

fn cached_system_specs() -> &'static llmfit_core::hardware::SystemSpecs {
    SYSTEM_SPECS.get_or_init(llmfit_core::hardware::SystemSpecs::detect)
}

/// Per-model hardware fit prediction returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelHardwareFit {
    /// Catalog filename (join key for the frontend).
    pub filename: String,
    /// `"perfect"` | `"good"` | `"marginal"` | `"too_tight"`
    pub fit_level: String,
    /// `"gpu"` | `"moe"` | `"cpu_gpu"` | `"cpu"`
    pub run_mode: String,
    /// Estimated memory required (GB).
    pub memory_required_gb: f64,
    /// Memory pool being used (GB).
    pub memory_available_gb: f64,
    /// Estimated tokens per second.
    pub estimated_tps: f64,
    /// Composite score 0–100.
    pub score: f64,
    /// Human-readable notes from the analyzer.
    pub notes: Vec<String>,
}

/// Parse a parameter count string from a family name like "4B", "27B", "270M".
fn parse_param_count(family_name: &str) -> (String, Option<u64>) {
    let mut best_label = String::from("7B");
    let mut best_raw: Option<u64> = None;

    let bytes = family_name.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let num_str = &family_name[start..i];
            if i < len
                && (bytes[i] == b'B' || bytes[i] == b'b' || bytes[i] == b'M' || bytes[i] == b'm')
            {
                let unit = (bytes[i] as char).to_uppercase().next().unwrap_or('B');
                if let Ok(num) = num_str.parse::<f64>() {
                    let raw = match unit {
                        'B' => (num * 1_000_000_000.0) as u64,
                        'M' => (num * 1_000_000.0) as u64,
                        _ => 0,
                    };
                    if best_raw.is_none() {
                        best_label = format!("{}{}", num_str, unit);
                        best_raw = Some(raw);
                    }
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    (best_label, best_raw)
}

/// Convert a catalog `LlmModelEntry` into an `llmfit_core::models::LlmModel`
/// for hardware-fit analysis.
fn catalog_entry_to_llm_model(
    entry: &crate::llm::catalog::LlmModelEntry,
) -> llmfit_core::models::LlmModel {
    let (param_count_str, parameters_raw) = parse_param_count(&entry.family_name);

    // Estimate min RAM from file size (GGUF size ≈ model weights; add ~0.5 GB overhead)
    let min_ram = (entry.size_gb as f64) + 0.5;
    let recommended_ram = min_ram * 1.3;

    // Detect MoE from tags or from family name (e.g. "35B-A3B")
    let name_lower = entry.family_name.to_lowercase();
    let is_moe = entry.tags.iter().any(|t| t == "moe")
        || (name_lower.contains("-a") && {
            let parts: Vec<&str> = entry.family_name.split('-').collect();
            parts.iter().any(|p| {
                let lower = p.to_lowercase();
                lower.starts_with('a')
                    && lower.len() > 1
                    && lower[1..].ends_with('b')
                    && lower[1..lower.len() - 1].parse::<f64>().is_ok()
            })
        });

    // Infer use_case from tags
    let use_case = if entry.tags.iter().any(|t| t == "coding") {
        "Coding"
    } else if entry.tags.iter().any(|t| t == "reasoning") {
        "Reasoning"
    } else if entry
        .tags
        .iter()
        .any(|t| t == "vision" || t == "multimodal")
    {
        "Multimodal"
    } else {
        "Chat"
    };

    llmfit_core::models::LlmModel {
        format: llmfit_core::models::ModelFormat::Gguf,
        name: entry.family_name.clone(),
        provider: entry
            .repo
            .split('/')
            .next()
            .unwrap_or("unknown")
            .to_string(),
        parameter_count: param_count_str,
        parameters_raw,
        min_ram_gb: min_ram,
        recommended_ram_gb: recommended_ram,
        min_vram_gb: Some(min_ram),
        quantization: entry.quant.clone(),
        context_length: 4096,
        use_case: use_case.to_string(),
        is_moe,
        num_experts: None,
        active_experts: None,
        active_parameters: None,
        release_date: None,
        gguf_sources: vec![],
        capabilities: vec![],
        num_attention_heads: None,
        num_key_value_heads: None,
        license: None,
    }
}

/// Lightweight fit verdict used by backend auto-launch guardrails.
#[derive(Debug, Clone)]
pub struct AutostartMemoryFit {
    pub fit_level: String,
    pub memory_required_gb: f64,
    pub memory_available_gb: f64,
}

impl AutostartMemoryFit {
    /// True when memory headroom is sufficient for automatic launch.
    pub fn enough_for_autostart(&self) -> bool {
        self.fit_level != "too_tight" && self.memory_available_gb >= self.memory_required_gb
    }
}

/// Analyze hardware fit for a single model entry.
pub fn model_autostart_memory_fit(
    entry: &crate::llm::catalog::LlmModelEntry,
) -> AutostartMemoryFit {
    let specs = cached_system_specs();
    let model = catalog_entry_to_llm_model(entry);
    let fit = llmfit_core::fit::ModelFit::analyze(&model, specs);

    let fit_level = match fit.fit_level {
        llmfit_core::fit::FitLevel::Perfect => "perfect",
        llmfit_core::fit::FitLevel::Good => "good",
        llmfit_core::fit::FitLevel::Marginal => "marginal",
        llmfit_core::fit::FitLevel::TooTight => "too_tight",
    };

    AutostartMemoryFit {
        fit_level: fit_level.to_string(),
        memory_required_gb: (fit.memory_required_gb * 10.0).round() / 10.0,
        memory_available_gb: (fit.memory_available_gb * 10.0).round() / 10.0,
    }
}

/// Predict hardware fit for all non-mmproj catalog entries.
///
/// Returns a list of `ModelHardwareFit` objects, one per model file, containing
/// fit level, run mode, estimated TPS, memory requirements, and notes from
/// `llmfit-core`'s `ModelFit::analyze`.
#[tauri::command]
pub fn get_model_hardware_fit(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<ModelHardwareFit> {
    let specs = cached_system_specs();
    let s = state.lock_or_recover();
    let __llm_arc = s.llm.clone();
    let llm = __llm_arc.lock_or_recover();

    llm.catalog
        .entries
        .iter()
        .filter(|e| !e.is_mmproj())
        .map(|entry| {
            let model = catalog_entry_to_llm_model(entry);
            let fit = llmfit_core::fit::ModelFit::analyze(&model, specs);

            let fit_level = match fit.fit_level {
                llmfit_core::fit::FitLevel::Perfect => "perfect",
                llmfit_core::fit::FitLevel::Good => "good",
                llmfit_core::fit::FitLevel::Marginal => "marginal",
                llmfit_core::fit::FitLevel::TooTight => "too_tight",
            };
            let run_mode = match fit.run_mode {
                llmfit_core::fit::RunMode::Gpu => "gpu",
                llmfit_core::fit::RunMode::MoeOffload => "moe",
                llmfit_core::fit::RunMode::CpuOffload => "cpu_gpu",
                llmfit_core::fit::RunMode::CpuOnly => "cpu",
                llmfit_core::fit::RunMode::TensorParallel => "tensor_parallel",
            };

            ModelHardwareFit {
                filename: entry.filename.clone(),
                fit_level: fit_level.to_string(),
                run_mode: run_mode.to_string(),
                memory_required_gb: (fit.memory_required_gb * 10.0).round() / 10.0,
                memory_available_gb: (fit.memory_available_gb * 10.0).round() / 10.0,
                estimated_tps: (fit.estimated_tps * 10.0).round() / 10.0,
                score: fit.score,
                notes: fit.notes,
            }
        })
        .collect()
}
