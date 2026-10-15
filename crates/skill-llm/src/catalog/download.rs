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
        .build()
        .into();

    let dl_agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(30)))
        // Per-read-call timeout: generous for slow connections (128 KB / slow
        // connection ≈ a few seconds; 300 s handles ~430 bytes/s).
        .timeout_recv_body(Some(std::time::Duration::from_secs(300)))
        .build()
        .into();

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
    let meta_req = if let Some(tok) = &hf_token {
        meta_req.header("Authorization", format!("Bearer {tok}"))
    } else {
        meta_req
    };
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

        // Fast path: exactly the right size *and* already linked into the
        // snapshot that names this commit. That combination only exists
        // because a previous run hashed these bytes and renamed them into
        // place, so hashing them again proves nothing a crash could have
        // invalidated — the rename is atomic and the name is the hash.
        //
        // It is not free to skip: re-verifying cost 11.7 s for a 378 MB model
        // on every call, and it scales, so merely opening a download for an
        // already-downloaded 5 GB model stalled for minutes. `hf_hub` does not
        // re-verify a cached blob either.
        //
        // The trade: same-size in-place corruption of an already-verified blob
        // is no longer caught here, and would surface later as a model that
        // fails to load. `SKILL_DOWNLOAD_VERIFY_CACHED=1` forces the full
        // re-check for anyone who suspects that, rather than leaving them to
        // delete the cache by hand.
        let always_verify = std::env::var("SKILL_DOWNLOAD_VERIFY_CACHED").is_ok_and(|v| v != "0");
        let snapshot_link = model_dir.join("snapshots").join(&commit_sha).join(filename);
        if can_trust_cached(always_verify, on_disk, remote_size, snapshot_link.exists()) {
            let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            p.state = DownloadState::Downloaded;
            p.status_msg = None;
            p.progress = 1.0;
            return Ok(snapshot_link);
        }

        if on_disk >= remote_size {
            // Re-verifying a cached multi-GB blob is the same silent stall as
            // verifying a fresh one, and it happens on every start-up probe.
            {
                let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                p.status_msg = Some("Verifying cached file…".into());
            }
            match sha256_file_hex_reporting(&blob_path, Some(progress)) {
                Ok(actual) if actual.eq_ignore_ascii_case(blob_sha.trim()) => {
                    let final_path = register_snapshot(&model_dir, &refs_dir, &commit_sha, filename, &blob_path)?;
                    let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    p.state = DownloadState::Downloaded;
                    p.status_msg = None;
                    p.progress = 1.0;
                    return Ok(final_path);
                }
                Ok(_) | Err(_) => {
                    // Stale/corrupt blob named after the expected hash — remove and re-download.
                    let _ = std::fs::remove_file(&blob_path);
                }
            }
        }
    }

    // ── 6. The URL, pinned to the commit the metadata came from ──────────────
    //
    // Not `main`. The blob hash we verify against, the `.incomplete` filename
    // and the snapshot directory all derive from `commit_sha`; fetching bytes
    // from `main` instead means that if the repo moves between the metadata
    // call and the transfer — or, far more likely, between a cancelled
    // download and a resume days later — we append the *new* file's bytes to
    // the *old* file's partial, and only find out at the final hash check,
    // having paid for the whole transfer twice.
    let file_url = blob_url(&endpoint, repo_id, &commit_sha, filename);

    // ── 6–9. Transfer, with resume-backed retry ──────────────────────────────
    //
    // A multi-gigabyte transfer over a real network will be interrupted. The
    // resume machinery above already handles that perfectly — it just used to
    // need a human to press the button again, which for a 5 GB model on hotel
    // wifi meant the download effectively never finished.
    //
    // Each attempt re-reads the on-disk length and continues from there, so a
    // retry is simply a resume: progress already made is never re-fetched, and
    // several short-lived failures still converge. Only transport faults are
    // retried — a cancellation, a pause, an auth failure or a hash mismatch
    // are all answers that will not change by asking again.
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        match transfer_attempt(
            &dl_agent,
            &file_url,
            hf_token.as_deref(),
            filename,
            &incomplete_path,
            remote_size,
            progress,
        ) {
            Ok(()) => break,
            Err(Transfer::Fatal(e)) => return Err(e),
            Err(Transfer::Transient(e)) => {
                if attempt >= MAX_TRANSFER_ATTEMPTS {
                    return Err(e.context(format!(
                        "{filename}: giving up after {MAX_TRANSFER_ATTEMPTS} attempts \
                         (the partial download is kept — starting again resumes it)"
                    )));
                }
                let wait = retry_backoff(attempt);
                {
                    let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    p.status_msg = Some(format!(
                        "Connection lost ({e}) — retrying in {}s ({attempt}/{MAX_TRANSFER_ATTEMPTS})…",
                        wait.as_secs().max(1)
                    ));
                }
                // Sleeping in slices so a cancel during the backoff is acted on
                // then, not after the full wait.
                if !sleep_unless_cancelled(wait, progress) {
                    let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        }
    }

    // ── 10. Sanity-check downloaded size + content hash ──────────────────────
    let final_size = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);
    if final_size < remote_size {
        anyhow::bail!(
            "Incomplete download for {filename}: \
             received {final_size} of {remote_size} bytes"
        )
    }

    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.status_msg = Some("Verifying…".into());
    }
    let actual_sha = sha256_file_hex_reporting(&incomplete_path, Some(progress))
        .with_context(|| format!("hash incomplete download for {filename}"))?;
    if !actual_sha.eq_ignore_ascii_case(blob_sha.trim()) {
        // Corrupt / poisoned blob — delete so the next attempt starts clean.
        let _ = std::fs::remove_file(&incomplete_path);
        anyhow::bail!(crate::error::DownloadError::HashMismatch {
            filename: filename.to_string(),
            expected: blob_sha.clone(),
            actual: actual_sha,
        });
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

/// The URL a file's bytes are fetched from, pinned to one commit.
///
/// Kept as a function so the pinning is testable: passing a moving ref here
/// is the kind of thing that only shows up as a corrupted resume days later.
fn blob_url(endpoint: &str, repo_id: &str, commit_sha: &str, filename: &str) -> String {
    format!("{endpoint}/{repo_id}/resolve/{commit_sha}/{filename}")
}

/// Whether an on-disk blob can be used without re-hashing it.
///
/// All three conditions matter:
/// * **exact** size — a `>=` would accept a longer file, and a shorter one is
///   a partial wearing the finished name;
/// * a non-zero expected size — `0` means the manifest told us nothing, so
///   there is nothing to compare against;
/// * the snapshot link for *this commit* already exists — that is the proof a
///   previous run hashed these bytes and promoted them, rather than them
///   being a stray file that happens to sit at the content-addressed path.
fn can_trust_cached(always_verify: bool, on_disk: u64, remote_size: u64, snapshot_exists: bool) -> bool {
    !always_verify && remote_size > 0 && on_disk == remote_size && snapshot_exists
}

// ── Transfer attempt ─────────────────────────────────────────────────────────

/// How many times one file's transfer is attempted before the error surfaces.
///
/// Five, with the backoff below, spans about half a minute of outage — enough
/// to ride out a roaming handover or a flaky hotspot, short enough that a
/// genuinely dead endpoint still reports promptly.
const MAX_TRANSFER_ATTEMPTS: u32 = 5;

/// Exponential backoff, capped. 1s, 2s, 4s, 8s, 8s…
fn retry_backoff(attempt: u32) -> std::time::Duration {
    let secs = 1u64 << attempt.min(3);
    std::time::Duration::from_secs(secs.min(8))
}

/// Sleep, waking early if the download is cancelled.
///
/// Returns `false` when cancellation was requested, so the caller can stop
/// instead of finishing the wait. A user who hits cancel during a backoff
/// should not watch the button do nothing for eight seconds.
fn sleep_unless_cancelled(total: std::time::Duration, progress: &Arc<Mutex<DownloadProgress>>) -> bool {
    const SLICE: std::time::Duration = std::time::Duration::from_millis(200);
    let mut slept = std::time::Duration::ZERO;
    while slept < total {
        if progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cancelled
        {
            return false;
        }
        let step = SLICE.min(total - slept);
        std::thread::sleep(step);
        slept += step;
    }
    true
}

/// Why a transfer attempt stopped.
enum Transfer {
    /// The network gave out. The partial file is the resume point, so another
    /// attempt continues rather than restarts.
    Transient(anyhow::Error),
    /// Another attempt cannot help: the user cancelled or paused, the server
    /// refused us, or the bytes are wrong.
    Fatal(anyhow::Error),
}

/// Human-readable transfer rate and time remaining.
///
/// Returns an empty string until there is enough of a sample to mean
/// anything — a rate computed over the first few milliseconds is noise, and a
/// wildly wrong ETA is worse than none.
fn rate_and_eta(done_this_attempt: u64, remaining: u64, elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs_f64();
    if secs < 1.0 || done_this_attempt == 0 {
        return String::new();
    }
    let bps = done_this_attempt as f64 / secs;
    if bps <= 0.0 {
        return String::new();
    }
    let mb_s = bps / 1_048_576.0;
    let eta = remaining as f64 / bps;
    let eta = if eta < 60.0 {
        format!("{eta:.0}s")
    } else if eta < 3600.0 {
        format!("{:.0}m{:02.0}s", eta / 60.0, eta % 60.0)
    } else {
        format!("{:.0}h{:02.0}m", eta / 3600.0, (eta % 3600.0) / 60.0)
    };
    format!(" · {mb_s:.1} MB/s · {eta} left")
}

/// One attempt at getting `remote_size` bytes into `incomplete_path`.
///
/// Resumes from whatever is already on disk, so repeated calls make forward
/// progress rather than starting over.
#[allow(clippy::too_many_arguments)]
fn transfer_attempt(
    dl_agent: &ureq::Agent,
    file_url: &str,
    hf_token: Option<&str>,
    filename: &str,
    incomplete_path: &Path,
    remote_size: u64,
    progress: &Arc<Mutex<DownloadProgress>>,
) -> Result<(), Transfer> {
    use std::io::{Read, Write};

    // Re-read the on-disk length every attempt: that is what makes a retry a
    // resume rather than a restart.
    let resume_from: u64 = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);

    // Already have everything — nothing to transfer, let the caller verify.
    if resume_from >= remote_size && remote_size > 0 {
        return Ok(());
    }

    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if p.cancelled {
            return Err(Transfer::Fatal(cancel_error(&mut p)));
        }
        p.progress = (resume_from as f32 / remote_size.max(1) as f32).min(0.99);
        p.status_msg = Some(if resume_from > 0 {
            format!(
                "Resuming from {:.0} / {:.0} MB…",
                resume_from as f64 / 1_048_576.0,
                remote_size as f64 / 1_048_576.0,
            )
        } else {
            format!("Downloading {filename}…")
        });
    }

    let dl_req = dl_agent.get(file_url);
    let dl_req = match hf_token {
        Some(tok) => dl_req.header("Authorization", format!("Bearer {tok}")),
        None => dl_req,
    };
    let mut get = dl_req.header("User-Agent", "skill-app/1.0");
    if resume_from > 0 {
        get = get.header("Range", format!("bytes={resume_from}-"));
    }

    let resp = get.call().map_err(|e| match e {
        // 4xx is an answer, not an outage: a missing file, a gated repo or a
        // bad token will say the same thing however many times we ask.
        ureq::Error::StatusCode(code) if (400..500).contains(&code) && code != 408 && code != 429 => {
            Transfer::Fatal(anyhow::anyhow!("HTTP {code} for {filename}"))
        }
        ureq::Error::StatusCode(code) => Transfer::Transient(anyhow::anyhow!("HTTP {code}")),
        other => Transfer::Transient(anyhow::anyhow!("{other}")),
    })?;

    let http_status = resp.status().as_u16();
    if http_status != 200 && http_status != 206 {
        return Err(Transfer::Transient(anyhow::anyhow!("unexpected HTTP {http_status}")));
    }
    // 200 = the server ignored our Range and is sending the whole file, so the
    // existing partial must be discarded rather than appended to.
    let writing_from: u64 = if http_status == 206 { resume_from } else { 0 };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(writing_from > 0)
        .truncate(writing_from == 0)
        .open(incomplete_path)
        .map_err(|e| Transfer::Fatal(anyhow::Error::new(e).context("open .incomplete file")))?;

    let mut reader = resp.into_body().into_reader();
    // 128 KB — balances syscall overhead against lock contention.
    let mut buf = vec![0u8; 128 * 1024];
    let mut written = writing_from;
    let total = remote_size.max(1);
    let started = std::time::Instant::now();
    let start_bytes = writing_from;

    loop {
        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            // A broken read is the common interruption; the bytes already
            // written stay on disk and the next attempt continues from them.
            Err(e) => return Err(Transfer::Transient(anyhow::anyhow!("{e}"))),
        };
        if n == 0 {
            break;
        }

        if let Err(e) = file.write_all(&buf[..n]) {
            // A write failure is local — a full disk or a vanished cache dir.
            // Retrying the network will not fix it.
            return Err(Transfer::Fatal(
                anyhow::Error::new(e).context(format!("write error for {filename}")),
            ));
        }
        written += n as u64;

        // Progress and cancellation under one lock, so the flag cannot change
        // between reading and acting on it.
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.progress = (written as f32 / total as f32).min(0.99);
        p.status_msg = Some(format!(
            "{:.0} / {:.0} MB{}",
            written as f64 / 1_048_576.0,
            total as f64 / 1_048_576.0,
            rate_and_eta(written - start_bytes, total.saturating_sub(written), started.elapsed()),
        ));
        if p.cancelled {
            // The partial stays on disk; it is the resume point.
            return Err(Transfer::Fatal(cancel_error(&mut p)));
        }
    }

    file.flush()
        .map_err(|e| Transfer::Fatal(anyhow::Error::new(e).context("flush .incomplete file")))?;
    drop(file);

    // Short read without an error: the connection closed early. Another
    // attempt resumes from what did arrive.
    let on_disk = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);
    if on_disk < remote_size {
        return Err(Transfer::Transient(anyhow::anyhow!(
            "connection closed at {on_disk} of {remote_size} bytes"
        )));
    }
    Ok(())
}

