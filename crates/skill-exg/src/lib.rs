// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Pure EEG embedding helpers extracted from `eeg_embeddings.rs`.
//!
//! Everything here is **Tauri-free**: cosine distance, fuzzy text matching,
//! UTC timestamp formatting, HuggingFace weight resolution and download,
//! cubecl GPU-cache setup, epoch metrics derivation, and panic helpers.

use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::time::SystemTime;

use anyhow::Context;
use skill_constants::{LUNA_CONFIG_FILE, ZUNA_CONFIG_FILE, ZUNA_WEIGHTS_FILE};
use skill_data::util::MutexExt;
use skill_eeg::eeg_bands::BandSnapshot;
use skill_eeg::eeg_model_config::EegModelStatus;

// ── Cosine distance ───────────────────────────────────────────────────────────

/// Cosine distance between two `f32` vectors (0 = identical, 2 = opposite).
///
/// Returns `2.0` on dimension mismatch or zero-norm inputs.
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 2.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (&av, &bv) in a.iter().zip(b.iter()) {
        dot += av * bv;
        na += av * av;
        nb += bv * bv;
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON {
        return 2.0;
    }
    let sim = dot / (na.sqrt() * nb.sqrt());
    1.0 - sim.clamp(-1.0, 1.0)
}

// ── Fuzzy text matching ───────────────────────────────────────────────────────

fn normalize_text(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace())
        .collect::<String>()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    if a_chars.is_empty() {
        return b_chars.len();
    }
    if b_chars.is_empty() {
        return a_chars.len();
    }

    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut curr = vec![0usize; b_chars.len() + 1];

    for (i, &ac) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &bc) in b_chars.iter().enumerate() {
            let cost = if ac == bc { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_chars.len()]
}

/// Fuzzy match a keyword against a candidate string.
///
/// Returns `true` if one contains the other (substring), or if the
/// normalised Levenshtein ratio is ≤ 0.32.
pub fn fuzzy_match(keyword: &str, candidate: &str) -> bool {
    let k = normalize_text(keyword);
    let c = normalize_text(candidate);
    if k.is_empty() || c.is_empty() {
        return false;
    }
    if c.contains(&k) || k.contains(&c) {
        return true;
    }
    let dist = levenshtein(&k, &c) as f32;
    let max_len = k.chars().count().max(c.chars().count()) as f32;
    (dist / max_len) <= 0.32
}

// ── UTC timestamp helpers ─────────────────────────────────────────────────────

// Delegated to `skill_data::util`.  Re-exported here for backward compat.
pub use skill_data::util::{yyyymmdd_utc, yyyymmddhhmmss_utc};

// ── Safetensors integrity check ───────────────────────────────────────────────

/// Validate that a `.safetensors` file has a complete header and the file size
/// covers all declared tensor data.  Returns `true` if the file looks intact.
///
/// Does NOT load the full file — only reads the 8-byte length prefix and the
/// JSON header to compute the expected minimum size.
pub fn validate_safetensors(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let file_size = f.metadata().map(|m| m.len()).unwrap_or(0);
    if file_size < 8 {
        return false;
    }
    let mut len_buf = [0u8; 8];
    if f.read_exact(&mut len_buf).is_err() {
        return false;
    }
    let header_len = u64::from_le_bytes(len_buf);
    // Sanity: header shouldn't be > 100 MB or exceed file size
    if header_len > 100_000_000 || 8 + header_len > file_size {
        return false;
    }
    let mut header_buf = vec![0u8; header_len as usize];
    if f.read_exact(&mut header_buf).is_err() {
        return false;
    }
    let Ok(header) = serde_json::from_slice::<serde_json::Value>(&header_buf) else {
        return false;
    };
    let Some(obj) = header.as_object() else { return false };
    // Find the maximum data offset across all tensors
    let mut max_offset: u64 = 0;
    for (key, val) in obj {
        if key == "__metadata__" { continue; }
        if let Some(offsets) = val.get("data_offsets").and_then(|v| v.as_array()) {
            if let Some(end) = offsets.get(1).and_then(|v| v.as_u64()) {
                max_offset = max_offset.max(end);
            }
        }
    }
    let expected = 8 + header_len + max_offset;
    // Burn's safetensors loader requires exact size match.
    // Allow a small tolerance (< 16 bytes) for alignment padding,
    // but reject files with significant extra data — they indicate
    // a corrupt or incomplete download.
    file_size >= expected && (file_size - expected) < 16
}

