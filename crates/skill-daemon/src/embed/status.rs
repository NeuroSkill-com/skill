// SPDX-License-Identifier: GPL-3.0-only
//! Live embed-worker state for `/v1/models/status`.
//!
//! The daemon owns one [`EegModelStatus`] (`AppState::exg_model_status`); the
//! download path writes the weight fields and the embed worker writes the
//! live ones through [`StatusReporter`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use skill_eeg::eeg_model_config::EegModelStatus;
use skill_exg::EpochMetrics;

use super::day_store::DayStore;

/// The daemon-wide status served by `/v1/models/status`.
pub(crate) type SharedExgStatus = Arc<Mutex<EegModelStatus>>;

/// Bumped by every worker start; only the newest worker may write the status.
static WORKER_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Weight of the newest sample in the `avg_embed_ms` moving average.
const EMBED_MS_EMA_ALPHA: f64 = 0.1;

/// One embed worker's view of the shared status.
///
/// A worker still draining its queue after a newer one started (session
/// rollover, reconnect) is superseded and stays silent — otherwise its exit
/// would clear the flags the new worker just set.
pub(super) struct StatusReporter {
    status: SharedExgStatus,
    generation: u64,
    backend: String,
    dim_described: bool,
}

impl StatusReporter {
    /// Claims the status for a new worker and marks it active.
    pub(super) fn start(status: SharedExgStatus, backend: &str) -> Self {
        let generation = WORKER_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let reporter = Self {
            status,
            generation,
            backend: backend.to_string(),
            dim_described: false,
        };
        reporter.update(|st| {
            st.embed_worker_active = true;
            st.encoder_loaded = false;
            st.encoder_describe = None;
            st.active_model_backend = Some(backend.to_string());
        });
        reporter
    }

    /// Today's store was opened (worker start or UTC day rollover).
    pub(super) fn day_store_opened(&self, store: Option<&DayStore>) {
        self.update(|st| {
            st.embeddings_today = store.map_or(0, DayStore::hnsw_len);
            st.daily_db_path = store.map(|s| s.db_path.display().to_string()).unwrap_or_default();
            st.daily_hnsw_path = store.map(|s| s.index_path.display().to_string()).unwrap_or_default();
        });
    }

    pub(super) fn encoder_loaded(&self, loaded: bool) {
        self.update(|st| {
            st.encoder_loaded = loaded;
            st.encoder_describe = loaded.then(|| self.backend.clone());
        });
    }

    /// An epoch was processed. `embedded` carries the encode time (ms) and
    /// the embedding dimension when the encoder produced a vector; metrics-only
    /// epochs still refresh `latest_metrics`.
    pub(super) fn epoch_done(
        &mut self,
        embedded: Option<(f64, usize)>,
        metrics: Option<&EpochMetrics>,
        epoch_timestamp: i64,
        store: Option<&DayStore>,
    ) {
        let describe = match embedded {
            Some((_, dim)) if !self.dim_described => {
                self.dim_described = true;
                Some(format!("{}  dim={dim}", self.backend))
            }
            _ => None,
        };
        self.update(|st| {
            if let Some((ms, _)) = embedded {
                st.last_embed_ms = ms;
                st.avg_embed_ms = if st.avg_embed_ms > 0.0 {
                    (1.0 - EMBED_MS_EMA_ALPHA) * st.avg_embed_ms + EMBED_MS_EMA_ALPHA * ms
                } else {
                    ms
                };
            }
            if let Some(d) = describe {
                st.encoder_describe = Some(d);
            }
            if let Some(m) = metrics {
                st.latest_metrics = Some(m.to_latest(epoch_timestamp));
            }
            if let Some(s) = store {
                st.embeddings_today = s.hnsw_len();
            }
        });
    }

    fn update(&self, f: impl FnOnce(&mut EegModelStatus)) {
        if WORKER_GENERATION.load(Ordering::SeqCst) != self.generation {
            return;
        }
        if let Ok(mut st) = self.status.lock() {
            f(&mut st);
        }
    }
}

/// Clears the live flags on exit, including an unwinding panic. Counters and
/// `latest_metrics` stay so the settings page still shows today's totals.
impl Drop for StatusReporter {
    fn drop(&mut self) {
        self.update(|st| {
            st.embed_worker_active = false;
            st.encoder_loaded = false;
            st.encoder_describe = None;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `WORKER_GENERATION` is process-global; run these one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn metrics(rel_alpha: f32) -> EpochMetrics {
        EpochMetrics {
            rel_alpha,
            relaxation: 42.0,
            ..EpochMetrics::default()
        }
    }

    #[test]
    fn lifecycle_sets_then_clears_live_flags() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let status = SharedExgStatus::default();
        let mut r = StatusReporter::start(status.clone(), "zuna");
        assert!(status.lock().unwrap().embed_worker_active);

        r.encoder_loaded(true);
        r.epoch_done(Some((400.0, 32)), Some(&metrics(0.75)), 20261027120000, None);
        r.epoch_done(Some((200.0, 32)), None, 20261027120005, None);
        {
            let st = status.lock().unwrap();
            assert!(st.encoder_loaded);
            assert_eq!(st.encoder_describe.as_deref(), Some("zuna  dim=32"));
            assert_eq!(st.last_embed_ms, 200.0);
            assert!((st.avg_embed_ms - 380.0).abs() < 1e-9);
            let lm = st.latest_metrics.as_ref().expect("latest_metrics set");
            assert_eq!(lm.rel_alpha, 0.75);
            assert_eq!(lm.relaxation_score, 42.0);
            assert_eq!(lm.epoch_timestamp, 20261027120000);
        }

        drop(r);
        let st = status.lock().unwrap();
        assert!(!st.embed_worker_active && !st.encoder_loaded && st.encoder_describe.is_none());
        assert!(st.latest_metrics.is_some());
        assert_eq!(st.last_embed_ms, 200.0);
    }

    #[test]
    fn metrics_only_epoch_updates_metrics_but_not_speed() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let status = SharedExgStatus::default();
        let mut r = StatusReporter::start(status.clone(), "zuna");
        r.encoder_loaded(false);
        r.epoch_done(None, Some(&metrics(0.5)), 20261027120000, None);
        let st = status.lock().unwrap();
        assert_eq!(st.avg_embed_ms, 0.0);
        assert!(st.encoder_describe.is_none());
        assert_eq!(st.latest_metrics.as_ref().map(|m| m.rel_alpha), Some(0.5));
    }

    #[test]
    fn superseded_reporter_does_not_clobber_newer_worker() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let status = SharedExgStatus::default();
        let mut old = StatusReporter::start(status.clone(), "zuna");
        let new = StatusReporter::start(status.clone(), "luna");
        new.encoder_loaded(true);
        old.epoch_done(Some((1.0, 8)), Some(&metrics(0.9)), 1, None);
        drop(old);
        let st = status.lock().unwrap();
        assert!(st.embed_worker_active && st.encoder_loaded);
        assert_eq!(st.active_model_backend.as_deref(), Some("luna"));
        assert!(st.latest_metrics.is_none());
        assert_eq!(st.last_embed_ms, 0.0);
    }
}
