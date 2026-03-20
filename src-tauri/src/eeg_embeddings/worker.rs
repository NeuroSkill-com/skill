// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Background embed worker, hook matcher, and HF weight helpers.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
};

use crate::skill_log::SkillLogger;
use crate::settings::{HookLastTrigger, HookRule};
use crate::MutexExt;
use crate::constants::{
    CHANNEL_NAMES, EEG_CHANNELS, EMBEDDING_EPOCH_SAMPLES,
    GLOBAL_HNSW_SAVE_EVERY, MUSE_SAMPLE_RATE,
};
use crate::global_eeg_index;
use skill_eeg::eeg_model_config::{EegModelConfig, EegModelStatus, ExgModelBackend};
use skill_exg::{
    EpochMetrics, configure_cubecl_cache, GPU_DEVICE_POISONED,
    panic_msg, yyyymmdd_utc, yyyymmddhhmmss_utc,
};

use super::EpochMsg;
use super::day_store::DayStore;


#[derive(serde::Serialize)]
struct HookBroadcastPayload {
    hook: String,
    context: String,
    command: String,
    text: String,
    scenario: String,
    distance: f32,
    label_id: i64,
    label_text: String,
    triggered_at_utc: u64,
}

struct HookReferenceSet {
    hook: HookRule,
    refs: Vec<HookReference>,
}

struct HookReference {
    emb: Vec<f32>,
    label_id: i64,
    label_text: String,
    eeg_start_utc: u64,
}

struct HookMatcher {
    skill_dir: PathBuf,
    hooks: Vec<HookRule>,
    label_idx: Arc<crate::label_index::LabelIndexState>,
    ws_broadcaster: crate::ws_server::WsBroadcaster,
    /// Shared app-wide text embedder — same instance used by labels and
    /// screenshot OCR.  Avoids loading a separate ~130 MB ONNX model copy.
    shared_embedder: Arc<crate::label_cmds::EmbedderState>,
    cache: Vec<HookReferenceSet>,
    last_refresh_unix: u64,
    last_fired_unix: HashMap<String, u64>,
    logger: Arc<SkillLogger>,
    hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
    app: tauri::AppHandle,
    hooks_log: Option<skill_data::hooks_log::HooksLog>,
}

impl HookMatcher {
    #[allow(clippy::too_many_arguments)]
    fn new(
        skill_dir: PathBuf,
        hooks: Vec<HookRule>,
        shared_embedder: Arc<crate::label_cmds::EmbedderState>,
        label_idx: Arc<crate::label_index::LabelIndexState>,
        ws_broadcaster: crate::ws_server::WsBroadcaster,
        logger: Arc<SkillLogger>,
        hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
        app: tauri::AppHandle,
    ) -> Self {
        let hooks_log = skill_data::hooks_log::HooksLog::open(&skill_dir);

        Self {
            skill_dir,
            hooks,
            label_idx,
            ws_broadcaster,
            shared_embedder,
            cache: Vec::new(),
            last_refresh_unix: 0,
            last_fired_unix: HashMap::new(),
            logger,
            hook_runtime,
            app,
            hooks_log,
        }
    }

    fn maybe_refresh(&mut self) {
        let now = crate::unix_secs();
        if now.saturating_sub(self.last_refresh_unix) < 20 {
            return;
        }
        self.last_refresh_unix = now;

        let recent_labels = load_recent_label_texts(&self.skill_dir, 180);

        // ── Phase 1: batch-embed all hook keywords while holding the lock ─────
        // Collect (hook_index, queries, embeddings) tuples.
        struct HookQueries {
            hook_idx:   usize,
            embeddings: Vec<Vec<f32>>,
        }
        let mut hook_queries: Vec<HookQueries> = Vec::new();

        {
            let mut guard = match self.shared_embedder.0.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let Some(te) = guard.as_mut() else {
                self.cache.clear();
                return;
            };

            for (idx, hook) in self.hooks.iter().enumerate().filter(|(_, h)| h.enabled) {
                let mut queries: Vec<String> = hook.keywords
                    .iter()
                    .map(|k| k.trim().to_owned())
                    .filter(|k| !k.is_empty())
                    .collect();

                if queries.is_empty() { continue; }

                for label in &recent_labels {
                    if queries.iter().any(|k| fuzzy_match(k, label)) && !queries.iter().any(|q| q == label) {
                        queries.push(label.clone());
                    }
                }

                let query_refs: Vec<&str> = queries.iter().map(String::as_str).collect();
                let embeddings = match te.embed(query_refs, None) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                hook_queries.push(HookQueries { hook_idx: idx, embeddings });
            }
        } // ── lock released here ──────────────────────────────────────────────

        // ── Phase 2: HNSW search (no lock needed) ─────────────────────────────
        let mut next_cache: Vec<HookReferenceSet> = Vec::new();

        for hq in &hook_queries {
            let hook = &self.hooks[hq.hook_idx];

            let mut neighbors: Vec<crate::label_index::LabelNeighbor> = Vec::new();
            for qvec in &hq.embeddings {
                neighbors.extend(crate::label_index::search_by_text_vec(
                    qvec,
                    6,
                    64,
                    &self.skill_dir,
                    &self.label_idx,
                ));
            }

            neighbors.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
            let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
            let mut refs: Vec<HookReference> = Vec::new();

            for n in neighbors {
                if !seen.insert(n.label_id) {
                    continue;
                }
                if let Some(eeg_ref) = crate::label_index::mean_eeg_for_window(&self.skill_dir, n.eeg_start, n.eeg_end) {
                    refs.push(HookReference {
                        emb: eeg_ref,
                        label_id: n.label_id,
                        label_text: n.text,
                        eeg_start_utc: n.eeg_start,
                    });
                }
                if refs.len() >= hook.recent_limit.clamp(10, 20) {
                    break;
                }
            }

            if !refs.is_empty() {
                next_cache.push(HookReferenceSet {
                    hook: hook.clone(),
                    refs,
                });
            }
        }

        if !next_cache.is_empty() {
            skill_log!(self.logger, "hooks", "cache refreshed: {} active hooks", next_cache.len());
        }
        self.cache = next_cache;
    }

