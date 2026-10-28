// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! `skill-bids` — export recordings as a BIDS-EEG dataset.
//!
//! Every recording that overlaps the requested time range is clipped to it
//! and written as one BIDS run:
//!
//! ```text
//! <out>/
//!   dataset_description.json  README  participants.tsv  participants.json
//!   sub-<subject>/ses-<YYYYMMDD>/
//!     sub-…_ses-…_scans.tsv
//!     eeg/    sub-…_task-…_run-N_eeg.{vhdr,vmrk,eeg,json}
//!             sub-…_task-…_run-N_channels.tsv
//!             sub-…_task-…_run-N_events.tsv        labels + signal gaps
//!             sub-…_task-…_run-N_physio.{tsv.gz,json}  PPG
//!     motion/ sub-…_task-…_tracksys-imu_run-N_{motion.tsv,motion.json,channels.tsv}
//! ```
//!
//! The session label is the local calendar day the clipped recording starts
//! on; runs are numbered per session in time order. Data files are written
//! by `bids-eeg`'s BrainVision/physio writers and the finished dataset is
//! checked with `bids-validate`.

pub mod source;

use bbtree::unsync::BBTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use bids_core::{Config, DatasetDescription, Entities, EntityValue};
use bids_eeg::writer::{
    iso8601_utc, write_channels_tsv, write_eeg_sidecar, write_events_tsv, BrainVisionWriter, PhysioWriter,
};
use bids_eeg::{Channel, ChannelType, EegEvent, EegMetadata};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub use source::Recording;

/// BIDS specification version the export targets.
pub const BIDS_VERSION: &str = "1.10.0";

/// Consecutive samples further apart than this are a gap in the signal
/// (Bluetooth drop, pause). Muse packets arrive every ~47 ms.
const GAP_SECS: f64 = 0.5;

/// PPG CSV columns that are derived metrics, not raw optical channels.
const PPG_DERIVED: &[&str] = &[
    "hr_bpm",
    "rmssd_ms",
    "sdnn_ms",
    "pnn50_pct",
    "lf_hf_ratio",
    "respiratory_rate_bpm",
    "spo2_pct",
    "perfusion_index_pct",
    "stress_index",
];

/// What to export. Every field is optional on the wire.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportOptions {
    /// Range start, Unix seconds. `None` = from the first recording.
    pub start_utc: Option<f64>,
    /// Range end (exclusive), Unix seconds. `None` = to the last sample.
    pub end_utc: Option<f64>,
    /// Restrict to these recordings (paths or file names of the `exg_*`
    /// files). Empty = every recording overlapping the range.
    pub recordings: Vec<String>,
    /// Output directory; must not exist or be empty. `None` =
    /// `<skill_dir>/exports/bids/<dataset-slug>-<timestamp>`.
    pub out_dir: Option<PathBuf>,
    pub dataset_name: String,
    /// BIDS `sub-` label (alphanumeric; other characters are dropped).
    pub subject: String,
    /// BIDS `task-` label (alphanumeric; other characters are dropped).
    pub task: String,
    /// Mains frequency for `PowerLineFrequency`; `None` writes `"n/a"`.
    pub power_line_hz: Option<f64>,
    /// Offset of local time from UTC, seconds — picks the `ses-` day.
    pub tz_offset_secs: i64,
    pub include_events: bool,
    pub include_ppg: bool,
    pub include_imu: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            start_utc: None,
            end_utc: None,
            recordings: Vec::new(),
            out_dir: None,
            dataset_name: "NeuroSkill recordings".into(),
            subject: "01".into(),
            task: "neuroskill".into(),
            power_line_hz: None,
            tz_offset_secs: 0,
            include_events: true,
            include_ppg: true,
            include_imu: true,
        }
    }
}

