// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LLM model catalog — loaded from the bundled `llm_catalog.json`.
//!
//! ## Source of truth
//!
//! `src-tauri/llm_catalog.json` is the **canonical** list of model families,
//! repos, quants, sizes and descriptions.  It is embedded at compile time via
//! `include_str!` and used in two ways:
//!
//! 1. **First run** – no `~/.skill/llm_catalog.json` exists yet.
//!    `LlmCatalog::load()` falls back to the bundled data directly.
//!
//! 2. **Subsequent runs** – persisted catalog exists (user may have models
//!    downloaded, custom `active_model`, etc.).  `load()` parses the persisted
//!    file and then **forward-merges** from the bundle:
//!    - New entries added to the bundle appear automatically.
//!    - Static metadata (description, tags, `recommended`, `advanced`) are
//!      refreshed from the bundle so edits propagate to existing users without
//!      losing their download state.
//!
//! To add a new model or change a description, **only edit `llm_catalog.json`**
//! — no Rust code changes are required.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

// ── Embedded catalog ──────────────────────────────────────────────────────────

/// The bundled default catalog, embedded at compile time.
const BUNDLED_CATALOG_JSON: &str = include_str!("../llm_catalog.json");

// Re-export from skill-constants.
pub use skill_constants::LLM_CATALOG_FILE as CATALOG_FILE;

// ── Per-file entry ────────────────────────────────────────────────────────────

/// Download / presence state for a single model file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    #[default]
    NotDownloaded,
    Downloading,
    Paused,
    Downloaded,
    Failed,
    Cancelled,
}

/// One entry in the catalog — a single GGUF file.
///
/// Fields in the first block come from `llm_catalog.json` (static knowledge).
/// Fields in the second block are runtime-only and never present in the
/// bundled JSON (they default to `None` / `NotDownloaded` / `0.0`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelEntry {
    // ── Static (from llm_catalog.json) ───────────────────────────────────────
    pub repo:        String,
    pub filename:    String,
    pub quant:       String,
    pub size_gb:     f32,
    pub description: String,
    pub family_id:   String,
    pub family_name: String,
    pub family_desc: String,
    /// e.g. `["chat","reasoning","small"]`
    pub tags:        Vec<String>,
    pub is_mmproj:   bool,
    pub recommended: bool,
    /// Hidden in simple view; shown under "Show all quants".
    pub advanced:    bool,

    // ── Runtime (persisted in skill_dir/llm_catalog.json) ────────────────────
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path:  Option<PathBuf>,
    #[serde(default)]
    pub state:       DownloadState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg:  Option<String>,
    #[serde(default)]
    pub progress:    f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initiated_at_unix: Option<u64>,
}

impl LlmModelEntry {
    /// Resolve local path from the HF Hub cache — filesystem only, no network.
    pub fn resolve_cached(&self) -> Option<PathBuf> {
        use hf_hub::{Cache, Repo};
        let cache = Cache::from_env();
        let repo  = cache.repo(Repo::model(self.repo.clone()));
        repo.get(&self.filename)
    }
}

// ── Full catalog ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCatalog {
    pub entries:       Vec<LlmModelEntry>,
    #[serde(default)]
    pub active_model:  String,
    #[serde(default)]
    pub active_mmproj: String,
}

// ── Bundled-catalog helpers ───────────────────────────────────────────────────

/// Parse and return the bundled catalog.  Panics at startup (compile-time
/// guarantee) if `llm_catalog.json` contains invalid JSON.
fn bundled() -> LlmCatalog {
    serde_json::from_str(BUNDLED_CATALOG_JSON)
        .expect("src-tauri/llm_catalog.json is not valid JSON — fix it and recompile")
}


impl Default for LlmCatalog {
    /// Returns the bundled catalog with all states set to `NotDownloaded`.
    fn default() -> Self { bundled() }
}

// ── Persistence & merge ───────────────────────────────────────────────────────

