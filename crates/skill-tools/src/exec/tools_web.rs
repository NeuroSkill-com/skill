// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Web tool handlers — `web_search`, `web_fetch`.

use serde_json::{Value, json};

use super::truncate::truncate_text;
use crate::search;
use crate::types::LlmToolConfig;

// ── web_search ────────────────────────────────────────────────────────────────

pub(crate) async fn exec_web_search(args: &Value, allowed_tools: &LlmToolConfig) -> Value {
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if query.is_empty() {
        return json!({ "ok": false, "tool": "web_search", "error": "missing query" });
    }

    let render = args.get("render").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let render_count = args.get("render_count").and_then(serde_json::Value::as_u64).unwrap_or(3).min(5) as usize;

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
                    if let Some(url_val) = obj.get("url").and_then(|v| v.as_str()).map(std::string::ToString::to_string) {
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
        if render && !results.is_empty() {
            let urls: Vec<String> = results.iter()
                .take(render_count)
                .filter_map(|r| r.get("url").and_then(|v| v.as_str()).map(std::string::ToString::to_string))
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
            build_compact_search_response(&query, &results, render, &compression)
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

/// Build a compact text representation of search results for context compression.
fn build_compact_search_response(
    query: &str,
    results: &[Value],
    render: bool,
    compression: &crate::types::ToolContextCompression,
) -> Value {
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

    // Build sources array for the UI.
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
}

// ── web_fetch ─────────────────────────────────────────────────────────────────

pub(crate) async fn exec_web_fetch(args: &Value, allowed_tools: &LlmToolConfig) -> Value {
    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return json!({ "ok": false, "tool": "web_fetch", "error": "url must start with http:// or https://" });
    }

    let render = args.get("render").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let max_content = allowed_tools.context_compression.effective_max_result_chars().max(1000);

    if render {
        exec_web_fetch_render(args, &url, max_content).await
    } else {
        exec_web_fetch_plain(&url, max_content).await
    }
}

/// Headless browser rendering path for web_fetch.
async fn exec_web_fetch_render(args: &Value, url: &str, max_content: usize) -> Value {
    let wait_ms = args.get("wait_ms").and_then(serde_json::Value::as_u64).unwrap_or(2000);
    let selector = args.get("selector").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
    let eval_js = args.get("eval_js").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
    let url_for_fetch = url.to_string();
    let url_owned = url.to_string();

    let mut result = tokio::task::spawn_blocking(move || {
        search::headless_fetch_url(&url_for_fetch, wait_ms, selector.as_deref(), eval_js.as_deref())
    }).await.unwrap_or_else(|e| json!({ "ok": false, "tool": "web_fetch", "url": url_owned, "error": e.to_string() }));

    // If headless browser is unavailable, fall back to plain HTTP fetch.
    let should_fallback = result.get("fallback").and_then(serde_json::Value::as_bool).unwrap_or(false);
    if should_fallback {
        crate::tool_log!("tool:web_fetch", "[render] headless unavailable, falling back to HTTP fetch");
        let url_fallback = url.to_string();
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
    if let Some(content) = result.get("content").and_then(|c| c.as_str()).map(std::string::ToString::to_string) {
        if content.len() > max_content {
            result["content"] = json!(truncate_text(&content, max_content));
            result["truncated"] = json!(true);
        }
    }
    result
}

/// Plain HTTP fetch path for web_fetch.
async fn exec_web_fetch_plain(url: &str, max_content: usize) -> Value {
    let url_for_fetch = url.to_string();
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
