//! Attention tensor layout (BSNH vs BHSD) — shared by encoder graph and parity tests.

use rlx::Device;

/// Q/K/V layout for `attention_kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttnLayout {
    /// `[B, seq, heads, dim]` — default for CPU, wgpu, MLX, CUDA, …
    Bsnh,
    /// `[B, heads, seq, dim]` — default for native Metal SDPA / MPSGraph.
    Bhsd,
}

/// Resolve layout from `BRAINJEPA_ATTN_LAYOUT` or per-backend defaults.
pub fn resolve_attn_layout(device: Device) -> anyhow::Result<AttnLayout> {
    match std::env::var("BRAINJEPA_ATTN_LAYOUT")
        .ok()
        .as_deref()
        .map(str::to_ascii_lowercase)
    {
        Some(v) if v == "bhsd" => Ok(AttnLayout::Bhsd),
        Some(v) if v == "bsnh" => Ok(AttnLayout::Bsnh),
        Some(other) => {
            anyhow::bail!("invalid BRAINJEPA_ATTN_LAYOUT={other:?} (expected bsnh or bhsd)")
        }
        None if matches!(device, Device::Metal) => Ok(AttnLayout::Bhsd),
        None => Ok(AttnLayout::Bsnh),
    }
}
