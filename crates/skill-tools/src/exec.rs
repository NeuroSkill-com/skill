// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Built-in tool execution — the runtime implementation of each tool.

use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{SecondsFormat, Utc, Local};

use crate::parse::ToolCall;
use crate::types::LlmToolConfig;
use crate::defs::is_builtin_tool_enabled;

// ── Public execution entry point ──────────────────────────────────────────────

/// Execute a single built-in tool call and return the JSON result.
pub async fn execute_builtin_tool_call(call: &ToolCall, allowed_tools: &LlmToolConfig, scripts_dir: &std::path::Path) -> Value {
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| json!({}));
    let tool_name = &call.function.name;

    if !is_builtin_tool_enabled(allowed_tools, tool_name) {
        tool_log!("tool", "[blocked] tool={} reason=disabled in settings", tool_name);
        return json!({ "ok": false, "tool": call.function.name, "error": "tool disabled in settings" });
    }

    tool_log!("tool", "[invoke] tool={} args={}", tool_name, args);
    let start = std::time::Instant::now();

    let result = match call.function.name.as_str() {
        "date" => {
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

        "location" => {
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
                            "ok": v.get("success").and_then(|x| x.as_bool()).unwrap_or(true),
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

        "web_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if query.is_empty() {
                return json!({ "ok": false, "tool": "web_search", "error": "missing query" });
            }

            let provider = allowed_tools.web_search_provider.clone();
            tokio::task::spawn_blocking(move || {
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(std::time::Duration::from_secs(5))
                    .timeout_read(std::time::Duration::from_secs(10))
                    .build();

                // Try the configured backend first.
                let results = match provider.backend.as_str() {
                    "brave" if !provider.brave_api_key.is_empty() => {
                        let r = brave_search(&agent, &provider.brave_api_key, &query);
                        if r.is_empty() { ddg_html_search(&agent, &query) } else { r }
                    }
                    "searxng" if !provider.searxng_url.is_empty() => {
                        let r = searxng_search(&agent, &provider.searxng_url, &query);
                        if r.is_empty() { ddg_html_search(&agent, &query) } else { r }
                    }
                    _ => ddg_html_search(&agent, &query),
                };

                if results.is_empty() {
                    json!({ "ok": true, "tool": "web_search", "query": query, "results": [], "note": "no results found" })
                } else {
                    json!({ "ok": true, "tool": "web_search", "query": query, "results": results })
                }
            }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "web_search", "error": e.to_string() }))
        }

        "web_fetch" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return json!({ "ok": false, "tool": "web_fetch", "error": "url must start with http:// or https://" });
            }

            let url_for_fetch = url.clone();
            tokio::task::spawn_blocking(move || {
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(std::time::Duration::from_secs(3))
                    .timeout_read(std::time::Duration::from_secs(8))
                    .build();
                let resp = agent
                    .get(&url_for_fetch)
                    .set("User-Agent", random_ua())
                    .call();

                match resp {
                    Ok(r) => {
                        let status = r.status();
                        let content_type = r.header("Content-Type").unwrap_or("").to_string();
                        let body = r.into_string().unwrap_or_default();
                        json!({
                            "ok": true,
                            "tool": "web_fetch",
                            "url": url_for_fetch,
                            "status": status,
                            "content_type": content_type,
                            "content": truncate_text(&body, 12_000),
                            "truncated": body.chars().count() > 12_000,
                        })
                    }
                    Err(e) => json!({ "ok": false, "tool": "web_fetch", "url": url_for_fetch, "error": e.to_string() }),
                }
            }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "web_fetch", "url": url, "error": e.to_string() }))
        }

        "bash" => {
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if command.is_empty() {
                return json!({ "ok": false, "tool": "bash", "error": "missing command" });
            }
            let timeout_secs = args.get("timeout").and_then(|v| v.as_f64()).map(|t| t as u64);

            // Safety check: require user approval for dangerous commands
            if let Some(reason) = check_bash_safety(&command) {
                tool_log!("tool:bash", "[safety] approval required: {}", reason);
                let approved = request_tool_approval("bash", &reason, &command).await;
                if !approved {
                    tool_log!("tool:bash", "[safety] user denied bash command");
                    return json!({ "ok": false, "tool": "bash", "error": "operation denied by user" });
                }
                tool_log!("tool:bash", "[safety] user approved bash command");
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
                                        "{}\n\n… [{} lines omitted — use search_output to explore] …\n\n{}",
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

        "read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if path.is_empty() {
                return json!({ "ok": false, "tool": "read_file", "error": "missing path" });
            }
            let offset = args.get("offset").and_then(|v| v.as_u64()).map(|v| v as usize);
            let limit = args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);

            tokio::task::spawn_blocking(move || {
                let resolved = resolve_tool_path(&path);

                match std::fs::read_to_string(&resolved) {
                    Ok(content) => {
                        let all_lines: Vec<&str> = content.split('\n').collect();
                        let total_file_lines = all_lines.len();

                        let start_line = offset.map(|o| (o.max(1)) - 1).unwrap_or(0);

                        if start_line >= all_lines.len() {
                            return json!({
                                "ok": false, "tool": "read_file",
                                "error": format!("offset {} is beyond end of file ({} lines total)", offset.unwrap_or(1), total_file_lines)
                            });
                        }

                        let end_line = if let Some(lim) = limit {
                            (start_line + lim).min(all_lines.len())
                        } else {
                            all_lines.len()
                        };

                        let selected: String = all_lines[start_line..end_line].join("\n");
                        let user_limited = limit.is_some() && end_line < all_lines.len();

                        // Truncate: keep first 2000 lines / 50 KB
                        let truncated = truncate_tool_output_head(&selected, 2000, 50 * 1024);
                        let start_display = start_line + 1;

                        let mut result = json!({
                            "ok": true,
                            "tool": "read_file",
                            "content": truncated.text,
                            "total_lines": total_file_lines,
                        });

                        if truncated.was_truncated {
                            let end_display = start_display + truncated.output_lines.saturating_sub(1);
                            let next_offset = end_display + 1;
                            result["truncated"] = json!(true);
                            result["showing_lines"] = json!(format!("{}-{}", start_display, end_display));
                            result["hint"] = json!(format!("Use offset={} to continue reading.", next_offset));
                        } else if user_limited {
                            let remaining = all_lines.len() - end_line;
                            let next_offset = end_line + 1;
                            result["remaining_lines"] = json!(remaining);
                            result["hint"] = json!(format!("Use offset={} to continue reading.", next_offset));
                        }

                        result
                    }
                    Err(e) => json!({ "ok": false, "tool": "read_file", "error": format!("{}: {}", resolved.display(), e) }),
                }
            }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "read_file", "error": e.to_string() }))
        }

        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if path.is_empty() {
                return json!({ "ok": false, "tool": "write_file", "error": "missing path" });
            }

            // Safety check: require approval for sensitive paths
            let resolved_check = resolve_tool_path(&path);
            if let Some(reason) = check_path_safety(&resolved_check) {
                tool_log!("tool:write_file", "[safety] approval required: {}", reason);
                let detail = format!("Write to: {}", resolved_check.display());
                let approved = request_tool_approval("write_file", &reason, &detail).await;
                if !approved {
                    tool_log!("tool:write_file", "[safety] user denied write");
                    return json!({ "ok": false, "tool": "write_file", "error": "operation denied by user" });
                }
                tool_log!("tool:write_file", "[safety] user approved write");
            }

            tokio::task::spawn_blocking(move || {
                let resolved = resolve_tool_path(&path);

                // Create parent directories
                if let Some(parent) = resolved.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return json!({ "ok": false, "tool": "write_file", "error": format!("cannot create directories: {}", e) });
                    }
                }

                match std::fs::write(&resolved, &content) {
                    Ok(()) => json!({
                        "ok": true,
                        "tool": "write_file",
                        "path": resolved.display().to_string(),
                        "bytes_written": content.len(),
                    }),
                    Err(e) => json!({ "ok": false, "tool": "write_file", "error": format!("{}: {}", resolved.display(), e) }),
                }
            }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "write_file", "error": e.to_string() }))
        }

        "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let old_text = args.get("old_text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let new_text = args.get("new_text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if path.is_empty() {
                return json!({ "ok": false, "tool": "edit_file", "error": "missing path" });
            }
            if old_text.is_empty() {
                return json!({ "ok": false, "tool": "edit_file", "error": "missing old_text" });
            }

            // Safety check: require approval for sensitive paths
            let resolved_check = resolve_tool_path(&path);
            if let Some(reason) = check_path_safety(&resolved_check) {
                tool_log!("tool:edit_file", "[safety] approval required: {}", reason);
                let detail = format!("Edit: {}", resolved_check.display());
                let approved = request_tool_approval("edit_file", &reason, &detail).await;
                if !approved {
                    tool_log!("tool:edit_file", "[safety] user denied edit");
                    return json!({ "ok": false, "tool": "edit_file", "error": "operation denied by user" });
                }
                tool_log!("tool:edit_file", "[safety] user approved edit");
            }

            tokio::task::spawn_blocking(move || {
                let resolved = resolve_tool_path(&path);

                let content = match std::fs::read_to_string(&resolved) {
                    Ok(c) => c,
                    Err(e) => return json!({ "ok": false, "tool": "edit_file", "error": format!("cannot read {}: {}", resolved.display(), e) }),
                };

                // Normalize line endings for matching
                let normalized_content = content.replace("\r\n", "\n");
                let normalized_old = old_text.replace("\r\n", "\n");
                let normalized_new = new_text.replace("\r\n", "\n");

                // Count occurrences
                let occurrences = normalized_content.matches(&normalized_old).count();

                if occurrences == 0 {
                    return json!({
                        "ok": false, "tool": "edit_file",
                        "error": "could not find the exact text in the file. The old_text must match exactly including all whitespace and newlines."
                    });
                }

                if occurrences > 1 {
                    return json!({
                        "ok": false, "tool": "edit_file",
                        "error": format!("found {} occurrences of the text. The text must be unique. Please provide more context to make it unique.", occurrences)
                    });
                }

                let new_content = normalized_content.replacen(&normalized_old, &normalized_new, 1);

                if normalized_content == new_content {
                    return json!({
                        "ok": false, "tool": "edit_file",
                        "error": "no changes made — the replacement produced identical content."
                    });
                }

                // Restore original line endings if file used CRLF
                let final_content = if content.contains("\r\n") {
                    new_content.replace('\n', "\r\n")
                } else {
                    new_content
                };

                match std::fs::write(&resolved, &final_content) {
                    Ok(()) => json!({
                        "ok": true,
                        "tool": "edit_file",
                        "path": resolved.display().to_string(),
                        "message": "successfully replaced text",
                    }),
                    Err(e) => json!({ "ok": false, "tool": "edit_file", "error": format!("cannot write {}: {}", resolved.display(), e) }),
                }
            }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "edit_file", "error": e.to_string() }))
        }

        "search_output" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if path.is_empty() {
                return json!({ "ok": false, "tool": "search_output", "error": "missing path" });
            }
            let pattern = args.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());
            let context_lines = args.get("context_lines").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
            let head_n = args.get("head").and_then(|v| v.as_u64()).map(|v| v as usize);
            let tail_n = args.get("tail").and_then(|v| v.as_u64()).map(|v| v as usize);
            let line_start = args.get("line_start").and_then(|v| v.as_u64()).map(|v| v as usize);
            let line_end = args.get("line_end").and_then(|v| v.as_u64()).map(|v| v as usize);
            let max_matches = args.get("max_matches").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

            tokio::task::spawn_blocking(move || {
                let resolved = resolve_tool_path(&path);
                let content = match std::fs::read_to_string(&resolved) {
                    Ok(c) => c,
                    Err(e) => return json!({ "ok": false, "tool": "search_output", "error": format!("cannot read {}: {}", resolved.display(), e) }),
                };
                let all_lines: Vec<&str> = content.lines().collect();
                let total_lines = all_lines.len();

                // Mode 1: Head/tail — return first or last N lines
                if let Some(n) = head_n {
                    let n = n.min(total_lines);
                    let result: Vec<String> = all_lines.iter().take(n)
                        .enumerate()
                        .map(|(i, l)| format!("{:>6}: {}", i + 1, l))
                        .collect();
                    return json!({
                        "ok": true, "tool": "search_output",
                        "mode": "head", "total_lines": total_lines,
                        "lines_returned": result.len(),
                        "output": result.join("\n"),
                    });
                }
                if let Some(n) = tail_n {
                    let n = n.min(total_lines);
                    let start = total_lines.saturating_sub(n);
                    let result: Vec<String> = all_lines.iter().skip(start)
                        .enumerate()
                        .map(|(i, l)| format!("{:>6}: {}", start + i + 1, l))
                        .collect();
                    return json!({
                        "ok": true, "tool": "search_output",
                        "mode": "tail", "total_lines": total_lines,
                        "lines_returned": result.len(),
                        "output": result.join("\n"),
                    });
                }

                // Mode 2: Line range
                if let Some(start) = line_start {
                    let start_idx = start.saturating_sub(1).min(total_lines);
                    let end_idx = line_end.unwrap_or(start_idx + 50).min(total_lines);
                    let result: Vec<String> = all_lines[start_idx..end_idx]
                        .iter()
                        .enumerate()
                        .map(|(i, l)| format!("{:>6}: {}", start_idx + i + 1, l))
                        .collect();
                    return json!({
                        "ok": true, "tool": "search_output",
                        "mode": "range", "total_lines": total_lines,
                        "line_start": start_idx + 1, "line_end": end_idx,
                        "lines_returned": result.len(),
                        "output": result.join("\n"),
                    });
                }

                // Mode 3: Regex search
                if let Some(ref pat) = pattern {
                    let re = match regex::RegexBuilder::new(pat)
                        .case_insensitive(true)
                        .build()
                    {
                        Ok(r) => r,
                        Err(e) => return json!({ "ok": false, "tool": "search_output", "error": format!("invalid regex: {}", e) }),
                    };

                    let mut matches: Vec<String> = Vec::new();
                    let mut match_count = 0usize;
                    let mut last_printed = 0usize;

                    for (i, line) in all_lines.iter().enumerate() {
                        if re.is_match(line) {
                            match_count += 1;
                            if match_count > max_matches { break; }

                            let ctx_start = i.saturating_sub(context_lines).max(last_printed);
                            let ctx_end = (i + context_lines + 1).min(total_lines);

                            if ctx_start > last_printed && last_printed > 0 {
                                matches.push("   ---".to_string());
                            }

                            for j in ctx_start..ctx_end {
                                let marker = if j == i { ">" } else { " " };
                                matches.push(format!("{}{:>5}: {}", marker, j + 1, all_lines[j]));
                            }
                            last_printed = ctx_end;
                        }
                    }

                    let total_matches = if match_count > max_matches {
                        format!("{}+ (capped at {})", max_matches, max_matches)
                    } else {
                        match_count.to_string()
                    };

                    return json!({
                        "ok": true, "tool": "search_output",
                        "mode": "regex", "pattern": pat,
                        "total_lines": total_lines,
                        "matches": total_matches,
                        "output": matches.join("\n"),
                    });
                }

                // No mode specified — return file summary
                json!({
                    "ok": true, "tool": "search_output",
                    "mode": "info", "total_lines": total_lines,
                    "total_bytes": content.len(),
                    "hint": "Use pattern, head, tail, or line_start/line_end to explore the file.",
                })
            }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "search_output", "error": e.to_string() }))
        }

        other => {
            tool_log!("tool", "[error] tool={} unsupported", other);
            json!({ "ok": false, "tool": other, "error": "unsupported tool" })
        }
    };

    let elapsed = start.elapsed();
    let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        tool_log!("tool", "[done] tool={} elapsed={:.1?}", tool_name, elapsed);
    } else {
        let err = result.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
        tool_log!("tool", "[fail] tool={} elapsed={:.1?} error={}", tool_name, elapsed, err);
    }
    result
}

