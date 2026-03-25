// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Resumable HuggingFace model downloader with multi-shard support.

use anyhow::Context;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use super::types::{DownloadProgress, DownloadState, LlmModelEntry};

/// Download a single GGUF file from HuggingFace Hub with byte-range resumption.
///
/// # Resume protocol
///
/// HuggingFace stores LFS files as content-addressed blobs.  We obtain the
/// blob's SHA-256 (its ETag and blob filename) from the Hub metadata API
/// **before** starting any transfer.  This means the incomplete download is
/// always saved to the exact path `{blobs_dir}/{sha256}.incomplete`, so
/// subsequent calls can find and resume it deterministically — even after a
/// crash or forced-quit.
///
/// ```text
/// first attempt   → GET bytes 0–N   → blobs/{sha256}.incomplete (created)
/// cancelled/crash → file stays on disk
/// next attempt    → GET bytes N–end  → blobs/{sha256}.incomplete (appended)
/// complete        → rename → blobs/{sha256}
///                   write  → refs/main
///                   link   → snapshots/{commit}/{filename} → ../../blobs/{sha256}
/// ```
///
/// The final cache layout is identical to what `hf_hub` would produce, so
/// `LlmModelEntry::resolve_cached()` and `hf_hub::Cache::repo().get()` both
/// find the file through their normal offline-cache checks.
///
/// # Progress
///
/// Progress is updated directly inside the streaming loop (every 128 KB
/// chunk), so the frontend receives smooth updates from the first byte.  No
/// external monitor thread is required.
///
/// # Cancellation
///
/// The caller sets `progress.cancelled = true` to request cancellation.  The
/// `.incomplete` file is **intentionally left on disk** so the next call can
/// resume exactly where the transfer stopped.
///
/// # Arguments
///
/// * `size_bytes` — expected file size from the catalog (used as fallback when
///   the API does not return a size; an incorrect value produces an inaccurate
///   progress percentage but is otherwise harmless).
pub fn download_file(
    repo_id: &str,
    filename: &str,
    progress: &Arc<Mutex<DownloadProgress>>,
    size_bytes: u64,
) -> anyhow::Result<PathBuf> {
    use std::io::{Read, Write};

    // ── 0. Initial state ──────────────────────────────────────────────────────
    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // Check for pre-existing cancellation (e.g. forwarded from multi-shard
        // monitor) before resetting state.
        if p.cancelled {
            if p.pause_requested {
                p.state = DownloadState::Paused;
                p.status_msg = Some("Paused.".into());
                anyhow::bail!("paused")
            }
            p.state = DownloadState::Cancelled;
            p.status_msg = Some("Cancelled.".into());
            anyhow::bail!("cancelled")
        }
        p.state = DownloadState::Downloading;
        p.status_msg = Some(format!("Connecting to HuggingFace ({repo_id})…"));
        p.progress = 0.0;
    }

    // ── 1. Environment / config ───────────────────────────────────────────────
    // Respect the same env vars that `hf_hub` uses so corporate proxy / mirror
    // configurations work unchanged.
    let endpoint = std::env::var("HF_ENDPOINT").unwrap_or_else(|_| "https://huggingface.co".into());

    // Auth token — optional for public models, required for gated ones.
    let hf_token: Option<String> = std::env::var("HF_TOKEN")
        .ok()
        .or_else(|| std::env::var("HUGGING_FACE_HUB_TOKEN").ok());

    // ── 2. HF Hub cache paths ─────────────────────────────────────────────────
    // Mirror the layout that `hf_hub` produces so offline lookups work.
    //
    //   {cache}/models--{org}--{repo}/
    //     blobs/          ← content-addressed file storage (named by sha256)
    //     refs/main       ← current HEAD commit hash
    //     snapshots/
    //       {commit}/
    //         {filename}  ← symlink (Unix) / hardlink (Windows) → ../../blobs/{sha256}
    let (model_dir, blobs_dir, refs_dir) = skill_data::util::hf_ensure_dirs(repo_id).context("create HF cache dirs")?;

    // ── 3. Build HTTP agents ──────────────────────────────────────────────────
    // Separate agents for metadata (short timeout) and download (long timeout).
    // Both follow up to 10 redirects — HF Hub redirects to a CDN for LFS blobs.
    let meta_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build().into();

    let dl_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(30)))
        // Per-read-call timeout: generous for slow connections (128 KB / slow
        // connection ≈ a few seconds; 300 s handles ~430 bytes/s).
        .timeout_recv_body(Some(std::time::Duration::from_secs(300)))
        .build().into();



    // ── 4. Fetch file metadata from the Hub API ───────────────────────────────
    //
    // `GET /api/models/{repo_id}?blobs=1` returns JSON that includes:
    //   • `sha`                   — current HEAD commit hash
    //   • `siblings[].lfs.sha256` — content SHA-256 of the LFS blob
    //   • `siblings[].lfs.size`   — true byte count
    //
    // The LFS sha256 is exactly what `hf_hub` uses to name the blob file on
    // disk, so it gives us a stable, deterministic path for the .incomplete
    // file across resume attempts.
    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.status_msg = Some(format!("Fetching metadata for {filename}…"));
    }

    let api_url = format!("{endpoint}/api/models/{repo_id}?blobs=1");
    let meta_req = meta_agent.get(&api_url);
    let meta_req = if let Some(tok) = &hf_token { meta_req.header("Authorization", format!("Bearer {tok}")) } else { meta_req };
    let api_resp = meta_req
        .header("User-Agent", "skill-app/1.0")
        .call()
        .context("HF metadata API error")?;

    let info: serde_json::Value = api_resp.into_body().read_json().context("HF metadata JSON parse")?;

    let commit_sha: String = info["sha"].as_str().unwrap_or("main").to_string();

    // Find this specific file in the siblings list.
    let file_meta = info["siblings"]
        .as_array()
        .and_then(|siblings| siblings.iter().find(|e| e["rfilename"].as_str() == Some(filename)))
        .ok_or_else(|| anyhow::anyhow!("{filename}: not listed in {repo_id} manifest"))?;

    // LFS sha256 → the blob's content hash, used as the blob filename on disk.
    // hf_hub derives this from the `x-linked-etag` response header (after
    // stripping quotes and any `sha256:` prefix); both produce the same value.
    let blob_sha: String = file_meta["lfs"]["sha256"]
        .as_str()
        .map(|s| s.trim_start_matches("sha256:").to_string())
        .ok_or_else(|| anyhow::anyhow!("{filename}: LFS sha256 absent in manifest — is this a non-LFS file?"))?;

    let remote_size: u64 = file_meta["lfs"]["size"]
        .as_u64()
        .or_else(|| file_meta["size"].as_u64())
        .unwrap_or(size_bytes); // fall back to catalog's declared size

    // ── 5. Check for a complete blob already on disk ──────────────────────────
    let blob_path = blobs_dir.join(&blob_sha);
    let incomplete_path = blobs_dir.join(format!("{blob_sha}.incomplete"));

    if blob_path.exists() {
        let on_disk = blob_path.metadata().map(|m| m.len()).unwrap_or(0);
        if on_disk >= remote_size {
            // Already fully downloaded — repair snapshot links if needed and return.
            let final_path = register_snapshot(&model_dir, &refs_dir, &commit_sha, filename, &blob_path)?;
            let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            p.state = DownloadState::Downloaded;
            p.status_msg = None;
            p.progress = 1.0;
            return Ok(final_path);
        }
    }

    // ── 6. Determine resume offset ────────────────────────────────────────────
    //
    // If a previous attempt was cancelled or crashed, .incomplete still exists
    // on disk.  We resume from its current size, sending `Range: bytes=N-`.
    let resume_from: u64 = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);

    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if resume_from > 0 {
            p.progress = (resume_from as f32 / remote_size as f32).min(0.99);
            p.status_msg = Some(format!(
                "Resuming from {:.0} / {:.0} MB…",
                resume_from as f64 / 1_048_576.0,
                remote_size as f64 / 1_048_576.0,
            ));
        } else {
            p.progress = 0.0;
            p.status_msg = Some(format!("Downloading {filename}…"));
        }
    }

    // ── 7. Issue GET (with Range header when resuming) ────────────────────────
    let file_url = format!("{endpoint}/{repo_id}/resolve/main/{filename}");
    let dl_req = dl_agent.get(&file_url);
    let dl_req = if let Some(tok) = &hf_token { dl_req.header("Authorization", format!("Bearer {tok}")) } else { dl_req };
    let mut get = dl_req.header("User-Agent", "skill-app/1.0");
    if resume_from > 0 {
        get = get.header("Range", format!("bytes={resume_from}-"));
    }

    let resp = get.call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => {
            anyhow::anyhow!("HTTP {code} for {filename}")
        }
        other => anyhow::anyhow!("download error: {other}"),
    })?;

    let http_status = resp.status().as_u16();
    // 200 = server ignored Range and sent full content → restart from byte 0.
    // 206 = server honoured Range → append to existing .incomplete file.
    if http_status != 200 && http_status != 206 {
        anyhow::bail!("unexpected HTTP {http_status} for {filename}")
    }
    let writing_from: u64 = if http_status == 206 { resume_from } else { 0 };

    // ── 8. Open (or create) the incomplete file ───────────────────────────────
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(writing_from > 0) // append only when the server honoured Range
        .truncate(writing_from == 0) // full restart: discard any stale partial data
        .open(&incomplete_path)
        .context("open .incomplete file")?;

    // ── 9. Stream response bytes to disk ──────────────────────────────────────
    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 128 * 1024]; // 128 KB chunks — balance between
                                         // syscall overhead and lock contention
    let mut written = writing_from;
    let total = remote_size.max(1);

    loop {
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("read error while downloading {filename}"))?;
        if n == 0 {
            break;
        }

        file.write_all(&buf[..n])
            .with_context(|| format!("write error for {filename}"))?;
        written += n as u64;

        // Update progress and honour cancellation inside the same lock acquisition
        // to avoid a TOCTOU race between reading and writing the flag.
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.progress = (written as f32 / total as f32).min(0.99);
        p.status_msg = Some(format!(
            "{:.0} / {:.0} MB",
            written as f64 / 1_048_576.0,
            total as f64 / 1_048_576.0,
        ));
        if p.cancelled {
            // Leave .incomplete on disk — it is the resume point.
            if p.pause_requested {
                p.state = DownloadState::Paused;
                p.status_msg = Some("Paused — resume to continue.".into());
                anyhow::bail!("paused")
            }
            p.state = DownloadState::Cancelled;
            p.status_msg = Some("Cancelled — will resume next time.".into());
            anyhow::bail!("cancelled")
        }
    }
    drop(file); // explicit flush + close before rename

    // ── 10. Sanity-check downloaded size ─────────────────────────────────────
    let final_size = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);
    if final_size < remote_size {
        anyhow::bail!(
            "Incomplete download for {filename}: \
             received {final_size} of {remote_size} bytes"
        )
    }

    // ── 11. Atomic promotion: .incomplete → blob ──────────────────────────────
    //
    // On the same filesystem (guaranteed: both paths are inside the HF Hub
    // cache directory) this is an O(1) atomic rename — no data is copied.
    std::fs::rename(&incomplete_path, &blob_path).context("rename .incomplete → blob")?;

    // ── 12. Register in the HF Hub cache structure ────────────────────────────
    let final_path = register_snapshot(&model_dir, &refs_dir, &commit_sha, filename, &blob_path)?;

    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.state = DownloadState::Downloaded;
        p.status_msg = None;
        p.progress = 1.0;
    }

    Ok(final_path)
}

