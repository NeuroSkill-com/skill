// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! System tool handlers — `date`, `location`, `bash`, `skill`.

use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{SecondsFormat, Utc, Local};

use super::helpers::format_utc_offset;
use super::safety::{check_bash_safety, request_tool_approval};
use super::truncate::truncate_text;
use super::status::format_status_as_text;
use crate::types::LlmToolConfig;

// ── date ──────────────────────────────────────────────────────────────────────

pub(crate) fn exec_date() -> Value {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let now_utc = Utc::now();
    let now_local = now_utc.with_timezone(&Local);
    let offset_seconds = now_local.offset().local_minus_utc();
    json!({
        "ok": true,
        "tool": "date",
        "unix": now.as_secs(),
        "unix_ms": now.as_millis() as u64,
        "iso_utc": now_utc.to_rfc3339_opts(SecondsFormat::Millis, true),
        "iso_local": now_local.to_rfc3339_opts(SecondsFormat::Millis, false),
        "timezone": {
            "name": now_local.format("%Z").to_string(),
            "offset": format_utc_offset(offset_seconds),
            "offset_seconds": offset_seconds
        },
        "tz_env": std::env::var("TZ").ok(),
        "lang_env": std::env::var("LANG").ok(),
    })
}

// ── location ──────────────────────────────────────────────────────────────────

pub(crate) async fn exec_location() -> Value {
    tokio::task::spawn_blocking(|| {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(2))
            .timeout_read(std::time::Duration::from_secs(3))
            .build();
        let resp = agent.get("https://ipwho.is/").call();
        match resp {
            Ok(r) => {
                let v: Value = r.into_json::<Value>().unwrap_or_else(|_| json!({}));
                json!({
                    "ok": v.get("success").and_then(serde_json::Value::as_bool).unwrap_or(true),
                    "tool": "location",
                    "country": v.get("country").cloned().unwrap_or(Value::Null),
                    "region": v.get("region").cloned().unwrap_or(Value::Null),
                    "city": v.get("city").cloned().unwrap_or(Value::Null),
                    "timezone": v.get("timezone").and_then(|z| z.get("id")).cloned().unwrap_or(Value::Null),
                    "lat": v.get("latitude").cloned().unwrap_or(Value::Null),
                    "lon": v.get("longitude").cloned().unwrap_or(Value::Null),
                    "ip": v.get("ip").cloned().unwrap_or(Value::Null),
                })
            }
            Err(e) => json!({ "ok": false, "tool": "location", "error": e.to_string() }),
        }
    }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "location", "error": e.to_string() }))
}

// ── bash ──────────────────────────────────────────────────────────────────────

