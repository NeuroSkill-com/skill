// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Human-readable text formatter for the `skill status` command output.

use serde_json::Value;

/// Convert the JSON response from the `status` command into a compact,
/// human-readable text block that is easier for both the LLM and the user
/// to consume in the chat window.
pub(crate) fn format_status_as_text(v: &Value) -> String {
    let mut out = String::with_capacity(2048);

    // ── Device ────────────────────────────────────────────────────────────
    let dev = &v["device"];
    let state = dev["state"].as_str().unwrap_or("unknown");
    let connected = dev["connected"].as_bool().unwrap_or(false);
    let streaming = dev["streaming"].as_bool().unwrap_or(false);
    out.push_str("# Device\n");
    if connected {
        let name = dev["name"].as_str().unwrap_or("?");
        out.push_str(&format!("State: {} | Name: {} | Streaming: {}\n", state, name, if streaming { "yes" } else { "no" }));
        if let Some(b) = dev["battery"].as_f64() {
            out.push_str(&format!("Battery: {:.0}%\n", b));
        }
        out.push_str(&format!("EEG samples: {}", dev["sample_count"].as_u64().unwrap_or(0)));
        let ppg = dev["ppg_sample_count"].as_u64().unwrap_or(0);
        if ppg > 0 { out.push_str(&format!(" | PPG samples: {}", ppg)); }
        out.push('\n');
    } else {
        out.push_str(&format!("State: {} (not connected)\n", state));
    }

    // ── Session ───────────────────────────────────────────────────────────
    let sess = &v["session"];
    if let Some(dur) = sess["duration_secs"].as_u64() {
        let mins = dur / 60;
        let secs = dur % 60;
        out.push_str(&format!("\n# Session\nDuration: {}m {}s\n", mins, secs));
    }

    // ── Embeddings ────────────────────────────────────────────────────────
    let emb = &v["embeddings"];
    out.push_str("\n# EEG Embeddings\n");
    out.push_str(&format!(
        "Today: {} | Total: {} | Recording days: {} | Encoder loaded: {}\n",
        emb["today"].as_u64().unwrap_or(0),
        emb["total"].as_u64().unwrap_or(0),
        emb["recording_days"].as_u64().unwrap_or(0),
        if emb["encoder_loaded"].as_bool().unwrap_or(false) { "yes" } else { "no" },
    ));

    // ── Labels ────────────────────────────────────────────────────────────
    let labels = &v["labels"];
    out.push_str("\n# Labels\n");
    out.push_str(&format!(
        "Total: {} | With text embeddings: {}\n",
        labels["total"].as_u64().unwrap_or(0),
        labels["embedded"].as_u64().unwrap_or(0),
    ));
    // Recent labels
    if let Some(recent) = labels["recent"].as_array() {
        if !recent.is_empty() {
            out.push_str("Recent: ");
            let texts: Vec<&str> = recent.iter()
                .filter_map(|r| r["text"].as_str())
                .collect();
            out.push_str(&texts.join(", "));
            out.push('\n');
        }
    }
    // Top labels by time period
    format_freq_list(&mut out, "Top labels (all time)", &labels["top_all_time"]);
    format_freq_list(&mut out, "Top labels (7d)", &labels["top_7d"]);
    format_freq_list(&mut out, "Top labels (24h)", &labels["top_24h"]);

    // ── Apps ──────────────────────────────────────────────────────────────
    let apps = &v["apps"];
    let has_apps = apps["top_all_time"].as_array().map(|a| !a.is_empty()).unwrap_or(false);
    if has_apps {
        out.push_str("\n# Most Used Apps\n");
        format_app_list(&mut out, "All time", &apps["top_all_time"]);
        format_app_list(&mut out, "Last 7d", &apps["top_7d"]);
        format_app_list(&mut out, "Last 24h", &apps["top_24h"]);
    }

    // ── Screenshots ──────────────────────────────────────────────────────
    let ss = &v["screenshots"];
    let ss_total = ss["total"].as_u64().unwrap_or(0);
    if ss_total > 0 {
        out.push_str("\n# Screenshots\n");
        out.push_str(&format!(
            "Total: {} | With vision embedding: {} | With OCR text: {} | With OCR embedding: {}\n",
            ss_total,
            ss["with_embedding"].as_u64().unwrap_or(0),
            ss["with_ocr"].as_u64().unwrap_or(0),
            ss["with_ocr_embedding"].as_u64().unwrap_or(0),
        ));
        format_ocr_apps(&mut out, "Top screenshotted apps (all time)", &ss["top_apps_all_time"]);
        format_ocr_apps(&mut out, "Top screenshotted apps (24h)", &ss["top_apps_24h"]);
    }

    // ── Signal quality ───────────────────────────────────────────────────
    if let Some(chs) = v["signal_quality"].as_array() {
        if !chs.is_empty() {
            out.push_str("\n# Signal Quality\n");
            for ch in chs {
                let name  = ch["channel"].as_str().unwrap_or("?");
                let good  = ch["good"].as_bool().unwrap_or(false);
                let score = ch["score"].as_f64().unwrap_or(0.0);
                out.push_str(&format!("{}: {} ({:.0}%), ", name, if good { "good" } else { "poor" }, score * 100.0));
            }
            if out.ends_with(", ") { out.truncate(out.len() - 2); }
            out.push('\n');
        }
    }

    // ── Scores ────────────────────────────────────────────────────────────
    let scores = &v["scores"];
    if !scores.is_null() {
        out.push_str("\n# Current Scores\n");
        let fields = [
            ("Meditation", "meditation"),
            ("Cognitive load", "cognitive_load"),
            ("Drowsiness", "drowsiness"),
            ("Relaxation", "relaxation"),
            ("Engagement", "engagement"),
            ("Mood", "mood"),
            ("SNR", "snr"),
            ("Heart rate", "hr"),
        ];
        let mut parts: Vec<String> = Vec::new();
        for (label, key) in fields {
            if let Some(val) = scores[key].as_f64() {
                parts.push(format!("{}: {:.1}", label, val));
            }
        }
        if !parts.is_empty() {
            out.push_str(&parts.join(" | "));
            out.push('\n');
        }

        // Band powers
        let bands = &scores["bands"];
        if !bands.is_null() {
            let band_fields = [
                ("Delta", "rel_delta"), ("Theta", "rel_theta"),
                ("Alpha", "rel_alpha"), ("Beta", "rel_beta"), ("Gamma", "rel_gamma"),
            ];
            let mut bp: Vec<String> = Vec::new();
            for (label, key) in band_fields {
                if let Some(val) = bands[key].as_f64() {
                    bp.push(format!("{}: {:.3}", label, val));
                }
            }
            if !bp.is_empty() {
                out.push_str(&format!("Bands: {}\n", bp.join(", ")));
            }
        }
    }

    // ── Hooks ─────────────────────────────────────────────────────────────
    let hooks = &v["hooks"];
    if !hooks.is_null() {
        let total   = hooks["total"].as_u64().unwrap_or(0);
        let enabled = hooks["enabled"].as_u64().unwrap_or(0);
        out.push_str(&format!("\n# Hooks\nTotal: {} | Enabled: {}\n", total, enabled));
        if let Some(lt) = hooks["latest_trigger"].as_object() {
            let hook = lt.get("hook").and_then(|v| v.as_str()).unwrap_or("?");
            let text = lt.get("label_text").and_then(|v| v.as_str()).unwrap_or("");
            out.push_str(&format!("Latest trigger: {} (\"{}\")\n", hook, text));
        }
    }

    // ── Sleep ─────────────────────────────────────────────────────────────
    let sleep = &v["sleep"];
    let total_ep = sleep["total_epochs"].as_u64().unwrap_or(0);
    if total_ep > 0 {
        let epoch_s = sleep["epoch_secs"].as_u64().unwrap_or(30);
        let total_mins = (total_ep * epoch_s) / 60;
        out.push_str(&format!("\n# Sleep (48h window)\nTotal: {}m | ", total_mins));
        let stages = [
            ("Wake", "wake_epochs"), ("N1", "n1_epochs"),
            ("N2", "n2_epochs"), ("N3", "n3_epochs"), ("REM", "rem_epochs"),
        ];
        for (label, key) in stages {
            let ep = sleep[key].as_u64().unwrap_or(0);
            if ep > 0 {
                out.push_str(&format!("{}: {}m, ", label, (ep * epoch_s) / 60));
            }
        }
        if out.ends_with(", ") { out.truncate(out.len() - 2); }
        out.push('\n');
    }

    // ── History ───────────────────────────────────────────────────────────
    let hist = &v["history"];
    if !hist.is_null() {
        out.push_str("\n# Recording History\n");
        let fields = [
            ("Total sessions", "total_sessions"),
            ("Total hours", "total_hours"),
            ("Streak days", "streak_days"),
        ];
        let mut parts: Vec<String> = Vec::new();
        for (label, key) in fields {
            if let Some(val) = hist[key].as_f64() {
                parts.push(format!("{}: {:.1}", label, val));
            } else if let Some(val) = hist[key].as_u64() {
                parts.push(format!("{}: {}", label, val));
            }
        }
        if !parts.is_empty() {
            out.push_str(&parts.join(" | "));
            out.push('\n');
        }
    }

    // ── Calibration ──────────────────────────────────────────────────────
    if let Some(ts) = v["calibration"]["last_calibration_utc"].as_u64() {
        if ts > 0 {
            out.push_str(&format!("\n# Calibration\nLast calibration: {} UTC\n", ts));
        }
    }

    out
}

