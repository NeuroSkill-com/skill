#![allow(clippy::unwrap_used)]
use std::path::PathBuf;

fn skill_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap()).join(".skill")
}

const TZ_EDT: i64 = -14400;

#[test]
fn e2e_full_pipeline() {
    let sd = skill_dir();
    if !sd.exists() {
        return;
    }

    // Step 1: list_session_days (what list_local_session_days calls internally)
    let utc_dirs = skill_history::list_session_days(&sd);
    eprintln!("1. list_session_days -> {} UTC dirs", utc_dirs.len());
    if utc_dirs.len() < 16 {
        eprintln!(
            "Skipping: not enough local session data ({} UTC dirs, need >= 16)",
            utc_dirs.len()
        );
        return;
    }

    // Step 2: list_local_session_days (what the IPC command calls)
    let local_days = skill_history::list_local_session_days(&sd, TZ_EDT);
    eprintln!("2. list_local_session_days -> {} local days", local_days.len());
    assert!(!local_days.is_empty());

    // Step 3: list_sessions_for_local_day for first (newest) day
    let newest = &local_days[0];
    let sessions = skill_history::list_sessions_for_local_day(&newest.key, TZ_EDT, &sd, None);
    eprintln!(
        "3. list_sessions_for_local_day({}) -> {} sessions",
        newest.key,
        sessions.len()
    );

    // Step 4: Verify session fields that the frontend uses
    let mut total = 0;
    for d in &local_days {
        let s = skill_history::list_sessions_for_local_day(&d.key, TZ_EDT, &sd, None);
        total += s.len();
        if !s.is_empty() {
            // Check first session has the fields the UI needs
            let first = &s[0];
            assert!(!first.csv_path.is_empty(), "csv_path empty for {}", d.key);
            assert!(
                first.session_start_utc.is_some() || first.session_end_utc.is_some(),
                "no timestamps for {} {}",
                d.key,
                first.csv_file
            );
        }
    }
    eprintln!("4. Total sessions across all days: {}", total);
    assert!(total > 0, "expected at least one session");

    // Step 5: Verify the SessionEntry serializes to JSON (what Tauri sends to frontend)
    let s = skill_history::list_sessions_for_local_day(&local_days[0].key, TZ_EDT, &sd, None);
    if let Some(first) = s.first() {
        let json = serde_json::to_string(first).unwrap();
        eprintln!(
            "5. SessionEntry JSON (first 200 chars): {}",
            &json[..json.len().min(200)]
        );
        // Verify it round-trips
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("csv_path").is_some());
        assert!(v.get("csv_file").is_some());
        assert!(v.get("session_start_utc").is_some());
        eprintln!("   csv_path: {}", v["csv_path"]);
        eprintln!("   session_start_utc: {}", v["session_start_utc"]);
        eprintln!("   sample_rate_hz: {}", v["sample_rate_hz"]);
    }

    // Step 6: Verify LocalDayInfo serializes (what Tauri sends for allLocalDays)
    let day_json = serde_json::to_string(&local_days[0]).unwrap();
    eprintln!("6. LocalDayInfo JSON: {}", day_json);
    let dv: serde_json::Value = serde_json::from_str(&day_json).unwrap();
    assert!(dv.get("key").is_some());
    assert!(dv.get("start_utc").is_some());
    assert!(dv.get("end_utc").is_some());
}