/// Validate a weights file; if it's corrupt, remove the blob and snapshot
/// symlink so the next download attempt starts fresh.
pub fn validate_or_remove(weights_path: &Path) -> bool {
    if !weights_path.exists() {
        return false;
    }
    let ext = weights_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    // Only validate safetensors files; .pth / .bin / .gguf have different formats
    if ext != "safetensors" {
        return true;
    }
    if validate_safetensors(weights_path) {
        return true;
    }
    eprintln!("[exg] corrupt safetensors file, removing: {}", weights_path.display());
    // Resolve symlink target (the blob) and remove both
    if let Ok(blob) = std::fs::read_link(weights_path) {
        // blob is relative to the symlink's parent
        let resolved = weights_path.parent().map(|p| p.join(&blob)).unwrap_or(blob);
        let _ = std::fs::remove_file(&resolved);
    }
    let _ = std::fs::remove_file(weights_path);
    false
}

// ── HuggingFace weight resolution ─────────────────────────────────────────────

/// Find ZUNA weights in the HuggingFace disk cache for the given `hf_repo`.
pub fn resolve_hf_weights(hf_repo: &str) -> Option<(PathBuf, PathBuf)> {
    let snaps = skill_data::util::hf_model_dir(hf_repo).join("snapshots");
    let mut dirs: Vec<_> = std::fs::read_dir(&snaps)
        .ok()?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    dirs.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    for snap in dirs.into_iter().rev() {
        let w = snap.path().join(ZUNA_WEIGHTS_FILE);
        let c = snap.path().join(ZUNA_CONFIG_FILE);
        if validate_or_remove(&w) && c.exists() {
            return Some((w, c));
        }
    }
    None
}

/// Public alias for [`resolve_hf_weights`] (backwards compatibility).
pub fn probe_hf_weights(hf_repo: &str) -> Option<(PathBuf, PathBuf)> {
    resolve_hf_weights(hf_repo)
}

/// Find LUNA weights in the HuggingFace disk cache for the given `hf_repo`
/// and `weights_file` (e.g. `LUNA_base.safetensors`).
pub fn resolve_luna_weights(hf_repo: &str, weights_file: &str) -> Option<(PathBuf, PathBuf)> {
    let snaps = skill_data::util::hf_model_dir(hf_repo).join("snapshots");
    let mut dirs: Vec<_> = std::fs::read_dir(&snaps)
        .ok()?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    dirs.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    for snap in dirs.into_iter().rev() {
        let w = snap.path().join(weights_file);
        let c = snap.path().join(LUNA_CONFIG_FILE);
        if validate_or_remove(&w) && c.exists() {
            return Some((w, c));
        }
    }
    None
}

/// Register a completed blob in the HF Hub snapshot directory structure.
///
/// Returns the snapshot path that `resolve_hf_weights` will find.
pub fn register_hf_snapshot(
    model_dir: &Path,
    refs_dir: &Path,
    commit_sha: &str,
    filename: &str,
    blob_path: &Path,
) -> anyhow::Result<PathBuf> {
    std::fs::write(refs_dir.join("main"), commit_sha).context("write refs/main")?;

    let snapshot_dir = model_dir.join("snapshots").join(commit_sha);
    std::fs::create_dir_all(&snapshot_dir).context("create snapshot dir")?;

    let snapshot_link = snapshot_dir.join(filename);
    if snapshot_link.exists() || snapshot_link.symlink_metadata().is_ok() {
        std::fs::remove_file(&snapshot_link).ok();
    }

    #[cfg(unix)]
    {
        let blob_name = blob_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let depth = std::path::Path::new(filename).components().count();
        let parents = "../".repeat(depth + 1);
        let rel_target = format!("{parents}blobs/{blob_name}");
        std::os::unix::fs::symlink(&rel_target, &snapshot_link).context("create symlink")?;
    }

    #[cfg(windows)]
    std::fs::hard_link(blob_path, &snapshot_link).context("create hardlink")?;

    #[cfg(not(any(unix, windows)))]
    std::fs::copy(blob_path, &snapshot_link).context("copy blob to snapshot")?;

    Ok(snapshot_link)
}

