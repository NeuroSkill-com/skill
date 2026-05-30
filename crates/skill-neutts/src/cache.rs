//! Reference-code cache — avoids re-encoding the same WAV file twice.
//!
//! [`RefCodeCache`] uses the SHA-256 hash of the WAV file's raw bytes as a
//! cache key.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::npy;
use crate::NeuCodecEncoder;

/// Disk cache for pre-encoded NeuCodec reference codes.
pub struct RefCodeCache {
    dir: PathBuf,
}

impl RefCodeCache {
    /// Create a cache backed by the platform default cache directory.
    pub fn new() -> Result<Self> {
        let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".neutts_cache"));
        Self::with_dir(base.join("neutts").join("ref_codes"))
    }

    /// Create a cache backed by a specific directory.
    pub fn with_dir(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).with_context(|| format!("Cannot create cache directory: {}", dir.display()))?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn cache_path_for(&self, wav_path: &Path) -> Result<PathBuf> {
        let hash = sha256_file(wav_path)?;
        Ok(self.dir.join(format!("{hash}.npy")))
    }

    pub fn is_cached(&self, wav_path: &Path) -> Result<bool> {
        let path = self.cache_path_for(wav_path)?;
        Ok(path.exists())
    }

    pub fn try_load(&self, wav_path: &Path) -> Result<Option<(Vec<i32>, CacheOutcome)>> {
        let hash = sha256_file(wav_path).with_context(|| format!("Failed to hash: {}", wav_path.display()))?;
        let cache_file = self.dir.join(format!("{hash}.npy"));

        if cache_file.exists() {
            let codes = npy::load_npy_i32(&cache_file)
                .with_context(|| format!("Failed to load cached codes: {}", cache_file.display()))?;
            Ok(Some((codes, CacheOutcome::Hit { path: cache_file, hash })))
        } else {
            Ok(None)
        }
    }

    pub fn store(&self, wav_path: &Path, codes: &[i32]) -> Result<CacheOutcome> {
        let hash = sha256_file(wav_path).with_context(|| format!("Failed to hash: {}", wav_path.display()))?;
        let cache_file = self.dir.join(format!("{hash}.npy"));
        npy::write_npy_i32(&cache_file, codes)
            .with_context(|| format!("Failed to write cache: {}", cache_file.display()))?;
        Ok(CacheOutcome::Miss { path: cache_file, hash })
    }

    pub fn get_or_encode(&self, wav_path: &Path, encoder: &NeuCodecEncoder) -> Result<(Vec<i32>, CacheOutcome)> {
        if let Some(hit) = self.try_load(wav_path)? {
            return Ok(hit);
        }
        let codes = encoder
            .encode_wav(wav_path)
            .with_context(|| format!("Failed to encode: {}", wav_path.display()))?;
        let outcome = self.store(wav_path, &codes)?;
        Ok((codes, outcome))
    }

    pub fn evict(&self, wav_path: &Path) -> Result<bool> {
        let path = self.cache_path_for(wav_path)?;
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("Failed to evict cache entry: {}", path.display()))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn clear(&self) -> Result<usize> {
        let mut count = 0;
        for entry in
            std::fs::read_dir(&self.dir).with_context(|| format!("Cannot read cache dir: {}", self.dir.display()))?
        {
            let entry = entry.context("Failed to read dir entry")?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("npy") {
                std::fs::remove_file(&path).with_context(|| format!("Failed to remove: {}", path.display()))?;
                count += 1;
            }
        }
        Ok(count)
    }
}

/// Result of a [`RefCodeCache::get_or_encode`] call.
#[derive(Debug, Clone)]
pub enum CacheOutcome {
    Hit { path: PathBuf, hash: String },
    Miss { path: PathBuf, hash: String },
}

impl CacheOutcome {
    pub fn is_hit(&self) -> bool {
        matches!(self, Self::Hit { .. })
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Hit { path, .. } | Self::Miss { path, .. } => path,
        }
    }

    pub fn hash(&self) -> &str {
        match self {
            Self::Hit { hash, .. } | Self::Miss { hash, .. } => hash,
        }
    }
}

impl std::fmt::Display for CacheOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hit { hash, path } => write!(f, "cache hit  (sha256: {}…)  ← {}", &hash[..16], path.display()),
            Self::Miss { hash, path } => write!(f, "cache miss (sha256: {}…)  → {}", &hash[..16], path.display()),
        }
    }
}

/// Compute the SHA-256 hex digest of a file's raw bytes (streaming 64 KiB buffer).
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("Cannot open file for hashing: {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65_536];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("IO error while hashing: {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_dir() -> PathBuf {
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("skill_neutts_cache_test_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn test_sha256_deterministic() {
        let dir = tmp_dir();
        let path = dir.join("test.bin");
        std::fs::write(&path, b"hello neutts").unwrap();
        let h1 = sha256_file(&path).unwrap();
        let h2 = sha256_file(&path).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn test_store_then_load() {
        let dir = tmp_dir();
        let cache = RefCodeCache::with_dir(&dir).unwrap();
        let wav = dir.join("ref.wav");
        std::fs::write(&wav, b"fake wav content 123").unwrap();

        assert!(cache.try_load(&wav).unwrap().is_none());

        let codes: Vec<i32> = vec![1, 2, 3, 42, 1023];
        let outcome = cache.store(&wav, &codes).unwrap();
        assert!(!outcome.is_hit());

        let (loaded, outcome2) = cache.try_load(&wav).unwrap().unwrap();
        assert!(outcome2.is_hit());
        assert_eq!(loaded, codes);
        assert_eq!(outcome.path(), outcome2.path());
    }
}
