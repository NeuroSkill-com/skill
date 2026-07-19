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
#[cfg(feature = "embed-eegdino")]
use skill_exg::eegdino::EegDino;
#[cfg(feature = "embed-lumamba")]
use skill_exg::lumamba::LuMamba;
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

            // Embed keywords as search queries (against the label index).
            let query_refs: Vec<&str> = queries.iter().map(String::as_str).collect();
            let Some(embeddings) = self.text_embedder.embed_queries(query_refs) else {
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
    let mut encoder = load_encoder(&config, &skill_dir);

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
        let embedding = encoder.as_mut().and_then(|enc| encode_epoch(enc, &msg));
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
    #[cfg(feature = "embed-luna")]
    Luna(Box<luna_rs::LunaEncoder>),
    #[cfg(feature = "embed-reve")]
    Reve(Box<ReveState>),
    #[cfg(feature = "embed-brainjepa")]
    Brainjepa(Box<brainjepa::rlx::BrainJepaEncoder>),
    #[cfg(feature = "embed-tribev2")]
    Tribev2(Box<Tribev2State>),
    #[cfg(feature = "embed-neurorvq")]
    NeuroRVQ(Box<NeuroRVQState>),
    #[cfg(feature = "embed-eegdino")]
    EegDino(Box<EegDinoState>),
    #[cfg(feature = "embed-lumamba")]
    Lumamba(Box<LumambaState>),
    None,
}

#[cfg(feature = "embed-tribev2")]
pub(crate) struct Tribev2State {
    #[allow(dead_code)]
    encoder: tribev2::TribeRlx,
}

#[cfg(feature = "embed-zuna")]
pub(crate) struct ZunaState {
    encoder: zuna_rs::ZunaEncoder,
    data_config: zuna_rs::config::DataConfig,
}

#[cfg(feature = "embed-neurorvq")]
pub(crate) struct NeuroRVQState {
    model: NeuroRVQFM,
}

#[cfg(feature = "embed-eegdino")]
pub(crate) struct EegDinoState {
    model: EegDino,
}

#[cfg(feature = "embed-lumamba")]
pub(crate) struct LumambaState {
    model: LuMamba,
}

#[cfg(feature = "embed-reve")]
pub(crate) struct ReveState {
    encoder: reve_rs::ReveEncoder,
    positions: reve_rs::PositionBank,
}

#[cfg(feature = "embed-reve")]
const REVE_WEIGHTS_FILE: &str = "model.safetensors";
#[cfg(feature = "embed-reve")]
const REVE_CONFIG_FILE: &str = "config.json";
#[cfg(feature = "embed-reve")]
const REVE_POSITIONS_REPO: &str = "brain-bzh/reve-positions";

#[cfg(feature = "embed-reve")]
fn load_reve_position_bank() -> reve_rs::PositionBank {
    if let Some(path) = skill_exg::resolve_hf_file(REVE_POSITIONS_REPO, "positions.json") {
        if let Some(path_str) = path.to_str() {
            if let Ok(bank) = reve_rs::PositionBank::from_json(path_str) {
                return bank;
            }
        }
    }
    reve_rs::PositionBank::from_json_str("{}").expect("empty REVE position bank")
}

