// SPDX-License-Identifier: GPL-3.0-only
//! Background embedding worker thread.
//!
//! Receives `EpochMsg` from the accumulator, runs the configured encoder
//! (ZUNA wgpu, LUNA, NeuroRVQ, …), stores results in the day store, and
//! evaluates proactive hook triggers against the live embedding stream.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use skill_daemon_common::EventEnvelope;
use skill_eeg::eeg_model_config::{ExgModelBackend, ExgModelConfig};
#[cfg(feature = "embed-neurorvq")]
use skill_exg::neurorvq::{Modality as NeuroModality, NeuroRVQFM};
use skill_settings::HookRule;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use super::accumulator::EpochMsg;
use super::day_store::DayStore;

/// Handle to the background embed worker.  Dropping it signals the worker
/// to shut down (the sender half of the channel is dropped).
pub(crate) struct EmbedWorkerHandle {
    pub tx: mpsc::SyncSender<EpochMsg>,
    _thread: std::thread::JoinHandle<()>,
}

impl EmbedWorkerHandle {
    /// Spawn the embed worker thread.
    pub fn spawn(
        skill_dir: PathBuf,
        config: ExgModelConfig,
        events_tx: broadcast::Sender<EventEnvelope>,
        hooks: Vec<HookRule>,
        text_embedder: crate::text_embedder::SharedTextEmbedder,
    ) -> Self {
        // Keep a larger pre-encoder buffer so epochs are not dropped while
        // heavy models (e.g. ZUNA) are still loading.
        let (tx, rx) = mpsc::sync_channel::<EpochMsg>(128);
        let thread = std::thread::Builder::new()
            .name("eeg-embed".into())
            .spawn(move || {
                embed_worker_main(rx, skill_dir, config, events_tx, hooks, text_embedder);
            })
            .expect("failed to spawn embed worker thread");

        Self { tx, _thread: thread }
    }
}

/// Compute YYYYMMDD string for today (UTC).
fn yyyymmdd_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}")
}