/// A user label, exported as an event and a BrainVision marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelEvent {
    pub start_utc: f64,
    pub end_utc: f64,
    pub text: String,
    #[serde(default)]
    pub context: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub done: usize,
    pub total: usize,
    pub current: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedRecording {
    pub source: String,
    /// Path of the `.vhdr`, relative to the dataset root.
    pub eeg_file: String,
    pub start_utc: f64,
    pub duration_s: f64,
    pub n_samples: u64,
    pub n_channels: usize,
    pub sampling_frequency: f64,
    pub n_labels: usize,
    pub n_gaps: usize,
    pub ppg: bool,
    pub imu: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedRecording {
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub out_dir: PathBuf,
    pub recordings: Vec<ExportedRecording>,
    pub skipped: Vec<SkippedRecording>,
    pub validation: Vec<ValidationIssue>,
    pub total_bytes: u64,
}

/// One row of [`preview`]: a recording that would be exported, with its
/// time span clipped to the range (end estimated when the sidecar has none).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    pub source: String,
    pub start_utc: f64,
    pub end_utc: Option<f64>,
    pub device_name: Option<String>,
    pub channel_names: Vec<String>,
    pub sample_rate: f64,
}

/// Recordings an export with these options would include.
pub fn preview(skill_dir: &Path, opts: &ExportOptions) -> Vec<PreviewItem> {
    source::discover(skill_dir, opts.start_utc, opts.end_utc, &opts.recordings)
        .into_iter()
        .map(|r| PreviewItem {
            source: r.eeg_path.to_string_lossy().into_owned(),
            start_utc: opts.start_utc.map_or(r.start_utc, |s| r.start_utc.max(s)),
            end_utc: match (r.end_utc, opts.end_utc) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
            device_name: r.device_name,
            channel_names: r.channel_names,
            sample_rate: r.sample_rate,
        })
        .collect()
}

/// Default output directory for an export started now.
pub fn default_out_dir(skill_dir: &Path, dataset_name: &str) -> PathBuf {
    let (stamp, _) = skill_data::util::yyyymmddhhmmss_utc_str();
    let slug: String = dataset_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() {
        "neuroskill".to_string()
    } else {
        slug
    };
    skill_dir.join("exports").join("bids").join(format!("{slug}-{stamp}"))
}

/// Export every recording matching `opts` into a new BIDS dataset.
///
/// `labels` are the user's labels for (at least) the range; each one that
/// overlaps an exported recording becomes an event. A recording that fails
/// is reported in `skipped` and the rest still export; an error is returned
/// only when nothing usable could be written (bad output dir, no matches).
pub fn export(
    skill_dir: &Path,
    opts: &ExportOptions,
    labels: &[LabelEvent],
    progress: &mut dyn FnMut(ExportProgress),
) -> Result<ExportSummary> {
    if let (Some(s), Some(e)) = (opts.start_utc, opts.end_utc) {
        if e <= s {
            bail!("range end must be after range start");
        }
    }
    let recordings = source::discover(skill_dir, opts.start_utc, opts.end_utc, &opts.recordings);
    if recordings.is_empty() {
        bail!("no recordings in the selected range");
    }

    let out_dir = opts
        .out_dir
        .clone()
        .unwrap_or_else(|| default_out_dir(skill_dir, &opts.dataset_name));
    prepare_out_dir(&out_dir)?;

    let subject = bids_label(&opts.subject, "01");
    let task = bids_label(&opts.task, "neuroskill");
    let ctx = Ctx {
        out_dir: &out_dir,
        opts,
        subject: &subject,
        task: &task,
        patterns: Config::bids().default_path_patterns.unwrap_or_default(),
    };

    let mut exported = Vec::new();
    let mut skipped = Vec::new();
    let mut runs: BBTreeMap<String, u32> = BBTreeMap::new();
    let mut scans: BBTreeMap<String, Vec<ScanEntry>> = BBTreeMap::new();
    let total = recordings.len();

    for (i, rec) in recordings.iter().enumerate() {
        let source = rec.eeg_path.to_string_lossy().into_owned();
        progress(ExportProgress {
            done: i,
            total,
            current: Some(source.clone()),
        });

        let clipped_start = opts.start_utc.map_or(rec.start_utc, |s| rec.start_utc.max(s));
        let session = local_day_label(clipped_start, opts.tz_offset_secs);
        let run = runs.get(&session).copied().unwrap_or(0) + 1;
        match export_recording(&ctx, rec, &session, run, labels) {
            Ok(Some((summary, files))) => {
                runs.insert(session.clone(), run);
                scans.entry(session).or_default().extend(files);
                exported.push(summary);
            }
            Ok(None) => skipped.push(SkippedRecording {
                source,
                reason: "no samples in the selected range".into(),
            }),
            Err(e) => skipped.push(SkippedRecording {
                source,
                reason: format!("{e:#}"),
            }),
        }
    }
    progress(ExportProgress {
        done: total,
        total,
        current: None,
    });

    if exported.is_empty() {
        let _ = std::fs::remove_dir_all(&out_dir);
        let why = skipped.first().map(|s| s.reason.clone()).unwrap_or_default();
        bail!("nothing exported: {why}");
    }

    for (session, files) in &scans {
        write_scans_tsv(&ctx, session, files)?;
    }
    write_dataset_files(&ctx, &exported)?;

    let validation = bids_validate::validate_dataset(&out_dir)
        .map(|r| {
            r.issues
                .into_iter()
                .map(|i| ValidationIssue {
                    severity: i.severity,
                    code: i.code,
                    message: i.message,
                    path: i.path,
                })
                .collect()
        })
        .unwrap_or_else(|e| {
            vec![ValidationIssue {
                severity: "error".into(),
                code: "VALIDATOR_FAILED".into(),
                message: e.to_string(),
                path: None,
            }]
        });

    Ok(ExportSummary {
        total_bytes: dir_size(&out_dir),
        out_dir,
        recordings: exported,
        skipped,
        validation,
    })
}