/// Download ZUNA weights from HuggingFace Hub with resumable streaming.
///
/// Progress is reported through `status`. Cancellation is honoured via `cancel`.
/// When `mark_needs_restart` is `true`, sets `download_needs_restart` on completion.
///
/// Log messages are printed to stderr via `eprintln!`.
pub fn download_hf_weights(
    hf_repo: &str,
    status: &Arc<Mutex<EegModelStatus>>,
    cancel: &Arc<AtomicBool>,
    mark_needs_restart: bool,
) -> Option<(PathBuf, PathBuf)> {
    download_hf_weights_files(
        hf_repo,
        ZUNA_WEIGHTS_FILE,
        ZUNA_CONFIG_FILE,
        status,
        cancel,
        mark_needs_restart,
    )
}

/// Generic download: fetches `weights_file` and `config_file` from `hf_repo`.
pub fn download_hf_weights_files(
    hf_repo: &str,
    weights_file: &str,
    config_file: &str,
    status: &Arc<Mutex<EegModelStatus>>,
    cancel: &Arc<AtomicBool>,
    mark_needs_restart: bool,
) -> Option<(PathBuf, PathBuf)> {
    use hf_hub::api::sync::Api;
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering;

    let endpoint = std::env::var("HF_ENDPOINT").unwrap_or_else(|_| "https://huggingface.co".into());

    eprintln!("[embedder] weights not in cache — downloading from HuggingFace: {hf_repo}/{weights_file}");

    {
        let mut st = status.lock_or_recover();
        st.downloading_weights = true;
        st.download_needs_restart = false;
        st.download_progress = 0.0;
        st.download_status_msg = Some(format!("Connecting to HuggingFace ({hf_repo})…"));
    }

    let api = match Api::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[embedder] hf-hub Api::new() failed: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("HF Hub init failed: {e}"));
            return None;
        }
    };
    let repo = api.model(hf_repo.to_string());

    {
        let mut st = status.lock_or_recover();
        st.download_status_msg = Some(format!("Downloading {config_file}…"));
    }
    let config_path = match repo.get(config_file) {
        Ok(p) => {
            eprintln!("[embedder] ✓ {config_file} → {}", p.display());
            p
        }
        Err(e) => {
            eprintln!("[embedder] failed to download {config_file}: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("Download failed ({config_file}): {e}"));
            return None;
        }
    };

    if cancel.load(Ordering::Relaxed) {
        eprintln!("[embedder] download cancelled by user after config.json");
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_progress = 0.0;
        st.download_status_msg = Some("Download cancelled.".to_string());
        return None;
    }

    let (model_dir, blobs_dir, refs_dir) = match skill_data::util::hf_ensure_dirs(hf_repo) {
        Ok(dirs) => dirs,
        Err(e) => {
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("Failed to create cache dirs: {e}"));
            return None;
        }
    };

    {
        let mut st = status.lock_or_recover();
        st.download_status_msg = Some(format!("Fetching metadata for {weights_file}…"));
    }

    let hf_token = std::env::var("HF_TOKEN")
        .ok()
        .or_else(|| std::env::var("HUGGING_FACE_HUB_TOKEN").ok());

    let meta_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .into();
    let dl_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(30)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(300)))
        .build()
        .into();

    let api_url = format!("{endpoint}/api/models/{hf_repo}?blobs=1");
    let meta_req = meta_agent.get(&api_url);
    let meta_req = if let Some(tok) = &hf_token {
        meta_req.header("Authorization", format!("Bearer {tok}"))
    } else {
        meta_req
    };
    let api_resp = match meta_req.header("User-Agent", "skill-app/1.0").call() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[embedder] HF metadata API error: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("Metadata fetch failed: {e}"));
            return None;
        }
    };

    let info: serde_json::Value = match api_resp.into_body().read_json() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[embedder] HF metadata JSON parse error: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("Metadata parse failed: {e}"));
            return None;
        }
    };

    let commit_sha = info["sha"].as_str().unwrap_or("main").to_string();

    let file_meta = info["siblings"]
        .as_array()
        .and_then(|s| s.iter().find(|e| e["rfilename"].as_str() == Some(weights_file)));

    let (blob_sha, remote_size) = match file_meta {
        Some(m) => {
            let sha = m["lfs"]["sha256"]
                .as_str()
                .map(|s| s.trim_start_matches("sha256:").to_string());
            let size = m["lfs"]["size"].as_u64().or_else(|| m["size"].as_u64());
            match (sha, size) {
                (Some(s), Some(n)) => (s, n),
                _ => {
                    eprintln!("[embedder] LFS metadata missing for {weights_file}, falling back to hf_hub");
                    {
                        let mut st = status.lock_or_recover();
                        st.download_status_msg = Some(format!("Downloading {weights_file}…"));
                    }
                    let weights_path = match repo.get(weights_file) {
                        Ok(p) => p,
                        Err(e) => {
                            let mut st = status.lock_or_recover();
                            st.downloading_weights = false;
                            st.download_progress = 0.0;
                            st.download_status_msg = Some(format!("Download failed ({weights_file}): {e}"));
                            return None;
                        }
                    };
                    let mut st = status.lock_or_recover();
                    st.downloading_weights = false;
                    st.download_progress = 1.0;
                    st.download_status_msg = None;
                    st.weights_found = true;
                    st.weights_path = Some(weights_path.display().to_string());
                    st.download_needs_restart = mark_needs_restart;
                    return Some((weights_path, config_path));
                }
            }
        }
        None => {
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("{weights_file}: not listed in {hf_repo} manifest"));
            return None;
        }
    };

    let blob_path = blobs_dir.join(&blob_sha);
    let incomplete_path = blobs_dir.join(format!("{blob_sha}.incomplete"));

    if blob_path.exists() && blob_path.metadata().map(|m| m.len()).unwrap_or(0) >= remote_size {
        eprintln!("[embedder] ✓ {weights_file} already in blob cache");
        let weights_path = match register_hf_snapshot(&model_dir, &refs_dir, &commit_sha, weights_file, &blob_path) {
            Ok(p) => p,
            Err(e) => {
                let mut st = status.lock_or_recover();
                st.downloading_weights = false;
                st.download_progress = 0.0;
                st.download_status_msg = Some(format!("Snapshot registration failed: {e}"));
                return None;
            }
        };
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_progress = 1.0;
        st.download_status_msg = None;
        st.weights_found = true;
        st.weights_path = Some(weights_path.display().to_string());
        st.download_needs_restart = mark_needs_restart;
        return Some((weights_path, config_path));
    }

    let resume_from: u64 = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);

    {
        let mut st = status.lock_or_recover();
        st.download_progress = (resume_from as f32 / remote_size.max(1) as f32).min(0.99);
        st.download_status_msg = Some(if resume_from > 0 {
            format!(
                "Resuming {weights_file} from {:.0} / {:.0} MB…",
                resume_from as f64 / 1_048_576.0,
                remote_size as f64 / 1_048_576.0
            )
        } else {
            format!(
                "Downloading {weights_file} ({:.0} MB)…",
                remote_size as f64 / 1_048_576.0
            )
        });
    }

    let file_url = format!("{endpoint}/{hf_repo}/resolve/main/{weights_file}");
    let dl_req = dl_agent.get(&file_url);
    let dl_req = if let Some(tok) = &hf_token {
        dl_req.header("Authorization", format!("Bearer {tok}"))
    } else {
        dl_req
    };
    let mut get = dl_req.header("User-Agent", "skill-app/1.0");
    if resume_from > 0 {
        get = get.header("Range", format!("bytes={resume_from}-"));
    }

    let resp = match get.call() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[embedder] HTTP error downloading {weights_file}: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("Download failed: {e}"));
            return None;
        }
    };

    let http_status = resp.status();
    let writing_from = if http_status == 206 { resume_from } else { 0 };

    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(writing_from > 0)
        .truncate(writing_from == 0)
        .open(&incomplete_path)
    {
        Ok(f) => f,
        Err(e) => {
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("Cannot open temp file: {e}"));
            return None;
        }
    };

    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 128 * 1024];
    let mut written = writing_from;
    let total = remote_size.max(1);

    loop {
        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("[embedder] read error: {e}");
                let mut st = status.lock_or_recover();
                st.downloading_weights = false;
                st.download_progress = 0.0;
                st.download_status_msg = Some(format!("Read error: {e}"));
                return None;
            }
        };
        if n == 0 {
            break;
        }

        if let Err(e) = file.write_all(&buf[..n]) {
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("Write error: {e}"));
            return None;
        }
        written += n as u64;

        {
            let mut st = status.lock_or_recover();
            st.download_progress = (written as f32 / total as f32).min(0.99);
            st.download_status_msg = Some(format!(
                "{:.0} / {:.0} MB",
                written as f64 / 1_048_576.0,
                total as f64 / 1_048_576.0
            ));
        }

        if cancel.load(Ordering::Relaxed) {
            eprintln!("[embedder] download cancelled by user");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some("Download cancelled.".to_string());
            return None;
        }
    }
    drop(file);

    let final_size = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);
    if final_size < remote_size {
        eprintln!("[embedder] incomplete download: {final_size} < {remote_size} bytes");
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_progress = final_size as f32 / remote_size as f32;
        st.download_status_msg = Some(format!(
            "Incomplete download ({final_size} / {remote_size} bytes) — retry to resume."
        ));
        return None;
    }

    if let Err(e) = std::fs::rename(&incomplete_path, &blob_path) {
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_progress = 0.0;
        st.download_status_msg = Some(format!("Failed to finalise download: {e}"));
        return None;
    }

    let weights_path = match register_hf_snapshot(&model_dir, &refs_dir, &commit_sha, weights_file, &blob_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[embedder] snapshot registration failed: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress = 0.0;
            st.download_status_msg = Some(format!("Snapshot registration failed: {e}"));
            return None;
        }
    };

    {
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_progress = 1.0;
        st.download_status_msg = None;
        st.weights_found = true;
        st.weights_path = Some(weights_path.display().to_string());
        st.download_needs_restart = mark_needs_restart;
    }
    eprintln!(
        "[embedder] weights downloaded successfully → {}",
        weights_path.display()
    );
    Some((weights_path, config_path))
}