    fn scenario_allows_fire(scenario: &str, metrics: Option<&EpochMetrics>) -> bool {
        let s = scenario.trim().to_lowercase();
        if s.is_empty() || s == "any" {
            return true;
        }
        let Some(m) = metrics else {
            return false;
        };

        match s.as_str() {
            // Elevated cognitive effort / load.
            "cognitive" => (m.cognitive_load >= 55.0) || (m.engagement >= 60.0),
            // Stress / affective strain patterns.
            "emotional" => (m.stress_index >= 55.0) || (m.mood <= 45.0) || (m.relaxation <= 35.0),
            // Physiological fatigue / strain patterns.
            "physical" => {
                (m.drowsiness >= 55.0)
                    || (m.headache_index >= 45.0)
                    || (m.migraine_index >= 45.0)
                    || (m.hr > 0.0 && (m.hr >= 105.0 || m.hr <= 52.0))
            }
            _ => true,
        }
    }

    fn maybe_fire(&mut self, embedding: &[f32], metrics: Option<&EpochMetrics>) {
        self.maybe_refresh();
        if self.cache.is_empty() {
            return;
        }
        let now = crate::unix_secs();

        for entry in &self.cache {
            if !Self::scenario_allows_fire(&entry.hook.scenario, metrics) {
                continue;
            }
            let threshold = entry.hook.distance_threshold.clamp(0.01, 1.0);
            let best = entry.refs.iter()
                .map(|r| (r, cosine_distance(embedding, &r.emb)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            let Some((best_ref, min_dist)) = best else {
                continue;
            };

            if min_dist > threshold {
                continue;
            }

            let last = self.last_fired_unix.get(&entry.hook.name).copied().unwrap_or(0);
            if now.saturating_sub(last) < 10 {
                continue;
            }

            self.last_fired_unix.insert(entry.hook.name.clone(), now);
            let ts_utc = msg_ts_utc_now();
            self.hook_runtime.lock_or_recover().insert(
                entry.hook.name.clone(),
                HookLastTrigger {
                    triggered_at_utc: ts_utc,
                    distance: min_dist,
                    label_id: Some(best_ref.label_id),
                    label_text: Some(best_ref.label_text.clone()),
                    label_eeg_start_utc: Some(best_ref.eeg_start_utc),
                },
            );

            skill_log!(
                self.logger,
                "hooks",
                "triggered hook='{}' scenario='{}' distance={:.4} label='{}' label_id={}",
                entry.hook.name,
                entry.hook.scenario,
                min_dist,
                best_ref.label_text,
                best_ref.label_id
            );

            let payload = HookBroadcastPayload {
                hook: entry.hook.name.clone(),
                context: "labels".to_owned(),
                command: entry.hook.command.clone(),
                text: entry.hook.text.clone(),
                scenario: entry.hook.scenario.clone(),
                distance: min_dist,
                label_id: best_ref.label_id,
                label_text: best_ref.label_text.clone(),
                triggered_at_utc: ts_utc,
            };
            self.ws_broadcaster.send("hook", &payload);

            // ── Audit log ─────────────────────────────────────────────────────
            if let Some(ref log) = self.hooks_log {
                use serde_json::{json, to_string};
                let hook_json   = to_string(&entry.hook).unwrap_or_default();
                let trigger_json = to_string(&json!({
                    "triggered_at_utc": ts_utc,
                    "distance":          min_dist,
                    "label_id":          best_ref.label_id,
                    "label_text":        &best_ref.label_text,
                    "label_eeg_start_utc": best_ref.eeg_start_utc,
                })).unwrap_or_default();
                let payload_json = to_string(&json!({
                    "context": "labels",
                    "command": &entry.hook.command,
                    "text":    &entry.hook.text,
                })).unwrap_or_default();
                log.record(skill_data::hooks_log::HookFireEntry {
                    triggered_at_utc: ts_utc as i64,
                    hook_json:        &hook_json,
                    trigger_json:     &trigger_json,
                    payload_json:     &payload_json,
                });
            }

            crate::send_toast(
                &self.app,
                crate::ToastLevel::Info,
                "Hook Triggered",
                &format!("{} · {}", entry.hook.name, best_ref.label_text),
            );
        }
    }
}

fn msg_ts_utc_now() -> u64 {
    yyyymmddhhmmss_utc().max(0) as u64
}

// Removed: use crate::unix_secs instead (was duplicated as unix_secs_now).

// Re-exported so callers can use `crate::eeg_embeddings::cosine_distance`.
// Prefer importing `skill_exg::cosine_distance` directly in new code.
pub(crate) use skill_exg::cosine_distance;

fn load_recent_label_texts(skill_dir: &Path, limit: usize) -> Vec<String> {
    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() {
        return Vec::new();
    }
    let Ok(conn) = skill_data::util::open_readonly(&labels_db) else {
        return Vec::new();
    };

    let max_rows = limit.clamp(10, 300) as i64;
    let Ok(mut stmt) = conn.prepare(
        "SELECT text FROM labels
         WHERE length(trim(text)) > 0
         GROUP BY text
         ORDER BY MAX(created_at) DESC
         LIMIT ?1",
    ) else {
        return Vec::new();
    };

    stmt.query_map(rusqlite::params![max_rows], |row| row.get::<_, String>(0))
        .map(|rows| {
            rows.flatten()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) use skill_exg::fuzzy_match;

// ── Background worker ─────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(super) fn embed_worker(
    rx:               mpsc::Receiver<EpochMsg>,
    skill_dir:        PathBuf,
    config:           EegModelConfig,
    status:           Arc<Mutex<EegModelStatus>>,
    cancel:           Arc<std::sync::atomic::AtomicBool>,
    reload_requested: Arc<std::sync::atomic::AtomicBool>,
    logger:           Arc<SkillLogger>,
    global_index:     Arc<Mutex<Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>>>,
    hooks:            Vec<HookRule>,
    shared_embedder:  Arc<crate::label_cmds::EmbedderState>,
    label_idx:        Arc<crate::label_index::LabelIndexState>,
    ws_broadcaster:   crate::ws_server::WsBroadcaster,
    hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
    app: tauri::AppHandle,
) {
    use burn::backend::{Wgpu, wgpu::WgpuDevice};
    use ndarray::Array2;
    use std::collections::HashMap;
    use std::time::Instant;
    use zuna_rs::{ZunaEncoder, config::DataConfig, load_from_named_tensor};

    skill_log!(logger, "embedder", "worker started — skill_dir={} backend={}", skill_dir.display(), config.model_backend);
    // Mark worker as active so the UI can distinguish "loading on GPU" from
    // "weights found but no session yet".
    status.lock_or_recover().embed_worker_active = true;

    // ── 1. Open today's DayStore immediately (files created before encoder) ───
    let mut current_date = yyyymmdd_utc();
    let mut store = DayStore::open(&skill_dir, &current_date, config.hnsw_m, config.hnsw_ef_construction, logger.clone());

    if let Some(ref s) = store {
        let mut st = status.lock_or_recover();
        st.daily_hnsw_path = s.index_path.display().to_string();
        st.daily_db_path   = s.db_path.display().to_string();
        st.embeddings_today = s.hnsw_len();
    }

    // ── 2. Locate weights — download with exponential-backoff retry ─────────
    // Backoff delays (seconds): 1 2 3 5 15 30 60 120 300 600 900 1800 1800 …
    const BACKOFF_SECS: &[u64] = &[1, 2, 3, 5, 15, 30, 60, 120, 300, 600, 900, 1800];

    let active_backend = config.model_backend.clone();

    // Resolve weights based on the selected backend.
    let resolve_fn = || -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        match active_backend {
            ExgModelBackend::Zuna => resolve_hf_weights(&config.hf_repo),
            ExgModelBackend::Luna => {
                let wf = config.luna_weights_file();
                skill_exg::resolve_luna_weights(&config.luna_hf_repo, wf)
            }
        }
    };

    let download_repo = match active_backend {
        ExgModelBackend::Zuna => config.hf_repo.clone(),
        ExgModelBackend::Luna => config.luna_hf_repo.clone(),
    };

    let weights = resolve_fn().or_else(|| {
        use std::sync::atomic::Ordering;
        use std::time::Duration;

        let mut attempt = 0u32;
        loop {
            if cancel.load(Ordering::Relaxed) {
                skill_log!(logger, "embedder", "auto-download cancelled by user — stopping retry loop");
                return None;
            }

            {
                let mut st = status.lock_or_recover();
                st.download_retry_attempt = attempt;
                st.download_retry_in_secs = 0;
            }

            if let Some(w) = download_hf_weights(&download_repo, &status, &cancel, false, &logger) {
                let mut st = status.lock_or_recover();
                st.download_retry_attempt = 0;
                st.download_retry_in_secs = 0;
                return Some(w);
            }

            if cancel.load(Ordering::Relaxed) {
                skill_log!(logger, "embedder", "download cancelled mid-attempt — stopping auto-retry");
                let mut st = status.lock_or_recover();
                st.download_retry_in_secs = 0;
                return None;
            }

            let delay = BACKOFF_SECS.get(attempt as usize).copied().unwrap_or(1800);
            attempt += 1;
            skill_log!(logger, "embedder", "download failed — retrying in {delay}s (attempt {attempt})");

            for remaining in (1..=delay).rev() {
                {
                    let mut st = status.lock_or_recover();
                    st.download_retry_in_secs = remaining;
                }
                while rx.try_recv().is_ok() {}
                std::thread::sleep(Duration::from_secs(1));
                if cancel.load(Ordering::Relaxed) {
                    skill_log!(logger, "embedder", "retry wait cancelled by user");
                    let mut st = status.lock_or_recover();
                    st.download_retry_in_secs = 0;
                    return None;
                }
                if reload_requested.load(Ordering::Relaxed) {
                    skill_log!(logger, "embedder", "reload requested during backoff wait — exiting for respawn");
                    let mut st = status.lock_or_recover();
                    st.download_retry_in_secs = 0;
                    return None;
                }
            }
            {
                let mut st = status.lock_or_recover();
                st.download_retry_in_secs = 0;
            }
        }
    });

    {
        let mut st = status.lock_or_recover();
        st.weights_found = weights.is_some();
        st.weights_path  = weights.as_ref().map(|(w, _)| w.display().to_string());
        st.active_model_backend = Some(active_backend.as_str().to_string());
    }
    if weights.is_none() {
        skill_log!(logger, "embedder", "{} weights unavailable — embeddings skipped.", active_backend);
    }

    // ── 3. Load ZUNA encoder on wgpu ─────────────────────────────────────────
    //
    // Pre-create the cubecl kernel-cache directory tree.
    // cubecl-common 0.9.0 uses dirs::home_dir().join(".cache/cubecl/0.9.0/…")
    // as its cache root.  On macOS ~/.cache/ does not exist by default, so
    // cubecl's own create_dir_all silently fails and File::create panics
    // (ENOENT).  This is invisible from a terminal (cache already built up from
    // prior runs) but always hits on a fresh .app launch.
    configure_cubecl_cache(&skill_dir);

    // If a previous worker already poisoned the wgpu device's internal mutexes,
    // skip GPU entirely — there is no recovery short of restarting the process.
    if GPU_DEVICE_POISONED.load(std::sync::atomic::Ordering::Relaxed) {
        skill_log!(logger, "embedder",
            "wgpu device poisoned from a previous panic — \
             GPU embeddings disabled for this process; metrics-only mode");
        for msg in rx {
            // Still honour reload requests even in metrics-only mode so the
            // accumulator can respawn a worker after a process restart clears
            // the poison flag.
            if reload_requested.load(std::sync::atomic::Ordering::Relaxed) {
                reload_requested.store(false, std::sync::atomic::Ordering::Relaxed);
                break;
            }
            store_metrics_only(&msg, &mut store, &status, &logger, &skill_dir, &config);
        }
        status.lock_or_recover().embed_worker_active = false;
        // No embeddings were produced, so nothing to flush into the global index.
        return;
    }

    let device = WgpuDevice::DefaultDevice;

    // ── Encoder variants ──────────────────────────────────────────────────────
    enum LoadedEncoder {
        Zuna(ZunaEncoder<Wgpu>),
        Luna(luna_rs::LunaEncoder<Wgpu>),
    }

    impl LoadedEncoder {
        fn describe(&self) -> String {
            match self {
                Self::Zuna(e) => e.describe().to_string(),
                Self::Luna(e) => e.describe(),
            }
        }
    }

    // Wrap the encoder load in `catch_unwind` so that a cubecl panic does not
    // kill the entire thread.  If it panics we mark the device poisoned and
    // fall back to metrics-only mode.
    let mut encoder: Option<LoadedEncoder> = weights.and_then(|(w, c)| {
        skill_log!(logger, "embedder", "loading {} encoder from {}", active_backend, w.display());
        let backend = active_backend.clone();
        let load_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(LoadedEncoder, f64), String> {
            match backend {
                ExgModelBackend::Zuna => {
                    ZunaEncoder::<Wgpu>::load(&c, &w, device.clone())
                        .map(|(enc, ms)| (LoadedEncoder::Zuna(enc), ms))
                        .map_err(|e| format!("{e:#}"))
                }
                ExgModelBackend::Luna => {
                    luna_rs::LunaEncoder::<Wgpu>::load(&c, &w, device.clone())
                        .map(|(enc, ms)| (LoadedEncoder::Luna(enc), ms))
                        .map_err(|e| format!("{e:#}"))
                }
            }
        }));
        match load_result {
            Ok(Ok((enc, ms))) => {
                let desc = enc.describe();
                skill_log!(logger, "embedder", "encoder ready ({ms:.0} ms) — {desc}");
                let mut st = status.lock_or_recover();
                st.encoder_loaded   = true;
                st.encoder_describe = Some(desc);
                Some(enc)
            }
            Ok(Err(e)) => {
                skill_log!(logger, "embedder", "encoder load failed: {e:#}");
                None
            }
            Err(panic_payload) => {
                skill_log!(logger, "embedder",
                    "encoder load panicked (cubecl cache issue?): {} — \
                     marking wgpu device poisoned; GPU embeddings disabled",
                    panic_msg(&panic_payload));
                GPU_DEVICE_POISONED.store(true, std::sync::atomic::Ordering::Relaxed);
                None
            }
        }
    });