struct Ctx<'a> {
    out_dir: &'a Path,
    opts: &'a ExportOptions,
    subject: &'a str,
    task: &'a str,
    patterns: Vec<String>,
}

impl Ctx<'_> {
    /// Dataset-relative path for a set of BIDS entities, via bids-io's
    /// path builder and the standard BIDS path patterns.
    fn rel_path(&self, session: &str, run: u32, extra: &[(&str, &str)]) -> Result<String> {
        let mut ents = Entities::new();
        ents.insert("subject".into(), EntityValue::Str(self.subject.into()));
        ents.insert("session".into(), EntityValue::Str(session.into()));
        ents.insert("task".into(), EntityValue::Str(self.task.into()));
        ents.insert("run".into(), EntityValue::Str(run.to_string()));
        for (k, v) in extra {
            ents.insert((*k).into(), EntityValue::Str((*v).into()));
        }
        let patterns: Vec<&str> = self.patterns.iter().map(String::as_str).collect();
        bids_io::path_builder::build_path(&ents, &patterns, true)
            .with_context(|| format!("no BIDS path pattern for {extra:?}"))
    }

    fn abs(&self, rel: &str) -> PathBuf {
        self.out_dir.join(rel)
    }
}

/// Write one recording. `Ok(None)` when no samples fall in the range.
/// On success returns the summary and the data files for `scans.tsv`
/// (session-relative path, acquisition start).
fn export_recording(
    ctx: &Ctx<'_>,
    rec: &Recording,
    session: &str,
    run: u32,
    labels: &[LabelEvent],
) -> Result<Option<(ExportedRecording, Vec<ScanEntry>)>> {
    let opts = ctx.opts;
    let eeg = |suffix: &str, ext: &str| {
        ctx.rel_path(
            session,
            run,
            &[("datatype", "eeg"), ("suffix", suffix), ("extension", ext)],
        )
    };
    let vhdr_rel = eeg("eeg", ".vhdr")?;
    let vhdr = ctx.abs(&vhdr_rel);

    let channels: Vec<Channel> = rec.channel_names.iter().map(|n| eeg_channel(n)).collect();
    let sr = rec.sample_rate;
    let mut writer = BrainVisionWriter::create(&vhdr, &channels, sr)?;

    // Gaps are cut out of the continuous file, so wall time and file time
    // drift apart at each one; `gaps` maps between them.
    let mut first_wall: Option<f64> = None;
    let mut prev_wall = 0.0;
    let mut gaps: Vec<Gap> = Vec::new();
    let mut shift = 0.0;
    let read = source::read_eeg(rec, opts.start_utc, opts.end_utc, |wall, frame| {
        match first_wall {
            None => first_wall = Some(wall),
            Some(t0) if wall - prev_wall > GAP_SECS => {
                // Missing time = wall clock minus file time written so far.
                // Packet timestamps are shared by several samples, so the
                // distance to the previous timestamp would overstate it.
                let file_time = writer.n_samples() as f64 / sr;
                let duration = (wall - t0 - file_time - shift).max(0.0);
                gaps.push(Gap {
                    file_onset: file_time,
                    wall_end: wall,
                    duration,
                    shift_after: shift + duration,
                });
                shift += duration;
            }
            Some(_) => {}
        }
        prev_wall = wall;
        writer.write_frame(frame).map_err(anyhow::Error::from)
    });
    let n_samples = writer.n_samples();
    let Some(t0) = first_wall.filter(|_| n_samples > 0) else {
        drop(writer);
        for ext in ["vhdr", "vmrk", "eeg"] {
            let _ = std::fs::remove_file(vhdr.with_extension(ext));
        }
        read?;
        return Ok(None);
    };
    read?;
    writer.set_recording_start(t0);
    let duration = n_samples as f64 / sr;
    // Wall time just past the last sample, through the same mapping.
    let last_wall = t0 + duration + shift;
    let to_file_time = |wall: f64| {
        let shift = gaps
            .iter()
            .rev()
            .find(|g| wall >= g.wall_end)
            .map_or(0.0, |g| g.shift_after);
        (wall - t0 - shift).clamp(0.0, duration)
    };

    let mut events: Vec<EegEvent> = Vec::new();
    let mut n_labels = 0;
    if opts.include_events {
        for l in labels.iter().filter(|l| l.end_utc > t0 && l.start_utc < last_wall) {
            let onset = to_file_time(l.start_utc.max(t0));
            let end = to_file_time(l.end_utc.min(last_wall));
            let mut extra = std::collections::HashMap::new();
            if !l.context.trim().is_empty() {
                extra.insert("context".to_string(), l.context.clone());
            }
            writer.add_marker(onset, end - onset, &l.text);
            events.push(event(onset, end - onset, "label", Some(&l.text), sr, extra));
            n_labels += 1;
        }
        for g in &gaps {
            events.push(event(
                g.file_onset,
                0.0,
                "gap",
                Some(&format!("{:.3}", g.duration)),
                sr,
                Default::default(),
            ));
        }
        events.sort_by(|a, b| a.onset.total_cmp(&b.onset));
    }
    writer.finish()?;

    let eeg_count = channels.iter().filter(|c| c.channel_type == ChannelType::EEG).count();
    let metadata: EegMetadata = serde_json::from_value(json!({
        "TaskName": ctx.task,
        "SamplingFrequency": sr,
        "EEGChannelCount": eeg_count,
        "MiscChannelCount": channels.len() - eeg_count,
        "EEGReference": eeg_reference(rec),
        "RecordingDuration": duration,
        "RecordingType": if gaps.is_empty() { "continuous" } else { "discontinuous" },
        "Manufacturer": rec.device_kind,
        "ManufacturersModelName": rec.device_name,
    }))?;
    let mut extra = serde_json::Map::new();
    extra.insert(
        "PowerLineFrequency".into(),
        opts.power_line_hz.map_or(json!("n/a"), |hz| json!(hz)),
    );
    extra.insert("SoftwareFilters".into(), json!("n/a"));
    if let Some(sn) = &rec.serial_number {
        extra.insert("DeviceSerialNumber".into(), json!(sn));
    }
    if let Some(fw) = &rec.firmware_version {
        extra.insert("SoftwareVersions".into(), json!(fw));
    }
    write_eeg_sidecar(&ctx.abs(&eeg("eeg", ".json")?), &metadata, Some(&extra))?;
    write_channels_tsv(&ctx.abs(&eeg("channels", ".tsv")?), &channels)?;
    if !events.is_empty() {
        write_events_tsv(&ctx.abs(&eeg("events", ".tsv")?), &events)?;
    }

    let session_dir = format!("sub-{}/ses-{session}/", ctx.subject);
    let rel_to_session = |rel: &str| rel.strip_prefix(&session_dir).unwrap_or(rel).to_string();
    let mut scan_files = vec![(rel_to_session(&vhdr_rel), t0)];

    // Sibling streams exist only next to CSV recordings.
    let csv_sibling = !rec.is_parquet();
    let mut ppg = false;
    if opts.include_ppg && csv_sibling {
        let rel = eeg("physio", ".tsv.gz")?;
        let path = skill_data::session_csv::ppg_csv_path(&rec.eeg_path);
        if let Some(start) = write_physio(&ctx.abs(&rel), &path, rec, t0, last_wall)? {
            ppg = true;
            scan_files.push((rel_to_session(&rel), start));
        }
    }
    let mut imu = false;
    if opts.include_imu && csv_sibling {
        let motion = |suffix: &str, ext: &str| {
            ctx.rel_path(
                session,
                run,
                &[
                    ("datatype", "motion"),
                    ("tracksys", "imu"),
                    ("suffix", suffix),
                    ("extension", ext),
                ],
            )
        };
        let rel = motion("motion", ".tsv")?;
        let paths = MotionPaths {
            tsv: ctx.abs(&rel),
            json: ctx.abs(&motion("motion", ".json")?),
            channels: ctx.abs(&motion("channels", ".tsv")?),
        };
        let path = skill_data::session_csv::imu_csv_path(&rec.eeg_path);
        if let Some(start) = write_motion(&paths, &path, rec, ctx.task, t0, last_wall)? {
            imu = true;
            scan_files.push((rel_to_session(&rel), start));
        }
    }

    Ok(Some((
        ExportedRecording {
            source: rec.eeg_path.to_string_lossy().into_owned(),
            eeg_file: vhdr_rel,
            start_utc: t0,
            duration_s: duration,
            n_samples,
            n_channels: channels.len(),
            sampling_frequency: sr,
            n_labels,
            n_gaps: gaps.len(),
            ppg,
            imu,
        },
        scan_files,
    )))
}