// ── Formatting helpers ────────────────────────────────────────────────────────

fn format_freq_list(out: &mut String, label: &str, arr: &Value) {
    if let Some(items) = arr.as_array() {
        if !items.is_empty() {
            out.push_str(&format!("{}:", label));
            for item in items {
                let text = item["text"].as_str().unwrap_or("?");
                let count = item["count"].as_u64().unwrap_or(0);
                out.push_str(&format!(" {} ({}x),", text, count));
            }
            // Remove trailing comma
            if out.ends_with(',') { out.pop(); }
            out.push('\n');
        }
    }
}

fn format_app_list(out: &mut String, label: &str, arr: &Value) {
    if let Some(items) = arr.as_array() {
        if !items.is_empty() {
            out.push_str(&format!("{}:", label));
            for item in items {
                let name = item["app_name"].as_str().unwrap_or("?");
                let switches = item["switches"].as_u64().unwrap_or(0);
                out.push_str(&format!(" {} ({}x),", name, switches));
            }
            if out.ends_with(',') { out.pop(); }
            out.push('\n');
        }
    }
}

fn format_ocr_apps(out: &mut String, label: &str, arr: &Value) {
    if let Some(items) = arr.as_array() {
        if !items.is_empty() {
            out.push_str(&format!("{}:", label));
            for item in items {
                let name = item["app_name"].as_str().unwrap_or("?");
                let count = item["count"].as_u64().unwrap_or(0);
                out.push_str(&format!(" {} ({}x),", name, count));
            }
            if out.ends_with(',') { out.pop(); }
            out.push('\n');
        }
    }
}