/// Mark the shared progress cancelled-or-paused and produce the matching error.
fn cancel_error(p: &mut DownloadProgress) -> anyhow::Error {
    if p.pause_requested {
        p.state = DownloadState::Paused;
        p.status_msg = Some("Paused — resume to continue.".into());
        anyhow::anyhow!("paused")
    } else {
        p.state = DownloadState::Cancelled;
        p.status_msg = Some("Cancelled — will resume next time.".into());
        anyhow::anyhow!("cancelled")
    }
}

// ── Multi-shard download ──────────────────────────────────────────────────────

/// Download a (possibly sharded) model.
///
/// For single-file models (`shard_files` empty) this delegates directly to
/// [`download_file`].  For split models it downloads each shard sequentially,
/// mapping overall progress across the entire set.
///
/// For mlx-community snapshot models ([`LlmModelEntry::is_mlx`]) it downloads
/// every listed file (or the full HF sibling list when `shard_files` is empty)
/// and returns the **snapshot directory**.
///
/// Returns the path to the **first shard** — the one llama.cpp needs — or the
/// MLX snapshot directory for mlx entries.
pub fn download_model(entry: &LlmModelEntry, progress: &Arc<Mutex<DownloadProgress>>) -> anyhow::Result<PathBuf> {
    if entry.is_mlx() {
        return download_mlx_snapshot(entry, progress);
    }

    let filenames: Vec<&str> = entry.all_filenames().collect();
    let total_shards = filenames.len();

    // Single-file fast path.
    if total_shards <= 1 {
        let size_bytes = (entry.size_gb * 1_073_741_824.0) as u64;
        return download_file(&entry.repo, entry.remote_filename(), progress, size_bytes);
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

/// Download an mlx-community HF snapshot (config + tokenizer + safetensors).
///
/// Returns the snapshot directory so [`crate::engine::rlx_backend`] can open it
/// with `Qwen3Runner::from_mlx_packed` / `MlxLoader`.
fn download_mlx_snapshot(entry: &LlmModelEntry, progress: &Arc<Mutex<DownloadProgress>>) -> anyhow::Result<PathBuf> {
    use hf_hub::api::sync::ApiBuilder;

    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        p.status_msg = Some(format!("Listing mlx-community files ({})…", entry.repo));
        p.progress = 0.0;
    }

    let endpoint = std::env::var("HF_ENDPOINT").unwrap_or_else(|_| "https://huggingface.co".into());
    let mut builder = ApiBuilder::new().with_endpoint(endpoint);
    if let Ok(token) = std::env::var("HF_TOKEN").or_else(|_| std::env::var("HUGGING_FACE_HUB_TOKEN")) {
        builder = builder.with_token(Some(token));
    }
    let api = builder.build().context("init hf-hub api for mlx snapshot")?;
    let repo = api.model(entry.repo.clone());
    let info = repo.info().with_context(|| format!("hf info for {}", entry.repo))?;

    // Prefer an explicit shard list from the catalog; otherwise take every
    // sibling except README / git metadata.
    let skip = |name: &str| {
        let lower = name.to_ascii_lowercase();
        lower == "readme.md" || lower == ".gitattributes" || lower.ends_with(".md") && !lower.contains("config")
    };
    let files: Vec<String> = if !entry.shard_files.is_empty() {
        entry.shard_files.clone()
    } else {
        info.siblings
            .iter()
            .map(|s| s.rfilename.clone())
            .filter(|n| !skip(n))
            .collect()
    };
    if files.is_empty() {
        anyhow::bail!("no files to download from {}", entry.repo);
    }

    let total = files.len();
    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.total_shards = total as u16;
        p.current_shard = 1;
    }

    let mut snap_dir: Option<PathBuf> = None;
    for (i, name) in files.iter().enumerate() {
        {
            let p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if p.cancelled {
                if p.pause_requested {
                    anyhow::bail!("paused")
                }
                anyhow::bail!("cancelled")
            }
        }
        {
            let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            p.current_shard = (i + 1) as u16;
            p.status_msg = Some(format!("Downloading {} ({}/{})…", name, i + 1, total));
            p.progress = i as f32 / total as f32;
        }

        let path = repo
            .get(name)
            .with_context(|| format!("download {}/{}", entry.repo, name))?;
        if snap_dir.is_none() {
            // `get` returns …/snapshots/{sha}/{rfilename}; walk up one level per
            // path component so nested files still yield the snapshot root.
            let mut root = path.clone();
            for _ in Path::new(name).components() {
                root = match root.parent() {
                    Some(p) => p.to_path_buf(),
                    None => break,
                };
            }
            snap_dir = Some(root);
        }
    }

    let dir = snap_dir.ok_or_else(|| anyhow::anyhow!("mlx snapshot empty for {}", entry.repo))?;
    if !dir.join("config.json").is_file() {
        anyhow::bail!(
            "mlx snapshot for {} is missing config.json (got {})",
            entry.repo,
            dir.display()
        );
    }

    {
        let mut p = progress.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        p.state = DownloadState::Downloaded;
        p.status_msg = None;
        p.progress = 1.0;
    }
    Ok(dir)
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

/// SHA-256 hex digest of a file (lowercase). Used to verify HF LFS blobs.
/// SHA-256 of a file, optionally reporting progress while it reads.
///
/// Verification is neither free nor quick: hashing a 378 MB model took ~12 s
/// of a measured 48 s download, and it scales with the file, so a 5 GB model
/// spends minutes here. Without this the bar sat at 99 % behind a stale
/// "378 / 378 MB · 10.6 MB/s · 0s left" line — indistinguishable from a hang,
/// at exactly the point where a user is most likely to force-quit and leave a
/// partial file behind.
fn sha256_file_hex_reporting(path: &Path, progress: Option<&Arc<Mutex<DownloadProgress>>>) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path).with_context(|| format!("open {} for hashing", path.display()))?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0).max(1);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 128 * 1024];
    let mut read_so_far: u64 = 0;
    // Touch the shared lock a few times a second, not once per 128 KB.
    let mut last_report = std::time::Instant::now();

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        read_so_far += n as u64;

        if let Some(p) = progress {
            if last_report.elapsed() >= std::time::Duration::from_millis(250) {
                last_report = std::time::Instant::now();
                let pct = read_so_far as f64 / total as f64 * 100.0;
                let mut g = p.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                g.status_msg = Some(format!("Verifying… {pct:.0}%"));
            }
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod hash_tests {
    use super::{
        blob_url, can_trust_cached, cancel_error, rate_and_eta, retry_backoff, sha256_file_hex_reporting,
        sleep_unless_cancelled, DownloadProgress, DownloadState, MAX_TRANSFER_ATTEMPTS,
    };
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[test]
    fn sha256_matches_known_vector() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.bin");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"hello").unwrap();
        drop(f);
        // echo -n hello | shasum -a 256
        assert_eq!(
            sha256_file_hex_reporting(&path, None).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    // ── Retry / resume behaviour ─────────────────────────────────────────────

    /// Backoff must grow and then stop growing — an unbounded doubling would
    /// have the last attempt waiting minutes.
    #[test]
    fn backoff_grows_then_caps() {
        let waits: Vec<u64> = (1..=6).map(|a| retry_backoff(a).as_secs()).collect();
        assert_eq!(waits, vec![2, 4, 8, 8, 8, 8]);
        // The whole retry budget stays in the tens of seconds, not minutes.
        let total: u64 = (1..MAX_TRANSFER_ATTEMPTS).map(|a| retry_backoff(a).as_secs()).sum();
        assert!(total <= 60, "total backoff {total}s should stay under a minute");
    }

    fn progress_handle(cancelled: bool) -> Arc<Mutex<DownloadProgress>> {
        Arc::new(Mutex::new(DownloadProgress {
            cancelled,
            ..Default::default()
        }))
    }

    /// Cancelling during a backoff must be acted on promptly, not after the
    /// full wait — otherwise the button appears dead for up to 8 seconds.
    #[test]
    fn a_cancel_during_backoff_returns_early() {
        let p = progress_handle(false);
        let p2 = Arc::clone(&p);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            p2.lock().unwrap().cancelled = true;
        });

        let started = std::time::Instant::now();
        let completed = sleep_unless_cancelled(std::time::Duration::from_secs(8), &p);
        let waited = started.elapsed();

        assert!(!completed, "should report that it was cancelled");
        assert!(waited < std::time::Duration::from_secs(2), "returned after {waited:?}");
    }

    #[test]
    fn an_uninterrupted_backoff_sleeps_the_full_time() {
        let p = progress_handle(false);
        let started = std::time::Instant::now();
        assert!(sleep_unless_cancelled(std::time::Duration::from_millis(400), &p));
        assert!(started.elapsed() >= std::time::Duration::from_millis(350));
    }

    #[test]
    fn an_already_cancelled_download_does_not_sleep_at_all() {
        let p = progress_handle(true);
        let started = std::time::Instant::now();
        assert!(!sleep_unless_cancelled(std::time::Duration::from_secs(8), &p));
        assert!(started.elapsed() < std::time::Duration::from_millis(100));
    }

    /// A pause and a cancel are different outcomes and the UI shows them
    /// differently; both must leave the partial file alone.
    #[test]
    fn cancel_and_pause_produce_distinct_states() {
        let mut p = DownloadProgress {
            cancelled: true,
            pause_requested: true,
            ..Default::default()
        };
        assert_eq!(cancel_error(&mut p).to_string(), "paused");
        assert_eq!(p.state, DownloadState::Paused);

        let mut p = DownloadProgress {
            cancelled: true,
            pause_requested: false,
            ..Default::default()
        };
        assert_eq!(cancel_error(&mut p).to_string(), "cancelled");
        assert_eq!(p.state, DownloadState::Cancelled);
    }

    // ── Rate / ETA ───────────────────────────────────────────────────────────

    /// A rate computed over a few milliseconds is noise, and a wildly wrong
    /// ETA is worse than none — so there is nothing to show yet.
    #[test]
    fn no_rate_until_the_sample_is_meaningful() {
        assert_eq!(rate_and_eta(1024, 1024, std::time::Duration::from_millis(10)), "");
        assert_eq!(rate_and_eta(0, 1024, std::time::Duration::from_secs(5)), "");
    }

    #[test]
    fn rate_and_eta_read_sensibly() {
        // 10 MB in 10 s = 1 MB/s, with 30 MB to go = 30 s left.
        let s = rate_and_eta(10 * 1_048_576, 30 * 1_048_576, std::time::Duration::from_secs(10));
        assert!(s.contains("1.0 MB/s"), "got {s}");
        assert!(s.contains("30s left"), "got {s}");
    }

    #[test]
    fn a_long_eta_is_shown_in_minutes_and_hours() {
        // 1 MB/s with 600 MB left = 10 minutes.
        let s = rate_and_eta(1_048_576, 600 * 1_048_576, std::time::Duration::from_secs(1));
        assert!(s.contains("10m00s left"), "got {s}");

        // 1 MB/s with 7200 MB left = 2 hours.
        let s = rate_and_eta(1_048_576, 7200 * 1_048_576, std::time::Duration::from_secs(1));
        assert!(s.contains("2h00m left"), "got {s}");
    }

    /// The bytes must come from the same commit the hash and the snapshot
    /// directory came from. Fetching from a moving ref means a resume days
    /// later can append a *different* file's bytes to the old partial, and
    /// only the final hash catches it — after paying for the transfer twice.
    #[test]
    fn the_download_url_is_pinned_to_a_commit() {
        let url = blob_url(
            "https://huggingface.co",
            "unsloth/Qwen3-0.6B-GGUF",
            "abc123def456",
            "Qwen3-0.6B-Q4_K_M.gguf",
        );
        assert_eq!(
            url,
            "https://huggingface.co/unsloth/Qwen3-0.6B-GGUF/resolve/abc123def456/Qwen3-0.6B-Q4_K_M.gguf"
        );
        assert!(!url.contains("/resolve/main/"), "must not fetch from a moving ref");
    }

    // ── Cached-blob fast path ────────────────────────────────────────────────

    /// The whole point: an already-downloaded, already-linked model resolves
    /// without re-reading gigabytes. Measured 11.7 s → 0.5 s for 378 MB.
    #[test]
    fn a_registered_blob_of_the_right_size_is_trusted() {
        assert!(can_trust_cached(false, 1000, 1000, true));
    }

    /// Each condition must be load-bearing on its own.
    #[test]
    fn anything_less_than_certain_is_re_verified() {
        // Wrong size, either way.
        assert!(!can_trust_cached(false, 999, 1000, true), "short file is a partial");
        assert!(!can_trust_cached(false, 1001, 1000, true), "long file is not this file");
        // No snapshot link: nothing proves these bytes were ever verified.
        assert!(!can_trust_cached(false, 1000, 1000, false));
        // The manifest gave no size, so there is nothing to compare.
        assert!(!can_trust_cached(false, 0, 0, true));
        // And the escape hatch overrides all of it.
        assert!(!can_trust_cached(true, 1000, 1000, true));
    }

    /// `HF_ENDPOINT` is how mirrors and corporate proxies are pointed at, so
    /// it has to survive into the transfer URL, not just the metadata call.
    #[test]
    fn a_custom_endpoint_is_honoured() {
        let url = blob_url("https://hf-mirror.example", "org/repo", "deadbeef", "m.gguf");
        assert!(
            url.starts_with("https://hf-mirror.example/org/repo/resolve/deadbeef/"),
            "got {url}"
        );
    }
}
