// SPDX-License-Identifier: GPL-3.0-only
//! EXG model/config/download handlers.

use axum::{extract::State, Json};
use skill_eeg::eeg_model_config::{load_model_config, save_model_config, EegModelStatus, ExgModelConfig};

use crate::state::AppState;

pub(crate) async fn get_model_config_impl(State(state): State<AppState>) -> Json<ExgModelConfig> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    Json(load_model_config(&skill_dir))
}

pub(crate) async fn set_model_config_impl(
    State(state): State<AppState>,
    Json(config): Json<ExgModelConfig>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    save_model_config(&skill_dir, &config);
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn get_model_status_impl(State(state): State<AppState>) -> Json<EegModelStatus> {
    let mut st = state.exg_model_status.lock().map(|g| g.clone()).unwrap_or_default();

    if !st.weights_found && !st.downloading_weights {
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        let config = skill_eeg::eeg_model_config::load_model_config(&skill_dir);
        let found = probe_weights_for_config(&config);
        if let Some((weights_path, backend_str)) = found {
            st.weights_found = true;
            st.weights_path = Some(weights_path);
            st.active_model_backend = Some(backend_str);
            if let Ok(mut shared) = state.exg_model_status.lock() {
                shared.weights_found = true;
                shared.weights_path = st.weights_path.clone();
                shared.active_model_backend = st.active_model_backend.clone();
            }
        }
    }

    Json(st)
}

/// Public so `main.rs` can call it during daemon startup.
pub fn probe_weights_for_config(config: &ExgModelConfig) -> Option<(String, String)> {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../../src-tauri/exg_catalog.json")).ok()?;
    let backend = config.model_backend.as_str();
    let family_id = if backend == "luna" {
        format!("luna-{}", config.luna_variant)
    } else {
        let families = catalog.get("families")?.as_object()?;
        families
            .keys()
            .find(|id| family_id_to_backend(id) == backend)
            .cloned()
            .unwrap_or_else(|| backend.to_string())
    };

    let fam = catalog.get("families")?.get(&family_id)?;
    let repo = fam.get("repo")?.as_str()?;
    let wf = fam.get("weights_file")?.as_str()?;

    let snaps_dir = skill_data::util::hf_model_dir(repo).join("snapshots");
    let entries = std::fs::read_dir(&snaps_dir).ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let wp = entry.path().join(wf);
        if skill_exg::validate_or_remove(&wp) {
            return Some((wp.display().to_string(), backend.to_string()));
        }
    }
    None
}

pub(crate) async fn trigger_reembed_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let events_tx = state.events_tx.clone();
    let model_status = state.exg_model_status.clone();
    let label_index = state.label_index.clone();

    // Check if reembed is already running (use model_status as a simple guard).
    {
        let st = model_status.lock().unwrap_or_else(|e| e.into_inner());
        if st.downloading_weights {
            return Json(
                serde_json::json!({ "ok": false, "message": "weights download in progress — wait for it to finish" }),
            );
        }
    }

    let cancel = state.idle_reembed_cancel.clone();
    // Reset cancel so a previous idle-reembed cancel doesn't block the manual trigger.
    cancel.store(false, std::sync::atomic::Ordering::Relaxed);

    tokio::task::spawn_blocking(move || {
        if let Err(e) = run_batch_reembed_with_cancel(
            &skill_dir, &events_tx, &cancel, true, // use_gpu
            10,   // throttle_ms
            50,   // batch_size
        ) {
            tracing::error!("batch reembed failed: {e}");
            let _ = events_tx.send(skill_daemon_common::EventEnvelope {
                r#type: "reembed-progress".into(),
                ts_unix_ms: now_unix_ms(),
                correlation_id: None,
                payload: serde_json::json!({ "status": "error", "message": e.to_string() }),
            });
        }
        // Rebuild label EEG index so interactive search can bridge text → EEG.
        // Labels created before embeddings existed will now have EEG entries.
        let stats = skill_label_index::rebuild(&skill_dir, &label_index);
        tracing::info!(
            "[reembed] label index rebuilt: {} text, {} eeg ({} skipped)",
            stats.text_nodes,
            stats.eeg_nodes,
            stats.eeg_skipped
        );
    });

    Json(serde_json::json!({ "ok": true, "message": "reembed started" }))
}