// ── Text truncation helpers ───────────────────────────────────────────────────

/// Truncate a string to at most `max_chars` characters.
pub fn truncate_text(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

#[allow(dead_code)]
struct TruncatedOutput {
    text: String,
    was_truncated: bool,
    total_lines: usize,
    total_bytes: usize,
    output_lines: usize,
}

/// Truncate from the tail (keep last N lines / max bytes).
/// Suitable for bash output where the end (errors/results) matters most.
#[allow(dead_code)]
fn truncate_tool_output(content: &str, max_lines: usize, max_bytes: usize) -> TruncatedOutput {
    let total_bytes = content.len();
    let lines: Vec<&str> = content.split('\n').collect();
    let total_lines = lines.len();

    if total_lines <= max_lines && total_bytes <= max_bytes {
        return TruncatedOutput {
            text: content.to_string(),
            was_truncated: false,
            total_lines,
            total_bytes,
            output_lines: total_lines,
        };
    }

    let mut output: Vec<&str> = Vec::new();
    let mut byte_count = 0usize;

    for &line in lines.iter().rev() {
        let lb = line.len() + if output.is_empty() { 0 } else { 1 };
        if byte_count + lb > max_bytes || output.len() >= max_lines {
            break;
        }
        output.push(line);
        byte_count += lb;
    }

    output.reverse();
    let output_lines = output.len();
    TruncatedOutput {
        text: output.join("\n"),
        was_truncated: true,
        total_lines,
        total_bytes,
        output_lines,
    }
}

/// Truncate from the head (keep first N lines / max bytes).
/// Suitable for file reads where you want to see the beginning.
fn truncate_tool_output_head(content: &str, max_lines: usize, max_bytes: usize) -> TruncatedOutput {
    let total_bytes = content.len();
    let lines: Vec<&str> = content.split('\n').collect();
    let total_lines = lines.len();

    if total_lines <= max_lines && total_bytes <= max_bytes {
        return TruncatedOutput {
            text: content.to_string(),
            was_truncated: false,
            total_lines,
            total_bytes,
            output_lines: total_lines,
        };
    }

    let mut output: Vec<&str> = Vec::new();
    let mut byte_count = 0usize;

    for &line in &lines {
        let lb = line.len() + if output.is_empty() { 0 } else { 1 };
        if byte_count + lb > max_bytes || output.len() >= max_lines {
            break;
        }
        output.push(line);
        byte_count += lb;
    }

    let output_lines = output.len();
    TruncatedOutput {
        text: output.join("\n"),
        was_truncated: true,
        total_lines,
        total_bytes,
        output_lines,
    }
}

// ── Filesystem helpers ────────────────────────────────────────────────────────

/// Resolve a path for filesystem tools.  Supports `~` expansion and relative
/// paths (resolved against the user's home directory).
pub fn resolve_tool_path(path: &str) -> std::path::PathBuf {
    let expanded = if path == "~" {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"))
    } else if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")).join(rest)
    } else {
        std::path::PathBuf::from(path)
    };

    if expanded.is_absolute() {
        expanded
    } else {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")).join(expanded)
    }
}