pub(crate) async fn exec_bash(args: &Value, scripts_dir: &std::path::Path) -> Value {
    let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if command.is_empty() {
        return json!({ "ok": false, "tool": "bash", "error": "missing command" });
    }
    let timeout_secs = args.get("timeout").and_then(serde_json::Value::as_f64).map(|t| t as u64);

    // Safety check: require user approval for dangerous commands
    if let Some(reason) = check_bash_safety(&command) {
        crate::tool_log!("tool:bash", "[safety] approval required: {}", reason);
        let approved = request_tool_approval("bash", &reason, &command).await;
        if !approved {
            crate::tool_log!("tool:bash", "[safety] user denied bash command");
            return json!({ "ok": false, "tool": "bash", "error": "operation denied by user" });
        }
        crate::tool_log!("tool:bash", "[safety] user approved bash command");
    }

    let scripts_dir = scripts_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        use std::process::Command;

        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));

        // If the command is long (>8 KB), write it to a script file
        // to avoid ARG_MAX / "prompt too long" errors.
        const SCRIPT_THRESHOLD: usize = skill_constants::TOOL_BASH_SCRIPT_THRESHOLD;
        let (actual_arg, script_path) = if command.len() > SCRIPT_THRESHOLD {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            let run_dir = scripts_dir.join(format!("run_{}", ts.as_secs()));
            let _ = std::fs::create_dir_all(&run_dir);
            let filename = format!("cmd_{}_{}.sh", ts.as_secs(), ts.subsec_millis());
            let path = run_dir.join(&filename);
            let script_content = format!("#!/usr/bin/env bash\nset -euo pipefail\n\n{}\n", command);
            if let Err(e) = std::fs::write(&path, &script_content) {
                return json!({ "ok": false, "tool": "bash", "error": format!("failed to write script: {}", e) });
            }
            // Make executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
            }
            (path.to_string_lossy().to_string(), Some(path))
        } else {
            (command.clone(), None)
        };

        let mut cmd = Command::new("bash");
        if script_path.is_some() {
            cmd.arg(&actual_arg).current_dir(&home);
        } else {
            cmd.arg("-c").arg(&actual_arg).current_dir(&home);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let child = cmd.spawn();
        match child {
            Ok(mut child) => {
                // If timeout specified, poll with deadline then kill
                let timed_out = if let Some(secs) = timeout_secs {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
                    loop {
                        match child.try_wait() {
                            Ok(Some(_)) => break false,
                            Ok(None) => {
                                if std::time::Instant::now() >= deadline {
                                    let _ = child.kill();
                                    break true;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            }
                            Err(_) => break false,
                        }
                    }
                } else {
                    false
                };

                match child.wait_with_output() {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        let mut combined = String::new();
                        if !stdout.is_empty() { combined.push_str(&stdout); }
                        if !stderr.is_empty() {
                            if !combined.is_empty() { combined.push('\n'); }
                            combined.push_str(&stderr);
                        }

                        let exit_code = out.status.code().unwrap_or(-1);
                        let total_lines = combined.lines().count();
                        let total_bytes = combined.len();

                        // Always save full output to a file for later search_output queries.
                        let output_file = {
                            let ts = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default();
                            let run_dir = scripts_dir.join(format!("run_{}", ts.as_secs()));
                            let _ = std::fs::create_dir_all(&run_dir);
                            let fname = format!("output_{}_{}.txt", ts.as_secs(), ts.subsec_millis());
                            let p = run_dir.join(&fname);
                            let _ = std::fs::write(&p, &combined);
                            p
                        };

                        // Build a compact summary: first 20 + last 20 lines if output is large.
                        const SUMMARY_HEAD: usize = skill_constants::TOOL_BASH_SUMMARY_HEAD;
                        const SUMMARY_TAIL: usize = skill_constants::TOOL_BASH_SUMMARY_TAIL;
                        const INLINE_THRESHOLD: usize = skill_constants::TOOL_BASH_INLINE_THRESHOLD;
                        let lines: Vec<&str> = combined.lines().collect();
                        let (summary, was_truncated) = if lines.len() <= INLINE_THRESHOLD {
                            (combined.clone(), false)
                        } else {
                            let head: Vec<&str> = lines.iter().take(SUMMARY_HEAD).copied().collect();
                            let tail: Vec<&str> = lines.iter().rev().take(SUMMARY_TAIL).copied().rev().collect();
                            let s = format!(
                                "{}\n\n\u{2026} [{} lines omitted \u{2014} use search_output to explore] \u{2026}\n\n{}",
                                head.join("\n"),
                                lines.len() - SUMMARY_HEAD - SUMMARY_TAIL,
                                tail.join("\n")
                            );
                            (s, true)
                        };

                        let mut result = json!({
                            "ok": exit_code == 0 && !timed_out,
                            "tool": "bash",
                            "command": command,
                            "exit_code": exit_code,
                            "output": summary,
                            "output_file": output_file.to_string_lossy(),
                            "total_lines": total_lines,
                            "total_bytes": total_bytes,
                        });
                        if was_truncated {
                            result["truncated"] = json!(true);
                        }
                        if timed_out {
                            result["error"] = json!(format!("command timed out after {} seconds", timeout_secs.unwrap_or(0)));
                        }
                        if let Some(ref sp) = script_path {
                            result["script_path"] = json!(sp.to_string_lossy());
                        }
                        result
                    }
                    Err(e) => json!({ "ok": false, "tool": "bash", "error": e.to_string() }),
                }
            }
            Err(e) => json!({ "ok": false, "tool": "bash", "error": e.to_string() }),
        }
    }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "bash", "error": e.to_string() }))
}

