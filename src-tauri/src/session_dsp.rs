// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//
//! Session-private DSP state.
//!
//! ## Why a separate module?
//!
//! The six DSP objects below are **only ever mutated by the active session
//! task** (Muse BLE or OpenBCI).  In the old design they lived inside
//! `AppState` behind the shared `Mutex<Box<AppState>>`, which meant every
//! `filter.push()` / FFT / band-power calculation held the lock while
//! running.  Any Tauri UI command that needed `AppState` during that window
//! (e.g. `get_status`, `get_dnd_status`, `get_latest_bands`) would block,
//! causing the frontend to stall or appear frozen.
//!
//! ## New design
//!
//! `SessionDsp` is created once per session at session-start by reading the
//! current configuration out of `AppState` (brief lock → immediate release).
//! All DSP then runs **without holding any lock**.  Results (channel quality
//! scores, latest band snapshot) are written back to `AppState` in short
//! targeted lock windows after the computation completes.
//!
//! Config changes made by Tauri commands while a session is running (e.g.
//! the user tweaks the notch filter or the embedding overlap) are stored in
//! `AppState::status` as before.  [`SessionDsp::sync_config`] is called at
//! the top of each sample frame; it takes a single brief read lock, compares
//! the stored values to the last-applied ones, and calls the appropriate
//! setter only if something changed.

use tauri::{AppHandle, Manager};

use crate::MutexExt;
use skill_eeg::artifact_detection::ArtifactDetector;
use skill_eeg::eeg_bands::BandAnalyzer;
use crate::eeg_embeddings::EegAccumulator;
use skill_eeg::eeg_filter::{EegFilter, FilterConfig};
use skill_eeg::eeg_quality::QualityMonitor;
use skill_eeg::head_pose::HeadPoseTracker;
use crate::AppStateExt;

// ── SessionDsp ────────────────────────────────────────────────────────────────

pub(crate) struct SessionDsp {
    // ── DSP pipeline ─────────────────────────────────────────────────────────
    pub filter:            EegFilter,
    pub band_analyzer:     BandAnalyzer,
    pub quality:           QualityMonitor,
    pub artifact_detector: ArtifactDetector,
    pub head_pose:         HeadPoseTracker,
    pub accumulator:       EegAccumulator,

    // ── Cached config — compared each frame to detect Tauri-command changes ──
    last_filter_config: FilterConfig,
    last_overlap_secs:  f32,
    last_hooks: Vec<crate::settings::HookRule>,
}

impl SessionDsp {
    /// Construct a fresh `SessionDsp` from the current `AppState` config.
    ///
    /// Acquires the `AppState` lock exactly once, clones the needed values,
    /// and releases it before building any DSP object.
    pub(crate) fn new(app: &AppHandle, channel_names: &[&str]) -> Self {
        let num_channels = channel_names.len();
        let (filter_cfg, overlap_secs, hooks, skill_dir, model_config,
             model_status, download_cancel, encoder_reload_requested, logger, hook_runtime) = {
            let r = app.app_state();
            let g = r.lock_or_recover();
            (
                g.status.filter_config,
                g.status.embedding_overlap_secs,
                g.hooks.clone(),
                g.skill_dir.clone(),
                g.embedding.model_config.clone(),
                g.embedding.model_status.clone(),
                g.embedding.download_cancel.clone(),
                g.embedding.encoder_reload_requested.clone(),
                g.logger.clone(),
                g.hook_runtime.clone(),
            )
        }; // lock released here

        // Shared text embedder — reuse the app-wide instance instead of
        // creating a separate ~130 MB copy per EEG session.
        let shared_embedder = std::sync::Arc::clone(
            &*app.state::<std::sync::Arc<crate::EmbedderState>>()
        );

        // Obtain a cloned Arc to the global cross-day HNSW index so the embed
        // worker can insert into it without going through Tauri's state system.
        let global_index = app
            .state::<std::sync::Arc<crate::global_eeg_index::GlobalEegIndex>>()
            .inner()
            .arc();
        let label_idx = app
            .state::<std::sync::Arc<crate::label_index::LabelIndexState>>()
            .inner()
            .clone();
        let ws_broadcaster = app
            .state::<crate::ws_server::WsBroadcaster>()
            .inner()
            .clone();

        let mut accumulator = EegAccumulator::new(
            skill_dir, model_config, model_status, download_cancel,
            encoder_reload_requested, logger, global_index,
            hooks.clone(), shared_embedder, label_idx, ws_broadcaster,
            hook_runtime, app.clone(),
        );
        accumulator.set_overlap_secs(overlap_secs);

        Self {
            filter:            EegFilter::new(filter_cfg),
            band_analyzer:     BandAnalyzer::new_with_rate(filter_cfg.sample_rate),
            quality:           QualityMonitor::with_window(
                num_channels, filter_cfg.sample_rate as usize,
            ),
            artifact_detector: ArtifactDetector::with_channels(
                filter_cfg.sample_rate as f64, channel_names,
            ),
            head_pose:         HeadPoseTracker::new(), // default 52 Hz; future: pass device IMU rate
            accumulator,
            last_filter_config: filter_cfg,
            last_overlap_secs:  overlap_secs,
            last_hooks: hooks,
        }
    }

    /// Sync DSP config from `AppState` at the top of every sample frame.
    ///
    /// Takes a single brief read lock, detects whether `filter_config` or
    /// `embedding_overlap_secs` changed since the last frame (e.g. because
    /// the user adjusted settings in the UI), and applies the change to the
    /// local DSP objects.  No-ops when nothing changed — cheap.
    pub(crate) fn sync_config(&mut self, app: &AppHandle) {
        let (filter_cfg, overlap_secs, hooks) = {
            let r = app.app_state();
            let g = r.lock_or_recover();
            (g.status.filter_config, g.status.embedding_overlap_secs, g.hooks.clone())
        };

        if filter_cfg != self.last_filter_config {
            self.filter.set_config(filter_cfg);
            self.last_filter_config = filter_cfg;
        }
        if (overlap_secs - self.last_overlap_secs).abs() > f32::EPSILON {
            self.accumulator.set_overlap_secs(overlap_secs);
            self.last_overlap_secs = overlap_secs;
        }
        if hooks != self.last_hooks {
            self.accumulator.set_hooks(hooks.clone());
            self.last_hooks = hooks;
        }
    }
}
