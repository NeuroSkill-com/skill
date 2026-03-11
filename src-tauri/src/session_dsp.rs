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
//! `AppState` behind the shared `Mutex<AppState>`, which meant every
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

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::AppState;
use crate::MutexExt;
use crate::artifact_detection::ArtifactDetector;
use crate::constants::EEG_CHANNELS;
use crate::eeg_bands::BandAnalyzer;
use crate::eeg_embeddings::EegAccumulator;
use crate::eeg_filter::{EegFilter, FilterConfig};
use crate::eeg_quality::QualityMonitor;
use crate::head_pose::HeadPoseTracker;

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
}

impl SessionDsp {
    /// Construct a fresh `SessionDsp` from the current `AppState` config.
    ///
    /// Acquires the `AppState` lock exactly once, clones the needed values,
    /// and releases it before building any DSP object.
    pub(crate) fn new(app: &AppHandle) -> Self {
        let (filter_cfg, overlap_secs, skill_dir, model_config,
             model_status, download_cancel, logger) = {
            let r = app.state::<Mutex<AppState>>();
            let g = r.lock_or_recover();
            (
                g.status.filter_config,
                g.status.embedding_overlap_secs,
                g.skill_dir.clone(),
                g.model_config.clone(),
                g.model_status.clone(),
                g.download_cancel.clone(),
                g.logger.clone(),
            )
        }; // lock released here

        // Obtain a cloned Arc to the global cross-day HNSW index so the embed
        // worker can insert into it without going through Tauri's state system.
        let global_index = app
            .state::<std::sync::Arc<crate::global_eeg_index::GlobalEegIndex>>()
            .inner()
            .arc();

        let mut accumulator = EegAccumulator::new(
            skill_dir, model_config, model_status, download_cancel, logger,
            global_index,
        );
        accumulator.set_overlap_secs(overlap_secs);

        Self {
            filter:            EegFilter::new(filter_cfg),
            band_analyzer:     BandAnalyzer::new(),
            quality:           QualityMonitor::new(EEG_CHANNELS),
            artifact_detector: ArtifactDetector::new(),
            head_pose:         HeadPoseTracker::new(),
            accumulator,
            last_filter_config: filter_cfg,
            last_overlap_secs:  overlap_secs,
        }
    }

    /// Sync DSP config from `AppState` at the top of every sample frame.
    ///
    /// Takes a single brief read lock, detects whether `filter_config` or
    /// `embedding_overlap_secs` changed since the last frame (e.g. because
    /// the user adjusted settings in the UI), and applies the change to the
    /// local DSP objects.  No-ops when nothing changed — cheap.
    pub(crate) fn sync_config(&mut self, app: &AppHandle) {
        let (filter_cfg, overlap_secs) = {
            let r = app.state::<Mutex<AppState>>();
            let g = r.lock_or_recover();
            (g.status.filter_config, g.status.embedding_overlap_secs)
        };

        if filter_cfg != self.last_filter_config {
            self.filter.set_config(filter_cfg);
            self.last_filter_config = filter_cfg;
        }
        if (overlap_secs - self.last_overlap_secs).abs() > f32::EPSILON {
            self.accumulator.set_overlap_secs(overlap_secs);
            self.last_overlap_secs = overlap_secs;
        }
    }
}