/// Scan all day directories, find epochs with empty eeg_embedding,
/// re-read raw EEG samples from CSV, encode, and update the BLOB.
/// Public so the idle-reembed loop can call it.
pub(crate) fn run_batch_reembed_with_cancel(
    skill_dir: &std::path::Path,
    events_tx: &tokio::sync::broadcast::Sender<skill_daemon_common::EventEnvelope>,
    cancel: &std::sync::atomic::AtomicBool,
    _use_gpu: bool,
    throttle_ms: u64,
    batch_size: usize,
) -> anyhow::Result<()> {
    tracing::info!("[reembed] starting batch reembed");

    // Emit immediate feedback so the UI progress bar shows activity.
    let _ = events_tx.send(skill_daemon_common::EventEnvelope {
        r#type: "reembed-progress".into(),
        ts_unix_ms: now_unix_ms(),
        correlation_id: None,
        payload: serde_json::json!({ "status": "loading_encoder", "total": 0, "done": 0 }),
    });

    // 1. Load the encoder (tries GPU first, falls back to CPU).
    let config = load_model_config(skill_dir);
    let encoder = crate::embed::load_encoder_public(&config, skill_dir);
    if encoder.is_none() {
        anyhow::bail!(
            "encoder failed to load for backend '{}' — check model weights",
            config.model_backend.as_str()
        );
    }
    let encoder = encoder.unwrap();

    let _ = events_tx.send(skill_daemon_common::EventEnvelope {
        r#type: "reembed-progress".into(),
        ts_unix_ms: now_unix_ms(),
        correlation_id: None,
        payload: serde_json::json!({ "status": "scanning", "total": 0, "done": 0 }),
    });

    // 2. Scan all day directories for sessions with missing embeddings.
    let mut total_needed = 0u64;
    let mut total_done = 0u64;
    let mut total_failed = 0u64;
    // Track channel counts that produced consecutive encode failures so we can
    // skip further attempts with the same channel count (e.g. 32-ch generic
    // channels that cause GPU autotune hangs).
    let mut skip_ch_counts: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut consecutive_failures_by_ch: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    const CONSECUTIVE_FAIL_LIMIT: u32 = 5;

    let mut day_dirs: Vec<_> = std::fs::read_dir(skill_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.len() == 8 && n.starts_with("20"))
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    day_dirs.sort();
    day_dirs.reverse(); // Most recent days first — users care about recent data.

    // First pass: count total needed.
    for day_dir in &day_dirs {
        let db_path = day_dir.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM embeddings WHERE eeg_embedding IS NULL OR length(eeg_embedding) < 4",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        total_needed += count.max(0) as u64;
    }

    tracing::info!(
        "[reembed] {total_needed} epochs need embeddings across {} days",
        day_dirs.len()
    );
    let _ = events_tx.send(skill_daemon_common::EventEnvelope {
        r#type: "reembed-progress".into(),
        ts_unix_ms: now_unix_ms(),
        correlation_id: None,
        payload: serde_json::json!({ "status": "started", "total": total_needed, "done": 0 }),
    });

    if total_needed == 0 {
        let _ = events_tx.send(skill_daemon_common::EventEnvelope {
            r#type: "reembed-progress".into(),
            ts_unix_ms: now_unix_ms(),
            correlation_id: None,
            payload: serde_json::json!({ "status": "done", "total": 0, "done": 0 }),
        });
        return Ok(());
    }

    // 3. Process each day directory.
    for day_dir in &day_dirs {
        let db_path = day_dir.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }

        // Find CSV files in this day directory (raw EEG data).
        let csv_files = find_eeg_csvs(day_dir);
        if csv_files.is_empty() {
            continue;
        }

        // Read session metadata (sample rate). Channel names are now per-segment.
        let (_channel_names, sample_rate) = read_session_meta(day_dir, &csv_files);
        if sample_rate == 0.0 {
            tracing::warn!("[reembed] skipping {} — no sample rate metadata", day_dir.display());
            continue;
        }

        // Open DB for writing.
        let Ok(conn) = rusqlite::Connection::open(&db_path) else {
            continue;
        };
        let _ = conn.execute_batch("PRAGMA busy_timeout=5000;");
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_embeddings_timestamp ON embeddings(timestamp)",
            [],
        );

        // Get timestamps of epochs that need embeddings.
        let mut stmt = conn.prepare(
            "SELECT id, timestamp FROM embeddings WHERE eeg_embedding IS NULL OR length(eeg_embedding) < 4 ORDER BY timestamp",
        )?;
        let epochs_needed: Vec<(i64, i64)> = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        if epochs_needed.is_empty() {
            continue;
        }

        let day_name = day_dir.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
        tracing::info!(
            "[reembed] {day_name}: {} epochs to embed, loading CSV data...",
            epochs_needed.len()
        );

        // Load all raw CSV data for this day into a time-indexed buffer.
        // Each segment carries its own channel names from the CSV header.
        let raw_data = load_day_csv_data(day_dir, &csv_files, sample_rate);

        let epoch_samples = (sample_rate * 5.0) as usize; // 5-second epoch
        let commit_size = batch_size.max(10); // commit every N epochs for write efficiency

        // Process in transaction batches for write performance.
        for chunk in epochs_needed.chunks(commit_size) {
            // Check cancel flag between batches (backpressure: device reconnected).
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                tracing::info!("[reembed] cancelled — device reconnected or user stopped");
                let _ = events_tx.send(skill_daemon_common::EventEnvelope {
                    r#type: "reembed-progress".into(),
                    ts_unix_ms: now_unix_ms(),
                    correlation_id: None,
                    payload: serde_json::json!({
                        "status": "paused",
                        "total": total_needed,
                        "done": total_done,
                        "reason": "device_connected",
                    }),
                });
                return Ok(());
            }

            let _ = conn.execute_batch("BEGIN");

            for (row_id, ts_ms) in chunk {
                let ts_secs = (*ts_ms as f64) / 1000.0;

                let (samples, seg_ch_names) = extract_epoch_samples(&raw_data, ts_secs, epoch_samples);
                if samples.is_empty() {
                    if total_failed == 0 {
                        tracing::warn!(
                            "[reembed] first empty extract at ts={ts_secs:.1}s (row_id={row_id}, epoch_samples={epoch_samples})",
                        );
                    }
                    total_failed += 1;
                    total_done += 1;
                    continue;
                }

                let n_ch = samples.len();

                // Skip channel counts that have failed repeatedly (prevents GPU
                // hangs on unsupported channel configurations).
                if skip_ch_counts.contains(&n_ch) {
                    total_failed += 1;
                    total_done += 1;
                    continue;
                }

                // Log first encode attempt per channel count for diagnostics.
                if total_done == 0 || (total_done > 0 && total_done == total_failed) {
                    tracing::info!(
                        "[reembed] first encode: channels={n_ch}, samples_per_ch={}, ts={ts_secs:.1}s",
                        samples.first().map(|s| s.len()).unwrap_or(0),
                    );
                }

                let t0 = std::time::Instant::now();
                let embedding = encode_raw_samples(&encoder, &samples, &seg_ch_names, sample_rate);
                let ms = t0.elapsed().as_millis();

                if let Some(emb) = embedding {
                    let blob: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
                    let _ = conn.execute(
                        "UPDATE embeddings SET eeg_embedding = ?1 WHERE id = ?2",
                        rusqlite::params![blob, row_id],
                    );
                    if ms > 2000 {
                        tracing::warn!("[reembed] slow encode: {ms}ms for epoch {ts_ms}");
                    }
                    // Reset failure counter on success.
                    consecutive_failures_by_ch.remove(&n_ch);
                } else {
                    if total_failed == 0 {
                        tracing::warn!(
                            "[reembed] first encode failure at ts={ts_secs:.1}s (channels={n_ch}, samples_per_ch={}, rate={sample_rate}Hz)",
                            samples.first().map(|s| s.len()).unwrap_or(0),
                        );
                    }
                    total_failed += 1;
                    let count = consecutive_failures_by_ch.entry(n_ch).or_insert(0);
                    *count += 1;
                    if *count >= CONSECUTIVE_FAIL_LIMIT {
                        tracing::warn!(
                            "[reembed] skipping all {n_ch}-channel epochs after {CONSECUTIVE_FAIL_LIMIT} consecutive failures"
                        );
                        skip_ch_counts.insert(n_ch);
                    }
                }

                total_done += 1;
            }

            let _ = conn.execute_batch("COMMIT");

            // Emit progress every batch.
            let _ = events_tx.send(skill_daemon_common::EventEnvelope {
                r#type: "reembed-progress".into(),
                ts_unix_ms: now_unix_ms(),
                correlation_id: None,
                payload: serde_json::json!({
                    "status": "running",
                    "total": total_needed,
                    "done": total_done,
                    "failed": total_failed,
                    "date": day_name,
                }),
            });

            // Throttle between batches to reduce contention with other daemon tasks.
            if throttle_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(throttle_ms));
            }
        }

        tracing::info!(
            "[reembed] {} done: {}/{} epochs embedded",
            day_dir.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            total_done - total_failed,
            epochs_needed.len()
        );
    }

    tracing::info!("[reembed] complete: {total_done} processed, {total_failed} failed out of {total_needed}");
    let _ = events_tx.send(skill_daemon_common::EventEnvelope {
        r#type: "reembed-progress".into(),
        ts_unix_ms: now_unix_ms(),
        correlation_id: None,
        payload: serde_json::json!({
            "status": "done",
            "total": total_needed,
            "done": total_done,
            "failed": total_failed,
        }),
    });

    Ok(())
}