/// A `scans.tsv` row: data file relative to the session dir, acquisition start.
type ScanEntry = (String, f64);

struct Gap {
    /// Seconds into the file where the signal resumes.
    file_onset: f64,
    /// Wall time where the signal resumes.
    wall_end: f64,
    duration: f64,
    /// Total wall time cut out up to and including this gap.
    shift_after: f64,
}

fn event(
    onset: f64,
    duration: f64,
    trial_type: &str,
    value: Option<&str>,
    sr: f64,
    extra: std::collections::HashMap<String, String>,
) -> EegEvent {
    EegEvent {
        onset: round6(onset),
        duration: round6(duration),
        trial_type: Some(trial_type.into()),
        value: value.map(String::from),
        sample: Some((onset * sr).round() as i64),
        response_time: None,
        extra,
    }
}

fn round6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

fn eeg_channel(name: &str) -> Channel {
    let lower = name.to_ascii_lowercase();
    let misc = lower.starts_with("aux") || lower.contains("drl") || lower.contains("ref");
    Channel {
        name: name.into(),
        channel_type: if misc { ChannelType::MISC } else { ChannelType::EEG },
        units: "µV".into(),
        description: None,
        sampling_frequency: None,
        low_cutoff: None,
        high_cutoff: None,
        notch: None,
        reference: None,
        status: Some("good".into()),
        status_description: None,
    }
}

