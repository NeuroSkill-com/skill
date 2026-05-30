//! Phonemisation via the [`espeak-ng`](https://crates.io/crates/espeak-ng) crate.
//!
//! All public functions compile regardless of the `espeak` feature; without it
//! they return informative errors so callers that use
//! [`NeuTTS::infer_from_ipa`](crate::model::NeuTTS::infer_from_ipa) directly
//! still compile without the espeak dependency.

use std::path::{Path, PathBuf};

#[cfg(not(feature = "espeak"))]
use anyhow::anyhow;
use anyhow::Result;
use once_cell::sync::OnceCell;

static DATA_PATH: OnceCell<PathBuf> = OnceCell::new();

/// Override the espeak-ng data directory (optional with bundled-data feature).
pub fn set_data_path(path: &Path) {
    let _ = DATA_PATH.set(path.to_path_buf());
}

#[cfg(feature = "espeak")]
mod inner {
    use std::path::PathBuf;

    use anyhow::{anyhow, Result};
    use once_cell::sync::OnceCell;

    use super::DATA_PATH;

    static BUNDLED_DATA_DIR: OnceCell<PathBuf> = OnceCell::new();

    fn get_data_dir() -> Result<&'static PathBuf> {
        if let Some(user_dir) = DATA_PATH.get() {
            return Ok(BUNDLED_DATA_DIR.get_or_init(|| user_dir.clone()));
        }
        BUNDLED_DATA_DIR.get_or_try_init(|| {
            if let Ok(p) = std::env::var("NEUTTS_ESPEAK_DATA_DIR") {
                if !p.is_empty() {
                    let path = PathBuf::from(p);
                    if path.is_dir() {
                        return Ok(path);
                    }
                    return Err(anyhow!("NEUTTS_ESPEAK_DATA_DIR is not a directory: {}", path.display()));
                }
            }
            let cache_dir = std::env::temp_dir().join("neutts-espeak-ng-data");
            std::fs::create_dir_all(&cache_dir).map_err(|e| anyhow!("Failed to create espeak-ng data dir: {}", e))?;
            espeak_ng::install_bundled_data(&cache_dir)
                .map_err(|e| anyhow!("Failed to install bundled espeak-ng data: {}", e))?;
            Ok(cache_dir)
        })
    }

    fn map_lang(lang: &str) -> &str {
        match lang {
            "en-us" => "en",
            "fr-fr" => "fr",
            other => other,
        }
    }

    fn create_engine(lang: &str) -> Result<espeak_ng::EspeakNg> {
        let data_dir = get_data_dir()?;
        let mapped = map_lang(lang);
        espeak_ng::EspeakNg::with_data_dir(mapped, data_dir)
            .map_err(|e| anyhow!("espeak-ng init for '{}' failed: {}", lang, e))
    }

    pub(super) fn is_available(lang: &str) -> bool {
        create_engine(lang).is_ok()
    }

    pub(super) fn run_phonemize(text: &str, lang: &str) -> Result<String> {
        if text.is_empty() {
            return Ok(String::new());
        }
        let engine = create_engine(lang)?;
        let ipa = engine
            .text_to_phonemes(text)
            .map_err(|e| anyhow!("espeak-ng phonemise failed: {}", e))?;
        Ok(ipa.trim().to_owned())
    }
}

/// Returns `true` if espeak-ng is available for the given language code.
pub fn is_espeak_available(lang: &str) -> bool {
    #[cfg(feature = "espeak")]
    {
        inner::is_available(lang)
    }
    #[cfg(not(feature = "espeak"))]
    {
        let _ = lang;
        false
    }
}

/// Convert `text` to IPA phonemes using the espeak-ng voice for `lang`.
///
/// **Requires the `espeak` Cargo feature.**
pub fn phonemize(text: &str, lang: &str) -> Result<String> {
    #[cfg(feature = "espeak")]
    {
        let raw = inner::run_phonemize(text, lang)?;
        let cleaned = if lang.starts_with("fr") {
            raw.replace('-', "")
        } else {
            raw
        };
        let tokens: Vec<&str> = cleaned.split_whitespace().collect();
        Ok(tokens.join(" "))
    }
    #[cfg(not(feature = "espeak"))]
    {
        let _ = (text, lang);
        Err(anyhow!(
            "phonemize() requires the `espeak` Cargo feature.\n\
             Enable it or use NeuTTS::infer_from_ipa() to bypass phonemisation."
        ))
    }
}
