// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

use super::*;
use std::fmt::Write as _;

/// 2026-10-26T09:59:20Z
const T0: f64 = 1_793_008_760.0;
const SR: f64 = 256.0;

/// A 60 s Muse-style recording: 12-sample packets sharing one timestamp,
/// a 5 s Bluetooth drop at t=30 s, PPG at 64 Hz and IMU at 52 Hz.
fn fake_skill_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let day = dir.path().join("20261026");
    std::fs::create_dir_all(&day).unwrap();
    let eeg = day.join(format!("exg_{}.csv", T0 as u64));

    let mut csv = String::from("timestamp_s,TP9,AF7,AF8,TP10\n");
    let mut i = 0u64;
    let mut t = T0;
    while t < T0 + 60.0 {
        if (30.0..35.0).contains(&(t - T0)) {
            t += 5.0;
            continue;
        }
        let packet_ts = t;
        for _ in 0..12 {
            let x = i as f64;
            writeln!(csv, "{packet_ts:.6},{x:.4},{:.4},{:.4},{:.4}", -x, x * 0.5, 1000.0).unwrap();
            i += 1;
            t += 1.0 / SR;
        }
    }
    std::fs::write(&eeg, csv).unwrap();

    let sidecar = json!({
        "channel_names": ["TP9", "AF7", "AF8", "TP10"],
        "csv_file": eeg.file_name().unwrap().to_str(),
        "device_kind": "muse",
        "device_name": "MuseS-F921",
        "sample_rate_hz": SR,
        "session_start_utc": T0 as u64,
        "session_end_utc": (T0 + 60.0) as u64,
    });
    std::fs::write(eeg.with_extension("json"), sidecar.to_string()).unwrap();

    let mut ppg = String::from("timestamp_s,ambient,infrared,red,hr_bpm\n");
    for k in 0..(60 * 64) {
        writeln!(
            ppg,
            "{:.6},{},{},{},",
            T0 + k as f64 / 64.0,
            150_000 + k,
            140_000 + k,
            300 + k % 7
        )
        .unwrap();
    }
    std::fs::write(skill_data::session_csv::ppg_csv_path(&eeg), ppg).unwrap();

    let mut imu = String::from("timestamp_s,accel_x,accel_y,accel_z,gyro_x,gyro_y,gyro_z\n");
    for k in 0..(60 * 52) {
        writeln!(imu, "{:.6},-0.99,0.01,-0.21,0,0,{}", T0 + k as f64 / 52.0, k % 3).unwrap();
    }
    std::fs::write(skill_data::session_csv::imu_csv_path(&eeg), imu).unwrap();
    dir
}

fn opts(out: &Path) -> ExportOptions {
    ExportOptions {
        start_utc: Some(T0 + 10.0),
        end_utc: Some(T0 + 50.0),
        out_dir: Some(out.to_path_buf()),
        subject: "test-01".into(),
        task: "rest".into(),
        power_line_hz: Some(60.0),
        tz_offset_secs: -4 * 3600,
        ..ExportOptions::default()
    }
}