/// Recording reference where the hardware fixes it; `n/a` otherwise.
fn eeg_reference(rec: &Recording) -> &'static str {
    let kind = rec.device_kind.as_deref().unwrap_or_default().to_ascii_lowercase();
    let name = rec.device_name.as_deref().unwrap_or_default().to_ascii_lowercase();
    if kind == "muse" || name.starts_with("muse") {
        "Fpz"
    } else {
        "n/a"
    }
}

/// PPG → `_physio.tsv.gz`. Returns the first sample's wall time, or `None`
/// when there is no PPG data in the window.
fn write_physio(out: &Path, ppg_csv: &Path, rec: &Recording, t0: f64, t_end: f64) -> Result<Option<f64>> {
    let mut writer: Option<PhysioWriter> = None;
    let stats = source::read_aux_csv(
        ppg_csv,
        rec.start_utc,
        t0,
        t_end,
        |c| !PPG_DERIVED.contains(&c),
        |cols, _, values| {
            if writer.is_none() {
                let names: Vec<&str> = cols.iter().map(String::as_str).collect();
                writer = Some(PhysioWriter::create(
                    out,
                    &names,
                    skill_data::session_csv::PPG_SAMPLE_RATE,
                )?);
            }
            if let Some(w) = writer.as_mut() {
                w.write_row(values)?;
            }
            Ok(())
        },
    )?;
    let (Some(writer), Some(stats)) = (writer, stats) else {
        return Ok(None);
    };
    let mut extra = serde_json::Map::new();
    let rate = stats
        .estimated_rate()
        .unwrap_or(skill_data::session_csv::PPG_SAMPLE_RATE);
    extra.insert("SamplingFrequency".into(), json!(rate));
    extra.insert("Manufacturer".into(), json!(rec.device_kind));
    let start_time = round6(stats.first_ts - t0);
    writer.finish(start_time, Some(&extra))?;
    Ok(Some(stats.first_ts))
}