// ── cubecl cache warm-up ──────────────────────────────────────────────────────

/// Pre-create the cubecl GPU-kernel cache directory and configure the
/// `GlobalConfig` so cubecl never tries to write to an inaccessible path.
///
/// Must be called **before** the first `WgpuDevice` access.
pub fn configure_cubecl_cache(skill_dir: &Path) {
    use cubecl_runtime::config::{cache::CacheConfig, GlobalConfig};
    use std::sync::atomic::{AtomicBool, Ordering};

    static CUBECL_CONFIGURED: AtomicBool = AtomicBool::new(false);

    let cache_dir = skill_dir.join("cubecl_cache");
    match std::fs::create_dir_all(&cache_dir) {
        Ok(_) => eprintln!("[embedder] cubecl cache dir: {}", cache_dir.display()),
        Err(e) => eprintln!("[embedder] warn: cubecl cache mkdir {}: {e}", cache_dir.display()),
    }

    if CUBECL_CONFIGURED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        let mut cfg = GlobalConfig::default();
        cfg.autotune.cache = CacheConfig::File(cache_dir);
        GlobalConfig::set(cfg);
    }
}

// ── GPU panic flag ────────────────────────────────────────────────────────────

/// Process-global flag: set to `true` after any GPU panic so respawned workers
/// skip wgpu device usage (whose internal mutexes are permanently poisoned).
pub static GPU_DEVICE_POISONED: AtomicBool = AtomicBool::new(false);