#[test]
fn exports_clipped_range_as_valid_bids() {
    let skill = fake_skill_dir();
    let out = skill.path().join("export");
    let labels = vec![
        LabelEvent {
            start_utc: T0 + 12.0,
            end_utc: T0 + 14.0,
            text: "eyes closed, relaxed".into(),
            context: String::new(),
        },
        // After the gap: file time is wall time minus the 5 s cut.
        LabelEvent {
            start_utc: T0 + 40.0,
            end_utc: T0 + 41.0,
            text: "focus".into(),
            context: "reading".into(),
        },
        LabelEvent {
            start_utc: T0 + 55.0,
            end_utc: T0 + 56.0,
            text: "outside range".into(),
            context: String::new(),
        },
    ];
    let mut seen = Vec::new();
    let summary = export(skill.path(), &opts(&out), &labels, &mut |p| seen.push(p.done)).unwrap();

    assert_eq!(seen.first(), Some(&0));
    assert_eq!(seen.last(), Some(&1));
    assert!(summary.skipped.is_empty(), "{:?}", summary.skipped);
    let errors: Vec<_> = summary.validation.iter().filter(|i| i.severity == "error").collect();
    assert!(errors.is_empty(), "{errors:?}");
    let warnings: Vec<_> = summary.validation.iter().filter(|i| i.severity == "warning").collect();
    assert!(warnings.is_empty(), "{warnings:?}");

    let rec = &summary.recordings[0];
    assert_eq!(
        rec.eeg_file,
        "sub-test01/ses-20261026/eeg/sub-test01_ses-20261026_task-rest_run-1_eeg.vhdr"
    );
    // [10 s, 50 s) minus the 5 s drop, at packet granularity.
    assert!((rec.duration_s - 35.0).abs() < 0.1, "{}", rec.duration_s);
    assert_eq!(rec.n_gaps, 1);
    assert_eq!(rec.n_labels, 2);
    assert!(rec.ppg && rec.imu);
    assert!((rec.start_utc - (T0 + 10.0)).abs() < 0.05);

    // Read it back the way a downstream user would: index with bids-rs
    // (DuckDB-backed layout) and load through the EEG layer.
    let layout = bids_layout::BidsLayout::new(&out).unwrap();
    let eeg = bids_eeg::EegLayout::new(&layout);
    let files = eeg.get_eeg_files().unwrap();
    let vhdr = files.iter().find(|f| f.filename.ends_with(".vhdr")).unwrap();
    let data = eeg.read_data(vhdr, &bids_eeg::ReadOptions::default()).unwrap();
    assert_eq!(data.channel_labels, ["TP9", "AF7", "AF8", "TP10"]);
    assert_eq!(data.data[0].len() as u64, rec.n_samples);
    assert_eq!(data.data[3][0], 1000.0);
    assert_eq!(data.data[1][5], -data.data[0][5]);

    let md = eeg.get_eeg_metadata(vhdr).unwrap().unwrap();
    assert_eq!(md.sampling_frequency, SR);
    assert_eq!(md.power_line_frequency, Some(60.0));
    assert_eq!(md.eeg_reference.as_deref(), Some("Fpz"));
    assert_eq!(md.recording_type.as_deref(), Some("discontinuous"));

    let events = eeg.get_events(vhdr).unwrap().unwrap();
    let kinds: Vec<_> = events
        .iter()
        .map(|e| e.trial_type.clone().unwrap_or_default())
        .collect();
    assert_eq!(kinds, ["label", "gap", "label"]);
    assert!((events[0].onset - 2.0).abs() < 0.05, "{}", events[0].onset);
    assert!((events[0].duration - 2.0).abs() < 0.05);
    assert_eq!(events[0].value.as_deref(), Some("eyes closed, relaxed"));
    assert!((events[1].onset - 20.0).abs() < 0.05, "gap at {}", events[1].onset);
    assert!(
        (events[2].onset - 25.0).abs() < 0.05,
        "post-gap label at {}",
        events[2].onset
    );
    assert_eq!(events[2].extra.get("context").map(String::as_str), Some("reading"));

    let vmrk = out.join(&rec.eeg_file).with_extension("vmrk");
    let markers = bids_eeg::read_brainvision_markers(&vmrk, SR).unwrap();
    let comments: Vec<_> = markers
        .iter()
        .filter(|m| !m.description.starts_with("New Segment"))
        .collect();
    assert_eq!(comments.len(), 2, "{markers:?}");
    assert!((comments[0].onset - 2.0).abs() < 0.05);

    let physio = eeg.get_physio(vhdr).unwrap().unwrap();
    assert_eq!(physio.columns, ["ambient", "infrared", "red"]);
    assert_eq!(physio.sampling_frequency, 64.0);
    // Aligned to the EEG window, which snaps to ~47 ms packet boundaries.
    assert!((physio.rows.len() as i64 - 40 * 64).abs() <= 4, "{}", physio.rows.len());
    assert!(physio.start_time.abs() < 0.05);

    let ses = out.join("sub-test01/ses-20261026");
    let motion =
        std::fs::read_to_string(ses.join("motion/sub-test01_ses-20261026_task-rest_tracksys-imu_run-1_motion.json"))
            .unwrap();
    let motion: serde_json::Value = serde_json::from_str(&motion).unwrap();
    assert_eq!(motion["SamplingFrequency"], 52.0);
    assert_eq!(motion["ACCELChannelCount"], 3);
    let scans = std::fs::read_to_string(ses.join("sub-test01_ses-20261026_scans.tsv")).unwrap();
    assert_eq!(scans.lines().count(), 4, "{scans}");
    assert!(scans.contains("eeg/sub-test01_ses-20261026_task-rest_run-1_eeg.vhdr\t2026-10-26T09:59:30"));

    let desc = DatasetDescription::from_dir(&out).unwrap();
    assert_eq!(desc.bids_version, BIDS_VERSION);
    assert!(summary.total_bytes > 0);
}