impl LlmCatalog {
    /// Load the catalog for `skill_dir`.
    ///
    /// 1. Parse the bundled JSON as the authoritative list of known entries.
    /// 2. Try to read `skill_dir/llm_catalog.json` (persisted user state).
    /// 3. Forward-merge:
    ///    - Copy download state / local_path / progress from persisted → bundled.
    ///    - Append persisted entries that have no match in the bundle (custom
    ///      models the user added manually or via the file picker).
    /// 4. Probe the HF Hub cache for any entries not already `Downloaded`.
    pub fn load(skill_dir: &Path) -> Self {
        let bundle = bundled();

        let persisted: Option<LlmCatalog> = skill_dir
            .join(CATALOG_FILE)
            .pipe(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok());

        let mut cat = match persisted {
            None => bundle, // first run — use bundled directly
            Some(mut p) => {
                // Build a map from the persisted entries for fast lookup.
                let mut pmap: std::collections::HashMap<String, LlmModelEntry> =
                    p.entries.drain(..).map(|e| (e.filename.clone(), e)).collect();

                // Start from the bundle; apply persisted runtime state where available.
                let mut merged: Vec<LlmModelEntry> = bundle
                    .entries
                    .into_iter()
                    .map(|mut bundled_entry| {
                        if let Some(saved) = pmap.remove(&bundled_entry.filename) {
                            // Keep runtime fields from the persisted copy.
                            bundled_entry.local_path = saved.local_path;
                            bundled_entry.state      = saved.state;
                            bundled_entry.status_msg = saved.status_msg;
                            bundled_entry.progress   = saved.progress;
                        }
                        bundled_entry
                    })
                    .collect();

                // Append any leftover persisted entries (custom / manually-added).
                merged.extend(pmap.into_values());

                LlmCatalog {
                    entries:       merged,
                    active_model:  p.active_model,
                    active_mmproj: p.active_mmproj,
                }
            }
        };

        cat.refresh_cache();
        cat
    }

