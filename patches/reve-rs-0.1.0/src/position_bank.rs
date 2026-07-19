/// REVE Position Bank — maps EEG channel names to 3D (x, y, z) coordinates.
///
/// The position bank is downloaded from HuggingFace on first use and cached locally.
/// It contains positions from the 92 datasets used during REVE pretraining.
///
/// URL: https://huggingface.co/brain-bzh/reve-positions/resolve/main/positions.json

use std::collections::HashMap;
#[cfg(feature = "hf-download")]
use std::path::PathBuf;

#[allow(dead_code)]
const POSITIONS_URL: &str =
    "https://huggingface.co/brain-bzh/reve-positions/resolve/main/positions.json";

/// Position bank mapping channel names → [x, y, z].
pub struct PositionBank {
    positions: HashMap<String, [f32; 3]>,
}

impl PositionBank {
    /// Load from a JSON file (map of channel_name → [x, y, z]).
    pub fn from_json(path: &str) -> anyhow::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let map: HashMap<String, [f32; 3]> = serde_json::from_str(&data)?;
        Ok(Self { positions: map })
    }

    /// Load from a JSON string.
    pub fn from_json_str(json: &str) -> anyhow::Result<Self> {
        let map: HashMap<String, [f32; 3]> = serde_json::from_str(json)?;
        Ok(Self { positions: map })
    }

    /// Download from HuggingFace and cache locally.
    #[cfg(feature = "hf-download")]
    pub fn download_and_cache(cache_dir: Option<&str>) -> anyhow::Result<Self> {
        let cache_path = match cache_dir {
            Some(dir) => PathBuf::from(dir).join("reve_positions.json"),
            None => {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(home).join(".cache").join("reve-rs").join("reve_positions.json")
            }
        };

        // Try cache first
        if cache_path.exists() {
            if let Ok(bank) = Self::from_json(cache_path.to_str().unwrap()) {
                return Ok(bank);
            }
        }

        // Download
        let resp = ureq::get(POSITIONS_URL).call()?;
        let body = resp.into_body().read_to_string()?;

        // Cache
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&cache_path, &body)?;

        Self::from_json_str(&body)
    }

    /// Look up positions for a list of channel names.
    /// Returns [C, 3] flat Vec<f32>.
    pub fn get_positions(&self, channel_names: &[&str]) -> Vec<f32> {
        let mut result = Vec::with_capacity(channel_names.len() * 3);
        for name in channel_names {
            if let Some(pos) = self.positions.get(*name) {
                result.extend_from_slice(pos);
            } else {
                eprintln!("Warning: channel '{}' not found in position bank, using [0,0,0]", name);
                result.extend_from_slice(&[0.0, 0.0, 0.0]);
            }
        }
        result
    }

    /// Get all available channel names.
    pub fn channel_names(&self) -> Vec<&str> {
        self.positions.keys().map(|s| s.as_str()).collect()
    }

    /// Number of channels in the bank.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}
