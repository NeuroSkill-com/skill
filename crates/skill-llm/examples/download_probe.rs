// SPDX-License-Identifier: GPL-3.0-only
//! Drive the real HuggingFace downloader end to end.
//!
//! Exercises the parts no unit test can: the metadata call, commit pinning,
//! the streaming loop with its rate/ETA line, the hash check, and — when run
//! twice with an interruption in between — resume.
//!
//! ```sh
//! HF_HOME=/tmp/dl-probe-cache \
//!   cargo run -p skill-llm --example download_probe -- \
//!   unsloth/Qwen3-0.6B-GGUF Qwen3-0.6B-Q4_K_M.gguf
//! ```
//!
//! Pass a third argument to cancel after that many megabytes, then re-run
//! without it to verify the transfer resumes rather than restarts.
use std::sync::{Arc, Mutex};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = args.first().map_or("unsloth/Qwen3-0.6B-GGUF", String::as_str);
    let file = args.get(1).map_or("Qwen3-0.6B-Q4_K_M.gguf", String::as_str);
    let cancel_after_mb: Option<u64> = args.get(2).and_then(|s| s.parse().ok());

    let progress = Arc::new(Mutex::new(skill_llm::catalog::DownloadProgress {
        filename: file.to_string(),
        ..Default::default()
    }));

    // Report what the downloader publishes, which is what the UI shows.
    let watch = Arc::clone(&progress);
    let reporter = std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let p = watch.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        println!(
            "  [{:>5.1}%] {}",
            p.progress * 100.0,
            p.status_msg.as_deref().unwrap_or("")
        );
        if !matches!(p.state, skill_llm::catalog::DownloadState::Downloading) {
            break;
        }
    });

    if let Some(mb) = cancel_after_mb {
        let canceller = Arc::clone(&progress);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let mut p = canceller.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let done_mb = (p.progress as f64 * 400.0) as u64; // rough, probe only
            if done_mb >= mb {
                println!("  >> requesting cancel at ~{done_mb} MB");
                p.cancelled = true;
                break;
            }
        });
    }

    let started = std::time::Instant::now();
    let result = skill_llm::catalog::download_file(repo, file, &progress, 0);
    let _ = reporter.join();

    match result {
        Ok(path) => {
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            println!(
                "OK  {} ({:.1} MB) in {:.1}s\n    {}",
                file,
                size as f64 / 1_048_576.0,
                started.elapsed().as_secs_f64(),
                path.display()
            );
        }
        Err(e) => println!("ERR {file}: {e:#}"),
    }
    Ok(())
}
