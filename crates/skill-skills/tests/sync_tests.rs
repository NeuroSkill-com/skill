// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

use skill_skills::sync::{last_sync_ts, sync_skills, SyncOutcome, DEFAULT_SKILLS_REFRESH_SECS};
use std::fs;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("skill-skills-sync-{tag}-{}-{}", std::process::id(), nanos));
    fs::create_dir_all(&p).expect("create temp dir");
    p
}

#[test]
fn last_sync_ts_none_when_no_meta() {
    let dir = temp_dir("no-meta");
    assert_eq!(last_sync_ts(&dir), None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_skills_download_failure() {
    let dir = temp_dir("download-fail");
    // Use an invalid URL to force a download error
    let outcome = sync_skills(&dir, 0, Some("http://localhost:0/invalid-url.tar.gz"));
    match outcome {
        SyncOutcome::Failed(msg) => assert!(msg.contains("download")),
        _ => panic!("Expected download failure, got: {:?}", outcome),
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sync_skills_fresh_skips_download() {
    use serde_json::json;
    use std::fs::File;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let dir = temp_dir("fresh-skip");
    let meta_path = dir.join(".skills_last_sync");
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let url = "http://localhost:0/never-used-url.tar.gz";
    let meta = json!({
        "last_sync_ts": now,
        "url": url
    });
    let mut f = File::create(&meta_path).unwrap();
    f.write_all(meta.to_string().as_bytes()).unwrap();

    let outcome = sync_skills(&dir, DEFAULT_SKILLS_REFRESH_SECS, Some(url));
    match outcome {
        SyncOutcome::Fresh { next_sync_in_secs } => assert!(next_sync_in_secs <= DEFAULT_SKILLS_REFRESH_SECS),
        _ => panic!("Expected Fresh outcome, got: {:?}", outcome),
    }
    let _ = fs::remove_dir_all(&dir);
}

// More tests for sync_skills would go here, including mocks/fakes for network and extraction.
// For real network tests, consider using a test tarball URL or local server.