/// A recording that was given a montage is exported under it: labels, what each
/// channel carries, which are bad, the reference.
#[test]
fn a_recordings_montage_is_what_is_exported() {
    use skill_data::montage::{set_session_montage, Montage, MontageOrigin, MontageSpec};
    let skill = fake_skill_dir();
    let eeg = source::discover(skill.path(), None, None, &[]).remove(0).eeg_path;
    let spec: MontageSpec =
        serde_json::from_str(r#"{"channels":[{"source":"TP9","label":"M1"},{"source":"AF8","label":"VEOG","kind":"eog"},{"source":"4","label":"Ext","kind":"misc","bad":true}],"reference":"Fpz","ground":"AFz"}"#).unwrap();
    set_session_montage(
        &eeg,
        Montage::from_spec(spec, &[]).unwrap(),
        MontageOrigin::Session,
        None,
    )
    .unwrap();

    let out = skill.path().join("montage");
    let o = ExportOptions {
        out_dir: Some(out.clone()),
        include_ppg: false,
        include_imu: false,
        ..ExportOptions::default()
    };
    assert_eq!(preview(skill.path(), &o)[0].channel_names, ["M1", "AF7", "VEOG", "Ext"]);
    export(skill.path(), &o, &[], &mut |_| {}).unwrap();
    let find = |suffix: &str| {
        walk(&out)
            .into_iter()
            .find(|p| p.to_string_lossy().ends_with(suffix))
            .unwrap_or_else(|| panic!("no {suffix}"))
    };
    let channels = std::fs::read_to_string(find("_channels.tsv")).unwrap();
    let row = |name: &str| {
        channels
            .lines()
            .find(|l| l.starts_with(&format!("{name}\t")))
            .unwrap_or_else(|| panic!("{name}: {channels}"))
            .to_string()
    };
    assert!(row("M1").contains("EEG") && row("AF7").contains("EEG"));
    assert!(row("VEOG").contains("EOG"));
    assert!(row("Ext").contains("MISC") && row("Ext").contains("bad"));
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(find("_eeg.json")).unwrap()).unwrap();
    assert_eq!(
        (&meta["EEGReference"], &meta["EEGGround"]),
        (&json!("Fpz"), &json!("AFz"))
    );
    assert_eq!(
        (
            &meta["EEGChannelCount"],
            &meta["EOGChannelCount"],
            &meta["MiscChannelCount"]
        ),
        (&json!(2), &json!(1), &json!(1))
    );
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            out.extend(walk(&e.path()));
        } else {
            out.push(e.path());
        }
    }
    out
}