// ── Multi-shard download ──────────────────────────────────────────────────────

/// Download a (possibly sharded) model.
///
/// For single-file models (`shard_files` empty) this delegates directly to
/// [`download_file`].  For split models it downloads each shard sequentially,
/// mapping overall progress across the entire set.
///
/// Returns the path to the **first shard** — the one llama.cpp needs.
pub fn download_model(entry: &LlmModelEntry, progress: &Arc<Mutex<DownloadProgress>>) -> anyhow::Result<PathBuf> {
    let filenames: Vec<&str> = entry.all_filenames().collect();
    let total_shards = filenames.len();

    // Single-file fast path.
    if total_shards <= 1 {
        let size_bytes = (entry.size_gb * 1_073_741_824.0) as u64;
        return download_file(&entry.repo, &entry.filename, progress, size_bytes);
    }

    // Multi-shard: compute per-shard sizes (estimate evenly when we don't
    // have per-shard sizes — the catalog only stores total `size_gb`).
    let total_bytes = (entry.size_gb * 1_073_741_824.0) as u64;
    let per_shard_bytes = total_bytes / total_shards as u64;

    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.total_shards = total_shards as u16;
        p.current_shard = 1;
    }

    let mut first_path: Option<PathBuf> = None;

    for (i, shard_name) in filenames.iter().enumerate() {
        // Check cancellation between shards.
        {
            let p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if p.cancelled {
                if p.pause_requested {
                    anyhow::bail!("paused")
                }
                anyhow::bail!("cancelled")
            }
        }

        // Create a per-shard progress wrapper that maps shard progress into
        // the overall [0..1] range.
        let shard_idx = i;
        let shard_progress = Arc::new(Mutex::new(DownloadProgress {
            filename: shard_name.to_string(),
            state: DownloadState::Downloading,
            status_msg: None,
            progress: 0.0,
            cancelled: false,
            pause_requested: false,
            current_shard: (i + 1) as u16,
            total_shards: total_shards as u16,
        }));

        // Spawn a monitor that maps per-shard progress → overall progress.
        let overall = Arc::clone(progress);
        let shard_prog_clone = Arc::clone(&shard_progress);
        let n_shards = total_shards;
        let monitor = std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let (shard_state, shard_pct, shard_msg, shard_cancelled) = {
                    let sp = shard_prog_clone
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    (sp.state.clone(), sp.progress, sp.status_msg.clone(), sp.cancelled)
                };

                // Map shard progress into overall range.
                let overall_pct = (shard_idx as f32 + shard_pct) / n_shards as f32;

                {
                    let mut op = overall.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    op.progress = overall_pct.min(0.99);
                    op.current_shard = (shard_idx + 1) as u16;
                    op.total_shards = n_shards as u16;
                    op.status_msg = Some(format!(
                        "Shard {}/{}: {}",
                        shard_idx + 1,
                        n_shards,
                        shard_msg.as_deref().unwrap_or("downloading...")
                    ));

                    // Forward cancellation from the overall handle to the shard.
                    if op.cancelled && !shard_cancelled {
                        let pause = op.pause_requested;
                        drop(op);
                        let mut sp = shard_prog_clone
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        sp.cancelled = true;
                        sp.pause_requested = pause;
                    }
                }

                if shard_state != DownloadState::Downloading {
                    break;
                }
            }
        });

        let path = download_file(&entry.repo, shard_name, &shard_progress, per_shard_bytes);

        // Wait for the monitor thread to notice the shard completed.
        let _ = monitor.join();

        match path {
            Ok(p) => {
                if i == 0 {
                    first_path = Some(p);
                }
            }
            Err(e) => {
                // Propagate the error state to the overall progress.
                let mut op = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let msg = e.to_string();
                if msg == "paused" {
                    op.state = DownloadState::Paused;
                    op.status_msg = Some(format!("Paused at shard {}/{}.", i + 1, total_shards));
                } else if msg == "cancelled" {
                    op.state = DownloadState::Cancelled;
                    op.status_msg = Some("Cancelled.".into());
                } else {
                    op.state = DownloadState::Failed;
                    op.status_msg = Some(format!("Shard {}/{} failed: {}", i + 1, total_shards, msg));
                }
                return Err(e);
            }
        }
    }

    // All shards complete.
    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.state = DownloadState::Downloaded;
        p.status_msg = None;
        p.progress = 1.0;
    }

    first_path.ok_or_else(|| anyhow::anyhow!("no shard files to download"))
}