    // Default channel names — overridden per-epoch from the message.
    let mut ch_names: Vec<String>               = CHANNEL_NAMES.iter().map(|s| s.to_string()).collect();
    let mut epoch_sample_rate: f32              = MUSE_SAMPLE_RATE;
    let data_cfg                                 = DataConfig::default();
    let pos_overrides: HashMap<String, [f32; 3]> = HashMap::new();

    // Embedding speed EMA (exponential moving average, alpha = 0.1).
    let mut embed_speed_ema: f64 = 0.0;
    let mut embed_speed_count: u64 = 0;

    // Counter for periodic global index saves.
    let mut global_save_counter: usize = 0;
    let mut hook_matcher = HookMatcher::new(
        skill_dir.clone(), hooks, shared_embedder, label_idx, ws_broadcaster, logger.clone(),
        hook_runtime, app,
    );

    // ── 4. Process epoch messages ─────────────────────────────────────────────
    for msg in rx {
        // If a new download completed and the UI asked for an in-place reload,
        // exit this worker cleanly.  EegAccumulator::push() detects the
        // channel disconnect and immediately respawns a fresh worker that will
        // call resolve_hf_weights (finding the newly downloaded files) and load
        // the encoder — no full app restart needed.
        if reload_requested.load(std::sync::atomic::Ordering::Relaxed) {
            skill_log!(logger, "embedder", "reload requested — exiting for in-place encoder reload");
            // Reset status so the UI shows the loading state while the new
            // worker initialises.
            {
                let mut st = status.lock_or_recover();
                st.encoder_loaded   = false;
                st.encoder_describe = None;
                st.download_needs_restart = false;
            }
            // Clear the flag so the respawned worker doesn't immediately exit too.
            reload_requested.store(false, std::sync::atomic::Ordering::Relaxed);
            break;
        }

        // Midnight UTC rollover — rotate both HNSW and SQLite.
        let today = yyyymmdd_utc();
        if today != current_date {
            skill_log!(logger, "embedder", "date rolled over {current_date} → {today}");
            current_date = today;
            store = DayStore::open(&skill_dir, &current_date, config.hnsw_m, config.hnsw_ef_construction, logger.clone());
            if let Some(ref s) = store {
                let mut st = status.lock_or_recover();
                st.daily_hnsw_path  = s.index_path.display().to_string();
                st.daily_db_path    = s.db_path.display().to_string();
                st.embeddings_today = 0;
            }
        }

        let Some(ref mut s) = store   else {
            skill_log!(logger, "embedder", "no day store — skipping epoch");
            continue;
        };

        // ── Update device-specific params from this epoch message ─────────────
        if !msg.channel_names.is_empty() {
            ch_names = msg.channel_names.clone();
        }
        if msg.sample_rate > 0.0 {
            epoch_sample_rate = msg.sample_rate;
        }

        // ── Preprocess + encode on wgpu ───────────────────────────────────────
        let flat: Vec<f32> = msg.samples.iter().flatten().copied().collect();
        let array = match Array2::from_shape_vec((EEG_CHANNELS, EMBEDDING_EPOCH_SAMPLES), flat) {
            Ok(a)  => a,
            Err(e) => { skill_log!(logger, "embedder", "array shape error: {e}"); continue; }
        };

        // If encoder is None (GPU never loaded or was poisoned), fall through
        // to metrics-only storage below.

        // ── GPU pipeline (optional — skipped when encoder is None) ─────────
        //
        // Both load_from_named_tensor (tensor prep on the wgpu device) AND
        // encode_batches (inference) can panic when cubecl's internal mutex has
        // been poisoned by an earlier panic.  A single catch_unwind covers the
        // whole pipeline:
        //   • First epoch, fresh .app launch (no ~/.cache/cubecl/):
        //       CacheFile::new → create_dir_all silently fails → File::create
        //       returns ENOENT → .unwrap() panics
        //   • Every subsequent epoch on the same poisoned device:
        //       SharedStateMap::lock → "poisoned lock" panic
        //
        // On ANY panic we mark the wgpu device permanently unusable for this
        // process lifetime (respawning the thread does not help — the global
        // wgpu device's mutexes stay poisoned) and set encoder = None so the
        // loop continues in metrics-only mode.
        let embed_start = Instant::now();
        let mean_emb: Option<Vec<f32>> = if let Some(ref enc) = encoder {
            let gpu_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                match enc {
                    LoadedEncoder::Zuna(zuna_enc) => {
                        // Pad channel names to EEG_CHANNELS so they match the array's
                        // row count.  Inactive channels (zero-filled) get synthetic
                        // names that won't match any 10-20 electrode position.
                        let mut padded_names: Vec<String> = ch_names.clone();
                        while padded_names.len() < EEG_CHANNELS {
                            padded_names.push(format!("_pad{}", padded_names.len()));
                        }
                        let ch_refs: Vec<&str> = padded_names.iter().map(|s| s.as_str()).collect();
                        let mut batches = load_from_named_tensor::<Wgpu>(
                            array, &ch_refs, epoch_sample_rate, config.data_norm,
                            &pos_overrides, &data_cfg, &device,
                        ).map_err(|e| format!("preprocess: {e:#}"))?;

                        if batches.is_empty() { return Err::<Vec<f32>, String>("empty batch".into()); }

                        let mut epochs = zuna_enc.encode_batches(batches.drain(..1).collect())
                            .map_err(|e| format!("encode: {e:#}"))?;

                        let epoch = epochs.pop().ok_or("no epoch output")?;
                        let dim   = epoch.output_dim();
                        let n_tok = epoch.n_tokens();
                        if dim == 0 || n_tok == 0 { return Err("zero dim/tokens".into()); }

                        let mut mean_emb = vec![0f32; dim];
                        for tok in epoch.embeddings.chunks(dim) {
                            for (i, &v) in tok.iter().enumerate() { mean_emb[i] += v; }
                        }
                        let scale = 1.0 / n_tok as f32;
                        for v in &mut mean_emb { *v *= scale; }
                        Ok(mean_emb)
                    }
                    LoadedEncoder::Luna(luna_enc) => {
                        // Build LUNA input: use active device channels only.
                        let n_ch = ch_names.len().min(EEG_CHANNELS);
                        let n_samples = EMBEDDING_EPOCH_SAMPLES;
                        let flat: Vec<f32> = (0..n_ch)
                            .flat_map(|ch| {
                                msg.samples[ch].iter().copied()
                            })
                            .collect();

                        let ch_refs: Vec<&str> = ch_names.iter().take(n_ch).map(|s| s.as_str()).collect();
                        let batch = luna_rs::build_batch_named::<Wgpu>(
                            flat,
                            &ch_refs,
                            n_samples,
                            &device,
                        );

                        let result = luna_enc.run_batch(&batch)
                            .map_err(|e| format!("luna encode: {e:#}"))?;

                        // LUNA output shape depends on mode:
                        // - Reconstruction: [C, T] — mean-pool over channels to get [T] then mean again
                        // - Classification: [num_classes]
                        // For embedding use, we take the full output and flatten/pool.
                        let out = &result.output;
                        if out.is_empty() { return Err("empty luna output".into()); }

                        // Use the output directly as the embedding vector.
                        // For reconstruction mode [C, T], mean-pool to get a fixed-size embedding.
                        Ok(out.clone())
                    }
                }
            }));

            match gpu_result {
                Ok(Ok(emb))  => Some(emb),
                Ok(Err(msg)) => {
                    skill_log!(logger, "embedder", "GPU pipeline error: {msg}");
                    None
                }
                Err(payload) => {
                    skill_log!(logger, "embedder",
                        "GPU pipeline panicked (cubecl — wgpu device now poisoned): {} — \
                         disabling GPU embeddings for this process",
                        panic_msg(&payload));
                    GPU_DEVICE_POISONED.store(true, std::sync::atomic::Ordering::Relaxed);
                    encoder = None;
                    None
                }
            }
        } else {
            None  // encoder unavailable — metrics-only mode
        };

        // ── Track embedding speed ─────────────────────────────────────────────
        let embed_elapsed_ms = embed_start.elapsed().as_secs_f64() * 1000.0;
        let current_speed_ms = if mean_emb.is_some() { Some(embed_elapsed_ms) } else { None };
        if let Some(ms) = current_speed_ms {
            embed_speed_count += 1;
            if embed_speed_count == 1 {
                embed_speed_ema = ms;
            } else {
                embed_speed_ema = embed_speed_ema * 0.9 + ms * 0.1;
            }
        }

        // ── Derive metrics from the band snapshot ───────────────────────────
        let metrics = msg.band_snapshot.as_ref().map(|snap| {
            let mut m = EpochMetrics::from_snapshot(snap);
            // Merge PPG-derived metrics if available
            if let Some(ref ppg) = msg.ppg_metrics {
                m.hr               = ppg.hr;
                m.rmssd            = ppg.rmssd;
                m.sdnn             = ppg.sdnn;
                m.pnn50            = ppg.pnn50;
                m.lf_hf_ratio      = ppg.lf_hf_ratio;
                m.respiratory_rate = ppg.respiratory_rate;
                m.spo2_estimate    = ppg.spo2_estimate;
                m.perfusion_index  = ppg.perfusion_index;
                m.stress_index     = ppg.stress_index;
            }
            m
        });

        // Serialise per-channel band powers as JSON for full-fidelity storage.
        let channels_json = msg.band_snapshot.as_ref().map(|snap| {
            let channels: Vec<serde_json::Value> = snap.channels.iter().map(|ch| {
                serde_json::json!({
                    "channel":        ch.channel,
                    "delta":          ch.delta,
                    "theta":          ch.theta,
                    "alpha":          ch.alpha,
                    "beta":           ch.beta,
                    "gamma":          ch.gamma,
                    "high_gamma":     ch.high_gamma,
                    "rel_delta":      ch.rel_delta,
                    "rel_theta":      ch.rel_theta,
                    "rel_alpha":      ch.rel_alpha,
                    "rel_beta":       ch.rel_beta,
                    "rel_gamma":      ch.rel_gamma,
                    "rel_high_gamma": ch.rel_high_gamma,
                    "dominant":       ch.dominant,
                })
            }).collect();
            serde_json::to_string(&channels).unwrap_or_default()
        });

        // ── Store ─────────────────────────────────────────────────────────────
        let has_metrics  = metrics.is_some();
        let has_channels = channels_json.is_some();
        let channels_len = channels_json.as_ref().map(|s| s.len()).unwrap_or(0);

        // When the GPU pipeline produced an embedding, store it in both HNSW
        // and SQLite.  When it didn't (encoder unavailable / device poisoned),
        // fall through to metrics-only SQLite storage.
        let hnsw_id = if let Some(ref emb) = mean_emb {
            let id = s.insert(
                msg.timestamp,
                msg.device_id.as_deref(),
                msg.device_name.as_deref(),
                emb,
                metrics.as_ref(),
                msg.ppg_averages.as_ref(),
                channels_json.as_deref(),
                Some(active_backend.as_str()),
                current_speed_ms,
            );

            // ── Also insert into the persistent cross-day global HNSW ─────
            // The global index accumulates every embedding across all days so
            // that a single HNSW search can find near-neighbors from any date.
            // The payload is the YYYYMMDDHHmmss timestamp; the date is derived
            // from it during search result hydration.
            {
                let mut g = global_index.lock_or_recover();
                if let Some(ref mut gidx) = *g {
                    gidx.insert(emb.clone(), msg.timestamp);
                    global_save_counter += 1;
                    if global_save_counter >= GLOBAL_HNSW_SAVE_EVERY {
                        global_save_counter = 0;
                        global_eeg_index::save_index(gidx, &skill_dir);
                    }
                }
            }

            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hook_matcher.maybe_fire(emb, metrics.as_ref());
            }))
            .map_err(|p| {
                skill_log!(
                    logger,
                    "hooks",
                    "hook matcher panicked; continuing embed worker: {}",
                    panic_msg(&p)
                );
            });

            id
        } else {
            // Metrics-only path: write a SQLite row without an embedding or
            // HNSW entry so that band/sleep metrics are still persisted.
            s.insert_metrics_only(
                msg.timestamp,
                msg.device_id.as_deref(),
                msg.device_name.as_deref(),
                metrics.as_ref(),
                msg.ppg_averages.as_ref(),
                channels_json.as_deref(),
            )
        };
        if !has_metrics || !has_channels {
            eprintln!(
                "[embed] ⚠ band data incomplete: metrics={has_metrics} channels_json={has_channels} (len={channels_len}) — band_snapshot was {}",
                if msg.band_snapshot.is_some() { "Some" } else { "None" }
            );
        }

        // Verify the row was stored correctly (first 3 rows only, to avoid log spam).
        let total = s.hnsw_len();
        if total <= 3 {
            match s.conn.query_row(
                "SELECT json_extract(metrics_json,'$.tar'), json_extract(metrics_json,'$.bar'), metrics_json IS NOT NULL, json_extract(metrics_json,'$.rel_high_gamma'), json_extract(metrics_json,'$.tbr'), json_extract(metrics_json,'$.sef95'), json_extract(metrics_json,'$.hjorth_activity'), json_extract(metrics_json,'$.permutation_entropy'), json_extract(metrics_json,'$.higuchi_fd'), json_extract(metrics_json,'$.dfa_exponent'), json_extract(metrics_json,'$.sample_entropy'), json_extract(metrics_json,'$.pac_theta_gamma'), json_extract(metrics_json,'$.laterality_index') FROM embeddings WHERE id = (SELECT MAX(id) FROM embeddings)",
                [],
                |row| Ok((
                    row.get::<_, Option<f64>>(0).ok().flatten(),
                    row.get::<_, Option<f64>>(1).ok().flatten(),
                    row.get::<_, bool>(2).unwrap_or(false),
                    row.get::<_, Option<f64>>(3).ok().flatten(),
                    row.get::<_, Option<f64>>(4).ok().flatten(),
                    row.get::<_, Option<f64>>(5).ok().flatten(),
                    row.get::<_, Option<f64>>(6).ok().flatten(),
                    row.get::<_, Option<f64>>(7).ok().flatten(),
                    row.get::<_, Option<f64>>(8).ok().flatten(),
                    row.get::<_, Option<f64>>(9).ok().flatten(),
                    row.get::<_, Option<f64>>(10).ok().flatten(),
                    row.get::<_, Option<f64>>(11).ok().flatten(),
                    row.get::<_, Option<f64>>(12).ok().flatten(),
                )),
            ) {
                Ok((tar, bar, has_json, rhg, tbr, sef95, ha, pe, hfd, dfa, se, pac, lat)) => {
                    eprintln!(
                        "[embed] ✓ verify row: tar={tar:?} bar={bar:?} has_channels_json={has_json} rel_high_gamma={rhg:?} tbr={tbr:?} sef95={sef95:?} hjorth_activity={ha:?} pe={pe:?} hfd={hfd:?} dfa={dfa:?} se={se:?} pac={pac:?} lat={lat:?}"
                    );
                }
                Err(e) => skill_log!(logger, "embedder", "✗ verify query failed: {e}"),
            }
        }
        {
            let mut st = status.lock_or_recover();
            st.embeddings_today = total;
            if let Some(ms) = current_speed_ms {
                st.last_embed_ms = ms;
                st.avg_embed_ms  = embed_speed_ema;
            }
            // Publish latest epoch metrics so the WS status command can return them.
            st.latest_metrics = metrics.as_ref().map(|m| {
                skill_eeg::eeg_model_config::LatestEpochMetrics {
                    rel_delta:        m.rel_delta,
                    rel_theta:        m.rel_theta,
                    rel_alpha:        m.rel_alpha,
                    rel_beta:         m.rel_beta,
                    rel_gamma:        m.rel_gamma,
                    rel_high_gamma:   m.rel_high_gamma,
                    relaxation_score: m.relaxation,
                    engagement_score: m.engagement,
                    faa:              m.faa,
                    tar:              m.tar,
                    bar:              m.bar,
                    dtr:              m.dtr,
                    pse:              m.pse,
                    apf:              m.apf,
                    bps:              m.bps,
                    snr:              m.snr,
                    coherence:        m.coherence,
                    mu_suppression:   m.mu_suppression,
                    mood:             m.mood,
                    tbr:              m.tbr,
                    sef95:            m.sef95,
                    spectral_centroid: m.spectral_centroid,
                    hjorth_activity:  m.hjorth_activity,
                    hjorth_mobility:  m.hjorth_mobility,
                    hjorth_complexity: m.hjorth_complexity,
                    permutation_entropy: m.permutation_entropy,
                    higuchi_fd:       m.higuchi_fd,
                    dfa_exponent:     m.dfa_exponent,
                    sample_entropy:   m.sample_entropy,
                    pac_theta_gamma:  m.pac_theta_gamma,
                    laterality_index: m.laterality_index,
                    hr:               m.hr,
                    rmssd:            m.rmssd,
                    sdnn:             m.sdnn,
                    pnn50:            m.pnn50,
                    lf_hf_ratio:      m.lf_hf_ratio,
                    respiratory_rate: m.respiratory_rate,
                    spo2_estimate:    m.spo2_estimate,
                    perfusion_index:  m.perfusion_index,
                    stress_index:     m.stress_index,
                    blink_count:      m.blink_count,
                    blink_rate:       m.blink_rate,
                    head_pitch:       m.head_pitch,
                    head_roll:        m.head_roll,
                    stillness:        m.stillness,
                    nod_count:        m.nod_count,
                    shake_count:      m.shake_count,
                    meditation:       m.meditation,
                    cognitive_load:   m.cognitive_load,
                    drowsiness:       m.drowsiness,
                    headache_index:         m.headache_index,
                    migraine_index:         m.migraine_index,
                    consciousness_lzc:          m.consciousness_lzc,
                    consciousness_wakefulness:  m.consciousness_wakefulness,
                    consciousness_integration:  m.consciousness_integration,
                    epoch_timestamp:  msg.timestamp,
                }
            });
        }

        let dim = mean_emb.as_ref().map(|e| e.len()).unwrap_or(0);
        let speed_str = current_speed_ms.map(|ms| format!(" {ms:.1}ms (avg {embed_speed_ema:.1}ms)")).unwrap_or_default();
        if let Some(ref m) = metrics {
            eprintln!(
                "[embed] #{hnsw_id} ts={} dev={} dim={dim} model={}{speed_str} relax={:.0} engage={:.0} faa={:.3} tar={:.2} bar={:.2} dtr={:.2} pse={:.2} apf={:.1} bps={:.2} snr={:.1} coh={:.3} mu={:.3} mood={:.0}",
                msg.timestamp,
                msg.device_name.as_deref().unwrap_or("?"),
                active_backend,
                m.relaxation, m.engagement, m.faa,
                m.tar, m.bar, m.dtr, m.pse, m.apf, m.bps, m.snr,
                m.coherence, m.mu_suppression, m.mood,
            );
        } else {
            eprintln!(
                "[embed] #{hnsw_id} ts={} dev={} dim={dim} model={}{speed_str} (no band data — metrics will be NULL)",
                msg.timestamp,
                msg.device_name.as_deref().unwrap_or("?"),
                active_backend,
            );
        }
    }

    // Final flush: persist any unsaved insertions to the global index.
    {
        let g = global_index.lock_or_recover();
        if let Some(ref gidx) = *g {
            global_eeg_index::save_index(gidx, &skill_dir);
            skill_log!(logger, "embedder", "global HNSW flushed on exit ({} entries)", gidx.len());
        }
    }

    status.lock_or_recover().embed_worker_active = false;
    skill_log!(logger, "embedder", "worker exiting");
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Metrics-only drain used by the early-exit path when the wgpu device is
/// permanently poisoned from a previous panic in this process.
fn store_metrics_only(
    msg:       &EpochMsg,
    store:     &mut Option<DayStore>,
    status:    &Arc<Mutex<EegModelStatus>>,
    logger:    &Arc<SkillLogger>,
    skill_dir: &Path,
    config:    &EegModelConfig,
) {
    // Rotate DayStore on midnight UTC rollover.
    let today = yyyymmdd_utc();
    let needs_rotate = store.as_ref()
        .map(|s| !s.db_path.to_string_lossy().contains(&today))
        .unwrap_or(true);
    if needs_rotate {
        *store = DayStore::open(skill_dir, &today, config.hnsw_m, config.hnsw_ef_construction, logger.clone());
    }
    let Some(ref mut s) = store else { return; };

    let metrics = msg.band_snapshot.as_ref().map(|snap| {
        let mut m = EpochMetrics::from_snapshot(snap);
        if let Some(ref ppg) = msg.ppg_metrics {
            m.hr               = ppg.hr;
            m.rmssd            = ppg.rmssd;
            m.sdnn             = ppg.sdnn;
            m.pnn50            = ppg.pnn50;
            m.lf_hf_ratio      = ppg.lf_hf_ratio;
            m.respiratory_rate = ppg.respiratory_rate;
            m.spo2_estimate    = ppg.spo2_estimate;
            m.perfusion_index  = ppg.perfusion_index;
            m.stress_index     = ppg.stress_index;
        }
        m
    });

    let channels_json = msg.band_snapshot.as_ref().map(|snap| {
        let channels: Vec<serde_json::Value> = snap.channels.iter().map(|ch| {
            serde_json::json!({
                "channel": ch.channel, "delta": ch.delta, "theta": ch.theta,
                "alpha": ch.alpha, "beta": ch.beta, "gamma": ch.gamma,
                "high_gamma": ch.high_gamma, "rel_delta": ch.rel_delta,
                "rel_theta": ch.rel_theta, "rel_alpha": ch.rel_alpha,
                "rel_beta": ch.rel_beta, "rel_gamma": ch.rel_gamma,
                "rel_high_gamma": ch.rel_high_gamma, "dominant": ch.dominant,
            })
        }).collect();
        serde_json::to_string(&channels).unwrap_or_default()
    });

    s.insert_metrics_only(
        msg.timestamp,
        msg.device_id.as_deref(),
        msg.device_name.as_deref(),
        metrics.as_ref(),
        msg.ppg_averages.as_ref(),
        channels_json.as_deref(),
    );

    // Keep status.latest_metrics fresh so the UI doesn't go stale.
    if let Some(ref m) = metrics {
        let mut st = status.lock_or_recover();
        st.latest_metrics = Some(skill_eeg::eeg_model_config::LatestEpochMetrics {
            rel_delta: m.rel_delta, rel_theta: m.rel_theta, rel_alpha: m.rel_alpha,
            rel_beta: m.rel_beta, rel_gamma: m.rel_gamma, rel_high_gamma: m.rel_high_gamma,
            relaxation_score: m.relaxation, engagement_score: m.engagement,
            faa: m.faa, tar: m.tar, bar: m.bar, dtr: m.dtr, pse: m.pse,
            apf: m.apf, bps: m.bps, snr: m.snr, coherence: m.coherence,
            mu_suppression: m.mu_suppression, mood: m.mood, tbr: m.tbr,
            sef95: m.sef95, spectral_centroid: m.spectral_centroid,
            hjorth_activity: m.hjorth_activity, hjorth_mobility: m.hjorth_mobility,
            hjorth_complexity: m.hjorth_complexity,
            permutation_entropy: m.permutation_entropy,
            higuchi_fd: m.higuchi_fd, dfa_exponent: m.dfa_exponent,
            sample_entropy: m.sample_entropy, pac_theta_gamma: m.pac_theta_gamma,
            laterality_index: m.laterality_index,
            hr: m.hr, rmssd: m.rmssd, sdnn: m.sdnn, pnn50: m.pnn50,
            lf_hf_ratio: m.lf_hf_ratio, respiratory_rate: m.respiratory_rate,
            spo2_estimate: m.spo2_estimate, perfusion_index: m.perfusion_index,
            stress_index: m.stress_index,
            blink_count: m.blink_count, blink_rate: m.blink_rate,
            head_pitch: m.head_pitch, head_roll: m.head_roll,
            stillness: m.stillness, nod_count: m.nod_count, shake_count: m.shake_count,
            meditation: m.meditation, cognitive_load: m.cognitive_load,
            drowsiness: m.drowsiness, headache_index: m.headache_index,
            migraine_index: m.migraine_index,
            consciousness_lzc: m.consciousness_lzc,
            consciousness_wakefulness: m.consciousness_wakefulness,
            consciousness_integration: m.consciousness_integration,
            epoch_timestamp: msg.timestamp,
        });
    }
}

