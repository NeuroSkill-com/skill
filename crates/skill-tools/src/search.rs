// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Web search backends (DuckDuckGo HTML, Brave API, SearXNG) and headless fetch.

use serde_json::{json, Value};

use super::exec::truncate_text;

// ── DuckDuckGo search helpers ─────────────────────────────────────────────────

/// Strip HTML tags from a string (simple regex-free approach).
pub(crate) fn strip_html_tags(s: &str) -> String {
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
pub(crate) fn random_ua() -> &'static str {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let idx = COUNTER.fetch_add(1, Ordering::Relaxed) % BROWSER_USER_AGENTS.len();
    BROWSER_USER_AGENTS[idx]
}

/// Build a ureq agent with browser-like defaults (redirects, timeouts).
pub(crate) fn browser_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(3))
        .timeout_read(std::time::Duration::from_secs(10))
        .redirects(5)
        .build()
}

/// Apply standard browser headers to a ureq request.
///
/// Many sites (AccuWeather, weather.com, etc.) return 403 if the request
/// is missing `Accept`, `Accept-Language`, or other headers that real
/// browsers send.
pub(crate) fn set_browser_headers(req: ureq::Request) -> ureq::Request {
    req.set("User-Agent", random_ua())
       .set("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
       .set("Accept-Language", "en-US,en;q=0.5")
       .set("Accept-Encoding", "identity")
       .set("DNT", "1")
       .set("Connection", "keep-alive")
       .set("Upgrade-Insecure-Requests", "1")
}

/// Fallback search: scrape DuckDuckGo HTML lite page.
///
/// Mimics a real browser form submission: the lite page has a `<form>` that
/// POSTs `q=<query>&b=` to `/html/`.  The `Origin` and `Referer` headers are
/// required to pass bot detection.
pub(crate) fn ddg_html_search(agent: &ureq::Agent, query: &str) -> Vec<Value> {
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
pub(crate) fn parse_ddg_html(body: &str) -> Vec<Value> {
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

pub(crate) fn extract_attr_value(html: &str, marker: &str, attr: &str) -> Option<String> {
    let marker_pos = html.find(marker)?;
    let after_marker = &html[marker_pos..];
    let attr_pos = after_marker.find(attr)?;
    let value_start = attr_pos + attr.len();
    let after_attr = &after_marker[value_start..];
    let end = after_attr.find('"')?;
    Some(after_attr[..end].to_string())
}

pub(crate) fn extract_tag_content(html: &str, marker: &str) -> Option<String> {
    let marker_pos = html.find(marker)?;
    let after_marker = &html[marker_pos..];
    let tag_close = after_marker.find('>')?;
    let content_start = tag_close + 1;
    let after_tag = &after_marker[content_start..];
    let end = after_tag.find("</").unwrap_or(after_tag.len().min(1000));
    Some(after_tag[..end].to_string())
}

pub(crate) fn extract_ddg_redirect_url(url: &str) -> Option<String> {
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
pub(crate) fn brave_search(agent: &ureq::Agent, api_key: &str, query: &str) -> Vec<Value> {
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
pub(crate) fn searxng_search(agent: &ureq::Agent, base_url: &str, query: &str) -> Vec<Value> {
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

// ── Headless browser helpers ──────────────────────────────────────────────────

/// Fetch a single URL using the headless browser, returning a JSON result.
///
/// Launches a temporary browser session, navigates to the URL, waits for
/// the page to load, and extracts the rendered text content.  Supports
/// optional CSS-selector waiting, custom JS evaluation, and configurable
/// wait time.
///
/// On macOS inside a Tauri app, the headless browser may fail because tao
/// requires the event loop on the main thread.  In that case, this function
/// returns an error JSON so the caller can fall back to plain HTTP fetch.
pub(crate) fn headless_fetch_url(
    url: &str,
    wait_ms: u64,
    selector: Option<&str>,
    eval_js: Option<&str>,
) -> Value {
    use skill_headless::{Browser, BrowserConfig, Command};

    // If the standalone browser is unavailable, try the external renderer
    // (Tauri's webview) first, then fall back to plain HTTP.
    if Browser::is_unavailable() {
        // Try external renderer.
        if let Some(Ok(text)) = skill_headless::external_fetch_page(url, wait_ms.max(4000)) {
            if !text.trim().is_empty() {
                let text = truncate_text(&text, 12_000);
                return json!({
                    "ok": true,
                    "tool": "web_fetch",
                    "url": url,
                    "mode": "external_renderer",
                    "content": text,
                    "truncated": text.len() >= 12_000,
                });
            }
        }

        // Fall back to plain HTTP.
        tool_log!("tool:web_fetch", "[render] external renderer failed/empty, falling back to HTTP for {}", url);
        let agent = browser_agent();
        return match set_browser_headers(agent.get(url)).call() {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.into_string().unwrap_or_default();
                let text = strip_html_tags(&body);
                let cleaned: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                let content = truncate_text(&cleaned, 12_000);
                json!({
                    "ok": true,
                    "tool": "web_fetch",
                    "url": url,
                    "status": status,
                    "mode": "http_fallback",
                    "content": content,
                    "truncated": cleaned.len() > 12_000,
                })
            }
            Err(e) => json!({ "ok": false, "tool": "web_fetch", "url": url, "error": e.to_string() }),
        };
    }

    let browser = match Browser::launch(BrowserConfig {
        user_agent: Some(random_ua().to_string()),
        timeout: std::time::Duration::from_secs(30),
        ..Default::default()
    }) {
        Ok(b) => b,
        Err(e) => return json!({ "ok": false, "tool": "web_fetch", "url": url, "error": format!("headless launch failed: {e}"), "fallback": true }),
    };

    // Navigate to the URL.
    if let Err(e) = browser.send(Command::Navigate { url: url.to_string() }) {
        let _ = browser.send(Command::Close);
        return json!({ "ok": false, "tool": "web_fetch", "url": url, "error": format!("navigate failed: {e}") });
    }

    // Wait for page to load — either a selector or a fixed delay.
    if let Some(sel) = selector {
        let resp = browser.send(Command::WaitForSelector {
            selector: sel.to_string(),
            timeout_ms: wait_ms.max(5000),
        });
        if let Ok(r) = &resp {
            if let Some(text) = r.as_text() {
                if text == "timeout" {
                    tool_log!("tool:web_fetch", "selector '{}' not found within timeout on {}", sel, url);
                }
            }
        }
    } else {
        std::thread::sleep(std::time::Duration::from_millis(wait_ms));
    }

    // If custom JS evaluation is requested, run it and return its result.
    if let Some(js) = eval_js {
        let js_result = match browser.send(Command::EvalJs { script: js.to_string() }) {
            Ok(r) => r.as_text().unwrap_or("null").to_string(),
            Err(e) => format!("eval error: {e}"),
        };
        let _ = browser.send(Command::Close);
        return json!({
            "ok": true,
            "tool": "web_fetch",
            "url": url,
            "mode": "headless",
            "eval_result": truncate_text(&js_result, 12_000),
            "truncated": js_result.chars().count() > 12_000,
        });
    }

    // Get the page title.
    let title = browser.send(Command::GetTitle)
        .ok()
        .and_then(|r| r.as_text().map(|s| s.to_string()))
        .unwrap_or_default();

    // Get the current URL (may differ after redirects).
    let final_url = browser.send(Command::GetUrl)
        .ok()
        .and_then(|r| r.as_text().map(|s| s.to_string()))
        .unwrap_or_else(|| url.to_string());

    // Extract visible text content via JS.
    let text_script = r#"
        (function() {
            function extractText(node) {
                if (!node) return '';
                var tag = (node.tagName || '').toLowerCase();
                if (tag === 'script' || tag === 'style' || tag === 'noscript' || tag === 'svg') return '';
                var style = node.nodeType === 1 ? getComputedStyle(node) : null;
                if (style && (style.display === 'none' || style.visibility === 'hidden')) return '';
                if (node.nodeType === 3) return node.textContent;
                var parts = [];
                for (var i = 0; i < node.childNodes.length; i++) {
                    parts.push(extractText(node.childNodes[i]));
                }
                var text = parts.join(' ');
                var block = style && (style.display === 'block' || style.display === 'flex' ||
                    style.display === 'grid' || style.display === 'table' ||
                    tag === 'br' || tag === 'p' || tag === 'div' || tag === 'li' ||
                    tag === 'h1' || tag === 'h2' || tag === 'h3' || tag === 'h4' ||
                    tag === 'h5' || tag === 'h6' || tag === 'tr');
                return block ? '\n' + text + '\n' : text;
            }
            var raw = extractText(document.body || document.documentElement);
            return raw.replace(/[ \t]+/g, ' ').replace(/\n{3,}/g, '\n\n').trim();
        })()
    "#;

    let body = match browser.send(Command::EvalJs { script: text_script.to_string() }) {
        Ok(r) => r.as_text().unwrap_or("").to_string(),
        Err(e) => {
            let _ = browser.send(Command::Close);
            return json!({ "ok": false, "tool": "web_fetch", "url": url, "error": format!("content extraction failed: {e}") });
        }
    };

    let _ = browser.send(Command::Close);

    json!({
        "ok": true,
        "tool": "web_fetch",
        "url": final_url,
        "mode": "headless",
        "title": title,
        "content": truncate_text(&body, 12_000),
        "truncated": body.chars().count() > 12_000,
    })
}

/// Render multiple URLs in a single headless browser session and return the
/// extracted visible text for each URL.  Used by `web_search` with
/// `render=true` to visit top search result pages.
///
/// Returns `None` if the browser fails to launch.  Individual page errors
/// result in an error string at that index instead of content.
pub(crate) fn headless_render_urls(urls: &[String]) -> Option<Vec<String>> {
    use skill_headless::{Browser, BrowserConfig, Command};

    // If the standalone browser is unavailable, try the external renderer
    // (Tauri's webview) before returning None (which triggers HTTP fallback).
    if Browser::is_unavailable() {
        let has_ext = Browser::has_external_renderer();
        let mut results = Vec::with_capacity(urls.len());

        for url in urls {
            let mut text: Option<String> = None;

            // Try external renderer first (Tauri webview).
            if has_ext {
                tool_log!("tool:web_search", "[render:external] visiting {}", url);
                match skill_headless::external_fetch_page(url, 4000) {
                    Some(Ok(t)) if !t.trim().is_empty() => {
                        text = Some(t);
                    }
                    Some(Ok(_)) => {
                        tool_log!("tool:web_search", "[render:external] empty result for {}, trying HTTP", url);
                    }
                    Some(Err(e)) => {
                        tool_log!("tool:web_search", "[render:external] failed for {}: {}, trying HTTP", url, e);
                    }
                    None => {}
                }
            }

            // Fall back to plain HTTP fetch + HTML stripping.
            if text.is_none() {
                tool_log!("tool:web_search", "[render:http] fetching {}", url);
                let agent = browser_agent();
                if let Ok(resp) = set_browser_headers(agent.get(url)).call() {
                    let body = resp.into_string().unwrap_or_default();
                    let stripped = strip_html_tags(&body);
                    let cleaned: String = stripped.split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !cleaned.trim().is_empty() {
                        text = Some(cleaned);
                    }
                }
            }

            results.push(truncate_text(text.as_deref().unwrap_or(""), 2_000));
        }

        return Some(results);
    }

    let browser = match Browser::launch(BrowserConfig {
        user_agent: Some(random_ua().to_string()),
        timeout: std::time::Duration::from_secs(20),
        ..Default::default()
    }) {
        Ok(b) => b,
        Err(e) => {
            tool_log!("tool:web_search", "[render] headless launch failed: {}", e);
            return None;
        }
    };

    let text_script = r#"
        (function() {
            function extractText(node) {
                if (!node) return '';
                var tag = (node.tagName || '').toLowerCase();
                if (tag === 'script' || tag === 'style' || tag === 'noscript' || tag === 'svg') return '';
                var style = node.nodeType === 1 ? getComputedStyle(node) : null;
                if (style && (style.display === 'none' || style.visibility === 'hidden')) return '';
                if (node.nodeType === 3) return node.textContent;
                var parts = [];
                for (var i = 0; i < node.childNodes.length; i++) {
                    parts.push(extractText(node.childNodes[i]));
                }
                var text = parts.join(' ');
                var block = style && (style.display === 'block' || style.display === 'flex' ||
                    style.display === 'grid' || style.display === 'table' ||
                    tag === 'br' || tag === 'p' || tag === 'div' || tag === 'li' ||
                    tag === 'h1' || tag === 'h2' || tag === 'h3' || tag === 'h4' ||
                    tag === 'h5' || tag === 'h6' || tag === 'tr');
                return block ? '\n' + text + '\n' : text;
            }
            var raw = extractText(document.body || document.documentElement);
            return raw.replace(/[ \t]+/g, ' ').replace(/\n{3,}/g, '\n\n').trim();
        })()
    "#;

    let mut results = Vec::with_capacity(urls.len());

    for url in urls {
        tool_log!("tool:web_search", "[render] visiting {}", url);

        if let Err(e) = browser.send(Command::Navigate { url: url.clone() }) {
            results.push(format!("[error: navigate failed: {e}]"));
            continue;
        }

        // Wait for the page to settle.
        std::thread::sleep(std::time::Duration::from_millis(2500));

        match browser.send(Command::EvalJs { script: text_script.to_string() }) {
            Ok(r) => {
                let text = r.as_text().unwrap_or("").to_string();
                results.push(truncate_text(&text, 2_000));
            }
            Err(e) => {
                results.push(format!("[error: content extraction failed: {e}]"));
            }
        }
    }

    let _ = browser.send(Command::Close);
    Some(results)
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