    /// Save the catalog (runtime state) to `skill_dir/llm_catalog.json`.
    pub fn save(&self, skill_dir: &Path) {
        let path = skill_dir.join(CATALOG_FILE);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Probe the HF Hub disk cache and update `local_path` / `state` for
    /// every entry that is not currently downloading.  Zero network I/O.
    pub fn refresh_cache(&mut self) {
        for entry in &mut self.entries {
            if entry.state == DownloadState::Downloading { continue; }
            entry.local_path = entry.resolve_cached();
            entry.state = if entry.local_path.is_some() {
                DownloadState::Downloaded
            } else {
                DownloadState::NotDownloaded
            };
        }
    }

    /// If `active_model` is empty, pick the first downloaded recommended model.
    pub fn auto_select(&mut self) {
        if !self.active_model.is_empty() { return; }
        if let Some(e) = self.entries.iter()
            .find(|e| !e.is_mmproj && e.recommended && e.state == DownloadState::Downloaded)
        {
            self.active_model = e.filename.clone();
        }
    }

    /// Active text model entry, regardless of download state.
    pub fn active_model_entry(&self) -> Option<&LlmModelEntry> {
        if self.active_model.is_empty() { return None; }
        self.entries.iter()
            .find(|e| !e.is_mmproj && e.filename == self.active_model)
    }

    /// Active mmproj entry, regardless of download state.
    pub fn active_mmproj_entry(&self) -> Option<&LlmModelEntry> {
        if self.active_mmproj.is_empty() { return None; }
        self.entries.iter()
            .find(|e| e.is_mmproj && e.filename == self.active_mmproj)
    }

    /// Local path of the active model if it is downloaded.
    pub fn active_model_path(&self) -> Option<PathBuf> {
        self.entries.iter()
            .find(|e| !e.is_mmproj && e.filename == self.active_model
                && e.state == DownloadState::Downloaded)
            .and_then(|e| e.local_path.clone())
    }

    /// Local path of the active mmproj if it is downloaded.
    pub fn active_mmproj_path(&self) -> Option<PathBuf> {
        if self.active_mmproj.is_empty() { return None; }
        self.entries.iter()
            .find(|e| e.is_mmproj && e.filename == self.active_mmproj
                && e.state == DownloadState::Downloaded)
            .and_then(|e| e.local_path.clone())
    }

    /// Whether the explicit mmproj selection matches the active text model.
    pub fn active_mmproj_matches_active_model(&self) -> bool {
        match (self.active_model_entry(), self.active_mmproj_entry()) {
            (Some(model), Some(mmproj)) => model.repo == mmproj.repo,
            _ => true,
        }
    }

    /// Best downloaded text model for a specific repo.
    pub fn best_downloaded_model_for_repo(&self, repo: &str) -> Option<&LlmModelEntry> {
        fn quant_rank(quant: &str) -> usize {
            let order = [
                "Q4_K_M", "Q4_0", "Q4_K_S", "Q4_K_L", "Q4_1",
                "Q5_K_M", "Q5_K_S", "Q5_K_L",
                "Q6_K", "Q6_K_L",
                "Q8_0",
                "IQ4_XS", "IQ4_NL",
                "Q3_K_M", "Q3_K_L", "Q3_K_XL", "Q3_K_S",
                "IQ3_M", "IQ3_XS", "IQ3_XXS",
                "Q2_K", "Q2_K_L",
                "IQ2_M", "IQ2_S", "IQ2_XS", "IQ2_XXS",
                "BF16", "F16", "F32",
            ];
            order.iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(quant))
                .unwrap_or(order.len())
        }

        self.entries.iter()
            .filter(|e| {
                !e.is_mmproj
                    && e.repo == repo
                    && e.state == DownloadState::Downloaded
            })
            .min_by(|a, b| {
                (!a.recommended)
                    .cmp(&!b.recommended)
                    .then_with(|| a.advanced.cmp(&b.advanced))
                    .then_with(|| quant_rank(&a.quant).cmp(&quant_rank(&b.quant)))
                    .then_with(|| a.size_gb.total_cmp(&b.size_gb))
                    .then_with(|| a.filename.cmp(&b.filename))
            })
    }

    /// Best downloaded text model that can pair with a specific mmproj file.
    pub fn best_model_for_mmproj(&self, mmproj_filename: &str) -> Option<&LlmModelEntry> {
        let repo = self.entries.iter()
            .find(|e| e.is_mmproj && e.filename == mmproj_filename)?
            .repo
            .clone();
        self.best_downloaded_model_for_repo(&repo)
    }

    /// Find the best downloaded mmproj for the currently active model.
    ///
    /// Matches by repo (same HuggingFace repo as the active model entry).
    /// Preference order: recommended first, then by quant (BF16 > F16 > F32).
    /// Returns `None` if no compatible mmproj is downloaded.
    pub fn best_mmproj_for_active_model(&self) -> Option<&LlmModelEntry> {
        // Find the repo of the active model.
        let active_repo = self.active_model_entry()?.repo.as_str();

        fn quant_rank(quant: &str) -> u8 {
            match quant.to_uppercase().as_str() {
                "BF16" => 0,
                "F16"  => 1,
                _      => 2,  // F32 and others
            }
        }

        self.entries.iter()
            .filter(|e| e.is_mmproj
                && e.repo == active_repo
                && e.state == DownloadState::Downloaded)
            .min_by_key(|e| (!e.recommended as u8, quant_rank(&e.quant)))
    }