// ── Dangerous operation detection ─────────────────────────────────────────────

/// Patterns that indicate a potentially dangerous bash command.
const DANGEROUS_BASH_PATTERNS: &[&str] = &[
    "rm ", "rm\t", "rmdir", "shred",
    "mkfs", "dd if=", "dd of=",
    "sudo ", "su -", "su\t",
    "> /dev/", "chmod", "chown",
    "kill ", "killall", "pkill",
    "shutdown", "reboot", "halt", "poweroff",
    "systemctl stop", "systemctl disable",
    ":(){ :|:& };:", // fork bomb
    "/etc/", "/boot/", "/usr/", "/var/", "/sys/", "/proc/",
];

/// Sensitive path prefixes that require approval for file write/edit.
const SENSITIVE_PATH_PREFIXES: &[&str] = &[
    "/etc/", "/boot/", "/usr/", "/var/", "/sys/", "/proc/",
    "/bin/", "/sbin/", "/lib/", "/opt/",
];

/// Check if a bash command looks dangerous and return a human-readable reason.
pub fn check_bash_safety(command: &str) -> Option<String> {
    let lower = command.to_lowercase();
    for pat in DANGEROUS_BASH_PATTERNS {
        if lower.contains(pat) {
            return Some(format!("Command contains `{}`", pat.trim()));
        }
    }
    None
}