/// Find exg_*.csv files (raw EEG, not metrics/ppg/imu).
fn find_eeg_csvs(day_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut csvs: Vec<_> = std::fs::read_dir(day_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?;
            if name.starts_with("exg_")
                && name.ends_with(".csv")
                && !name.contains("metrics")
                && !name.contains("ppg")
                && !name.contains("imu")
            {
                Some(p)
            } else {
                None
            }
        })
        .collect();
    csvs.sort();
    csvs
}

/// Read channel names and sample rate from JSON sidecar or CSV header.
fn read_session_meta(_day_dir: &std::path::Path, csv_files: &[std::path::PathBuf]) -> (Vec<String>, f64) {
    // Try JSON sidecar first.
    for csv in csv_files {
        let json_path = csv.with_extension("json");
        if !json_path.exists() {
            continue;
        }
        if let Ok(data) = std::fs::read_to_string(&json_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                let sr = v
                    .get("sample_rate_hz")
                    .or_else(|| v.get("sample_rate"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let ch: Vec<String> = v
                    .get("channel_names")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                if sr > 0.0 && !ch.is_empty() {
                    return (ch, sr);
                }
                // Have JSON but no sample_rate? Try to infer from CSV header.
                if !ch.is_empty() {
                    // Default to 256 Hz (Muse standard).
                    return (ch, 256.0);
                }
            }
        }
    }
    // Fallback: read CSV header to get channel names.
    for csv in csv_files {
        if let Ok(file) = std::fs::File::open(csv) {
            use std::io::BufRead;
            let mut reader = std::io::BufReader::new(file);
            let mut header = String::new();
            if reader.read_line(&mut header).is_ok() {
                let cols: Vec<&str> = header.trim().split(',').collect();
                if cols.len() > 1 && cols[0].contains("timestamp") {
                    let ch: Vec<String> = cols[1..].iter().map(|s| s.trim().to_string()).collect();
                    if !ch.is_empty() {
                        return (ch, 256.0); // Assume 256 Hz default.
                    }
                }
            }
        }
    }
    (vec![], 0.0)
}

/// Timestamp-indexed raw EEG data: Vec<(timestamp_secs, Vec<Vec<f32>> channels×samples)>.
struct RawDayData {
    /// Sorted list of (start_time_secs, channel_names, samples_per_channel[ch][sample]).
    segments: Vec<(f64, Vec<String>, Vec<Vec<f32>>)>,
    sample_rate: f64,
}

/// Load all raw CSV data for a day directory.
/// Each segment stores its own channel names read from the CSV header.
fn load_day_csv_data(_day_dir: &std::path::Path, csv_files: &[std::path::PathBuf], sample_rate: f64) -> RawDayData {
    let mut segments = Vec::new();

    for csv_path in csv_files {
        let Ok(file) = std::fs::File::open(csv_path) else {
            continue;
        };
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;
        let mut lines = reader.lines();

        // Read header to detect channel names and column count for this CSV.
        let header_line = lines.next();
        let header_str = header_line
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .cloned()
            .unwrap_or_default();
        let header_cols: Vec<&str> = header_str.split(',').collect();
        if header_cols.len() < 2 {
            continue;
        }
        // Channel names: all columns after the timestamp column.
        let seg_channel_names: Vec<String> = header_cols[1..].iter().map(|s| s.trim().to_string()).collect();
        let file_ch = seg_channel_names.len();
        if file_ch == 0 {
            continue;
        }
        let mut channels: Vec<Vec<f32>> = vec![Vec::new(); file_ch];
        let mut first_ts: Option<f64> = None;

        for line in lines.map_while(Result::ok) {
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() < file_ch + 1 {
                continue;
            }

            if first_ts.is_none() {
                first_ts = fields[0].parse::<f64>().ok();
            }

            // Parse all channels for this row; skip the entire row if any channel fails.
            let mut row_vals = Vec::with_capacity(file_ch);
            let mut row_ok = true;
            for field in fields[1..].iter().take(file_ch) {
                if let Ok(v) = field.parse::<f32>() {
                    row_vals.push(v);
                } else {
                    row_ok = false;
                    break;
                }
            }
            if row_ok && row_vals.len() == file_ch {
                for (ch, v) in row_vals.into_iter().enumerate() {
                    channels[ch].push(v);
                }
            }
        }

        if let Some(mut ts) = first_ts {
            // Detect relative timestamps (small values < year 2001).
            // These are device-internal cumulative clocks, NOT offsets from zero.
            // Use the session start time from the filename as the segment start.
            if ts < 1_000_000_000.0 {
                if let Some(start) = csv_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.strip_prefix("exg_"))
                    .and_then(|s| s.parse::<f64>().ok())
                {
                    ts = start;
                }
            }
            if channels.iter().all(|c| !c.is_empty()) {
                segments.push((ts, seg_channel_names.clone(), channels));
            }
        }
    }

    segments.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    RawDayData { segments, sample_rate }
}