    /// If `autoload_mmproj` is requested and no mmproj is currently selected,
    /// pick the best available one for the active model and return its path.
    /// Does **not** mutate `active_mmproj` — the caller decides whether to
    /// persist the selection.
    pub fn resolve_mmproj_path(&self, autoload: bool) -> Option<PathBuf> {
        // Explicit selection always wins.
        if let path @ Some(_) = self.active_mmproj_path() {
            return path;
        }
        if autoload {
            self.best_mmproj_for_active_model()
                .and_then(|e| e.local_path.clone())
        } else {
            None
        }
    }
}

// ── Path extension helper (avoids a temporary binding) ───────────────────────

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R { f(self) }
}
impl<T> Pipe for T {}

// ── Shared download progress ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    pub filename:   String,
    pub state:      DownloadState,
    pub status_msg: Option<String>,
    pub progress:   f32,
    pub cancelled:  bool,
    pub pause_requested: bool,
}

// ── Resumable model downloader ────────────────────────────────────────────────

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
    repo_id:    &str,
    filename:   &str,
    progress:   &Arc<Mutex<DownloadProgress>>,
    size_bytes: u64,
) -> Result<PathBuf, String> {
    use std::io::{Read, Write};

    // ── 0. Initial state ──────────────────────────────────────────────────────
    {
        let mut p = progress.lock().unwrap();
        p.state      = DownloadState::Downloading;
        p.status_msg = Some(format!("Connecting to HuggingFace ({repo_id})…"));
        p.progress   = 0.0;
        p.cancelled  = false;
        p.pause_requested = false;
    }
    if progress.lock().unwrap().cancelled {
        let mut p = progress.lock().unwrap();
        p.state      = DownloadState::Cancelled;
        p.status_msg = Some("Cancelled.".into());
        return Err("cancelled".into());
    }

    // ── 1. Environment / config ───────────────────────────────────────────────
    // Respect the same env vars that `hf_hub` uses so corporate proxy / mirror
    // configurations work unchanged.
    let endpoint = std::env::var("HF_ENDPOINT")
        .unwrap_or_else(|_| "https://huggingface.co".into());

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
    let (model_dir, blobs_dir, refs_dir) = skill_data::util::hf_ensure_dirs(repo_id)
        .map_err(|e| format!("create HF cache dirs: {e}"))?;

    // ── 3. Build HTTP agents ──────────────────────────────────────────────────
    // Separate agents for metadata (short timeout) and download (long timeout).
    // Both follow up to 10 redirects — HF Hub redirects to a CDN for LFS blobs.
    let meta_agent = ureq::AgentBuilder::new()
        .redirects(10)
        .timeout(std::time::Duration::from_secs(30))
        .build();

    let dl_agent = ureq::AgentBuilder::new()
        .redirects(10)
        .timeout_connect(std::time::Duration::from_secs(30))
        // Per-read-call timeout: generous for slow connections (128 KB / slow
        // connection ≈ a few seconds; 300 s handles ~430 bytes/s).
        .timeout_read(std::time::Duration::from_secs(300))
        .build();

    // Convenience: attach Bearer token if one is configured.
    let auth = |req: ureq::Request| -> ureq::Request {
        match &hf_token {
            Some(tok) => req.set("Authorization", &format!("Bearer {tok}")),
            None      => req,
        }
    };

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
        let mut p = progress.lock().unwrap();
        p.status_msg = Some(format!("Fetching metadata for {filename}…"));
    }

    let api_url  = format!("{endpoint}/api/models/{repo_id}?blobs=1");
    let api_resp = auth(meta_agent.get(&api_url))
        .set("User-Agent", "skill-app/1.0")
        .call()
        .map_err(|e| format!("HF metadata API error: {e}"))?;

    let info: serde_json::Value = api_resp
        .into_json()
        .map_err(|e| format!("HF metadata JSON parse: {e}"))?;

    let commit_sha: String = info["sha"]
        .as_str()
        .unwrap_or("main")
        .to_string();

    // Find this specific file in the siblings list.
    let file_meta = info["siblings"]
        .as_array()
        .and_then(|siblings| {
            siblings.iter().find(|e| e["rfilename"].as_str() == Some(filename))
        })
        .ok_or_else(|| format!("{filename}: not listed in {repo_id} manifest"))?;

    // LFS sha256 → the blob's content hash, used as the blob filename on disk.
    // hf_hub derives this from the `x-linked-etag` response header (after
    // stripping quotes and any `sha256:` prefix); both produce the same value.
    let blob_sha: String = file_meta["lfs"]["sha256"]
        .as_str()
        .map(|s| s.trim_start_matches("sha256:").to_string())
        .ok_or_else(|| {
            format!("{filename}: LFS sha256 absent in manifest — is this a non-LFS file?")
        })?;

    let remote_size: u64 = file_meta["lfs"]["size"]
        .as_u64()
        .or_else(|| file_meta["size"].as_u64())
        .unwrap_or(size_bytes); // fall back to catalog's declared size

    // ── 5. Check for a complete blob already on disk ──────────────────────────
    let blob_path       = blobs_dir.join(&blob_sha);
    let incomplete_path = blobs_dir.join(format!("{blob_sha}.incomplete"));

    if blob_path.exists() {
        let on_disk = blob_path.metadata().map(|m| m.len()).unwrap_or(0);
        if on_disk >= remote_size {
            // Already fully downloaded — repair snapshot links if needed and return.
            let final_path =
                register_snapshot(&model_dir, &refs_dir, &commit_sha, filename, &blob_path)?;
            let mut p = progress.lock().unwrap();
            p.state      = DownloadState::Downloaded;
            p.status_msg = None;
            p.progress   = 1.0;
            return Ok(final_path);
        }
    }

    // ── 6. Determine resume offset ────────────────────────────────────────────
    //
    // If a previous attempt was cancelled or crashed, .incomplete still exists
    // on disk.  We resume from its current size, sending `Range: bytes=N-`.
    let resume_from: u64 = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);

    {
        let mut p = progress.lock().unwrap();
        if resume_from > 0 {
            p.progress   = (resume_from as f32 / remote_size as f32).min(0.99);
            p.status_msg = Some(format!(
                "Resuming from {:.0} / {:.0} MB…",
                resume_from as f64 / 1_048_576.0,
                remote_size as f64 / 1_048_576.0,
            ));
        } else {
            p.progress   = 0.0;
            p.status_msg = Some(format!("Downloading {filename}…"));
        }
    }

    // ── 7. Issue GET (with Range header when resuming) ────────────────────────
    let file_url = format!("{endpoint}/{repo_id}/resolve/main/{filename}");
    let mut get  = auth(dl_agent.get(&file_url))
        .set("User-Agent", "skill-app/1.0");
    if resume_from > 0 {
        get = get.set("Range", &format!("bytes={resume_from}-"));
    }

    let resp = get.call().map_err(|e| match e {
        ureq::Error::Status(code, r) => {
            let body = r.into_string().unwrap_or_default();
            format!("HTTP {code} for {filename}: {body}")
        }
        other => format!("download error: {other}"),
    })?;

    let http_status = resp.status();
    // 200 = server ignored Range and sent full content → restart from byte 0.
    // 206 = server honoured Range → append to existing .incomplete file.
    if http_status != 200 && http_status != 206 {
        return Err(format!("unexpected HTTP {http_status} for {filename}"));
    }
    let writing_from: u64 = if http_status == 206 { resume_from } else { 0 };

    // ── 8. Open (or create) the incomplete file ───────────────────────────────
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(writing_from > 0)   // append only when the server honoured Range
        .truncate(writing_from == 0) // full restart: discard any stale partial data
        .open(&incomplete_path)
        .map_err(|e| format!("open .incomplete file: {e}"))?;

    // ── 9. Stream response bytes to disk ──────────────────────────────────────
    let mut reader  = resp.into_reader();
    let mut buf     = vec![0u8; 128 * 1024]; // 128 KB chunks — balance between
                                              // syscall overhead and lock contention
    let mut written = writing_from;
    let total       = remote_size.max(1);

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("read error while downloading {filename}: {e}"))?;
        if n == 0 { break; }

        file.write_all(&buf[..n])
            .map_err(|e| format!("write error for {filename}: {e}"))?;
        written += n as u64;

        // Update progress and honour cancellation inside the same lock acquisition
        // to avoid a TOCTOU race between reading and writing the flag.
        let mut p = progress.lock().unwrap();
        p.progress   = (written as f32 / total as f32).min(0.99);
        p.status_msg = Some(format!(
            "{:.0} / {:.0} MB",
            written as f64 / 1_048_576.0,
            total   as f64 / 1_048_576.0,
        ));
        if p.cancelled {
            // Leave .incomplete on disk — it is the resume point.
            if p.pause_requested {
                p.state      = DownloadState::Paused;
                p.status_msg = Some("Paused — resume to continue.".into());
                return Err("paused".into());
            }
            p.state      = DownloadState::Cancelled;
            p.status_msg = Some("Cancelled — will resume next time.".into());
            return Err("cancelled".into());
        }
    }
    drop(file); // explicit flush + close before rename

    // ── 10. Sanity-check downloaded size ─────────────────────────────────────
    let final_size = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);
    if final_size < remote_size {
        return Err(format!(
            "Incomplete download for {filename}: \
             received {final_size} of {remote_size} bytes"
        ));
    }

    // ── 11. Atomic promotion: .incomplete → blob ──────────────────────────────
    //
    // On the same filesystem (guaranteed: both paths are inside the HF Hub
    // cache directory) this is an O(1) atomic rename — no data is copied.
    std::fs::rename(&incomplete_path, &blob_path)
        .map_err(|e| format!("rename .incomplete → blob: {e}"))?;

    // ── 12. Register in the HF Hub cache structure ────────────────────────────
    let final_path =
        register_snapshot(&model_dir, &refs_dir, &commit_sha, filename, &blob_path)?;

    {
        let mut p = progress.lock().unwrap();
        p.state      = DownloadState::Downloaded;
        p.status_msg = None;
        p.progress   = 1.0;
    }

    Ok(final_path)
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
    model_dir:  &Path,
    refs_dir:   &Path,
    commit_sha: &str,
    filename:   &str,
    blob_path:  &Path,
) -> Result<PathBuf, String> {
    // Write refs/main so hf_hub can resolve the snapshot directory.
    std::fs::write(refs_dir.join("main"), commit_sha)
        .map_err(|e| format!("write refs/main: {e}"))?;

    // Build the snapshot directory, handling filenames that contain subdirectories
    // (e.g. "subfolder/model.gguf" — uncommon but valid in HF repos).
    let snapshot_dir  = model_dir.join("snapshots").join(commit_sha);
    let snapshot_link = snapshot_dir.join(filename);

    std::fs::create_dir_all(
        snapshot_link.parent().unwrap_or(&snapshot_dir),
    )
    .map_err(|e| format!("create snapshot dir: {e}"))?;

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
        let blob_name = blob_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let depth = std::path::Path::new(filename).components().count(); // ≥ 1
        let parents = "../".repeat(depth + 1); // +1 for the commit_sha dir level
        let relative_target = format!("{parents}blobs/{blob_name}");
        std::os::unix::fs::symlink(&relative_target, &snapshot_link)
            .map_err(|e| format!("create symlink: {e}"))?;
    }

    #[cfg(windows)]
    std::fs::hard_link(blob_path, &snapshot_link)
        .map_err(|e| format!("create hardlink: {e}"))?;

    #[cfg(not(any(unix, windows)))]
    std::fs::copy(blob_path, &snapshot_link)
        .map_err(|e| format!("copy blob to snapshot: {e}"))?;

    Ok(snapshot_link)
}
