// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Built-in tool execution — the runtime implementation of each tool.

use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{SecondsFormat, Utc, Local};

use crate::parse::ToolCall;
use crate::types::LlmToolConfig;
use crate::defs::is_builtin_tool_enabled;
use crate::search;

// ── Public execution entry point ──────────────────────────────────────────────

/// Execute a single built-in tool call and return the JSON result.
pub async fn execute_builtin_tool_call(call: &ToolCall, allowed_tools: &LlmToolConfig, scripts_dir: &std::path::Path) -> Value {
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| json!({}));
    let tool_name = &call.function.name;

    // Distinguish "unknown tool" from "known but disabled".
    // When the name matches a Skill API sub-command or neuroskill alias,
    // give a precise hint so the model self-corrects in one step.
    if !crate::defs::is_known_builtin_tool(tool_name) {
        if let Some(cmd) = crate::defs::resolve_skill_alias(tool_name) {
            tool_log!("tool", "[blocked] tool={} reason=skill alias, should be skill({})", tool_name, cmd);
            return json!({
                "ok": false,
                "tool": call.function.name,
                "error": format!(
                    "\"{}\" is not a top-level tool — it maps to the \"skill\" tool. \
                     Call the \"skill\" tool with {{\"command\": \"{}\"}} instead.",
                    tool_name, cmd
                )
            });
        }
        tool_log!("tool", "[blocked] tool={} reason=unsupported tool", tool_name);
        return json!({ "ok": false, "tool": call.function.name, "error": format!("unsupported tool \"{}\". Use one of the available tools listed in the system prompt.", tool_name) });
    }
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

            let render = args.get("render").and_then(|v| v.as_bool()).unwrap_or(false);
            let render_count = args.get("render_count").and_then(|v| v.as_u64()).unwrap_or(3).min(5) as usize;

            let provider = allowed_tools.web_search_provider.clone();
            let compression = allowed_tools.context_compression.clone();
            tokio::task::spawn_blocking(move || {
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(std::time::Duration::from_secs(5))
                    .timeout_read(std::time::Duration::from_secs(10))
                    .build();

                // Try the configured backend first.
                let mut results = match provider.backend.as_str() {
                    "brave" if !provider.brave_api_key.is_empty() => {
                        let r = search::brave_search(&agent, &provider.brave_api_key, &query);
                        if r.is_empty() { search::ddg_html_search(&agent, &query) } else { r }
                    }
                    "searxng" if !provider.searxng_url.is_empty() => {
                        let r = search::searxng_search(&agent, &provider.searxng_url, &query);
                        if r.is_empty() { search::ddg_html_search(&agent, &query) } else { r }
                    }
                    _ => search::ddg_html_search(&agent, &query),
                };

                // Cap results based on compression settings.
                let max_results = compression.effective_max_search_results();
                results.truncate(max_results);

                // Compact each result when compression is active.
                if compression.should_truncate_urls() {
                    let max_url_len = skill_constants::TOOL_WEB_SEARCH_MAX_URL_LEN;
                    for r in results.iter_mut() {
                        if let Some(obj) = r.as_object_mut() {
                            if let Some(url_val) = obj.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()) {
                                if url_val.len() > max_url_len {
                                    let truncated_url = format!("{}...", &url_val[..max_url_len]);
                                    obj.insert("url".to_string(), json!(truncated_url));
                                }
                            }
                            // Remove empty/useless snippets to save tokens.
                            if let Some(snippet) = obj.get("snippet").and_then(|v| v.as_str()) {
                                if snippet.trim().len() < 10 {
                                    obj.remove("snippet");
                                }
                            }
                        }
                    }
                }

                // If render=true, visit top N result pages and append their
                // rendered text content to each result.
                //
                // First try the headless browser; if it's unavailable (e.g.
                // macOS inside Tauri where tao can't create a second event
                // loop), fall back to plain HTTP fetch + HTML tag stripping.
                // Fetch top result pages in parallel and attach rendered text.
                // Uses headless_render_urls (standalone browser) if available,
                // otherwise fetch_urls_parallel (external renderer → HTTP).
                if render && !results.is_empty() {
                    let urls: Vec<String> = results.iter()
                        .take(render_count)
                        .filter_map(|r| r.get("url").and_then(|v| v.as_str()).map(|s| s.to_string()))
                        .collect();

                    let rendered = search::headless_render_urls(&urls)
                        .unwrap_or_else(|| search::fetch_urls_parallel(&urls));

                    for (i, content) in rendered.into_iter().enumerate() {
                        if i < results.len() {
                            if let Some(obj) = results[i].as_object_mut() {
                                obj.insert("rendered_text".to_string(), json!(content));
                            }
                        }
                    }
                }

                if results.is_empty() {
                    json!({ "ok": true, "tool": "web_search", "query": query, "results": [], "note": "no results found" })
                } else if compression.should_compress_old_results() {
                    // ── Compact text format ─────────────────────────────
                    // Instead of returning verbose JSON (which eats context),
                    // emit a concise text list the model can parse instantly.
                    //
                    // When rendered content is available, score each result
                    // by text quality and only include the best 1–2 to
                    // avoid wasting context on garbage/empty pages.
                    let max_chars = compression.effective_max_search_result_chars();

                    // Score rendered results to find the best ones.
                    let mut scored: Vec<(usize, u32)> = results.iter().enumerate()
                        .map(|(i, r)| {
                            let text = r.get("rendered_text").and_then(|t| t.as_str()).unwrap_or("");
                            (i, search::score_rendered_text(text))
                        })
                        .collect();
                    scored.sort_by(|a, b| b.1.cmp(&a.1));

                    // Indices of the best 2 rendered results (score > 30).
                    let best_rendered: std::collections::HashSet<usize> = scored.iter()
                        .filter(|(_, s)| *s > 30)
                        .take(2)
                        .map(|(i, _)| *i)
                        .collect();

                    // Build sources array for the UI (not included in compact
                    // text for the LLM, but stored in the result JSON for the
                    // tool card to display).
                    let sources: Vec<Value> = results.iter().enumerate()
                        .map(|(i, r)| {
                            let url = r.get("url").and_then(|u| u.as_str()).unwrap_or("");
                            let title = r.get("title").and_then(|t| t.as_str()).unwrap_or("");
                            let rendered = r.get("rendered_text").and_then(|t| t.as_str()).unwrap_or("");
                            let score = scored.iter().find(|(idx,_)| *idx == i).map(|(_,s)| *s).unwrap_or(0);
                            let domain = url.split('/').nth(2).unwrap_or(url);
                            json!({
                                "domain": domain,
                                "url": url,
                                "title": title,
                                "score": score,
                                "best": best_rendered.contains(&i),
                                "chars": rendered.len(),
                                "preview": truncate_text(rendered, 300),
                            })
                        })
                        .collect();

                    let mut compact = format!("web_search query=\"{}\" results={}{}:\n",
                        query, results.len(), if render { " rendered=true" } else { "" });

                    for (i, r) in results.iter().enumerate() {
                        let title = r.get("title").and_then(|t| t.as_str()).unwrap_or("?");
                        let url   = r.get("url").and_then(|u| u.as_str()).unwrap_or("");
                        let snip  = r.get("snippet").and_then(|s| s.as_str()).unwrap_or("");

                        let mut entry = format!("{}. {}\n   {}\n", i + 1, title, url);
                        if !snip.is_empty() && !best_rendered.contains(&i) {
                            entry.push_str(&format!("   {}\n", truncate_text(snip, 150)));
                        }

                        // Only include rendered text for the best results.
                        if best_rendered.contains(&i) {
                            if let Some(rendered) = r.get("rendered_text").and_then(|t| t.as_str()) {
                                if !rendered.is_empty() {
                                    // Give the best result more space.
                                    let max_rendered = if best_rendered.len() == 1 {
                                        (max_chars * 2 / 3).min(1500)
                                    } else {
                                        (max_chars / 3).min(800)
                                    };
                                    entry.push_str(&format!("   --- page content ---\n   {}\n",
                                        truncate_text(rendered, max_rendered)));
                                }
                            }
                        }

                        if compact.len() + entry.len() > max_chars {
                            compact.push_str("...(remaining results omitted for context)\n");
                            break;
                        }
                        compact.push_str(&entry);
                    }

                    if !render {
                        compact.push_str("Note: only links returned. Use web_fetch to read a page, or re-call with render=true.\n");
                    }

                    let mut result = json!({ "ok": true, "tool": "web_search", "compact": compact });
                    if render && !sources.is_empty() {
                        result["sources"] = json!(sources);
                    }
                    result
                } else {
                    // Compression off — return full JSON.
                    let mut result = json!({ "ok": true, "tool": "web_search", "query": query, "results": results });
                    if render {
                        result["rendered"] = json!(true);
                    } else {
                        result["hint"] = json!("These are search result links only. To get actual content, use web_fetch on a URL or re-call web_search with render=true.");
                    }
                    result
                }
            }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "web_search", "error": e.to_string() }))
        }

        "web_fetch" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return json!({ "ok": false, "tool": "web_fetch", "error": "url must start with http:// or https://" });
            }

            let render = args.get("render").and_then(|v| v.as_bool()).unwrap_or(false);
            let max_content = allowed_tools.context_compression.effective_max_result_chars().max(1000);

            if render {
                // ── Headless browser rendering path ──────────────────
                let wait_ms = args.get("wait_ms").and_then(|v| v.as_u64()).unwrap_or(2000);
                let selector = args.get("selector").and_then(|v| v.as_str()).map(|s| s.to_string());
                let eval_js = args.get("eval_js").and_then(|v| v.as_str()).map(|s| s.to_string());
                let url_for_fetch = url.clone();

                let mut result = tokio::task::spawn_blocking(move || {
                    search::headless_fetch_url(&url_for_fetch, wait_ms, selector.as_deref(), eval_js.as_deref())
                }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "web_fetch", "url": url, "error": e.to_string() }));

                // If headless browser is unavailable, fall back to plain HTTP fetch.
                let should_fallback = result.get("fallback").and_then(|f| f.as_bool()).unwrap_or(false);
                if should_fallback {
                    tool_log!("tool:web_fetch", "[render] headless unavailable, falling back to HTTP fetch");
                    let url_fallback = url.clone();
                    result = tokio::task::spawn_blocking(move || {
                        let agent = search::browser_agent();
                        match search::set_browser_headers(agent.get(&url_fallback)).call() {
                            Ok(r) => {
                                let status = r.status();
                                let body = r.into_string().unwrap_or_default();
                                let text = search::strip_html_tags(&body);
                                let cleaned: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                                json!({
                                    "ok": true,
                                    "tool": "web_fetch",
                                    "url": url_fallback,
                                    "status": status,
                                    "mode": "http_fallback",
                                    "content": truncate_text(&cleaned, max_content),
                                    "truncated": cleaned.len() > max_content,
                                })
                            }
                            Err(e) => json!({ "ok": false, "tool": "web_fetch", "url": url_fallback, "error": e.to_string() }),
                        }
                    }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "web_fetch", "url": url, "error": e.to_string() }));
                }

                // Cap rendered content to the configured limit.
                if let Some(content) = result.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()) {
                    if content.len() > max_content {
                        result["content"] = json!(truncate_text(&content, max_content));
                        result["truncated"] = json!(true);
                    }
                }
                result
            } else {
                // ── Plain HTTP fetch path (original) ─────────────────
                let url_for_fetch = url.clone();
                tokio::task::spawn_blocking(move || {
                    let agent = search::browser_agent();
                    let resp = search::set_browser_headers(agent.get(&url_for_fetch)).call();

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
                                "content": truncate_text(&body, max_content),
                                "truncated": body.chars().count() > max_content,
                            })
                        }
                        Err(e) => json!({ "ok": false, "tool": "web_fetch", "url": url_for_fetch, "error": e.to_string() }),
                    }
                }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "web_fetch", "url": url, "error": e.to_string() }))
            }
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

                            for (j, line) in all_lines.iter().enumerate().take(ctx_end).skip(ctx_start) {
                                let marker = if j == i { ">" } else { " " };
                                matches.push(format!("{}{:>5}: {}", marker, j + 1, line));
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

        "skill" => {
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


#[cfg(test)]
mod tests {
    use super::*;

    // ── truncate_text ─────────────────────────────────────────────────

    #[test]
    fn truncate_text_within_limit() {
        assert_eq!(truncate_text("hello", 10), "hello");
    }

    #[test]
    fn truncate_text_at_limit() {
        assert_eq!(truncate_text("hello", 5), "hello");
    }

    #[test]
    fn truncate_text_over_limit() {
        assert_eq!(truncate_text("hello world", 5), "hello");
    }

    #[test]
    fn truncate_text_empty() {
        assert_eq!(truncate_text("", 5), "");
    }

    #[test]
    fn truncate_text_unicode() {
        // Each emoji is one char
        assert_eq!(truncate_text("🧠🔬🧬🧪", 2), "🧠🔬");
    }

    // ── truncate_tool_output (tail) ───────────────────────────────────

    #[test]
    fn truncate_output_no_truncation() {
        let out = truncate_tool_output("line1\nline2\nline3", 10, 1000);
        assert!(!out.was_truncated);
        assert_eq!(out.total_lines, 3);
        assert_eq!(out.output_lines, 3);
        assert_eq!(out.text, "line1\nline2\nline3");
    }

    #[test]
    fn truncate_output_by_lines() {
        let content = (0..100).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        let out = truncate_tool_output(&content, 5, 100_000);
        assert!(out.was_truncated);
        assert_eq!(out.total_lines, 100);
        assert_eq!(out.output_lines, 5);
        // Should keep the LAST 5 lines
        assert!(out.text.contains("line99"));
        assert!(out.text.contains("line95"));
        assert!(!out.text.contains("line0"));
    }

    #[test]
    fn truncate_output_by_bytes() {
        let content = (0..100).map(|i| format!("line{i:03}")).collect::<Vec<_>>().join("\n");
        let out = truncate_tool_output(&content, 1000, 50);
        assert!(out.was_truncated);
        // Should have truncated to fit under 50 bytes
        assert!(out.text.len() <= 50);
    }

    // ── truncate_tool_output_head ─────────────────────────────────────

    #[test]
    fn truncate_head_no_truncation() {
        let out = truncate_tool_output_head("a\nb\nc", 10, 1000);
        assert!(!out.was_truncated);
        assert_eq!(out.text, "a\nb\nc");
    }

    #[test]
    fn truncate_head_by_lines() {
        let content = (0..100).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        let out = truncate_tool_output_head(&content, 5, 100_000);
        assert!(out.was_truncated);
        assert_eq!(out.output_lines, 5);
        // Should keep the FIRST 5 lines
        assert!(out.text.contains("line0"));
        assert!(out.text.contains("line4"));
        assert!(!out.text.contains("line99"));
    }

    #[test]
    fn truncate_head_by_bytes() {
        let content = (0..100).map(|i| format!("line{i:03}")).collect::<Vec<_>>().join("\n");
        let out = truncate_tool_output_head(&content, 1000, 50);
        assert!(out.was_truncated);
        assert!(out.text.len() <= 50);
        assert!(out.text.starts_with("line000"));
    }

    // ── resolve_tool_path ─────────────────────────────────────────────

    #[test]
    fn resolve_absolute_path() {
        let p = resolve_tool_path("/tmp/test.txt");
        assert_eq!(p, std::path::PathBuf::from("/tmp/test.txt"));
    }

    #[test]
    fn resolve_tilde() {
        let p = resolve_tool_path("~");
        assert!(p.is_absolute());
        // Should be the home directory
        assert_eq!(p, dirs::home_dir().unwrap());
    }

    #[test]
    fn resolve_tilde_slash() {
        let p = resolve_tool_path("~/Documents/file.txt");
        assert!(p.is_absolute());
        assert!(p.ends_with("Documents/file.txt"));
    }

    #[test]
    fn resolve_relative_path() {
        let p = resolve_tool_path("some/relative/path");
        assert!(p.is_absolute()); // Should be resolved to absolute via home
    }

    // ── check_bash_safety ─────────────────────────────────────────────

    #[test]
    fn bash_safety_safe_command() {
        assert!(check_bash_safety("ls -la").is_none());
        assert!(check_bash_safety("echo hello").is_none());
        assert!(check_bash_safety("cat file.txt").is_none());
        assert!(check_bash_safety("grep pattern file").is_none());
    }

    #[test]
    fn bash_safety_dangerous_rm() {
        assert!(check_bash_safety("rm -rf /").is_some());
        assert!(check_bash_safety("rm file.txt").is_some());
    }

    #[test]
    fn bash_safety_dangerous_sudo() {
        assert!(check_bash_safety("sudo apt install").is_some());
    }

    #[test]
    fn bash_safety_dangerous_dd() {
        assert!(check_bash_safety("dd if=/dev/zero of=/dev/sda").is_some());
    }

    #[test]
    fn bash_safety_dangerous_shutdown() {
        assert!(check_bash_safety("shutdown -h now").is_some());
        assert!(check_bash_safety("reboot").is_some());
    }

    #[test]
    fn bash_safety_case_insensitive() {
        assert!(check_bash_safety("SUDO apt install").is_some());
        assert!(check_bash_safety("Rm -rf /").is_some());
    }

    // ── check_path_safety ─────────────────────────────────────────────

    #[test]
    fn path_safety_safe() {
        assert!(check_path_safety(std::path::Path::new("/home/user/file.txt")).is_none());
        assert!(check_path_safety(std::path::Path::new("/tmp/test")).is_none());
    }

    #[test]
    fn path_safety_sensitive() {
        assert!(check_path_safety(std::path::Path::new("/etc/passwd")).is_some());
        assert!(check_path_safety(std::path::Path::new("/boot/vmlinuz")).is_some());
        assert!(check_path_safety(std::path::Path::new("/usr/bin/ls")).is_some());
        assert!(check_path_safety(std::path::Path::new("/sys/class")).is_some());
    }

    // ── format_utc_offset ─────────────────────────────────────────────

    #[test]
    fn utc_offset_positive() {
        assert_eq!(format_utc_offset(3600), "+01:00");
        assert_eq!(format_utc_offset(19800), "+05:30"); // India
    }

    #[test]
    fn utc_offset_negative() {
        assert_eq!(format_utc_offset(-18000), "-05:00"); // EST
        assert_eq!(format_utc_offset(-28800), "-08:00"); // PST
    }

    #[test]
    fn utc_offset_zero() {
        assert_eq!(format_utc_offset(0), "+00:00");
    }
}