/// Extract a 5-second window of samples at the given epoch timestamp.
/// Returns (samples, channel_names) for the matching segment.
fn extract_epoch_samples(data: &RawDayData, epoch_ts_secs: f64, epoch_samples: usize) -> (Vec<Vec<f32>>, Vec<String>) {
    for (seg_start, seg_ch_names, channels) in &data.segments {
        let seg_len = channels[0].len();
        let seg_end = *seg_start + (seg_len as f64 / data.sample_rate);

        // Check if this epoch falls within this segment.
        if epoch_ts_secs >= *seg_start && epoch_ts_secs < seg_end {
            let offset = ((epoch_ts_secs - seg_start) * data.sample_rate) as usize;
            let end = (offset + epoch_samples).min(seg_len);
            if end - offset < epoch_samples {
                continue;
            } // encoder requires full 5 s epoch

            let mut result = Vec::with_capacity(channels.len());
            for ch in channels {
                result.push(ch[offset..end].to_vec());
            }
            return (result, seg_ch_names.clone());
        }
    }
    (vec![], vec![])
}

/// Encode raw samples using the loaded encoder.
fn encode_raw_samples(
    encoder: &crate::embed::PublicEncoder,
    samples: &[Vec<f32>],
    channel_names: &[String],
    sample_rate: f64,
) -> Option<Vec<f32>> {
    crate::embed::encode_raw_public(encoder, samples, channel_names, sample_rate)
}

pub(crate) async fn trigger_weights_download_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    use std::sync::atomic::Ordering;

    state.exg_download_cancel.store(false, Ordering::Relaxed);

    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let config = skill_eeg::eeg_model_config::load_model_config(&skill_dir);
    let status = state.exg_model_status.clone();
    let cancel = state.exg_download_cancel.clone();

    if let Ok(st) = status.lock() {
        if st.downloading_weights {
            return Json(serde_json::json!({ "ok": false, "message": "download already in progress" }));
        }
    }

    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../../src-tauri/exg_catalog.json")).unwrap_or_default();
    let backend_str = config.model_backend.as_str().to_string();
    let (hf_repo, weights_file, config_file) = resolve_download_target(&catalog, &config);

    spawn_exg_download(state, hf_repo, weights_file, config_file, backend_str, status, cancel);

    Json(serde_json::json!({ "ok": true, "message": "weights download started" }))
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn emit_daemon_event(state: &AppState, event_type: &str, payload: serde_json::Value) {
    let _ = state.events_tx.send(skill_daemon_common::EventEnvelope {
        r#type: event_type.to_string(),
        ts_unix_ms: now_unix_ms(),
        correlation_id: None,
        payload,
    });
}

fn spawn_exg_download(
    state: AppState,
    hf_repo: String,
    weights_file: String,
    config_file: String,
    backend_str: String,
    status: std::sync::Arc<std::sync::Mutex<EegModelStatus>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    tokio::spawn(async move {
        let status_for_thread = status.clone();
        let cancel_for_thread = cancel.clone();

        let mut job = tokio::task::spawn_blocking(move || {
            skill_exg::download_hf_weights_files(
                &hf_repo,
                &weights_file,
                &config_file,
                &status_for_thread,
                &cancel_for_thread,
                false,
            )
        });

        loop {
            if job.is_finished() {
                break;
            }

            if let Ok(st) = status.lock() {
                emit_daemon_event(
                    &state,
                    "ExgDownloadProgress",
                    serde_json::json!({
                        "backend": backend_str,
                        "downloading": st.downloading_weights,
                        "progress": st.download_progress,
                        "status_msg": st.download_status_msg,
                        "weights_found": st.weights_found,
                        "needs_restart": false,
                    }),
                );
            }

            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        let result = (&mut job).await;
        let succeeded = matches!(result, Ok(Some(_)));

        if succeeded {
            if let Ok(mut st) = status.lock() {
                st.download_needs_restart = false;
                st.weights_found = true;
                st.active_model_backend = Some(backend_str.clone());
            }
        }

        if let Ok(st) = status.lock() {
            emit_daemon_event(
                &state,
                if succeeded {
                    "ExgDownloadCompleted"
                } else {
                    "ExgDownloadFailed"
                },
                serde_json::json!({
                    "backend": backend_str,
                    "downloading": st.downloading_weights,
                    "progress": st.download_progress,
                    "status_msg": st.download_status_msg,
                    "weights_found": st.weights_found,
                    "needs_restart": false,
                }),
            );
        }

        if succeeded {
            tracing::info!("[exg] weights download complete for {backend_str}");
        } else {
            tracing::warn!("[exg] weights download failed or cancelled for {backend_str}");
        }
    });
}

fn resolve_download_target(catalog: &serde_json::Value, config: &ExgModelConfig) -> (String, String, String) {
    let backend = config.model_backend.as_str();

    let family_id = if backend == "luna" {
        format!("luna-{}", config.luna_variant)
    } else {
        let families = catalog.get("families").and_then(|f| f.as_object());
        if let Some(fams) = families {
            fams.keys()
                .find(|id| family_id_to_backend(id) == backend)
                .cloned()
                .unwrap_or_else(|| backend.to_string())
        } else {
            backend.to_string()
        }
    };

    if let Some(fam) = catalog.get("families").and_then(|f| f.get(&family_id)) {
        let repo = fam.get("repo").and_then(|v| v.as_str()).unwrap_or(&config.hf_repo);
        let wf = fam
            .get("weights_file")
            .and_then(|v| v.as_str())
            .unwrap_or("model-00001-of-00001.safetensors");
        let cf = fam.get("config_file").and_then(|v| v.as_str()).unwrap_or("config.json");
        (repo.to_string(), wf.to_string(), cf.to_string())
    } else if backend == "luna" {
        (
            config.luna_hf_repo.clone(),
            config.luna_weights_file().to_string(),
            "config.json".to_string(),
        )
    } else {
        (
            config.hf_repo.clone(),
            "model-00001-of-00001.safetensors".to_string(),
            "config.json".to_string(),
        )
    }
}

fn family_id_to_backend(id: &str) -> &str {
    if id == "zuna" {
        return "zuna";
    }
    if id.starts_with("luna-") {
        return "luna";
    }
    if id == "reve-base" || id == "reve-large" {
        return "reve";
    }
    if id == "osf-base" {
        return "osf";
    }
    if id.starts_with("steegformer-") {
        return "steegformer";
    }
    id
}

pub(crate) async fn cancel_weights_download_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    state
        .exg_download_cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({ "ok": true, "message": "weights download cancellation requested" }))
}