/// Check if a file path is in a sensitive location.
pub fn check_path_safety(path: &std::path::Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    for prefix in SENSITIVE_PATH_PREFIXES {
        if path_str.starts_with(prefix) {
            return Some(format!("Path is in sensitive location `{}`", prefix));
        }
    }
    None
}

/// Show a blocking approval dialog for a dangerous tool operation.
/// Returns `true` if the user approves, `false` if they deny.
pub async fn request_tool_approval(tool_name: &str, reason: &str, detail: &str) -> bool {
    let message = format!(
        "The LLM wants to use the {} tool.\n\n⚠️ {}\n\n{}\n\nAllow this operation?",
        tool_name, reason, detail
    );

    tokio::task::spawn_blocking(move || {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("NeuroSkill — Tool Approval Required")
            .set_description(&message)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show() == rfd::MessageDialogResult::Yes
    }).await.unwrap_or(false)
}

// ── Formatting helpers ────────────────────────────────────────────────────────

fn format_utc_offset(offset_seconds: i32) -> String {
    let sign = if offset_seconds >= 0 { '+' } else { '-' };
    let total = offset_seconds.unsigned_abs();
    let hours = total / 3600;
    let mins = (total % 3600) / 60;
    format!("{sign}{hours:02}:{mins:02}")
}