fn resolve_hf_weights(hf_repo: &str) -> Option<(PathBuf, PathBuf)> {
    skill_exg::resolve_hf_weights(hf_repo)
}

/// Resolve LUNA weights — searches for the variant-specific safetensors file.
#[allow(dead_code)]
fn resolve_luna_weights(hf_repo: &str, weights_file: &str) -> Option<(PathBuf, PathBuf)> {
    skill_exg::resolve_luna_weights(hf_repo, weights_file)
}

/// Download ZUNA weights from HuggingFace Hub using the `hf-hub` crate.
///
/// Called automatically by [`embed_worker`] when [`resolve_hf_weights`]
/// returns `None`, and also directly by the `trigger_weights_download` Tauri
/// command when the user presses the Download / Retry button.
///
/// Files are saved into the standard HF disk cache
/// (`~/.cache/huggingface/hub` or `$HF_HOME`), so a subsequent call to
/// [`resolve_hf_weights`] will find them without re-downloading.
///
/// * `cancel` — setting this `AtomicBool` to `true` aborts between the two
///   file downloads (config.json first, then the large safetensors).
/// * `mark_needs_restart` — when `true` and the download succeeds, sets
///   [`EegModelStatus::download_needs_restart`] so the UI can prompt the
///   user to restart the app and load the freshly downloaded encoder.
///   Pass `false` from [`embed_worker`] because the startup path loads the
///   encoder immediately after the download returns.
pub(crate) fn download_hf_weights(
    hf_repo:            &str,
    status:             &Arc<Mutex<EegModelStatus>>,
    cancel:             &Arc<std::sync::atomic::AtomicBool>,
    mark_needs_restart: bool,
    _logger:            &Arc<SkillLogger>,
) -> Option<(PathBuf, PathBuf)> {
    // Delegate to skill_exg (logger replaced with eprintln! in the crate).
    skill_exg::download_hf_weights(hf_repo, status, cancel, mark_needs_restart)
}