struct MotionPaths {
    tsv: PathBuf,
    json: PathBuf,
    channels: PathBuf,
}

/// IMU → BIDS-Motion (`tracksys-imu`). Returns the first sample's wall
/// time, or `None` when there is no IMU data in the window.
fn write_motion(
    paths: &MotionPaths,
    imu_csv: &Path,
    rec: &Recording,
    task: &str,
    t0: f64,
    t_end: f64,
) -> Result<Option<f64>> {
    let mut out: Option<std::io::BufWriter<std::fs::File>> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut line = String::new();
    let stats = source::read_aux_csv(
        imu_csv,
        rec.start_utc,
        t0,
        t_end,
        |_| true,
        |cols, _, values| {
            if out.is_none() {
                if let Some(parent) = paths.tsv.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                out = Some(std::io::BufWriter::new(std::fs::File::create(&paths.tsv)?));
                columns = cols.to_vec();
            }
            line.clear();
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    line.push('\t');
                }
                match v {
                    Some(x) if x.is_finite() => line.push_str(&x.to_string()),
                    _ => line.push_str("n/a"),
                }
            }
            line.push('\n');
            if let Some(w) = out.as_mut() {
                w.write_all(line.as_bytes())?;
            }
            Ok(())
        },
    )?;
    let (Some(mut w), Some(stats)) = (out, stats) else {
        return Ok(None);
    };
    w.flush()?;

    let rows: Vec<Vec<String>> = columns
        .iter()
        .map(|c| {
            let (kind, units) = match c.split('_').next().unwrap_or_default() {
                "accel" => ("ACCEL", "g"),
                "gyro" => ("GYRO", "deg/s"),
                "mag" => ("MAGN", "n/a"),
                _ => ("MISC", "n/a"),
            };
            let component = c
                .rsplit('_')
                .next()
                .filter(|s| matches!(*s, "x" | "y" | "z"))
                .unwrap_or("n/a");
            vec![c.clone(), component.into(), kind.into(), "head".into(), units.into()]
        })
        .collect();
    bids_io::tsv::write_tsv(
        &paths.channels,
        &["name", "component", "type", "tracked_point", "units"],
        &rows,
    )?;

    let count = |kind: &str| rows.iter().filter(|r| r[2] == kind).count();
    let rate = stats.estimated_rate().unwrap_or(0.0);
    let sidecar = json!({
        "TaskName": task,
        "SamplingFrequency": if rate > 0.0 { json!(rate) } else { json!("n/a") },
        "TrackingSystemName": "IMU",
        "Manufacturer": rec.device_kind,
        "ManufacturersModelName": rec.device_name,
        "MotionChannelCount": rows.len(),
        "ACCELChannelCount": count("ACCEL"),
        "GYROChannelCount": count("GYRO"),
        "MAGNChannelCount": count("MAGN"),
        "RecordingDuration": round6(stats.last_ts - stats.first_ts),
    });
    std::fs::write(&paths.json, serde_json::to_string_pretty(&sidecar)? + "\n")?;
    Ok(Some(stats.first_ts))
}