// ── DuckDuckGo search helpers ─────────────────────────────────────────────────

/// Strip HTML tags from a string (simple regex-free approach).
fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
       .replace("&lt;", "<")
       .replace("&gt;", ">")
       .replace("&quot;", "\"")
       .replace("&#x27;", "'")
       .replace("&#39;", "'")
       .replace("&nbsp;", " ")
}

/// Pool of realistic browser User-Agent strings, rotated randomly to reduce
/// fingerprinting and bot-detection risk.
const BROWSER_USER_AGENTS: &[&str] = &[
    // Chrome on Windows
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
    // Chrome on macOS
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
    // Firefox on Windows
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0",
    // Firefox on Linux
    "Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0",
    // Safari on macOS
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_7_2) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.2 Safari/605.1.15",
    // Edge on Windows
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0",
];

/// Pick a random browser User-Agent from the pool.
fn random_ua() -> &'static str {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let idx = COUNTER.fetch_add(1, Ordering::Relaxed) % BROWSER_USER_AGENTS.len();
    BROWSER_USER_AGENTS[idx]
}

/// Fallback search: scrape DuckDuckGo HTML lite page.
///
/// Mimics a real browser form submission: the lite page has a `<form>` that
/// POSTs `q=<query>&b=` to `/html/`.  The `Origin` and `Referer` headers are
/// required to pass bot detection.
fn ddg_html_search(agent: &ureq::Agent, query: &str) -> Vec<Value> {
    let ua = random_ua();
    let resp = agent
        .post("https://html.duckduckgo.com/html/")
        .set("User-Agent", ua)
        .set("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .set("Accept-Language", "en-US,en;q=0.5")
        .set("Origin", "https://html.duckduckgo.com")
        .set("Referer", "https://html.duckduckgo.com/html/")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&format!("q={}&b=", urlencoding::encode(query)));

    let Ok(r) = resp else { return Vec::new(); };
    let body = match r.into_string() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    parse_ddg_html(&body)
}