/// Extract a human-readable message from a caught panic payload.
pub fn panic_msg(payload: &Box<dyn std::any::Any + Send>) -> &str {
    payload
        .downcast_ref::<String>()
        .map(std::string::String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("(non-string panic payload)")
}

// ── Epoch metrics ─────────────────────────────────────────────────────────────

/// Per-epoch band-derived metrics stored alongside each embedding.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EpochMetrics {
    pub rel_delta: f32,
    pub rel_theta: f32,
    pub rel_alpha: f32,
    pub rel_beta: f32,
    pub rel_gamma: f32,
    pub rel_high_gamma: f32,
    pub relaxation: f32,
    pub engagement: f32,
    pub faa: f32,
    pub tar: f32,
    pub bar: f32,
    pub dtr: f32,
    pub pse: f32,
    pub apf: f32,
    pub bps: f32,
    pub snr: f32,
    pub coherence: f32,
    pub mu_suppression: f32,
    pub mood: f32,
    pub tbr: f32,
    pub sef95: f32,
    pub spectral_centroid: f32,
    pub hjorth_activity: f32,
    pub hjorth_mobility: f32,
    pub hjorth_complexity: f32,
    pub permutation_entropy: f32,
    pub higuchi_fd: f32,
    pub dfa_exponent: f32,
    pub sample_entropy: f32,
    pub pac_theta_gamma: f32,
    pub laterality_index: f32,
    pub hr: f64,
    pub rmssd: f64,
    pub sdnn: f64,
    pub pnn50: f64,
    pub lf_hf_ratio: f64,
    pub respiratory_rate: f64,
    pub spo2_estimate: f64,
    pub perfusion_index: f64,
    pub stress_index: f64,
    pub blink_count: u64,
    pub blink_rate: f64,
    pub head_pitch: f64,
    pub head_roll: f64,
    pub stillness: f64,
    pub nod_count: u64,
    pub shake_count: u64,
    pub meditation: f64,
    pub cognitive_load: f64,
    pub drowsiness: f64,
    pub headache_index: f32,
    pub migraine_index: f32,
    pub consciousness_lzc: f32,
    pub consciousness_wakefulness: f32,
    pub consciousness_integration: f32,
}