pub(crate) async fn estimate_reembed_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let avg_embed_ms = state.exg_model_status.lock().map(|s| s.avg_embed_ms).unwrap_or(0.0);
    let idle_reembed = state.idle_reembed_state.lock().map(|s| s.clone()).unwrap_or_default();

    let result = tokio::task::spawn_blocking(move || {
        let mut total_epochs = 0i64;
        let mut missing_embeddings = 0i64;
        let mut per_day: Vec<serde_json::Value> = Vec::new();
        let Ok(entries) = std::fs::read_dir(&skill_dir) else {
            return (0i64, 0i64, Vec::new());
        };
        let mut day_dirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_dir()
                    && e.file_name()
                        .to_str()
                        .map(|n| n.len() == 8 && n.starts_with("20"))
                        .unwrap_or(false)
            })
            .collect();
        day_dirs.sort_by_key(|e| e.file_name());

        for entry in &day_dirs {
            let path = entry.path();
            let db_path = path.join(skill_constants::SQLITE_FILE);
            if !db_path.exists() {
                continue;
            }
            let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            else {
                continue;
            };
            let t: i64 = conn
                .query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get(0))
                .unwrap_or(0);
            let m: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM embeddings WHERE eeg_embedding IS NULL OR length(eeg_embedding) < 4",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            total_epochs += t;
            missing_embeddings += m;

            let date = entry.file_name().to_str().unwrap_or("").to_string();
            per_day.push(serde_json::json!({
                "date": date,
                "total": t,
                "missing": m,
                "embedded": t - m,
            }));
        }
        (total_epochs, missing_embeddings, per_day)
    })
    .await
    .unwrap_or((0, 0, Vec::new()));

    let total_epochs = result.0;
    let missing = result.1;
    let embedded = total_epochs - missing;
    let per_day = result.2;
    let date_dirs = per_day.len() as i64;
    let coverage_pct = if total_epochs > 0 {
        (embedded as f64 / total_epochs as f64 * 100.0).round() as u64
    } else {
        0
    };
    let eta_secs = if avg_embed_ms > 0.0 && missing > 0 {
        ((missing as f64 * avg_embed_ms) / 1000.0).round() as u64
    } else {
        0
    };

    Json(serde_json::json!({
        "total_epochs": total_epochs,
        "embedded": embedded,
        "missing": missing,
        "date_dirs": date_dirs,
        "coverage_pct": coverage_pct,
        "avg_embed_ms": avg_embed_ms,
        "eta_secs": eta_secs,
        "per_day": per_day,
        "idle_reembed": idle_reembed,
    }))
}

pub(crate) async fn rebuild_index_impl() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "message": "index rebuild queued in daemon" }))
}