/// Parse DuckDuckGo HTML response body into search results.
fn parse_ddg_html(body: &str) -> Vec<Value> {
    let mut results = Vec::new();

    // Each result is wrapped in: <div class="result results_links results_links_deep web-result ">
    // Split on the outer result wrapper to get one chunk per result.
    for chunk in body.split("class=\"result results_links") {
        if results.len() >= 10 {
            break;
        }

        // Title + URL from <a class="result__a" href="...">Title</a>
        let url = extract_attr_value(chunk, "class=\"result__a\"", "href=\"");

        let title = extract_tag_content(chunk, "class=\"result__a\"");

        // Snippet from <a class="result__snippet" href="...">Snippet text</a>
        let snippet = extract_tag_content(chunk, "class=\"result__snippet\"");

        if let Some(url) = url {
            let real_url = extract_ddg_redirect_url(&url).unwrap_or_else(|| url.clone());

            if real_url.contains("duckduckgo.com") {
                continue;
            }

            let title_text = title.map(|t| strip_html_tags(&t)).unwrap_or_default();
            let snippet_text = snippet.map(|s| strip_html_tags(&s)).unwrap_or_default();

            if !title_text.is_empty() || !snippet_text.is_empty() {
                results.push(json!({
                    "title":   if title_text.is_empty() { real_url.clone() } else { title_text },
                    "url":     real_url,
                    "snippet": truncate_text(&snippet_text, 500),
                }));
            }
        }
    }

    results
}

fn extract_attr_value(html: &str, marker: &str, attr: &str) -> Option<String> {
    let marker_pos = html.find(marker)?;
    let after_marker = &html[marker_pos..];
    let attr_pos = after_marker.find(attr)?;
    let value_start = attr_pos + attr.len();
    let after_attr = &after_marker[value_start..];
    let end = after_attr.find('"')?;
    Some(after_attr[..end].to_string())
}

fn extract_tag_content(html: &str, marker: &str) -> Option<String> {
    let marker_pos = html.find(marker)?;
    let after_marker = &html[marker_pos..];
    let tag_close = after_marker.find('>')?;
    let content_start = tag_close + 1;
    let after_tag = &after_marker[content_start..];
    let end = after_tag.find("</").unwrap_or(after_tag.len().min(1000));
    Some(after_tag[..end].to_string())
}

fn extract_ddg_redirect_url(url: &str) -> Option<String> {
    if let Some(pos) = url.find("uddg=") {
        let after = &url[pos + 5..];
        let end = after.find('&').unwrap_or(after.len());
        let encoded = &after[..end];
        Some(urlencoding::decode(encoded).unwrap_or_else(|_| encoded.into()).into_owned())
    } else {
        None
    }
}