/// Register a completed blob in the HF Hub snapshot directory structure so that
/// `hf_hub::Cache::repo().get(filename)` and `LlmModelEntry::resolve_cached()`
/// can both locate it through their normal offline-cache logic.
///
/// Creates or overwrites:
/// - `{model_dir}/refs/main`                         ← `{commit_sha}`
/// - `{model_dir}/snapshots/{commit_sha}/{filename}` ← relative symlink / hardlink
///   pointing to the blob
///
/// Returns the snapshot path (what `resolve_cached` returns as `local_path`).
fn register_snapshot(
    model_dir: &Path,
    refs_dir: &Path,
    commit_sha: &str,
    filename: &str,
    blob_path: &Path,
) -> anyhow::Result<PathBuf> {
    // Write refs/main so hf_hub can resolve the snapshot directory.
    std::fs::write(refs_dir.join("main"), commit_sha).context("write refs/main")?;

    // Build the snapshot directory, handling filenames that contain subdirectories
    // (e.g. "subfolder/model.gguf" — uncommon but valid in HF repos).
    let snapshot_dir = model_dir.join("snapshots").join(commit_sha);
    let snapshot_link = snapshot_dir.join(filename);

    std::fs::create_dir_all(snapshot_link.parent().unwrap_or(&snapshot_dir)).context("create snapshot dir")?;

    // Remove a stale link left by a previous (possibly failed) registration.
    if snapshot_link.exists() || snapshot_link.symlink_metadata().is_ok() {
        std::fs::remove_file(&snapshot_link).ok();
    }

    // The blob filename is just the sha256 — derive the relative path from
    // the snapshot file's position in the directory tree:
    //   snapshots/{commit}/{filename}  →  ../../blobs/{sha256}
    // For filenames with subdirs (depth > 1) each extra component needs
    // an additional `../`.
    // On Unix a relative symlink is the natural representation.
    // On Windows, symlinks require Developer Mode or admin privileges, so we
    // use a hardlink instead (both paths are on the same NTFS volume, which
    // is always true for files within the same HF Hub cache directory).
    // On other platforms we fall back to a file copy (WASM, etc.).
    #[cfg(unix)]
    {
        let blob_name = blob_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let depth = std::path::Path::new(filename).components().count(); // ≥ 1
        let parents = "../".repeat(depth + 1); // +1 for the commit_sha dir level
        let relative_target = format!("{parents}blobs/{blob_name}");
        std::os::unix::fs::symlink(&relative_target, &snapshot_link).context("create symlink")?;
    }

    #[cfg(windows)]
    std::fs::hard_link(blob_path, &snapshot_link).context("create hardlink")?;

    #[cfg(not(any(unix, windows)))]
    std::fs::copy(blob_path, &snapshot_link).context("copy blob to snapshot")?;

    Ok(snapshot_link)
}
