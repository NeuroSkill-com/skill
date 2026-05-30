// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Local-only UMAP profiling harness using hotpath-rs.
//
// Not built or run in CI. Run with:
//   cargo run -p skill-router --release --example umap_hotpath \
//       --features='gpu,hotpath'
//
// Optional extras:
//   --features='gpu,hotpath,hotpath-alloc'   # also tracks allocations
//
// Adjust N_A / N_B below for larger/smaller workloads.

#[cfg(all(feature = "hotpath", feature = "gpu"))]
mod runner {
    use std::fs;
    use std::path::PathBuf;

    const N_A: usize = 500;
    const N_B: usize = 500;

    fn seed(tag: &str, n_a: usize, n_b: usize) -> (PathBuf, u64, u64, u64, u64) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let skill_dir = std::env::temp_dir().join(format!("skill-umap-hotpath-{tag}-{nanos}"));
        let day_dir = skill_dir.join("20260303");
        fs::create_dir_all(&day_dir).expect("create temp day dir");

        let db_path = day_dir.join("eeg.sqlite");
        let conn = rusqlite::Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE embeddings (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp       INTEGER NOT NULL,
                device_id       TEXT,
                device_name     TEXT,
                hnsw_id         INTEGER NOT NULL,
                eeg_embedding   BLOB NOT NULL,
                label           TEXT,
                extra_embedding BLOB,
                metrics_json    TEXT
            );
            CREATE INDEX idx_timestamp ON embeddings (timestamp);",
        )
        .expect("create table");

        let a_start_ms: i64 = 1_700_000_000_000;
        let b_start_ms: i64 = a_start_ms + (n_a as i64) * 250 + 60_000;
        let dim = 32_usize;
        let mut rng_seed: u64 = 42;

        let mut insert = conn
            .prepare(
                "INSERT INTO embeddings (timestamp, device_id, device_name, hnsw_id, eeg_embedding)
                 VALUES (?1, 'bench', 'bench', ?2, ?3)",
            )
            .expect("prepare insert");

        let mut write_epochs = |start_ms: i64, count: usize, cluster_offset: f32| {
            for i in 0..count {
                let ts = start_ms + (i as i64) * 250;
                let emb: Vec<f32> = (0..dim)
                    .map(|d| {
                        rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        let raw = ((rng_seed >> 33) as f32) / (u32::MAX as f32) - 0.5;
                        raw + cluster_offset * (d as f32 / dim as f32)
                    })
                    .collect();
                let blob: Vec<u8> = emb.iter().flat_map(|v| v.to_le_bytes()).collect();
                insert
                    .execute(rusqlite::params![ts, i as i64, blob])
                    .expect("insert embedding");
            }
        };

        write_epochs(a_start_ms, n_a, 1.0);
        write_epochs(b_start_ms, n_b, -1.0);

        drop(insert);
        conn.close().ok();

        let a_start_s = (a_start_ms / 1000) as u64;
        let a_end_s = ((a_start_ms + (n_a as i64) * 250) / 1000) as u64;
        let b_start_s = (b_start_ms / 1000) as u64;
        let b_end_s = ((b_start_ms + (n_b as i64) * 250) / 1000) as u64;
        (skill_dir, a_start_s, a_end_s, b_start_s, b_end_s)
    }

    #[hotpath::main]
    pub fn run() {
        let (skill_dir, a_start, a_end, b_start, b_end) = seed("run", N_A, N_B);
        eprintln!(
            "[hotpath] seeded {} + {} embeddings in {}",
            N_A,
            N_B,
            skill_dir.display()
        );

        match skill_router::umap_compute_inner(&skill_dir, a_start, a_end, b_start, b_end, None) {
            Ok(v) => {
                let backend = v["backend"].as_str().unwrap_or("?");
                let internal_ms = v["elapsed_ms"].as_u64().unwrap_or(0);
                eprintln!("[hotpath] backend={backend} internal_ms={internal_ms}");
            }
            Err(e) => eprintln!("[hotpath] umap_compute_inner failed: {e:#}"),
        }

        let _ = fs::remove_dir_all(&skill_dir);
    }
}

#[cfg(all(feature = "hotpath", feature = "gpu"))]
fn main() {
    runner::run();
}

#[cfg(not(all(feature = "hotpath", feature = "gpu")))]
fn main() {
    eprintln!(
        "umap_hotpath requires --features='hotpath,gpu'.\n\
         e.g. cargo run -p skill-router --release --example umap_hotpath \\\n\
                  --features='gpu,hotpath'"
    );
}