// ── Brave Search API ──────────────────────────────────────────────────────────

/// Search using the Brave Search API (free tier: 2 000 queries/month).
/// See <https://brave.com/search/api/>.
fn brave_search(agent: &ureq::Agent, api_key: &str, query: &str) -> Vec<Value> {
    let resp = agent
        .get("https://api.search.brave.com/res/v1/web/search")
        .query("q", query)
        .query("count", "10")
        .set("Accept", "application/json")
        .set("Accept-Encoding", "gzip")
        .set("X-Subscription-Token", api_key)
        .call();

    let Ok(r) = resp else { return Vec::new() };
    let body: Value = r.into_json::<Value>().unwrap_or_else(|_| json!({}));

    let Some(items) = body.pointer("/web/results").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for item in items.iter().take(10) {
        let title   = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let url     = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let snippet = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();

        if url.is_empty() { continue; }

        results.push(json!({
            "title":   if title.is_empty() { url.clone() } else { strip_html_tags(&title) },
            "url":     url,
            "snippet": truncate_text(&strip_html_tags(&snippet), 500),
        }));
    }
    results
}

// ── SearXNG search ────────────────────────────────────────────────────────────

/// Search using a self-hosted SearXNG instance JSON API.
fn searxng_search(agent: &ureq::Agent, base_url: &str, query: &str) -> Vec<Value> {
    let url = format!("{}/search", base_url.trim_end_matches('/'));
    let resp = agent
        .get(&url)
        .query("q", query)
        .query("format", "json")
        .query("categories", "general")
        .set("Accept", "application/json")
        .call();

    let Ok(r) = resp else { return Vec::new() };
    let body: Value = r.into_json::<Value>().unwrap_or_else(|_| json!({}));

    let Some(items) = body.get("results").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for item in items.iter().take(10) {
        let title   = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let url     = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let snippet = item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();

        if url.is_empty() { continue; }

        results.push(json!({
            "title":   if title.is_empty() { url.clone() } else { title },
            "url":     url,
            "snippet": truncate_text(&snippet, 500),
        }));
    }
    results
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod web_search_tests {
    use super::*;

    fn make_agent() -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(10))
            .timeout_read(std::time::Duration::from_secs(15))
            .build()
    }

    /// Dump raw DDG HTML response for debugging.
    /// Run manually: `cargo test -p skill-tools debug_ddg_raw_response -- --nocapture --ignored`
    #[test]
    #[ignore]
    fn debug_ddg_raw_response() {
        let agent = make_agent();
        let query = "rust programming language";
        let ua = BROWSER_USER_AGENTS[0];

        // Current approach: POST to /html/
        let resp = agent
            .post("https://html.duckduckgo.com/html/")
            .set("User-Agent", ua)
            .set("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .set("Accept-Language", "en-US,en;q=0.5")
            .set("Origin", "https://html.duckduckgo.com")
            .set("Referer", "https://html.duckduckgo.com/html/")
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(&format!("q={}&b=", urlencoding::encode(query)));

        match resp {
            Ok(r) => {
                let status = r.status();
                let body = r.into_string().unwrap_or_default();
                let result_count = body.matches("result__body").count();
                let has_captcha = body.contains("bot") || body.contains("anomaly");
                println!("[POST /html/] status={status} len={} results={result_count} captcha={has_captcha}",
                    body.len());

                // Show result-related CSS classes
                let mut classes: Vec<&str> = Vec::new();
                for part in body.split("class=\"") {
                    let cls = part.split('"').next().unwrap_or("");
                    if cls.contains("result") && !classes.contains(&cls) {
                        classes.push(cls);
                    }
                }
                println!("[POST /html/] result classes: {classes:?}");

                if has_captcha && result_count == 0 {
                    for line in body.lines() {
                        let l = line.trim();
                        if l.contains("bot") || l.contains("anomaly") {
                            println!("[CAPTCHA] {}", &l[..l.len().min(200)]);
                        }
                    }
                }
                // Save body for inspection
                std::fs::write("/tmp/ddg_html_response.html", &body).ok();
                println!("[POST /html/] saved to /tmp/ddg_html_response.html");
            }
            Err(e) => println!("[POST /html/] ERROR: {e}"),
        }

        // Also try lite endpoint
        println!("\n--- lite endpoint ---");
        let resp = agent
            .post("https://lite.duckduckgo.com/lite/")
            .set("User-Agent", ua)
            .set("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .set("Accept-Language", "en-US,en;q=0.5")
            .set("Origin", "https://lite.duckduckgo.com")
            .set("Referer", "https://lite.duckduckgo.com/lite/")
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(&format!("q={}", urlencoding::encode(query)));

        match resp {
            Ok(r) => {
                let status = r.status();
                let body = r.into_string().unwrap_or_default();
                let has_captcha = body.contains("bot") || body.contains("anomaly");

                // Count lite-style results (table rows with result-link class)
                let link_count = body.matches("result-link").count();
                let snippet_count = body.matches("result-snippet").count();
                println!("[POST /lite/] status={status} len={} links={link_count} snippets={snippet_count} captcha={has_captcha}",
                    body.len());

                let mut classes: Vec<&str> = Vec::new();
                for part in body.split("class=\"") {
                    let cls = part.split('"').next().unwrap_or("");
                    if cls.contains("result") && !classes.contains(&cls) {
                        classes.push(cls);
                    }
                }
                println!("[POST /lite/] result classes: {classes:?}");

                std::fs::write("/tmp/ddg_lite_response.html", &body).ok();
                println!("[POST /lite/] saved to /tmp/ddg_lite_response.html");
            }
            Err(e) => println!("[POST /lite/] ERROR: {e}"),
        }
    }

    /// Integration test: ddg_html_search should return results.
    /// May fail in environments where DDG rate-limits (returns captcha).
    /// Run manually: `cargo test -p skill-tools test_ddg_html_search_live -- --nocapture --ignored`
    #[test]
    #[ignore]
    fn test_ddg_html_search_live() {
        let agent = make_agent();
        let results = ddg_html_search(&agent, "rust programming language");
        println!("ddg_html_search returned {} results", results.len());
        for (i, r) in results.iter().enumerate() {
            println!("  [{i}] title={} url={}",
                r.get("title").and_then(|v| v.as_str()).unwrap_or("?"),
                r.get("url").and_then(|v| v.as_str()).unwrap_or("?"));
        }
        // Don't assert — DDG may captcha-block this environment.
    }

    /// Offline test: verify parsing of known DDG HTML structure.
    #[test]
    fn test_ddg_html_parsing() {
        let html = r#"
<div class="serp__results">
  <div class="result results_links results_links_deep web-result ">
    <div class="links_main links_deep result__body">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="https://rust-lang.org/">Rust Programming Language</a>
      </h2>
      <a class="result__snippet" href="https://rust-lang.org/">A fast, reliable <b>language</b>.</a>
    </div>
  </div>
  <div class="result results_links results_links_deep web-result ">
    <div class="links_main links_deep result__body">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="https://en.wikipedia.org/wiki/Rust_(programming_language)">Rust (programming language) - Wikipedia</a>
      </h2>
      <a class="result__snippet" href="https://en.wikipedia.org/wiki/Rust_(programming_language)">General-purpose language.</a>
    </div>
  </div>
  <div class="result results_links results_links_deep web-result ">
    <div class="links_main links_deep result__body">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="https://www.w3schools.com/rust/">Rust Tutorial</a>
      </h2>
      <a class="result__snippet" href="https://www.w3schools.com/rust/">Learn Rust with examples.</a>
    </div>
  </div>
</div>
"#;
        let results = parse_ddg_html(html);
        println!("Parsed {} results:", results.len());
        for (i, r) in results.iter().enumerate() {
            println!("  [{i}] title={:?} url={:?} snippet={:?}",
                r.get("title").and_then(|v| v.as_str()),
                r.get("url").and_then(|v| v.as_str()),
                r.get("snippet").and_then(|v| v.as_str()));
        }
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["url"], "https://rust-lang.org/");
        assert_eq!(results[0]["title"], "Rust Programming Language");
        assert_eq!(results[1]["url"], "https://en.wikipedia.org/wiki/Rust_(programming_language)");
        assert_eq!(results[2]["url"], "https://www.w3schools.com/rust/");
    }

    /// Offline test: DDG redirect URLs are properly unwrapped.
    #[test]
    fn test_ddg_redirect_unwrap() {
        let html = r#"
<div class="result results_links results_links_deep web-result ">
  <div class="links_main links_deep result__body">
    <h2 class="result__title">
      <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=abc">Example</a>
    </h2>
    <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage&rut=abc">A snippet.</a>
  </div>
</div>
"#;
        let results = parse_ddg_html(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["url"], "https://example.com/page");
    }
}