impl EpochMetrics {
    /// Derive metrics from a `BandSnapshot` by averaging across all channels.
    pub fn from_snapshot(snap: &BandSnapshot) -> Self {
        let n = snap.channels.len() as f32;
        if n < 1.0 {
            return Self::default();
        }

        let mut rd = 0.0f32;
        let mut rt = 0.0f32;
        let mut ra = 0.0f32;
        let mut rb = 0.0f32;
        let mut rg = 0.0f32;
        let mut rhg = 0.0f32;
        let mut sum_relax = 0.0f32;
        let mut sum_engage = 0.0f32;

        for ch in &snap.channels {
            rd += ch.rel_delta;
            rt += ch.rel_theta;
            ra += ch.rel_alpha;
            rb += ch.rel_beta;
            rg += ch.rel_gamma;
            rhg += ch.rel_high_gamma;
            let a = ch.rel_alpha;
            let b = ch.rel_beta;
            let t = ch.rel_theta;
            let d1 = a + t;
            let d2 = b + t;
            if d2 > 1e-6 {
                sum_relax += a / d2;
            }
            if d1 > 1e-6 {
                sum_engage += b / d1;
            }
        }
        rd /= n;
        rt /= n;
        ra /= n;
        rb /= n;
        rg /= n;
        rhg /= n;

        let faa = if snap.channels.len() >= 3 {
            let af7_alpha = snap.channels[1].alpha.max(1e-6);
            let af8_alpha = snap.channels[2].alpha.max(1e-6);
            af8_alpha.ln() - af7_alpha.ln()
        } else {
            0.0
        };

        Self {
            rel_delta: rd,
            rel_theta: rt,
            rel_alpha: ra,
            rel_beta: rb,
            rel_gamma: rg,
            rel_high_gamma: rhg,
            relaxation: Self::sigmoid100(sum_relax / n, 2.5, 1.0),
            engagement: Self::sigmoid100(sum_engage / n, 2.0, 0.8),
            faa,
            tar: snap.tar,
            bar: snap.bar,
            dtr: snap.dtr,
            pse: snap.pse,
            apf: snap.apf,
            bps: snap.bps,
            snr: snap.snr,
            coherence: snap.coherence,
            mu_suppression: snap.mu_suppression,
            mood: snap.mood,
            tbr: snap.tbr,
            sef95: snap.sef95,
            spectral_centroid: snap.spectral_centroid,
            hjorth_activity: snap.hjorth_activity,
            hjorth_mobility: snap.hjorth_mobility,
            hjorth_complexity: snap.hjorth_complexity,
            permutation_entropy: snap.permutation_entropy,
            higuchi_fd: snap.higuchi_fd,
            dfa_exponent: snap.dfa_exponent,
            sample_entropy: snap.sample_entropy,
            pac_theta_gamma: snap.pac_theta_gamma,
            laterality_index: snap.laterality_index,
            hr: 0.0,
            rmssd: 0.0,
            sdnn: 0.0,
            pnn50: 0.0,
            lf_hf_ratio: 0.0,
            respiratory_rate: 0.0,
            spo2_estimate: 0.0,
            perfusion_index: 0.0,
            stress_index: 0.0,
            blink_count: snap.blink_count.unwrap_or(0),
            blink_rate: snap.blink_rate.unwrap_or(0.0),
            head_pitch: snap.head_pitch.unwrap_or(0.0),
            head_roll: snap.head_roll.unwrap_or(0.0),
            stillness: snap.stillness.unwrap_or(0.0),
            nod_count: snap.nod_count.unwrap_or(0),
            shake_count: snap.shake_count.unwrap_or(0),
            meditation: snap.meditation.unwrap_or(0.0),
            cognitive_load: snap.cognitive_load.unwrap_or(0.0),
            drowsiness: snap.drowsiness.unwrap_or(0.0),
            headache_index: snap.headache_index,
            migraine_index: snap.migraine_index,
            consciousness_lzc: snap.consciousness_lzc,
            consciousness_wakefulness: snap.consciousness_wakefulness,
            consciousness_integration: snap.consciousness_integration,
        }
    }