fn load_encoder(config: &ExgModelConfig, _skill_dir: &Path) -> Option<Encoder> {
    let device_pref = skill_settings::load_settings(_skill_dir).exg_inference_device;
    let backend = config.model_backend.clone();
    info!(backend = backend.as_str(), device = %device_pref, "loading EXG encoder");
    let result = match &backend {
        #[cfg(feature = "embed-neurorvq")]
        ExgModelBackend::Neurorvq => {
            info!("loading NeuroRVQ encoder");
            let device = super::resolve_exg_device(&device_pref);
            match NeuroRVQFM::from_default_hf(NeuroModality::EEG, device) {
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
        #[cfg(feature = "embed-eegdino")]
        ExgModelBackend::Eegdino => {
            info!(variant = %config.eegdino_variant, "loading EEG-DINO encoder");
            let device = super::resolve_exg_device(&device_pref);
            match EegDino::from_hf(&config.eegdino_hf_repo, &config.eegdino_variant, device) {
                Ok(model) => {
                    info!(dim = model.embed_dim(), "EEG-DINO encoder loaded");
                    Some(Encoder::EegDino(Box::new(EegDinoState { model })))
                }
                Err(e) => {
                    warn!(%e, "EEG-DINO load failed — metrics-only");
                    None
                }
            }
        }
        #[cfg(not(feature = "embed-eegdino"))]
        ExgModelBackend::Eegdino => {
            warn!("EEG-DINO backend selected but support is not compiled (enable feature: embed-eegdino)");
            None
        }
        #[cfg(feature = "embed-lumamba")]
        ExgModelBackend::Lumamba => {
            info!(variant = %config.lumamba_variant, repo = %config.lumamba_hf_repo, "loading LuMamba encoder");
            let device = super::resolve_exg_device(&device_pref);
            match LuMamba::load(&config.lumamba_hf_repo, &config.lumamba_variant, device) {
                Ok(model) => {
                    info!(dim = model.embed_dim(), "LuMamba encoder loaded");
                    Some(Encoder::Lumamba(Box::new(LumambaState { model })))
                }
                Err(e) => {
                    warn!(%e, repo = %config.lumamba_hf_repo, "LuMamba load failed — metrics-only");
                    None
                }
            }
        }
        #[cfg(not(feature = "embed-lumamba"))]
        ExgModelBackend::Lumamba => {
            warn!("LuMamba backend selected but support is not compiled (enable feature: embed-lumamba)");
            None
        }
        #[cfg(feature = "embed-zuna")]
        ExgModelBackend::Zuna => {
            info!(repo = %config.hf_repo, "loading ZUNA encoder");
            let device = super::resolve_exg_device(&device_pref);
            load_zuna(config, device)
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
            let device = super::resolve_exg_device(&device_pref);
            let wf = config.luna_weights_file();
            skill_exg::resolve_luna_weights(&config.luna_hf_repo, wf).and_then(
                |(w, c)| match luna_rs::LunaEncoder::load(&c, &w, device) {
                    Ok((enc, ms)) => {
                        info!(ms, "LUNA encoder loaded");
                        Some(Encoder::Luna(Box::new(enc)))
                    }
                    Err(e) => {
                        warn!(%e, "LUNA encoder load failed");
                        None
                    }
                },
            )
        }
        #[cfg(not(feature = "embed-luna"))]
        ExgModelBackend::Luna => {
            warn!("LUNA backend selected but support is not compiled (enable feature: embed-luna)");
            None
        }
        #[cfg(feature = "embed-reve")]
        ExgModelBackend::Reve => {
            let device = super::resolve_exg_device(&device_pref);
            info!(repo = %config.hf_repo, "loading REVE encoder");
            skill_exg::resolve_exg_weights(&config.hf_repo, REVE_WEIGHTS_FILE, REVE_CONFIG_FILE)
                .and_then(|(w, c)| match reve_rs::ReveEncoder::load(&c, &w, device) {
                    Ok((enc, ms)) => {
                        let positions = load_reve_position_bank();
                        info!(ms, "REVE encoder loaded");
                        Some(Encoder::Reve(Box::new(ReveState {
                            encoder: enc,
                            positions,
                        })))
                    }
                    Err(e) => {
                        warn!(%e, "REVE encoder load failed");
                        None
                    }
                })
                .or_else(|| {
                    warn!(
                        repo = %config.hf_repo,
                        "REVE weights not found — metrics-only"
                    );
                    None
                })
        }
        #[cfg(not(feature = "embed-reve"))]
        ExgModelBackend::Reve => {
            warn!("REVE backend selected but support is not compiled (enable feature: embed-reve)");
            None
        }
        #[cfg(feature = "embed-brainjepa")]
        ExgModelBackend::Brainjepa => {
            let device = super::resolve_exg_device(&device_pref);
            let repo = if config.hf_repo.is_empty() || config.hf_repo == skill_constants::ZUNA_HF_REPO {
                skill_exg::BRAINJEPA_HF_REPO
            } else {
                config.hf_repo.as_str()
            };
            info!(repo, "loading Brain-JEPA encoder");
            skill_exg::resolve_brainjepa_weights(repo).and_then(|(w, g)| {
                let w_str = w.to_str()?;
                let g_str = g.to_str()?;
                match brainjepa::rlx::BrainJepaEncoder::from_weights(
                    w_str,
                    g_str,
                    &brainjepa::ModelConfig::default(),
                    &brainjepa::DataConfig::default(),
                    &device,
                ) {
                    Ok((enc, ms)) => {
                        info!(ms, "Brain-JEPA encoder loaded");
                        Some(Encoder::Brainjepa(Box::new(enc)))
                    }
                    Err(e) => {
                        warn!(%e, "Brain-JEPA encoder load failed");
                        None
                    }
                }
            })
        }
        #[cfg(not(feature = "embed-brainjepa"))]
        ExgModelBackend::Brainjepa => {
            warn!("Brain-JEPA backend selected but support is not compiled (enable feature: embed-brainjepa)");
            None
        }
        #[cfg(feature = "embed-tribev2")]
        ExgModelBackend::Tribev2 => {
            let device = super::resolve_exg_device(&device_pref);
            resolve_catalog_hf("tribev2").and_then(|(w, c)| {
                match tribev2::TribeRlx::from_pretrained(c.to_str()?, w.to_str()?, None) {
                    Ok(enc) => {
                        let enc = enc.with_device(device);
                        info!("TRIBEv2 encoder loaded");
                        Some(Encoder::Tribev2(Box::new(Tribev2State { encoder: enc })))
                    }
                    Err(e) => {
                        warn!(%e, "TRIBEv2 encoder load failed");
                        None
                    }
                }
            })
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
fn load_zuna(config: &ExgModelConfig, device: rlx::Device) -> Option<ZunaState> {
    match skill_exg::resolve_hf_weights(&config.hf_repo) {
        Some((weights_path, config_path)) => {
            info!(weights = %weights_path.display(), config = %config_path.display(), "ZUNA weights resolved");
            match zuna_rs::ZunaEncoder::load(&config_path, &weights_path, device) {
                Ok((encoder, ms)) => {
                    info!(ms, "ZUNA encoder loaded");
                    let data_config = zuna_rs::config::DataConfig {
                        num_fine_time_pts: encoder.model_cfg.input_dim,
                        ..Default::default()
                    };
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

fn encode_epoch(encoder: &mut Encoder, msg: &EpochMsg) -> Option<Vec<f32>> {
    match encoder {
        #[cfg(feature = "embed-zuna")]
        Encoder::Zuna(state) => encode_zuna(state, msg),
        #[cfg(feature = "embed-luna")]
        Encoder::Luna(enc) => encode_luna(enc, msg),
        #[cfg(feature = "embed-reve")]
        Encoder::Reve(state) => encode_reve(state, msg),
        #[cfg(feature = "embed-brainjepa")]
        Encoder::Brainjepa(enc) => encode_brainjepa(enc),
        #[cfg(feature = "embed-tribev2")]
        Encoder::Tribev2(state) => encode_tribev2(state, msg),
        #[cfg(feature = "embed-neurorvq")]
        Encoder::NeuroRVQ(state) => encode_neurorvq(state, msg),
        #[cfg(feature = "embed-eegdino")]
        Encoder::EegDino(state) => encode_eegdino(state, msg),
        #[cfg(feature = "embed-lumamba")]
        Encoder::Lumamba(state) => encode_lumamba(state, msg),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

#[cfg(feature = "embed-zuna")]
fn encode_zuna(state: &mut ZunaState, msg: &EpochMsg) -> Option<Vec<f32>> {
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
    let empty_pos: HashMap<String, [f32; 3]> = HashMap::new();
    let batches = zuna_rs::csv_loader::load_from_named_tensor(
        data,
        &ch_names,
        msg.sample_rate,
        10.0,
        &empty_pos,
        &state.data_config,
    )
    .ok()?;
    batches.into_iter().find_map(|ep| {
        let emb = state
            .encoder
            .encode_one(&ep.eeg_tokens, &ep.tok_idx, &ep.chan_pos, ep.n_channels, ep.tc)
            .ok()?;
        let n_tok = emb.shape.first().copied().unwrap_or(0);
        let dim = emb.shape.get(1).copied().unwrap_or(0);
        if dim == 0 || n_tok == 0 {
            return None;
        }
        let mut pooled = vec![0.0f32; dim];
        for t in 0..n_tok {
            for (d, p) in pooled.iter_mut().enumerate() {
                *p += emb.embeddings[t * dim + d];
            }
        }
        let inv = 1.0 / n_tok as f32;
        for p in &mut pooled {
            *p *= inv;
        }
        Some(pooled)
    })
}

#[cfg(feature = "embed-luna")]
fn encode_luna(enc: &mut luna_rs::LunaEncoder, msg: &EpochMsg) -> Option<Vec<f32>> {
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    // Filter to channels in LUNA's vocabulary; collect xyz positions and vocab indices.
    let mut src_indices: Vec<usize> = Vec::new();
    let mut chan_pos: Vec<f32> = Vec::new();
    let mut vocab_indices: Vec<i32> = Vec::new();
    for (idx, name) in msg.channel_names.iter().take(n_ch).enumerate() {
        let upper = name.to_uppercase();
        if let Some(vi) = luna_rs::channel_index(&upper) {
            let xyz = luna_rs::channel_positions::channel_xyz(&upper).unwrap_or([0.0, 0.0, 0.0]);
            src_indices.push(idx);
            chan_pos.extend_from_slice(&xyz);
            vocab_indices.push(vi as i32);
        }
    }
    if src_indices.is_empty() {
        return None;
    }
    let valid_ch = src_indices.len();
    let flat: Vec<f32> = src_indices
        .iter()
        .flat_map(|&ch| msg.samples[ch].iter().copied())
        .collect();
    let ep = enc
        .run_epoch(&flat, &chan_pos, Some(&vocab_indices), valid_ch, n_samples)
        .ok()?;
    Some(ep.output)
}

#[cfg(feature = "embed-reve")]
fn encode_reve(state: &mut ReveState, msg: &EpochMsg) -> Option<Vec<f32>> {
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let n_samples = msg.samples[0].len();
    let ch_names: Vec<&str> = msg.channel_names.iter().take(n_ch).map(String::as_str).collect();
    let positions = state.positions.get_positions(&ch_names);
    let flat: Vec<f32> = (0..n_ch).flat_map(|ch| msg.samples[ch].iter().copied()).collect();
    state
        .encoder
        .run_one(flat, positions, n_ch, n_samples)
        .ok()
        .map(|out| out.output)
}

#[cfg(feature = "embed-brainjepa")]
fn encode_brainjepa(_enc: &mut brainjepa::rlx::BrainJepaEncoder) -> Option<Vec<f32>> {
    // Brain-JEPA expects parcellated fMRI time series, not live EEG epochs.
    None
}

#[cfg(feature = "embed-tribev2")]
fn encode_tribev2(_state: &mut Tribev2State, _msg: &EpochMsg) -> Option<Vec<f32>> {
    // TRIBEv2 takes multimodal fMRI features (text/audio/video), not EEG epochs.
    None
}

#[cfg(feature = "embed-eegdino")]
fn encode_eegdino(state: &mut EegDinoState, msg: &EpochMsg) -> Option<Vec<f32>> {
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let ch_names: Vec<&str> = msg.channel_names.iter().take(n_ch).map(String::as_str).collect();
    let samples: Vec<Vec<f32>> = msg.samples.iter().take(n_ch).cloned().collect();
    state.model.encode_pooled(&samples, &ch_names).ok()
}

#[cfg(feature = "embed-lumamba")]
fn encode_lumamba(state: &mut LumambaState, msg: &EpochMsg) -> Option<Vec<f32>> {
    let n_ch = msg.channel_names.len().min(msg.samples.len());
    if n_ch == 0 {
        return None;
    }
    let ch_names: Vec<&str> = msg.channel_names.iter().take(n_ch).map(String::as_str).collect();
    let samples: Vec<Vec<f32>> = msg.samples.iter().take(n_ch).cloned().collect();
    state.model.encode_pooled(&samples, &ch_names).ok()
}

#[cfg(feature = "embed-neurorvq")]
fn encode_neurorvq(state: &mut NeuroRVQState, msg: &EpochMsg) -> Option<Vec<f32>> {
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

// ── Public API for batch reembedding ─────────────────────────────────────────

pub enum PublicEncoder {
    Inner(Encoder),
}

pub fn load_encoder_public(config: &ExgModelConfig, skill_dir: &Path) -> Option<PublicEncoder> {
    load_encoder(config, skill_dir).map(PublicEncoder::Inner)
}

/// Encode raw EEG samples into an embedding vector.
pub fn encode_raw_public(
    encoder: &mut PublicEncoder,
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
        PublicEncoder::Inner(enc) => encode_epoch(enc, &msg),
    }
}