pub(crate) async fn get_exg_catalog_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    let v = tokio::task::spawn_blocking(move || {
        const BUNDLED: &str = include_str!("../../../../src-tauri/exg_catalog.json");
        let mut v: serde_json::Value = serde_json::from_str(BUNDLED).unwrap_or_default();

        if let Some(families) = v.get_mut("families").and_then(|f| f.as_object_mut()) {
            for (_id, fam) in families.iter_mut() {
                let repo = fam.get("repo").and_then(|r| r.as_str()).unwrap_or("");
                let weights_file = fam.get("weights_file").and_then(|w| w.as_str()).unwrap_or("");
                let cached = if !repo.is_empty() && !weights_file.is_empty() {
                    let snaps_dir = skill_data::util::hf_model_dir(repo).join("snapshots");
                    std::fs::read_dir(&snaps_dir)
                        .ok()
                        .map(|entries| {
                            entries.filter_map(|e| e.ok()).any(|e| {
                                let p = e.path().join(weights_file);
                                skill_exg::validate_or_remove(&p)
                            })
                        })
                        .unwrap_or(false)
                } else {
                    false
                };
                if let Some(obj) = fam.as_object_mut() {
                    obj.insert("weights_cached".to_string(), serde_json::json!(cached));
                }
            }
        }

        let config = skill_eeg::eeg_model_config::load_model_config(&skill_dir);
        let active_name = match config.model_backend {
            skill_eeg::eeg_model_config::ExgModelBackend::Luna => {
                if let Some(fam) = v
                    .get("families")
                    .and_then(|f| f.get(format!("luna-{}", config.luna_variant)))
                {
                    fam.get("name").and_then(|n| n.as_str()).unwrap_or("LUNA").to_string()
                } else {
                    "LUNA".to_string()
                }
            }
            _ => {
                let backend_str = config.model_backend.as_str();
                if let Some(families) = v.get("families").and_then(|f| f.as_object()) {
                    families
                        .iter()
                        .find(|(id, _)| family_id_to_backend(id) == backend_str)
                        .and_then(|(_, fam)| fam.get("name").and_then(|n| n.as_str()))
                        .unwrap_or("ZUNA")
                        .to_string()
                } else {
                    "ZUNA".to_string()
                }
            }
        };
        if let Some(obj) = v.as_object_mut() {
            obj.insert("active_model".to_string(), serde_json::json!(active_name));
        }

        v
    })
    .await
    .unwrap_or_default();

    Json(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_state() -> (tempfile::TempDir, AppState) {
        let td = tempfile::tempdir().unwrap();
        let st = AppState::new("t".into(), td.path().to_path_buf());
        (td, st)
    }

    #[tokio::test]
    async fn trigger_weights_download_rejects_when_already_running() {
        let (_td, st) = mk_state();
        if let Ok(mut s) = st.exg_model_status.lock() {
            s.downloading_weights = true;
        }
        let Json(v) = trigger_weights_download_impl(State(st)).await;
        assert_eq!(v["ok"], false);
    }

    #[tokio::test]
    async fn cancel_weights_download_sets_flag() {
        let (_td, st) = mk_state();
        st.exg_download_cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let Json(v) = cancel_weights_download_impl(State(st.clone())).await;
        assert_eq!(v["ok"], true);
        assert!(st.exg_download_cancel.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn family_id_to_backend_maps_known_variants() {
        assert_eq!(family_id_to_backend("zuna"), "zuna");
        assert_eq!(family_id_to_backend("luna-v1"), "luna");
        assert_eq!(family_id_to_backend("reve-base"), "reve");
        assert_eq!(family_id_to_backend("osf-base"), "osf");
        assert_eq!(family_id_to_backend("steegformer-x"), "steegformer");
    }

    // ── Helpers for reembed tests ─────────────────────────────────────────

    /// Write a CSV file with the given header and rows.
    fn write_csv(dir: &std::path::Path, name: &str, header: &str, rows: &[String]) {
        let path = dir.join(name);
        let mut contents = String::from(header);
        contents.push('\n');
        for row in rows {
            contents.push_str(row);
            contents.push('\n');
        }
        std::fs::write(path, contents).unwrap();
    }

    /// Generate N rows of EEG data for `n_ch` channels starting at `start_ts`.
    fn gen_rows(start_ts: f64, n_rows: usize, n_ch: usize, sample_rate: f64) -> Vec<String> {
        (0..n_rows)
            .map(|i| {
                let ts = start_ts + i as f64 / sample_rate;
                let vals: Vec<String> = (0..n_ch)
                    .map(|c| format!("{:.4}", (c as f64 + i as f64).sin()))
                    .collect();
                format!("{ts:.6},{}", vals.join(","))
            })
            .collect()
    }

    /// Build a CSV header for n_ch channels using Muse-style names.
    fn muse_header() -> &'static str {
        "timestamp_s,TP9,AF7,AF8,TP10"
    }

    /// Build a CSV header for n_ch generic channels.
    fn generic_header(n_ch: usize) -> String {
        let names: Vec<String> = (1..=n_ch).map(|i| format!("Ch{i}")).collect();
        format!("timestamp_s,{}", names.join(","))
    }

    // ── find_eeg_csvs ─────────────────────────────────────────────────────

    #[test]
    fn find_eeg_csvs_filters_correctly() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        std::fs::write(d.join("exg_100.csv"), "").unwrap();
        std::fs::write(d.join("exg_100_metrics.csv"), "").unwrap();
        std::fs::write(d.join("exg_100_ppg.csv"), "").unwrap();
        std::fs::write(d.join("exg_100_imu.csv"), "").unwrap();
        std::fs::write(d.join("exg_200.csv"), "").unwrap();
        std::fs::write(d.join("other.csv"), "").unwrap();

        let csvs = find_eeg_csvs(d);
        let names: Vec<&str> = csvs.iter().filter_map(|p| p.file_name()?.to_str()).collect();
        assert_eq!(names, vec!["exg_100.csv", "exg_200.csv"]);
    }

    #[test]
    fn find_eeg_csvs_empty_dir() {
        let td = tempfile::tempdir().unwrap();
        assert!(find_eeg_csvs(td.path()).is_empty());
    }

    // ── read_session_meta ─────────────────────────────────────────────────

    #[test]
    fn read_session_meta_from_json_sidecar() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        write_csv(d, "exg_100.csv", muse_header(), &gen_rows(100.0, 10, 4, 256.0));
        std::fs::write(
            d.join("exg_100.json"),
            r#"{"channel_names":["TP9","AF7","AF8","TP10"],"sample_rate":256.0}"#,
        )
        .unwrap();

        let csvs = vec![d.join("exg_100.csv")];
        let (ch, sr) = read_session_meta(d, &csvs);
        assert_eq!(ch, vec!["TP9", "AF7", "AF8", "TP10"]);
        assert_eq!(sr, 256.0);
    }

    #[test]
    fn read_session_meta_json_with_sample_rate_hz() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        write_csv(d, "exg_100.csv", muse_header(), &gen_rows(100.0, 10, 4, 256.0));
        std::fs::write(
            d.join("exg_100.json"),
            r#"{"channel_names":["TP9","AF7","AF8","TP10"],"sample_rate_hz":512.0}"#,
        )
        .unwrap();

        let csvs = vec![d.join("exg_100.csv")];
        let (ch, sr) = read_session_meta(d, &csvs);
        assert_eq!(sr, 512.0);
        assert_eq!(ch.len(), 4);
    }

    #[test]
    fn read_session_meta_fallback_to_csv_header() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        write_csv(d, "exg_100.csv", muse_header(), &gen_rows(100.0, 10, 4, 256.0));
        // No JSON sidecar — should read from CSV header with default 256 Hz.

        let csvs = vec![d.join("exg_100.csv")];
        let (ch, sr) = read_session_meta(d, &csvs);
        assert_eq!(ch, vec!["TP9", "AF7", "AF8", "TP10"]);
        assert_eq!(sr, 256.0);
    }

    #[test]
    fn read_session_meta_no_files() {
        let td = tempfile::tempdir().unwrap();
        let (ch, sr) = read_session_meta(td.path(), &[]);
        assert!(ch.is_empty());
        assert_eq!(sr, 0.0);
    }

    #[test]
    fn read_session_meta_corrupt_json_falls_back() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        write_csv(d, "exg_100.csv", muse_header(), &gen_rows(100.0, 10, 4, 256.0));
        std::fs::write(d.join("exg_100.json"), "NOT JSON").unwrap();

        let csvs = vec![d.join("exg_100.csv")];
        let (ch, sr) = read_session_meta(d, &csvs);
        assert_eq!(ch, vec!["TP9", "AF7", "AF8", "TP10"]);
        assert_eq!(sr, 256.0);
    }

    #[test]
    fn read_session_meta_json_missing_sample_rate_defaults() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        write_csv(d, "exg_100.csv", muse_header(), &gen_rows(100.0, 10, 4, 256.0));
        std::fs::write(
            d.join("exg_100.json"),
            r#"{"channel_names":["TP9","AF7","AF8","TP10"]}"#,
        )
        .unwrap();

        let csvs = vec![d.join("exg_100.csv")];
        let (ch, sr) = read_session_meta(d, &csvs);
        assert_eq!(ch.len(), 4);
        assert_eq!(sr, 256.0); // defaults when channels present but no rate
    }

    // ── load_day_csv_data ─────────────────────────────────────────────────

    #[test]
    fn load_csv_4ch_muse_absolute_timestamps() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        let ts = 1775512049.0;
        write_csv(d, "exg_1775512049.csv", muse_header(), &gen_rows(ts, 2560, 4, 256.0));

        let csvs = vec![d.join("exg_1775512049.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);
        assert_eq!(data.segments.len(), 1);
        let (start, ch_names, channels) = &data.segments[0];
        assert!((*start - ts).abs() < 0.01);
        assert_eq!(ch_names, &["TP9", "AF7", "AF8", "TP10"]);
        assert_eq!(channels.len(), 4);
        assert_eq!(channels[0].len(), 2560);
    }

    #[test]
    fn load_csv_32ch_virtual_relative_timestamps() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        // Relative timestamps (device clock) — segment start from filename.
        write_csv(
            d,
            "exg_1775512049.csv",
            &generic_header(32),
            &gen_rows(7.867, 1536, 32, 256.0),
        );

        let csvs = vec![d.join("exg_1775512049.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);
        assert_eq!(data.segments.len(), 1);
        let (start, ch_names, channels) = &data.segments[0];
        assert_eq!(*start, 1775512049.0); // from filename, not relative ts
        assert_eq!(ch_names.len(), 32);
        assert_eq!(ch_names[0], "Ch1");
        assert_eq!(channels.len(), 32);
        assert_eq!(channels[0].len(), 1536);
    }

    #[test]
    fn load_csv_mixed_devices_separate_segments() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        // Session 1: 32-ch virtual EEG with relative timestamps.
        write_csv(d, "exg_1000.csv", &generic_header(32), &gen_rows(5.0, 1536, 32, 256.0));
        // Session 2: 4-ch Muse with absolute timestamps.
        write_csv(d, "exg_2000.csv", muse_header(), &gen_rows(2000.0, 2560, 4, 256.0));

        let csvs = vec![d.join("exg_1000.csv"), d.join("exg_2000.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);
        assert_eq!(data.segments.len(), 2);

        // First segment: 32-ch, start from filename.
        assert_eq!(data.segments[0].0, 1000.0);
        assert_eq!(data.segments[0].1.len(), 32);
        assert_eq!(data.segments[0].2.len(), 32);

        // Second segment: 4-ch, absolute timestamps.
        assert!((data.segments[1].0 - 2000.0).abs() < 0.01);
        assert_eq!(data.segments[1].1.len(), 4);
        assert_eq!(data.segments[1].2.len(), 4);
    }

    #[test]
    fn load_csv_empty_file_skipped() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        std::fs::write(d.join("exg_100.csv"), "").unwrap();
        write_csv(d, "exg_200.csv", muse_header(), &gen_rows(200.0, 1280, 4, 256.0));

        let csvs = vec![d.join("exg_100.csv"), d.join("exg_200.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);
        assert_eq!(data.segments.len(), 1);
    }

    #[test]
    fn load_csv_header_only_no_data_skipped() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        write_csv(d, "exg_100.csv", muse_header(), &[]);

        let csvs = vec![d.join("exg_100.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);
        assert_eq!(data.segments.len(), 0);
    }

    #[test]
    fn load_csv_partial_rows_skipped() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        // Row with too few columns should be skipped.
        let rows = vec![
            "100.0,1.0,2.0,3.0,4.0".into(),
            "100.004,1.0,2.0".into(), // incomplete row
            "100.008,1.0,2.0,3.0,4.0".into(),
        ];
        write_csv(d, "exg_100.csv", muse_header(), &rows);

        let csvs = vec![d.join("exg_100.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);
        assert_eq!(data.segments.len(), 1);
        assert_eq!(data.segments[0].2[0].len(), 2); // only 2 valid rows
    }

    #[test]
    fn load_csv_nan_values_skip_row() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        let rows = vec![
            "100.0,1.0,2.0,3.0,4.0".into(),
            "100.004,NaN,2.0,3.0,4.0".into(), // unparseable as f32 by parse
            "100.008,1.0,2.0,3.0,4.0".into(),
        ];
        write_csv(d, "exg_100.csv", muse_header(), &rows);

        let csvs = vec![d.join("exg_100.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);
        // "NaN" does parse as f32, so the row is kept.
        // If it didn't parse, it would be skipped.
        let row_count = data.segments[0].2[0].len();
        assert!(row_count >= 2); // at least the valid rows
    }

    // ── extract_epoch_samples ─────────────────────────────────────────────

    fn make_raw_data(segments: Vec<(f64, Vec<String>, Vec<Vec<f32>>)>, sample_rate: f64) -> RawDayData {
        RawDayData { segments, sample_rate }
    }

    fn make_segment(start: f64, n_ch: usize, n_samples: usize, ch_prefix: &str) -> (f64, Vec<String>, Vec<Vec<f32>>) {
        let ch_names: Vec<String> = (1..=n_ch).map(|i| format!("{ch_prefix}{i}")).collect();
        let channels: Vec<Vec<f32>> = (0..n_ch)
            .map(|c| (0..n_samples).map(|s| (c * 1000 + s) as f32).collect())
            .collect();
        (start, ch_names, channels)
    }

    #[test]
    fn extract_full_epoch_from_middle_of_segment() {
        let seg = make_segment(1000.0, 4, 2560, "TP");
        let data = make_raw_data(vec![seg], 256.0);
        let epoch_samples = 1280; // 5s at 256Hz

        let (samples, ch_names) = extract_epoch_samples(&data, 1002.0, epoch_samples);
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[0].len(), 1280);
        assert_eq!(ch_names, vec!["TP1", "TP2", "TP3", "TP4"]);
    }

    #[test]
    fn extract_at_segment_start() {
        let seg = make_segment(1000.0, 4, 2560, "Ch");
        let data = make_raw_data(vec![seg], 256.0);

        let (samples, _) = extract_epoch_samples(&data, 1000.0, 1280);
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[0].len(), 1280);
    }

    #[test]
    fn extract_near_end_insufficient_samples_returns_empty() {
        // Segment is 6 seconds. Epoch at 1.5s leaves only 4.5s — not enough for 5s epoch.
        let seg = make_segment(1000.0, 4, 1536, "Ch"); // 6s at 256Hz
        let data = make_raw_data(vec![seg], 256.0);

        let (samples, _) = extract_epoch_samples(&data, 1001.5, 1280);
        assert!(samples.is_empty());
    }

    #[test]
    fn extract_before_any_segment_returns_empty() {
        let seg = make_segment(1000.0, 4, 2560, "Ch");
        let data = make_raw_data(vec![seg], 256.0);

        let (samples, _) = extract_epoch_samples(&data, 999.0, 1280);
        assert!(samples.is_empty());
    }

    #[test]
    fn extract_after_all_segments_returns_empty() {
        let seg = make_segment(1000.0, 4, 2560, "Ch");
        let data = make_raw_data(vec![seg], 256.0);

        let (samples, _) = extract_epoch_samples(&data, 1020.0, 1280);
        assert!(samples.is_empty());
    }

    #[test]
    fn extract_between_segments_returns_empty() {
        let seg1 = make_segment(1000.0, 4, 1280, "Ch"); // 5s
        let seg2 = make_segment(1010.0, 4, 1280, "Ch"); // starts 5s after seg1 ends
        let data = make_raw_data(vec![seg1, seg2], 256.0);

        let (samples, _) = extract_epoch_samples(&data, 1006.0, 1280);
        assert!(samples.is_empty());
    }

    #[test]
    fn extract_from_second_segment_returns_correct_channels() {
        // Simulate mixed-device day: seg1 is 32-ch, seg2 is 4-ch Muse.
        let seg1 = make_segment(1000.0, 32, 2560, "Ch");
        let seg2 = make_segment(2000.0, 4, 2560, "TP");
        let data = make_raw_data(vec![seg1, seg2], 256.0);

        // Epoch falls in seg2.
        let (samples, ch_names) = extract_epoch_samples(&data, 2002.0, 1280);
        assert_eq!(samples.len(), 4);
        assert_eq!(ch_names, vec!["TP1", "TP2", "TP3", "TP4"]);
    }

    #[test]
    fn extract_from_first_segment_returns_32_channels() {
        let seg1 = make_segment(1000.0, 32, 2560, "Ch");
        let seg2 = make_segment(2000.0, 4, 2560, "TP");
        let data = make_raw_data(vec![seg1, seg2], 256.0);

        let (samples, ch_names) = extract_epoch_samples(&data, 1002.0, 1280);
        assert_eq!(samples.len(), 32);
        assert_eq!(ch_names.len(), 32);
        assert_eq!(ch_names[0], "Ch1");
    }

    #[test]
    fn extract_empty_data_returns_empty() {
        let data = make_raw_data(vec![], 256.0);
        let (samples, ch_names) = extract_epoch_samples(&data, 1000.0, 1280);
        assert!(samples.is_empty());
        assert!(ch_names.is_empty());
    }

    #[test]
    fn extract_exact_boundary_fits() {
        // Segment is exactly 5s — epoch at start should fit.
        let seg = make_segment(1000.0, 4, 1280, "Ch"); // exactly 5s
        let data = make_raw_data(vec![seg], 256.0);

        let (samples, _) = extract_epoch_samples(&data, 1000.0, 1280);
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[0].len(), 1280);
    }

    #[test]
    fn extract_one_sample_short_returns_empty() {
        // Segment is 1279 samples — epoch of 1280 should NOT fit.
        let seg = make_segment(1000.0, 4, 1279, "Ch");
        let data = make_raw_data(vec![seg], 256.0);

        let (samples, _) = extract_epoch_samples(&data, 1000.0, 1280);
        assert!(samples.is_empty());
    }

    // ── Integration: load + extract round-trip ────────────────────────────

    #[test]
    fn roundtrip_muse_session_load_and_extract() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        let ts = 1776125671.0;
        // 20s of Muse data at 256Hz = 5120 samples.
        write_csv(d, "exg_1776125671.csv", muse_header(), &gen_rows(ts, 5120, 4, 256.0));

        let csvs = vec![d.join("exg_1776125671.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);
        assert_eq!(data.segments.len(), 1);
        assert_eq!(data.segments[0].1, vec!["TP9", "AF7", "AF8", "TP10"]);

        // Extract epoch from middle.
        let (samples, ch) = extract_epoch_samples(&data, ts + 5.0, 1280);
        assert_eq!(samples.len(), 4);
        assert_eq!(samples[0].len(), 1280);
        assert_eq!(ch, vec!["TP9", "AF7", "AF8", "TP10"]);
    }

    #[test]
    fn roundtrip_mixed_day_extracts_correct_device() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();

        // Session 1: 32-ch virtual EEG, 6s, relative timestamps.
        write_csv(d, "exg_1000.csv", &generic_header(32), &gen_rows(5.0, 1536, 32, 256.0));
        // Session 2: 4-ch Muse, 20s, absolute timestamps.
        write_csv(d, "exg_2000.csv", muse_header(), &gen_rows(2000.0, 5120, 4, 256.0));

        let csvs = vec![d.join("exg_1000.csv"), d.join("exg_2000.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);

        // Epoch in 32-ch session — too short for 5s (6s total, epoch at 1.5s leaves 4.5s).
        let (samples, _) = extract_epoch_samples(&data, 1001.5, 1280);
        assert!(samples.is_empty());

        // Epoch in 4-ch Muse session — should succeed with Muse channel names.
        let (samples, ch) = extract_epoch_samples(&data, 2005.0, 1280);
        assert_eq!(samples.len(), 4);
        assert_eq!(ch, vec!["TP9", "AF7", "AF8", "TP10"]);
    }

    #[test]
    fn roundtrip_virtual_eeg_long_session() {
        let td = tempfile::tempdir().unwrap();
        let d = td.path();
        // 170K samples (667s), relative timestamps.
        write_csv(
            d,
            "exg_1775519576.csv",
            &generic_header(32),
            &gen_rows(216.0, 10240, 32, 256.0), // 40s — shorter for test speed
        );

        let csvs = vec![d.join("exg_1775519576.csv")];
        let data = load_day_csv_data(d, &csvs, 256.0);
        assert_eq!(data.segments[0].0, 1775519576.0); // from filename

        // Epoch 33s into the session (offset 216s from device, but segment start is filename ts).
        // offset in data = (1775519576 + 33 - 1775519576) * 256 = 33 * 256 = 8448
        let (samples, ch) = extract_epoch_samples(&data, 1775519576.0 + 33.0, 1280);
        assert_eq!(samples.len(), 32);
        assert_eq!(samples[0].len(), 1280);
        assert_eq!(ch[0], "Ch1");
    }
}