#[test]
fn whole_recording_without_range() {
    let skill = fake_skill_dir();
    let out = skill.path().join("all");
    let o = ExportOptions {
        out_dir: Some(out.clone()),
        include_ppg: false,
        include_imu: false,
        ..ExportOptions::default()
    };
    let summary = export(skill.path(), &o, &[], &mut |_| {}).unwrap();
    let rec = &summary.recordings[0];
    assert!((rec.duration_s - 55.0).abs() < 0.1, "{}", rec.duration_s);
    assert!(!rec.ppg && !rec.imu);
    assert!(!out.join("sub-01/ses-20261026/motion").exists());
    assert_eq!(preview(skill.path(), &o).len(), 1);
}

#[test]
fn empty_range_and_bad_inputs_fail_cleanly() {
    let skill = fake_skill_dir();
    let out = skill.path().join("none");
    let mut o = opts(&out);
    o.start_utc = Some(T0 + 1000.0);
    o.end_utc = Some(T0 + 2000.0);
    assert!(export(skill.path(), &o, &[], &mut |_| {}).is_err());
    assert!(!out.exists(), "nothing written for an empty range");

    o.start_utc = Some(T0 + 50.0);
    o.end_utc = Some(T0 + 10.0);
    assert!(export(skill.path(), &o, &[], &mut |_| {}).is_err());

    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("keep.txt"), "x").unwrap();
    let o = opts(&out);
    let err = export(skill.path(), &o, &[], &mut |_| {}).unwrap_err();
    assert!(err.to_string().contains("not empty"), "{err}");
    assert!(out.join("keep.txt").exists());
}

#[test]
fn selecting_recordings_by_name() {
    let skill = fake_skill_dir();
    let o = ExportOptions {
        recordings: vec!["exg_1.csv".into()],
        ..opts(&skill.path().join("x"))
    };
    assert!(preview(skill.path(), &o).is_empty());
    let o = ExportOptions {
        recordings: vec![format!("/elsewhere/exg_{}.parquet", T0 as u64)],
        ..o
    };
    assert_eq!(preview(skill.path(), &o).len(), 1);
}

#[test]
fn labels_and_days() {
    assert_eq!(bids_label("test-01 ", "x"), "test01");
    assert_eq!(bids_label("—", "01"), "01");
    // 2026-10-26T02:00Z is still the 25th in UTC−4.
    assert_eq!(local_day_label(1_792_980_000.0, -4 * 3600), "20261025");
    assert_eq!(local_day_label(1_792_980_000.0, 0), "20261026");
}

/// Markers recorded beside the signal become events at their exact times, and
/// the labels the daemon derived from them are not exported a second time.
#[test]
fn recorded_markers_become_events() {
    let skill = fake_skill_dir();
    let out = skill.path().join("export");
    let eeg_path = skill.path().join("20261026").join(format!("exg_{}.csv", T0 as u64));
    use skill_data::session_csv::{append_marker, MARKER_LABEL_CONTEXT};
    append_marker(&eeg_path, T0 + 12.25, "TaskEvents", "stimulus, left").unwrap();
    append_marker(&eeg_path, T0 + 13.5, "TriggerCodes", "42").unwrap();
    append_marker(&eeg_path, T0 + 5.0, "TaskEvents", "before the range").unwrap();
    let labels = vec![
        // what the daemon stores for the first marker, to the second
        LabelEvent {
            start_utc: T0 + 12.0,
            end_utc: T0 + 12.0,
            text: "stimulus, left".into(),
            context: format!("{MARKER_LABEL_CONTEXT}TaskEvents"),
        },
        // a label a person wrote stays
        LabelEvent {
            start_utc: T0 + 20.0,
            end_utc: T0 + 22.0,
            text: "felt drowsy".into(),
            context: String::new(),
        },
    ];
    let summary = export(skill.path(), &opts(&out), &labels, &mut |_| {}).unwrap();
    let rec = &summary.recordings[0];
    let events_tsv = std::fs::read_to_string(
        out.join(&rec.eeg_file).with_file_name(
            Path::new(&rec.eeg_file)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .replace("_eeg.vhdr", "_events.tsv"),
        ),
    )
    .unwrap();
    let rows: Vec<Vec<&str>> = events_tsv.lines().map(|l| l.split('\t').collect()).collect();
    let col = |name: &str| {
        rows[0]
            .iter()
            .position(|c| *c == name)
            .unwrap_or_else(|| panic!("{name} in {:?}", rows[0]))
    };
    let (onset, kind, value) = (col("onset"), col("trial_type"), col("value"));
    let got: Vec<(f64, &str, &str)> = rows[1..]
        .iter()
        .filter(|r| r[kind] != "gap")
        .map(|r| (r[onset].parse().unwrap(), r[kind], r[value]))
        .collect();
    // the export starts at T0 + 10
    assert_eq!(got.len(), 3, "{events_tsv}");
    assert_eq!((got[0].1, got[0].2), ("marker", "stimulus, left"));
    assert!((got[0].0 - 2.25).abs() < 0.05, "{events_tsv}");
    assert_eq!((got[1].1, got[1].2), ("marker", "42"));
    assert!((got[1].0 - 3.5).abs() < 0.05, "{events_tsv}");
    // 1.25 s apart, to the sample
    assert!((got[1].0 - got[0].0 - 1.25).abs() < 1e-6, "{events_tsv}");
    assert_eq!((got[2].1, got[2].2), ("label", "felt drowsy"));
    assert!(
        events_tsv.contains("TaskEvents") && rows[0].contains(&"source"),
        "{events_tsv}"
    );
}

