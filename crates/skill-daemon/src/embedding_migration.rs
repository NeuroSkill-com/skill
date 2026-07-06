// SPDX-License-Identifier: GPL-3.0-only
//! One-time migration to the nomic task-prefix embedding scheme.
//!
//! Earlier builds stored **unprefixed** text embeddings and used
//! OCR-text-as-vision for screenshots. This release switches to nomic
//! `search_query:` / `search_document:` prefixes and real
//! `nomic-embed-vision-v1.5` image embeddings. Existing vectors are in the old
//! scheme, so this invalidates them — the source text/images on disk are
//! untouched, only the *derived* vectors are cleared — and lets the existing
//! re-embedders regenerate them. Gated by a marker file so it runs at most once.

use skill_daemon_state::AppState;
use tracing::{info, warn};

// Two markers, so a failed/interrupted label re-embed doesn't wedge the whole
// migration: the cheap invalidations run once, while the label re-embed is
// retried (across restarts) until it succeeds.
const CLEARS_MARKER: &str = ".embedding_scheme_clears_v1";
const LABELS_MARKER: &str = ".embedding_scheme_labels_v1";

/// Run the migration. The cheap invalidations run synchronously, so call this
/// **before** the screenshot worker spawns (its backfill then picks up the
/// cleared rows); the label re-embed runs on a background thread so it doesn't
/// block daemon startup.
pub fn run_if_needed(state: &AppState) {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    if skill_dir.as_os_str().is_empty() {
        return;
    }

    // ── Invalidation (runs once) ────────────────────────────────────────────
    // Clear derived vectors so the re-embedders regenerate them. Gated by its
    // own marker so we never re-clear data that's since been re-embedded.
    let clears_marker = skill_dir.join(CLEARS_MARKER);
    if !clears_marker.exists() {
        info!("[embed-migrate] applying nomic task-prefix embedding scheme (invalidation)");

        // Terminal — clear vectors; the tty embed worker re-embeds.
        if let Some(store) = skill_data::activity_store::ActivityStore::open(&skill_dir) {
            let n = store.clear_terminal_embeddings();
            info!("[embed-migrate] cleared {n} terminal embeddings");
        }

        // Screenshots — clear vectors (keep OCR text) and drop the HNSW files;
        // the backfill re-embeds vision (nomic-vision) and the existing OCR
        // text (nomic-text) without re-running OCR.
        if let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) {
            let n = store.clear_embeddings();
            info!("[embed-migrate] cleared {n} screenshot embeddings");
        }
        for f in [skill_constants::SCREENSHOTS_HNSW, skill_constants::SCREENSHOTS_OCR_HNSW] {
            let _ = std::fs::remove_file(skill_dir.join(f));
        }

        if let Err(e) = std::fs::write(&clears_marker, b"v1\n") {
            warn!("[embed-migrate] could not write clears marker: {e}");
        }
    }

    // ── Labels (retried until success) ──────────────────────────────────────
    // Re-embed in place (search_document) on a background thread; its marker is
    // written only on success, so a failed run is retried on the next start
    // rather than silently leaving labels on the old scheme.
    let labels_marker = skill_dir.join(LABELS_MARKER);
    if !labels_marker.exists() {
        let skill_dir2 = skill_dir.clone();
        let reembed_cfg = skill_settings::load_settings(&skill_dir).reembed;
        let label_index = state.label_index.clone();
        let embedder = state.text_embedder.clone();
        std::thread::spawn(move || {
            match crate::routes::labels::reembed_all_labels_blocking(&skill_dir2, &reembed_cfg, &label_index, &embedder)
            {
                Ok(n) => {
                    info!("[embed-migrate] re-embedded {n} labels");
                    if let Err(e) = std::fs::write(&labels_marker, b"v1\n") {
                        warn!("[embed-migrate] could not write labels marker: {e}");
                    }
                }
                Err(e) => warn!("[embed-migrate] label re-embed failed: {e} — will retry next start"),
            }
        });
    }
}