    /// Sigmoid mapping (0, ∞) → (0, 100).
    pub fn sigmoid100(x: f32, k: f32, mid: f32) -> f32 {
        100.0 / (1.0 + (-k * (x - mid)).exp())
    }
}

impl Default for EpochMetrics {
    fn default() -> Self {
        Self {
            rel_delta: 0.0,
            rel_theta: 0.0,
            rel_alpha: 0.0,
            rel_beta: 0.0,
            rel_gamma: 0.0,
            rel_high_gamma: 0.0,
            relaxation: 0.0,
            engagement: 0.0,
            faa: 0.0,
            tar: 0.0,
            bar: 0.0,
            dtr: 0.0,
            pse: 0.0,
            apf: 0.0,
            bps: 0.0,
            snr: 0.0,
            coherence: 0.0,
            mu_suppression: 1.0,
            mood: 50.0,
            tbr: 0.0,
            sef95: 0.0,
            spectral_centroid: 0.0,
            hjorth_activity: 0.0,
            hjorth_mobility: 0.0,
            hjorth_complexity: 0.0,
            permutation_entropy: 0.0,
            higuchi_fd: 0.0,
            dfa_exponent: 0.0,
            sample_entropy: 0.0,
            pac_theta_gamma: 0.0,
            laterality_index: 0.0,
            hr: 0.0,
            rmssd: 0.0,
            sdnn: 0.0,
            pnn50: 0.0,
            lf_hf_ratio: 0.0,
            respiratory_rate: 0.0,
            spo2_estimate: 0.0,
            perfusion_index: 0.0,
            stress_index: 0.0,
            blink_count: 0,
            blink_rate: 0.0,
            head_pitch: 0.0,
            head_roll: 0.0,
            stillness: 0.0,
            nod_count: 0,
            shake_count: 0,
            meditation: 0.0,
            cognitive_load: 0.0,
            drowsiness: 0.0,
            headache_index: 0.0,
            migraine_index: 0.0,
            consciousness_lzc: 0.0,
            consciousness_wakefulness: 0.0,
            consciousness_integration: 0.0,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── cosine_distance ───────────────────────────────────────────────────

    #[test]
    fn cosine_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_distance(&v, &v) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_opposite_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_distance(&a, &b) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine_distance(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_empty_returns_2() {
        assert_eq!(cosine_distance(&[], &[]), 2.0);
    }

    #[test]
    fn cosine_mismatched_lengths_returns_2() {
        assert_eq!(cosine_distance(&[1.0, 2.0], &[1.0]), 2.0);
    }

    #[test]
    fn cosine_zero_vector_returns_2() {
        assert_eq!(cosine_distance(&[0.0, 0.0], &[1.0, 1.0]), 2.0);
    }

    // ── fuzzy_match ───────────────────────────────────────────────────────

    #[test]
    fn fuzzy_exact_match() {
        assert!(fuzzy_match("meditation", "meditation"));
    }

    #[test]
    fn fuzzy_case_insensitive() {
        assert!(fuzzy_match("Meditation", "meditation"));
    }

    #[test]
    fn fuzzy_substring_match() {
        assert!(fuzzy_match("med", "meditation"));
    }

    #[test]
    fn fuzzy_reverse_substring() {
        assert!(fuzzy_match("meditation session", "meditation"));
    }

    #[test]
    fn fuzzy_close_typo() {
        assert!(fuzzy_match("meditatoin", "meditation")); // transposition
    }

    #[test]
    fn fuzzy_no_match() {
        assert!(!fuzzy_match("completely different", "meditation"));
    }

    #[test]
    fn fuzzy_empty_keyword() {
        assert!(!fuzzy_match("", "meditation"));
    }

    #[test]
    fn fuzzy_empty_candidate() {
        assert!(!fuzzy_match("meditation", ""));
    }

    // ── levenshtein (via normalize_text) ──────────────────────────────────

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn levenshtein_one_edit() {
        assert_eq!(levenshtein("abc", "ab"), 1);
    }

    #[test]
    fn levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
    }
}
