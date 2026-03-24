// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Filesystem tool handlers — `read_file`, `write_file`, `edit_file`, `search_output`.

use serde_json::{Value, json};

use super::helpers::resolve_tool_path;
use super::safety::{check_path_safety, request_tool_approval};
use super::truncate::truncate_tool_output_head;

// ── read_file ─────────────────────────────────────────────────────────────────

pub(crate) async fn exec_read_file(args: &Value) -> Value {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if path.is_empty() {
        return json!({ "ok": false, "tool": "read_file", "error": "missing path" });
    }
    let offset = args.get("offset").and_then(serde_json::Value::as_u64).map(|v| v as usize);
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).map(|v| v as usize);

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

// ── write_file ────────────────────────────────────────────────────────────────

pub(crate) async fn exec_write_file(args: &Value) -> Value {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if path.is_empty() {
        return json!({ "ok": false, "tool": "write_file", "error": "missing path" });
    }

    // Safety check: require approval for sensitive paths
    let resolved_check = resolve_tool_path(&path);
    if let Some(reason) = check_path_safety(&resolved_check) {
        crate::tool_log!("tool:write_file", "[safety] approval required: {}", reason);
        let detail = format!("Write to: {}", resolved_check.display());
        let approved = request_tool_approval("write_file", &reason, &detail).await;
        if !approved {
            crate::tool_log!("tool:write_file", "[safety] user denied write");
            return json!({ "ok": false, "tool": "write_file", "error": "operation denied by user" });
        }
        crate::tool_log!("tool:write_file", "[safety] user approved write");
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

// ── edit_file ─────────────────────────────────────────────────────────────────

pub(crate) async fn exec_edit_file(args: &Value) -> Value {
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
        crate::tool_log!("tool:edit_file", "[safety] approval required: {}", reason);
        let detail = format!("Edit: {}", resolved_check.display());
        let approved = request_tool_approval("edit_file", &reason, &detail).await;
        if !approved {
            crate::tool_log!("tool:edit_file", "[safety] user denied edit");
            return json!({ "ok": false, "tool": "edit_file", "error": "operation denied by user" });
        }
        crate::tool_log!("tool:edit_file", "[safety] user approved edit");
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
                "error": "no changes made \u{2014} the replacement produced identical content."
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

// ── search_output ─────────────────────────────────────────────────────────────

pub(crate) async fn exec_search_output(args: &Value) -> Value {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if path.is_empty() {
        return json!({ "ok": false, "tool": "search_output", "error": "missing path" });
    }
    let pattern = args.get("pattern").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
    let context_lines = args.get("context_lines").and_then(serde_json::Value::as_u64).unwrap_or(2) as usize;
    let head_n = args.get("head").and_then(serde_json::Value::as_u64).map(|v| v as usize);
    let tail_n = args.get("tail").and_then(serde_json::Value::as_u64).map(|v| v as usize);
    let line_start = args.get("line_start").and_then(serde_json::Value::as_u64).map(|v| v as usize);
    let line_end = args.get("line_end").and_then(serde_json::Value::as_u64).map(|v| v as usize);
    let max_matches = args.get("max_matches").and_then(serde_json::Value::as_u64).unwrap_or(50) as usize;

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
            return exec_search_output_regex(pat, &all_lines, total_lines, context_lines, max_matches);
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

/// Regex search mode for `search_output`.
fn exec_search_output_regex(
    pat: &str,
    all_lines: &[&str],
    total_lines: usize,
    context_lines: usize,
    max_matches: usize,
) -> Value {
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

    json!({
        "ok": true, "tool": "search_output",
        "mode": "regex", "pattern": pat,
        "total_lines": total_lines,
        "matches": total_matches,
        "output": matches.join("\n"),
    })
}