// ── skill ─────────────────────────────────────────────────────────────────────

pub(crate) async fn exec_skill(args: &Value, allowed_tools: &LlmToolConfig) -> Value {
    let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if command.is_empty() {
        return json!({ "ok": false, "tool": "skill", "error": "missing command" });
    }
    let port = allowed_tools.skill_api_port;
    if port == 0 {
        return json!({ "ok": false, "tool": "skill", "error": "Skill API port not configured" });
    }

    // Dangerous commands that should not be callable from the LLM
    // (LLM management would be recursive / nonsensical).
    const BLOCKED: &[&str] = &[
        "llm_start", "llm_stop", "llm_chat",
        "llm_delete", "llm_select_model", "llm_select_mmproj",
        "llm_set_autoload_mmproj", "llm_add_model",
        "llm_download", "llm_cancel_download", "llm_pause_download",
        "llm_resume_download", "llm_refresh_catalog", "llm_logs",
    ];
    if BLOCKED.contains(&command.as_str()) {
        return json!({ "ok": false, "tool": "skill", "error": format!("command \"{}\" is blocked from LLM tool use", command) });
    }

    // Build the JSON payload: { "command": "<cmd>", ...args }
    let extra_args = args.get("args").cloned().unwrap_or(json!({}));
    let payload = if let Some(obj) = extra_args.as_object() {
        let mut m = obj.clone();
        m.insert("command".to_string(), json!(command));
        Value::Object(m)
    } else {
        json!({ "command": command })
    };

    let url = format!("http://127.0.0.1:{}/", port);
    let payload_str = payload.to_string();

    tokio::task::spawn_blocking(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(3))
            .timeout_read(std::time::Duration::from_secs(30))
            .build();

        match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_string(&payload_str)
        {
            Ok(resp) => {
                match resp.into_json::<Value>() {
                    Ok(mut v) => {
                        // Ensure the response always has "tool" so the
                        // model can identify where the data came from.
                        if let Some(obj) = v.as_object_mut() {
                            obj.entry("tool".to_string())
                                .or_insert(json!("skill"));
                        }
                        // For status command, convert to readable text
                        // so the LLM and chat UI see a human-friendly
                        // representation instead of raw JSON.
                        if command == "status" {
                            let text = format_status_as_text(&v);
                            return json!({
                                "ok": true,
                                "tool": "skill",
                                "command": "status",
                                "text": text,
                            });
                        }

                        // Truncate very large responses to avoid
                        // blowing up the context window.
                        let s = v.to_string();
                        if s.len() > 24_000 {
                            json!({
                                "ok": v.get("ok").cloned().unwrap_or(json!(true)),
                                "tool": "skill",
                                "command": v.get("command").cloned().unwrap_or(json!(null)),
                                "truncated": true,
                                "response_preview": truncate_text(&s, 12_000),
                                "total_bytes": s.len(),
                                "hint": "Response was truncated. Use more specific queries or narrow the time range."
                            })
                        } else {
                            v
                        }
                    }
                    Err(e) => json!({ "ok": false, "tool": "skill", "error": format!("invalid JSON from server: {}", e) }),
                }
            }
            Err(e) => json!({ "ok": false, "tool": "skill", "error": format!("HTTP request failed: {}", e) }),
        }
    }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "skill", "error": e.to_string() }))
}
