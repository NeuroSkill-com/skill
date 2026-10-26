// SPDX-License-Identifier: GPL-3.0-only
//! Import a copy of a real `~/.skill` into DuckDB and check nothing was lost.
//!
//! ```sh
//! mkdir -p /tmp/skill-import && cd ~/.skill
//! cp *.sqlite /tmp/skill-import/            # labels, activity, screenshots, …
//! for d in 20*/; do mkdir -p /tmp/skill-import/$d; cp $d/eeg.sqlite /tmp/skill-import/$d 2>/dev/null; done
//! SKILL_IMPORT_FIXTURE=/tmp/skill-import \
//!   cargo test -p skill-data --test import_real_stores -- --ignored --nocapture
//! ```
//!
//! Run it on a copy: opening each store imports its `.sqlite` and renames it
//! to `.sqlite.bak`. Every table is compared row-for-row by count against
//! the backup.
#![allow(clippy::unwrap_used)]

use std::path::Path;

/// `(table, rows)` in a SQLite file.
fn sqlite_counts(path: &Path) -> Vec<(String, i64)> {
    let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND sql NOT LIKE '%VIRTUAL%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|t: &String| !t.contains("_fts"))
        .collect();
    tables
        .into_iter()
        .map(|t| {
            let n: i64 = conn
                .query_row(&format!("SELECT count(*) FROM \"{t}\""), [], |r| r.get(0))
                .unwrap();
            (t, n)
        })
        .collect()
}

fn check(duck: &skill_data::duck::Connection, backup: &Path, label: &str) -> usize {
    let mut tables = 0;
    for (table, expected) in sqlite_counts(backup) {
        let got: i64 = duck
            .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| r.get(0))
            .unwrap_or_else(|e| panic!("{label}: table {table} missing in DuckDB: {e}"));
        println!("  {label:<16} {table:<26} sqlite={expected:<8} duckdb={got}");
        assert_eq!(got, expected, "{label}.{table}: row count changed");
        tables += 1;
    }
    tables
}

#[test]
#[ignore = "requires SKILL_IMPORT_FIXTURE pointing at a *copy* of ~/.skill's .sqlite files"]
fn imports_every_real_store_without_loss() {
    let Ok(dir) = std::env::var("SKILL_IMPORT_FIXTURE") else {
        panic!("set SKILL_IMPORT_FIXTURE to a directory holding copies of the .sqlite stores");
    };
    let dir = std::path::PathBuf::from(dir);
    let t0 = std::time::Instant::now();
    let mut tables = 0;

    macro_rules! store {
        ($file:literal, $open:expr) => {
            if dir.join($file).exists() {
                let started = std::time::Instant::now();
                let conn = $open;
                let backup = dir.join(concat!($file, ".bak"));
                assert!(backup.exists(), "{} was not moved to .bak", $file);
                println!("{} imported in {:.2?}", $file, started.elapsed());
                tables += check(&conn, &backup, $file);
            }
        };
    }

    store!(
        "labels.sqlite",
        skill_data::label_store::LabelStore::connect(&dir).unwrap()
    );
    store!("activity.sqlite", {
        skill_data::activity_store::ActivityStore::open(&dir).unwrap();
        skill_data::duck::open(&dir.join(skill_constants::ACTIVITY_FILE)).unwrap()
    });
    store!("screenshots.sqlite", {
        skill_data::screenshot_store::ScreenshotStore::open(&dir).unwrap();
        skill_data::duck::open(&dir.join(skill_constants::SCREENSHOTS_SQLITE)).unwrap()
    });
    store!("hooks.sqlite", {
        skill_data::hooks_log::HooksLog::open(&dir).unwrap();
        skill_data::duck::open(&dir.join(skill_constants::HOOKS_LOG_FILE)).unwrap()
    });
    store!("validation.sqlite", {
        skill_data::validation_store::ValidationStore::open(&dir).unwrap();
        skill_data::duck::open(&dir.join(skill_constants::VALIDATION_FILE)).unwrap()
    });
    store!("health.sqlite", {
        skill_health::HealthStore::open(&dir).unwrap();
        skill_data::duck::open(&dir.join("health.duckdb")).unwrap()
    });

    for (date, day) in skill_data::util::date_dirs(&dir) {
        if !day.join("eeg.sqlite").exists() {
            continue;
        }
        let started = std::time::Instant::now();
        let conn = skill_data::eeg_store::open(&day).unwrap();
        println!("{date}/eeg.sqlite imported in {:.2?}", started.elapsed());
        tables += check(&conn, &day.join("eeg.sqlite.bak"), &format!("{date}/eeg"));
        let missing: i64 = conn
            .query_row("SELECT count(*) FROM embeddings WHERE unix_ms IS NULL", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(missing, 0, "{date}: unix_ms not backfilled");
    }

    println!("verified {tables} tables in {:.2?}", t0.elapsed());
    assert!(tables > 0, "fixture contained no stores");
}