fn write_scans_tsv(ctx: &Ctx<'_>, session: &str, files: &[ScanEntry]) -> Result<()> {
    let path = ctx
        .out_dir
        .join(format!("sub-{}", ctx.subject))
        .join(format!("ses-{session}"))
        .join(format!("sub-{}_ses-{session}_scans.tsv", ctx.subject));
    let rows: Vec<Vec<String>> = files
        .iter()
        .map(|(f, t)| vec![f.clone(), format!("{}Z", iso8601_utc(*t))])
        .collect();
    bids_io::tsv::write_tsv(&path, &["filename", "acq_time"], &rows)?;
    Ok(())
}

fn write_dataset_files(ctx: &Ctx<'_>, exported: &[ExportedRecording]) -> Result<()> {
    let opts = ctx.opts;
    let description: DatasetDescription = serde_json::from_value(json!({
        "Name": if opts.dataset_name.trim().is_empty() { "NeuroSkill recordings" } else { opts.dataset_name.trim() },
        "BIDSVersion": BIDS_VERSION,
        "DatasetType": "raw",
        "GeneratedBy": [{
            "Name": "NeuroSkill",
            "Version": env!("CARGO_PKG_VERSION"),
            "CodeURL": "https://neuroskill.com",
        }],
    }))?;
    description.validate()?;
    description.save_to(ctx.out_dir)?;

    bids_io::tsv::write_tsv(
        &ctx.out_dir.join("participants.tsv"),
        &["participant_id"],
        &[vec![format!("sub-{}", ctx.subject)]],
    )?;
    std::fs::write(
        ctx.out_dir.join("participants.json"),
        serde_json::to_string_pretty(&json!({
            "participant_id": { "Description": "Unique participant identifier" }
        }))? + "\n",
    )?;

    let first = exported.iter().map(|r| r.start_utc).fold(f64::INFINITY, f64::min);
    let last = exported
        .iter()
        .map(|r| r.start_utc + r.duration_s)
        .fold(f64::NEG_INFINITY, f64::max);
    let readme = format!(
        "{name}\n\
         {underline}\n\n\
         EEG recordings exported from NeuroSkill as a BIDS {BIDS_VERSION} dataset.\n\n\
         - Recordings: {n}\n\
         - Time span (UTC): {from}Z to {to}Z\n\
         - EEG: BrainVision (IEEE float32, microvolts)\n\
         - Events: user labels (trial_type \"label\") and signal gaps (trial_type \"gap\",\n\
           value = seconds of signal missing; the data file is continuous across them)\n\
         - PPG: `_physio.tsv.gz` next to the EEG; IMU: `motion/` (tracksys-imu)\n\n\
         Each recording session becomes a run; the session label is the local day it\n\
         starts on. Clock times come from the device stream and may drift slightly\n\
         from the nominal sampling rate.\n",
        name = description.name,
        underline = "=".repeat(description.name.chars().count().max(3)),
        n = exported.len(),
        from = iso8601_utc(first),
        to = iso8601_utc(last),
    );
    std::fs::write(ctx.out_dir.join("README"), readme)?;
    Ok(())
}

fn prepare_out_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        let mut entries = std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))?;
        if entries.next().is_some() {
            bail!("output folder is not empty: {}", dir.display());
        }
    }
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))
}

/// BIDS labels are alphanumeric.
fn bids_label(raw: &str, fallback: &str) -> String {
    let s: String = raw.chars().filter(char::is_ascii_alphanumeric).collect();
    if s.is_empty() {
        fallback.into()
    } else {
        s
    }
}

/// `YYYYMMDD` of the local day containing `utc`.
fn local_day_label(utc: f64, tz_offset_secs: i64) -> String {
    iso8601_utc(utc + tz_offset_secs as f64)[..10].replace('-', "")
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            match e.metadata() {
                Ok(m) if m.is_dir() => stack.push(e.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

#[cfg(test)]
mod tests;