fn day_dir(skill_dir: &Path) -> PathBuf {
    let date = yyyymmdd_utc();
    let dir = skill_dir.join(&date);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn broadcast_ev(tx: &broadcast::Sender<EventEnvelope>, event_type: &str, payload: serde_json::Value) {
    let _ = tx.send(EventEnvelope {
        r#type: event_type.to_string(),
        ts_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        correlation_id: None,
        payload,
    });
}

// ── Hook matcher ──────────────────────────────────────────────────────────────

struct HookReference {
    emb: Vec<f32>,
    label_id: i64,
    label_text: String,
}

struct HookReferenceSet {
    hook: HookRule,
    refs: Vec<HookReference>,
}

struct HookMatcher {
    skill_dir: PathBuf,
    hooks: Vec<HookRule>,
    label_state: skill_label_index::LabelIndexState,
    text_embedder: crate::text_embedder::SharedTextEmbedder,
    cache: Vec<HookReferenceSet>,
    last_refresh_unix: u64,
    last_fired_unix: HashMap<String, u64>,
    hooks_log: Option<skill_data::hooks_log::HooksLog>,
    events_tx: broadcast::Sender<EventEnvelope>,
}

impl HookMatcher {
    fn new(
        skill_dir: PathBuf,
        hooks: Vec<HookRule>,
        events_tx: broadcast::Sender<EventEnvelope>,
        text_embedder: crate::text_embedder::SharedTextEmbedder,
    ) -> Self {
        let hooks_log = skill_data::hooks_log::HooksLog::open(&skill_dir);
        let label_state = skill_label_index::LabelIndexState::new();
        label_state.load(&skill_dir);

        Self {
            skill_dir,
            hooks,
            label_state,
            text_embedder,
            cache: Vec::new(),
            last_refresh_unix: 0,
            last_fired_unix: HashMap::new(),
            hooks_log,
            events_tx,
        }
    }

    /// Periodically refresh the hook reference cache (keyword → label → EEG embeddings).
    fn maybe_refresh(&mut self) {
        let now = unix_secs();
        if now.saturating_sub(self.last_refresh_unix) < 20 {
            return;
        }
        self.last_refresh_unix = now;

        let mut next_cache: Vec<HookReferenceSet> = Vec::new();

        for hook in self.hooks.iter().filter(|h| h.enabled) {
            let queries: Vec<String> = hook
                .keywords
                .iter()
                .map(|k| k.trim().to_owned())
                .filter(|k| !k.is_empty())
                .collect();
            if queries.is_empty() {
                continue;
            }

            // Embed keywords.
            let query_refs: Vec<&str> = queries.iter().map(String::as_str).collect();
            let Some(embeddings) = self.text_embedder.embed_batch(query_refs) else {
                continue;
            };

            // Search label index for each keyword embedding.
            let mut refs: Vec<HookReference> = Vec::new();
            let mut seen = std::collections::HashSet::new();

            for qvec in &embeddings {
                let neighbors = skill_label_index::search_by_text_vec(qvec, 6, 64, &self.skill_dir, &self.label_state);
                for n in neighbors {
                    if !seen.insert(n.label_id) {
                        continue;
                    }
                    if let Some(eeg_ref) =
                        skill_label_index::mean_eeg_for_window(&self.skill_dir, n.eeg_start, n.eeg_end)
                    {
                        refs.push(HookReference {
                            emb: eeg_ref,
                            label_id: n.label_id,
                            label_text: n.text,
                        });
                    }
                    if refs.len() >= hook.recent_limit.clamp(10, 20) {
                        break;
                    }
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
            info!(hooks = next_cache.len(), "hook cache refreshed");
        }
        self.cache = next_cache;
    }

    /// Check whether the scenario allows firing based on current metrics.
    fn scenario_allows(scenario: &str, metrics: Option<&skill_exg::EpochMetrics>) -> bool {
        let s = scenario.trim().to_lowercase();
        if s.is_empty() || s == "any" {
            return true;
        }
        let Some(m) = metrics else { return false };
        match s.as_str() {
            "cognitive" => m.cognitive_load >= 55.0 || m.engagement >= 60.0,
            "emotional" => m.stress_index >= 55.0 || m.mood <= 45.0 || m.relaxation <= 35.0,
            "physical" => {
                m.drowsiness >= 55.0
                    || m.headache_index >= 45.0
                    || m.migraine_index >= 45.0
                    || (m.hr > 0.0 && (m.hr >= 105.0 || m.hr <= 52.0))
            }
            _ => true,
        }
    }

    /// Evaluate all hooks against the current embedding.
    fn maybe_fire(&mut self, embedding: &[f32], metrics: Option<&skill_exg::EpochMetrics>) {
        self.maybe_refresh();
        if self.cache.is_empty() {
            return;
        }
        let now = unix_secs();

        for entry in &self.cache {
            if !Self::scenario_allows(&entry.hook.scenario, metrics) {
                continue;
            }
            let threshold = entry.hook.distance_threshold.clamp(0.01, 1.0);
            let best = entry
                .refs
                .iter()
                .map(|r| (r, skill_exg::cosine_distance(embedding, &r.emb)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            let Some((best_ref, min_dist)) = best else { continue };
            if min_dist > threshold {
                continue;
            }

            // Rate limit: once per 10 seconds per hook.
            let last = self.last_fired_unix.get(&entry.hook.name).copied().unwrap_or(0);
            if now.saturating_sub(last) < 10 {
                continue;
            }
            self.last_fired_unix.insert(entry.hook.name.clone(), now);

            let ts_utc = skill_exg::yyyymmddhhmmss_utc();

            info!(
                hook = %entry.hook.name,
                scenario = %entry.hook.scenario,
                distance = min_dist,
                label = %best_ref.label_text,
                "hook triggered"
            );

            // Broadcast hook event to WS clients.
            broadcast_ev(
                &self.events_tx,
                "hook",
                serde_json::json!({
                    "hook": entry.hook.name,
                    "context": "labels",
                    "command": entry.hook.command,
                    "text": entry.hook.text,
                    "scenario": entry.hook.scenario,
                    "distance": min_dist,
                    "label_id": best_ref.label_id,
                    "label_text": best_ref.label_text,
                    "triggered_at_utc": ts_utc,
                }),
            );

            // Audit log.
            if let Some(ref log) = self.hooks_log {
                let hook_json = serde_json::to_string(&entry.hook).unwrap_or_default();
                let trigger_json = serde_json::to_string(&serde_json::json!({
                    "triggered_at_utc": ts_utc,
                    "distance": min_dist,
                    "label_id": best_ref.label_id,
                    "label_text": &best_ref.label_text,
                }))
                .unwrap_or_default();
                let payload_json = serde_json::to_string(&serde_json::json!({
                    "context": "labels",
                    "command": &entry.hook.command,
                    "text": &entry.hook.text,
                }))
                .unwrap_or_default();
                log.record(&skill_data::hooks_log::HookFireEntry {
                    triggered_at_utc: ts_utc,
                    hook_json: &hook_json,
                    trigger_json: &trigger_json,
                    payload_json: &payload_json,
                });
            }
        }
    }
}

// ── Main worker loop ──────────────────────────────────────────────────────────

fn embed_worker_main(
    rx: mpsc::Receiver<EpochMsg>,
    skill_dir: PathBuf,
    config: ExgModelConfig,
    events_tx: broadcast::Sender<EventEnvelope>,
    hooks: Vec<HookRule>,
    text_embedder: crate::text_embedder::SharedTextEmbedder,
) {
    info!(
        backend = config.model_backend.as_str(),
        repo = %config.hf_repo,
        "embed worker started"
    );

    // Open today's day store.
    let mut current_date = yyyymmdd_utc();
    let mut store = DayStore::open(&day_dir(&skill_dir), config.hnsw_m, config.hnsw_ef_construction);

    if let Some(ref s) = store {
        info!(
            hnsw_len = s.hnsw_len(),
            db = %s.db_path.display(),
            "day store opened"
        );

        if s.hnsw_rebuilt() {
            let recovered = s.hnsw_rebuilt_count();
            warn!(
                path = %s.index_path.display(),
                recovered_embeddings = recovered,
                "HNSW index was rebuilt after load failure; search quality may be temporarily reduced until enough new embeddings are inserted"
            );
            broadcast_ev(
                &events_tx,
                "EmbedWorkerWarning",
                serde_json::json!({
                    "code": "hnsw_rebuilt",
                    "path": s.index_path.display().to_string(),
                    "recovered_embeddings": recovered,
                    "message": "HNSW index was rebuilt after load failure; search quality may be temporarily reduced until enough new embeddings are inserted"
                }),
            );
        }
    }

    // Load encoder.
    let encoder = load_encoder(&config, &skill_dir);

    // Initialize hook matcher.
    let mut hook_matcher = if hooks.iter().any(|h| h.enabled) {
        Some(HookMatcher::new(
            skill_dir.clone(),
            hooks,
            events_tx.clone(),
            text_embedder,
        ))
    } else {
        None
    };

    let (hnsw_rebuilt, recovered_embeddings) = store
        .as_ref()
        .map(|s| (s.hnsw_rebuilt(), s.hnsw_rebuilt_count()))
        .unwrap_or((false, 0));

    if hnsw_rebuilt {
        info!(recovered_embeddings, "startup HNSW recovery summary");
    }

    broadcast_ev(
        &events_tx,
        "EmbedWorkerStatus",
        serde_json::json!({
            "status": if encoder.is_some() { "ready" } else { "metrics_only" },
            "backend": config.model_backend.as_str(),
            "hnsw_rebuilt": hnsw_rebuilt,
            "recovered_embeddings": recovered_embeddings,
        }),
    );

    let mut epoch_count = 0u64;
    let mut save_counter = 0u32;
    let mut metrics_only_streak = 0u64;

    for msg in rx.iter() {
        // Roll over to new day if needed.
        let today = yyyymmdd_utc();
        if today != current_date {
            if let Some(ref s) = store {
                s.save_hnsw();
            }
            current_date = today;
            store = DayStore::open(&day_dir(&skill_dir), config.hnsw_m, config.hnsw_ef_construction);
            info!("day store rolled to {current_date}");
            if let Some(ref s) = store {
                if s.hnsw_rebuilt() {
                    let recovered = s.hnsw_rebuilt_count();
                    warn!(
                        path = %s.index_path.display(),
                        recovered_embeddings = recovered,
                        "HNSW index was rebuilt after load failure; search quality may be temporarily reduced until enough new embeddings are inserted"
                    );
                    broadcast_ev(
                        &events_tx,
                        "EmbedWorkerWarning",
                        serde_json::json!({
                            "code": "hnsw_rebuilt",
                            "path": s.index_path.display().to_string(),
                            "recovered_embeddings": recovered,
                            "message": "HNSW index was rebuilt after load failure; search quality may be temporarily reduced until enough new embeddings are inserted"
                        }),
                    );
                }
            }
        }

        // Compute epoch metrics from band snapshot.
        let metrics = msg.band_snapshot.as_ref().map(skill_exg::EpochMetrics::from_snapshot);

        let ts_ms = msg.timestamp * 1000;

        // Encode the epoch.
        let t0 = std::time::Instant::now();
        let embedding = encoder.as_ref().and_then(|enc| encode_epoch(enc, &msg));
        let embed_ms = t0.elapsed().as_millis();

        if embedding.is_none() && encoder.is_some() {
            metrics_only_streak += 1;
            // Log every 100 consecutive failures, plus the first one.
            if metrics_only_streak == 1 || metrics_only_streak % 100 == 0 {
                tracing::warn!(
                    ts = ts_ms,
                    streak = metrics_only_streak,
                    "[embed] encoder returned None — storing metrics-only (streak: {metrics_only_streak})"
                );
            }
        } else if embedding.is_some() {
            if metrics_only_streak > 0 {
                tracing::info!(
                    streak = metrics_only_streak,
                    "[embed] encoder recovered after {metrics_only_streak} metrics-only epochs"
                );
                metrics_only_streak = 0;
            }
            if embed_ms > 2000 {
                tracing::warn!(ms = embed_ms, "[embed] slow encode: {embed_ms}ms");
            }
        } else if encoder.is_none() && epoch_count == 0 {
            tracing::warn!("[embed] no encoder loaded — all epochs will be metrics-only");
        }
        epoch_count += 1;

        // Store in day store.
        if let Some(ref mut s) = store {
            if let Some(ref emb) = embedding {
                s.insert(ts_ms, msg.device_name.as_deref(), emb, metrics.as_ref());
            } else if let Some(ref m) = metrics {
                s.insert_metrics_only(ts_ms, msg.device_name.as_deref(), m);
            }
        }

        // Evaluate hook triggers.
        if let (Some(ref mut matcher), Some(ref emb)) = (&mut hook_matcher, &embedding) {
            matcher.maybe_fire(emb, metrics.as_ref());
        }

        // Broadcast embedding event.
        if embedding.is_some() {
            broadcast_ev(
                &events_tx,
                "EegEmbedding",
                serde_json::json!({
                    "timestamp": msg.timestamp,
                    "dim": embedding.as_ref().map(Vec::len).unwrap_or(0),
                    "epoch": epoch_count,
                }),
            );
        }

        // Periodically save HNSW.
        save_counter += 1;
        if save_counter >= 10 {
            if let Some(ref s) = store {
                s.save_hnsw();
            }
            save_counter = 0;
        }
    }

    if let Some(ref s) = store {
        s.save_hnsw();
    }
    info!(epochs = epoch_count, "embed worker exiting");
}

// ── Encoder loading ──────────────────────────────────────────────────────────

#[allow(dead_code)]
pub(crate) enum Encoder {
    #[cfg(feature = "embed-zuna")]
    Zuna(Box<ZunaState>),
    #[cfg(feature = "embed-zuna-gpu")]
    ZunaGpu(Box<ZunaGpuState>),
    #[cfg(feature = "embed-zuna-gpu-f16")]
    ZunaGpuF16(Box<ZunaGpuF16State>),
    #[cfg(feature = "embed-luna")]
    Luna(Box<luna_rs::LunaEncoder<burn::backend::NdArray>>),
    #[cfg(feature = "embed-reve")]
    Reve(Box<reve_rs::ReveEncoder<burn::backend::NdArray>>),
    #[cfg(feature = "embed-osf")]
    Osf(Box<osf_rs::OsfEncoder<burn::backend::NdArray>>),
    #[cfg(feature = "embed-sleepfm")]
    SleepFM(Box<sleepfm::SleepFmEncoder<burn::backend::NdArray>>),
    #[cfg(feature = "embed-sleeplm")]
    SleepLM(Box<sleeplm::SleepLMEncoder<burn::backend::NdArray>>),
    #[cfg(feature = "embed-steegformer")]
    STEEGFormer(Box<steegformer::STEEGFormerEncoder<burn::backend::NdArray>>),
    #[cfg(feature = "embed-tribev2")]
    Tribev2(Box<Tribev2State>),
    #[cfg(feature = "embed-neurorvq")]
    NeuroRVQ(Box<NeuroRVQState>),
    None,
}

#[cfg(feature = "embed-tribev2")]
pub(crate) struct Tribev2State {
    _placeholder: (),
}

#[cfg(feature = "embed-zuna")]
pub(crate) struct ZunaState {
    encoder: zuna_rs::ZunaEncoder<burn::backend::NdArray>,
    data_config: zuna_rs::config::DataConfig,
}

#[cfg(feature = "embed-zuna-gpu")]
pub(crate) struct ZunaGpuState {
    encoder: zuna_rs::ZunaEncoder<burn::backend::wgpu::Wgpu>,
    data_config: zuna_rs::config::DataConfig,
}

#[cfg(feature = "embed-zuna-gpu-f16")]
#[allow(dead_code)]
pub(crate) struct ZunaGpuF16State {
    encoder: zuna_rs::ZunaEncoder<burn::backend::wgpu::Wgpu<half::f16, i32, u32>>,
    data_config: zuna_rs::config::DataConfig,
}

#[cfg(feature = "embed-neurorvq")]
pub(crate) struct NeuroRVQState {
    model: NeuroRVQFM,
}

fn load_encoder(config: &ExgModelConfig, _skill_dir: &Path) -> Option<Encoder> {
    let use_gpu = skill_settings::load_settings(_skill_dir).exg_inference_device != "cpu";
    let backend = config.model_backend.clone();
    info!(backend = backend.as_str(), gpu = use_gpu, "loading EXG encoder");
    let result = match &backend {
        #[cfg(feature = "embed-neurorvq")]
        ExgModelBackend::Neurorvq => {
            info!("loading NeuroRVQ encoder");
            match NeuroRVQFM::from_default_hf(NeuroModality::EEG) {
                Ok(model) => {
                    info!("NeuroRVQ encoder loaded");
                    Some(Encoder::NeuroRVQ(Box::new(NeuroRVQState { model })))
                }
                Err(e) => {
                    warn!(%e, "NeuroRVQ load failed — metrics-only");
                    None
                }
            }
        }
        #[cfg(not(feature = "embed-neurorvq"))]
        ExgModelBackend::Neurorvq => {
            warn!("NeuroRVQ backend selected but support is not compiled (enable feature: embed-neurorvq)");
            None
        }
        #[cfg(feature = "embed-zuna")]
        ExgModelBackend::Zuna => {
            info!(repo = %config.hf_repo, "loading ZUNA encoder");
            // Try GPU backends first when user selects GPU.
            #[cfg(feature = "embed-zuna-gpu-f16")]
            if use_gpu {
                if let Some(s) = load_zuna_gpu_f16(config) {
                    info!("ZUNA GPU f16 encoder loaded");
                    return Some(Encoder::ZunaGpuF16(Box::new(s)));
                }
                warn!("GPU f16 unavailable, trying GPU f32");
            }
            #[cfg(feature = "embed-zuna-gpu")]
            if use_gpu {
                if let Some(s) = load_zuna_gpu(config) {
                    info!("ZUNA GPU encoder loaded");
                    return Some(Encoder::ZunaGpu(Box::new(s)));
                }
                warn!("GPU f32 unavailable, falling back to CPU");
            }
            load_zuna(config)
                .map(|s| {
                    info!("ZUNA encoder loaded");
                    Encoder::Zuna(Box::new(s))
                })
                .or_else(|| {
                    warn!("ZUNA weights not found — metrics-only");
                    None
                })
        }
        #[cfg(feature = "embed-luna")]
        ExgModelBackend::Luna => {
            let wf = config.luna_weights_file();
            skill_exg::resolve_luna_weights(&config.luna_hf_repo, wf).and_then(|(w, c)| {
                let device = burn::backend::ndarray::NdArrayDevice::Cpu;
                luna_rs::LunaEncoder::<burn::backend::NdArray>::load(&c, &w, device)
                    .ok()
                    .map(|(enc, _)| Encoder::Luna(Box::new(enc)))
            })
        }
        #[cfg(feature = "embed-reve")]
        ExgModelBackend::Reve => resolve_catalog_hf("reve-base").and_then(|(w, c)| {
            let device = burn::backend::ndarray::NdArrayDevice::Cpu;
            reve_rs::ReveEncoder::<burn::backend::NdArray>::load(&c, &w, device)
                .ok()
                .map(|(enc, _)| Encoder::Reve(Box::new(enc)))
        }),
        #[cfg(feature = "embed-osf")]
        ExgModelBackend::Osf => resolve_catalog_hf("osf-base").and_then(|(w, c)| {
            let device = burn::backend::ndarray::NdArrayDevice::Cpu;
            osf_rs::OsfEncoder::<burn::backend::NdArray>::load(&c, &w, device)
                .ok()
                .map(|(enc, _)| Encoder::Osf(Box::new(enc)))
        }),
        #[cfg(feature = "embed-sleepfm")]
        ExgModelBackend::Sleepfm => resolve_catalog_hf("sleepfm").and_then(|(w, c)| {
            let device = burn::backend::ndarray::NdArrayDevice::Cpu;
            sleepfm::SleepFmEncoder::<burn::backend::NdArray>::load(&c, &w, device)
                .ok()
                .map(|(enc, _)| Encoder::SleepFM(Box::new(enc)))
        }),
        #[cfg(feature = "embed-sleeplm")]
        ExgModelBackend::Sleeplm => resolve_catalog_hf("sleeplm").and_then(|(w, c)| {
            let device = burn::backend::ndarray::NdArrayDevice::Cpu;
            sleeplm::SleepLMEncoder::<burn::backend::NdArray>::load(&c, &w, device)
                .ok()
                .map(|(enc, _)| Encoder::SleepLM(Box::new(enc)))
        }),
        #[cfg(feature = "embed-steegformer")]
        ExgModelBackend::Steegformer => resolve_catalog_hf("steegformer-base").and_then(|(w, c)| {
            let device = burn::backend::ndarray::NdArrayDevice::Cpu;
            steegformer::STEEGFormerEncoder::<burn::backend::NdArray>::load(&c, &w, device)
                .ok()
                .map(|(enc, _)| Encoder::STEEGFormer(Box::new(enc)))
        }),
        #[cfg(feature = "embed-tribev2")]
        ExgModelBackend::Tribev2 => {
            // TRIBEv2 is a complex multimodal fMRI encoder.
            // Weight loading is supported but runtime encoding requires
            // the full modality pipeline.  For now, report as available
            // but fall through to metrics-only.
            info!("TRIBEv2 weights can be resolved but runtime encoding is not yet integrated");
            None
        }
        #[allow(unreachable_patterns)]
        other => {
            info!(backend = other.as_str(), "no compiled encoder for this backend");
            None
        }
    };
    if result.is_some() {
        info!(backend = backend.as_str(), "encoder loaded successfully");
    } else {
        error!(
            backend = backend.as_str(),
            "encoder FAILED to load — ALL epochs will be metrics-only (no embeddings). \
             Check model weights are downloaded and the backend feature is compiled."
        );
    }
    result
}

/// Resolve weights+config from HF cache using the exg_catalog.json family ID.
#[allow(dead_code)]
fn resolve_catalog_hf(family_id: &str) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../../src-tauri/exg_catalog.json")).ok()?;
    let fam = catalog.get("families")?.get(family_id)?;
    let repo = fam.get("repo")?.as_str()?;
    let wf = fam.get("weights_file")?.as_str()?;
    let cf = fam.get("config_file")?.as_str().unwrap_or("config.json");
    let cache = hf_hub::Cache::from_env();
    let hf_repo = cache.repo(hf_hub::Repo::model(repo.to_string()));
    let w = hf_repo.get(wf)?;
    let c = hf_repo.get(cf)?;
    Some((w, c))
}

#[cfg(feature = "embed-zuna")]
fn load_zuna(config: &ExgModelConfig) -> Option<ZunaState> {
    match skill_exg::resolve_hf_weights(&config.hf_repo) {
        Some((weights_path, config_path)) => {
            info!(weights = %weights_path.display(), config = %config_path.display(), "ZUNA weights resolved");
            match zuna_rs::ZunaEncoder::<burn::backend::NdArray>::load(
                &config_path,
                &weights_path,
                burn::backend::ndarray::NdArrayDevice::Cpu,
            ) {
                Ok((encoder, ms)) => {
                    info!(ms, "ZUNA encoder loaded");
                    let data_config = encoder.data_cfg.clone();
                    Some(ZunaState { encoder, data_config })
                }
                Err(e) => {
                    warn!(%e, "ZUNA encoder load failed");
                    None
                }
            }
        }
        None => {
            let cache = skill_data::util::hf_model_dir(&config.hf_repo);
            warn!(repo = %config.hf_repo, cache_dir = %cache.display(), "ZUNA weights not in HF cache");
            None
        }
    }
}

// ── Per-epoch encoding ──────────────────────────────────────────────────────

fn encode_epoch(encoder: &Encoder, msg: &EpochMsg) -> Option<Vec<f32>> {
    match encoder {
        #[cfg(feature = "embed-zuna")]
        Encoder::Zuna(state) => encode_zuna(state, msg),
        #[cfg(feature = "embed-zuna-gpu")]
        Encoder::ZunaGpu(state) => encode_zuna_gpu(state, msg),
        #[cfg(feature = "embed-zuna-gpu-f16")]
        Encoder::ZunaGpuF16(state) => encode_zuna_gpu_f16(state, msg),
        #[cfg(feature = "embed-luna")]
        Encoder::Luna(enc) => encode_luna(enc, msg),
        #[cfg(feature = "embed-reve")]
        Encoder::Reve(enc) => encode_reve(enc, msg),
        #[cfg(feature = "embed-osf")]
        Encoder::Osf(enc) => encode_osf(enc, msg),
        #[cfg(feature = "embed-sleepfm")]
        Encoder::SleepFM(_enc) => {
            // SleepFM requires PSG-specific tensor layout; use catalog embedding dim as fallback.
            None
        }
        #[cfg(feature = "embed-sleeplm")]
        Encoder::SleepLM(enc) => encode_sleeplm(enc, msg),
        #[cfg(feature = "embed-steegformer")]
        Encoder::STEEGFormer(enc) => encode_steegformer(enc, msg),
        #[cfg(feature = "embed-tribev2")]
        Encoder::Tribev2(state) => encode_tribev2(state, msg),
        #[cfg(feature = "embed-neurorvq")]
        Encoder::NeuroRVQ(state) => encode_neurorvq(state, msg),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

#[cfg(feature = "embed-zuna")]
fn encode_zuna(state: &ZunaState, msg: &EpochMsg) -> Option<Vec<f32>> {
    use std::collections::HashMap as HM;
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    let mut data = ndarray::Array2::<f32>::zeros((n_ch, n_samples));
    for (ch, samples) in msg.samples.iter().enumerate().take(n_ch) {
        for (s, &v) in samples.iter().enumerate() {
            data[[ch, s]] = v;
        }
    }
    let ch_names: Vec<&str> = msg.channel_names.iter().take(n_ch).map(String::as_str).collect();
    let device = burn::backend::ndarray::NdArrayDevice::Cpu;
    let empty_pos: HM<String, [f32; 3]> = HM::new();
    let batches = zuna_rs::load_from_named_tensor::<burn::backend::NdArray>(
        data,
        &ch_names,
        msg.sample_rate,
        10.0,
        &empty_pos,
        &state.data_config,
        &device,
    )
    .ok()?;
    let epochs = state.encoder.encode_batches(batches).ok()?;
    epochs.first().map(|ep| {
        let dim = ep.output_dim();
        let n_tok = ep.n_tokens();
        if dim == 0 || n_tok == 0 {
            return Vec::new();
        }
        let mut pooled = vec![0.0f32; dim];
        for t in 0..n_tok {
            for (d, p) in pooled.iter_mut().enumerate() {
                *p += ep.embeddings[t * dim + d];
            }
        }
        for v in &mut pooled {
            *v /= n_tok as f32;
        }
        pooled
    })
}

#[cfg(feature = "embed-luna")]
fn encode_luna(enc: &luna_rs::LunaEncoder<burn::backend::NdArray>, msg: &EpochMsg) -> Option<Vec<f32>> {
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    // LUNA needs uppercase channel names from its vocabulary.
    let mut luna_names: Vec<String> = Vec::new();
    let mut luna_indices: Vec<usize> = Vec::new();
    for (idx, name) in msg.channel_names.iter().take(n_ch).enumerate() {
        let upper = name.to_uppercase();
        if luna_rs::channel_index(&upper).is_some() {
            luna_names.push(upper);
            luna_indices.push(idx);
        }
    }
    if luna_names.is_empty() {
        return None;
    }
    let flat: Vec<f32> = luna_indices
        .iter()
        .flat_map(|&ch| msg.samples[ch].iter().copied())
        .collect();
    let ch_refs: Vec<&str> = luna_names.iter().map(String::as_str).collect();
    let device = burn::backend::ndarray::NdArrayDevice::Cpu;
    let batch = luna_rs::build_batch_named::<burn::backend::NdArray>(flat, &ch_refs, n_samples, &device);
    let ep = enc.run_batch(&batch).ok()?;
    Some(ep.output)
}

#[cfg(feature = "embed-reve")]
fn encode_reve(enc: &reve_rs::ReveEncoder<burn::backend::NdArray>, msg: &EpochMsg) -> Option<Vec<f32>> {
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    let flat: Vec<f32> = (0..n_ch).flat_map(|ch| msg.samples[ch].iter().copied()).collect();
    let positions = vec![0.0f32; n_ch * 3];
    let device = burn::backend::ndarray::NdArrayDevice::Cpu;
    let batch = reve_rs::build_batch::<burn::backend::NdArray>(flat, positions, n_ch, n_samples, &device);
    let result = enc.run_batch(&batch).ok()?;
    Some(result.output)
}

#[cfg(feature = "embed-osf")]
fn encode_osf(enc: &osf_rs::OsfEncoder<burn::backend::NdArray>, msg: &EpochMsg) -> Option<Vec<f32>> {
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    let flat: Vec<f32> = (0..n_ch).flat_map(|ch| msg.samples[ch].iter().copied()).collect();
    let device = burn::backend::ndarray::NdArrayDevice::Cpu;
    let batch = osf_rs::build_batch::<burn::backend::NdArray>(flat, n_ch, n_samples, &device);
    let ep = enc.run_batch(&batch).ok()?;
    Some(ep.cls_emb)
}

#[cfg(feature = "embed-sleeplm")]
fn encode_sleeplm(enc: &sleeplm::SleepLMEncoder<burn::backend::NdArray>, msg: &EpochMsg) -> Option<Vec<f32>> {
    // SleepLM expects [10, 1920] (10 PSG channels × 1920 samples).
    // Pad/truncate the EEG signal to fit.
    let n_ch = msg.channel_names.len().min(msg.samples.len()).min(10);
    if n_ch == 0 {
        return None;
    }
    let target_samples = 1920;
    let mut flat = vec![0.0f32; 10 * target_samples];
    for ch in 0..n_ch {
        let src = &msg.samples[ch];
        let copy_len = src.len().min(target_samples);
        for (i, &v) in src.iter().take(copy_len).enumerate() {
            flat[ch * target_samples + i] = v;
        }
    }
    let device = burn::backend::ndarray::NdArrayDevice::Cpu;
    let batch = sleeplm::build_batch::<burn::backend::NdArray>(flat, &device);
    let ep = enc.encode(&batch).ok()?;
    Some(ep.embedding)
}

#[cfg(feature = "embed-steegformer")]
fn encode_steegformer(
    enc: &steegformer::STEEGFormerEncoder<burn::backend::NdArray>,
    msg: &EpochMsg,
) -> Option<Vec<f32>> {
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    // ST-EEGFormer has a channel vocabulary like LUNA.
    let mut names: Vec<String> = Vec::new();
    let mut indices: Vec<usize> = Vec::new();
    for (idx, name) in msg.channel_names.iter().take(n_ch).enumerate() {
        let upper = name.to_uppercase();
        if steegformer::channel_index(&upper).is_some() {
            names.push(upper);
            indices.push(idx);
        }
    }
    if names.is_empty() {
        return None;
    }
    let flat: Vec<f32> = indices.iter().flat_map(|&ch| msg.samples[ch].iter().copied()).collect();
    let ch_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let device = burn::backend::ndarray::NdArrayDevice::Cpu;
    let batch = steegformer::build_batch_named::<burn::backend::NdArray>(flat, &ch_refs, n_samples, &device);
    let ep = enc.run_batch(&batch).ok()?;
    Some(ep.output)
}

#[cfg(feature = "embed-tribev2")]
fn encode_tribev2(_state: &Tribev2State, _msg: &EpochMsg) -> Option<Vec<f32>> {
    // TRIBEv2 runtime encoding requires the full multimodal pipeline.
    // Not yet integrated — weights are downloaded but encoding is pending.
    None
}

#[cfg(feature = "embed-neurorvq")]
fn encode_neurorvq(state: &NeuroRVQState, msg: &EpochMsg) -> Option<Vec<f32>> {
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    let mut signal = Vec::with_capacity(n_ch * n_samples);
    for s in 0..n_samples {
        for ch in 0..n_ch {
            signal.push(msg.samples[ch].get(s).copied().unwrap_or(0.0));
        }
    }
    let ch_names: Vec<&str> = msg.channel_names.iter().take(n_ch).map(String::as_str).collect();
    state.model.encode_pooled(&signal, &ch_names).ok()
}

// ── GPU ZUNA encoder ────────────────────────────────────────────────────────

#[cfg(feature = "embed-zuna-gpu")]
fn load_zuna_gpu(config: &ExgModelConfig) -> Option<ZunaGpuState> {
    match skill_exg::resolve_hf_weights(&config.hf_repo) {
        Some((weights_path, config_path)) => {
            info!(weights = %weights_path.display(), "loading ZUNA encoder on GPU (wgpu/Metal)");
            let device = burn::backend::wgpu::WgpuDevice::default();
            match zuna_rs::ZunaEncoder::<burn::backend::wgpu::Wgpu>::load(&config_path, &weights_path, device) {
                Ok((encoder, ms)) => {
                    info!(ms, "ZUNA GPU encoder loaded");
                    let data_config = encoder.data_cfg.clone();
                    Some(ZunaGpuState { encoder, data_config })
                }
                Err(e) => {
                    warn!(%e, "ZUNA GPU encoder load failed — will fall back to CPU");
                    None
                }
            }
        }
        None => {
            warn!("ZUNA weights not found for GPU encoder");
            None
        }
    }
}

#[cfg(feature = "embed-zuna-gpu")]
fn encode_zuna_gpu(state: &ZunaGpuState, msg: &EpochMsg) -> Option<Vec<f32>> {
    use std::collections::HashMap as HM;
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    let mut data = ndarray::Array2::<f32>::zeros((n_ch, n_samples));
    for (ch, samples) in msg.samples.iter().enumerate().take(n_ch) {
        for (s, &v) in samples.iter().enumerate() {
            data[[ch, s]] = v;
        }
    }
    let ch_names: Vec<&str> = msg.channel_names.iter().take(n_ch).map(String::as_str).collect();
    let device = burn::backend::wgpu::WgpuDevice::default();
    let empty_pos: HM<String, [f32; 3]> = HM::new();
    let batches = zuna_rs::load_from_named_tensor::<burn::backend::wgpu::Wgpu>(
        data,
        &ch_names,
        msg.sample_rate,
        10.0,
        &empty_pos,
        &state.data_config,
        &device,
    )
    .ok()?;
    let epochs = state.encoder.encode_batches(batches).ok()?;
    epochs.first().map(|ep| {
        let dim = ep.output_dim();
        let n_tok = ep.n_tokens();
        if dim == 0 || n_tok == 0 {
            return Vec::new();
        }
        let mut pooled = vec![0.0f32; dim];
        for t in 0..n_tok {
            for (d, p) in pooled.iter_mut().enumerate() {
                *p += ep.embeddings[t * dim + d];
            }
        }
        let inv = 1.0 / n_tok as f32;
        for p in &mut pooled {
            *p *= inv;
        }
        pooled
    })
}

// ── GPU f16 ZUNA encoder ────────────────────────────────────────────────────
// Currently unused for batch reembed (burn f16→f32 extraction bug).
// Kept for future use when zuna-rs/burn fix the TypeMismatch issue.

#[cfg(feature = "embed-zuna-gpu-f16")]
#[allow(dead_code)]
fn load_zuna_gpu_f16(config: &ExgModelConfig) -> Option<ZunaGpuF16State> {
    match skill_exg::resolve_hf_weights(&config.hf_repo) {
        Some((weights_path, config_path)) => {
            info!(weights = %weights_path.display(), "loading ZUNA encoder on GPU f16 (wgpu/Metal half-precision)");
            let device = burn::backend::wgpu::WgpuDevice::default();
            match zuna_rs::ZunaEncoder::<burn::backend::wgpu::Wgpu<half::f16, i32, u32>>::load(
                &config_path,
                &weights_path,
                device,
            ) {
                Ok((encoder, ms)) => {
                    info!(ms, "ZUNA GPU f16 encoder loaded");
                    let data_config = encoder.data_cfg.clone();
                    Some(ZunaGpuF16State { encoder, data_config })
                }
                Err(e) => {
                    warn!(%e, "ZUNA GPU f16 encoder load failed — will try f32 or CPU");
                    None
                }
            }
        }
        None => {
            warn!("ZUNA weights not found for GPU f16 encoder");
            None
        }
    }
}

#[cfg(feature = "embed-zuna-gpu-f16")]
#[allow(dead_code)]
fn encode_zuna_gpu_f16(state: &ZunaGpuF16State, msg: &EpochMsg) -> Option<Vec<f32>> {
    use std::collections::HashMap as HM;
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    let mut data = ndarray::Array2::<f32>::zeros((n_ch, n_samples));
    for (ch, samples) in msg.samples.iter().enumerate().take(n_ch) {
        for (s, &v) in samples.iter().enumerate() {
            data[[ch, s]] = v;
        }
    }
    let ch_names: Vec<&str> = msg.channel_names.iter().take(n_ch).map(String::as_str).collect();
    let device = burn::backend::wgpu::WgpuDevice::default();
    let empty_pos: HM<String, [f32; 3]> = HM::new();
    let batches = zuna_rs::load_from_named_tensor::<burn::backend::wgpu::Wgpu<half::f16, i32, u32>>(
        data,
        &ch_names,
        msg.sample_rate,
        10.0,
        &empty_pos,
        &state.data_config,
        &device,
    )
    .ok()?;
    let epochs = match state.encoder.encode_batches(batches) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("[encode-gpu-f16] encode_batches failed: {e}");
            return None;
        }
    };
    epochs.first().map(|ep| {
        let dim = ep.output_dim();
        let n_tok = ep.n_tokens();
        if dim == 0 || n_tok == 0 {
            return Vec::new();
        }
        let mut pooled = vec![0.0f32; dim];
        for t in 0..n_tok {
            for (d, p) in pooled.iter_mut().enumerate() {
                *p += ep.embeddings[t * dim + d];
            }
        }
        let inv = 1.0 / n_tok as f32;
        for p in &mut pooled {
            *p *= inv;
        }
        pooled
    })
}

// ── Public API for batch reembedding ─────────────────────────────────────────

/// Opaque encoder for batch reembed — GPU f32 or CPU.
///
/// GPU f16 is intentionally excluded: burn's `TensorData::to_vec::<f32>()`
/// has a TypeMismatch bug when the wgpu backend uses half-precision floats,
/// so embeddings cannot be extracted. Real-time streaming uses f16 fine
/// because it goes through a different code path.
pub enum PublicEncoder {
    Cpu(Encoder),
    #[cfg(feature = "embed-zuna-gpu")]
    Gpu(Box<ZunaGpuState>),
}

/// Load an encoder for batch reembed: tries GPU f32 → CPU.
///
/// GPU f16 is intentionally skipped for batch reembed because burn's
/// `TensorData::to_vec::<f32>()` has a TypeMismatch bug when the backend
/// uses half-precision floats — the embeddings cannot be extracted as f32.
/// Real-time streaming uses f16 fine because it goes through a different
/// code path that doesn't extract to Vec.
pub fn load_encoder_public(config: &ExgModelConfig, skill_dir: &Path) -> Option<PublicEncoder> {
    if matches!(config.model_backend, ExgModelBackend::Zuna) {
        // GPU f32 — works correctly with TensorData extraction.
        #[cfg(feature = "embed-zuna-gpu")]
        {
            if let Some(gpu) = load_zuna_gpu(config) {
                return Some(PublicEncoder::Gpu(Box::new(gpu)));
            }
            warn!("GPU f32 unavailable, falling back to CPU");
        }
    }
    load_encoder(config, skill_dir).map(PublicEncoder::Cpu)
}

/// Encode raw EEG samples into an embedding vector.
pub fn encode_raw_public(
    encoder: &PublicEncoder,
    samples: &[Vec<f32>],
    channel_names: &[String],
    sample_rate: f64,
) -> Option<Vec<f32>> {
    let msg = EpochMsg {
        timestamp: 0,
        samples: samples.to_vec(),
        channel_names: channel_names.to_vec(),
        sample_rate: sample_rate as f32,
        band_snapshot: None,
        device_name: None,
    };
    match encoder {
        PublicEncoder::Cpu(enc) => encode_epoch(enc, &msg),
        #[cfg(feature = "embed-zuna-gpu")]
        PublicEncoder::Gpu(gpu) => encode_zuna_gpu(gpu, &msg),
    }
}