/// A recording that carries measurements of its computer's clock: the export
/// says what the clock was off by, and gives acquisition times on the time
/// server's clock — unless asked to leave them.
#[test]
fn clock_checks_correct_acquisition_times() {
    use skill_data::wall_clock::{append_check, ClockCheck};
    let skill = fake_skill_dir();
    let eeg = skill.path().join("20261026").join(format!("exg_{}.csv", T0 as u64));
    // the recording computer was 2.5 s behind the time server
    for k in 0..3 {
        append_check(
            &eeg,
            &ClockCheck {
                unix_s: T0 + f64::from(k) * 20.0,
                offset_s: 2.5,
                round_trip_s: 0.010,
                server: "time.example".into(),
            },
        )
        .unwrap();
    }
    let ses = |out: &Path| out.join("sub-test01/ses-20261026");
    let scans = |out: &Path| std::fs::read_to_string(ses(out).join("sub-test01_ses-20261026_scans.tsv")).unwrap();
    let sidecar = |out: &Path| -> serde_json::Value {
        serde_json::from_str(
            &std::fs::read_to_string(ses(out).join("eeg/sub-test01_ses-20261026_task-rest_run-1_eeg.json")).unwrap(),
        )
        .unwrap()
    };

    let out = skill.path().join("corrected");
    let summary = export(skill.path(), &opts(&out), &[], &mut |_| {}).unwrap();
    let errors: Vec<_> = summary.validation.iter().filter(|i| i.severity == "error").collect();
    assert!(errors.is_empty(), "{errors:?}");
    // 09:59:30.0 on the computer's clock is 09:59:32.5 on the server's
    assert!(
        scans(&out).contains("_eeg.vhdr\t2026-10-26T09:59:32.5"),
        "{}",
        scans(&out)
    );
    let json = sidecar(&out);
    assert_eq!(json["ClockOffsetSeconds"], 2.5);
    assert_eq!(json["ClockChecks"], 3);
    assert_eq!(json["AcquisitionTimeCorrected"], true);
    // the clock file itself is not mistaken for a recording or exported as one
    assert_eq!(summary.recordings.len(), 1);

    let out = skill.path().join("as-recorded");
    let o = ExportOptions {
        correct_clock: false,
        ..opts(&out)
    };
    export(skill.path(), &o, &[], &mut |_| {}).unwrap();
    assert!(
        scans(&out).contains("_eeg.vhdr\t2026-10-26T09:59:30"),
        "{}",
        scans(&out)
    );
    let json = sidecar(&out);
    assert_eq!(json["ClockOffsetSeconds"], 2.5);
    assert_eq!(json["AcquisitionTimeCorrected"], false);
}
