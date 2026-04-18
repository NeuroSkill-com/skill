// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! macOS-only external renderer for the headless browser subsystem.
//!
//! On macOS, tao cannot create a second event loop — this module provides
//! an alternative that reuses Tauri's existing webview infrastructure.
//!
//! The renderer creates a hidden `WebviewWindow`, navigates to the URL,
//! waits for the page to load, extracts the visible text via `eval()` +
//! `title()` polling, and returns the content.

pub(crate) fn setup(app: &mut tauri::App) {
    use anyhow::Context as _;
    let handle = app.handle().clone();

    skill_headless::Browser::set_external_renderer(move |url, _wait_ms| {
        use std::sync::mpsc;
        use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let label = format!("hfetch_{}", ts);

        let parsed_url: tauri::Url = url.parse().context("invalid URL")?;

        // Channel to detect initial page-load completion (DOM ready).
        let (load_tx, load_rx) = mpsc::sync_channel::<()>(1);

        /// Navigate to `about:blank` and sleep briefly before destroying a
        /// webview.  This gives WebKit's `ScrollingTree` and display-link
        /// callbacks time to detach, avoiding a use-after-free crash in
        /// `WebCore::ScrollingTree::takePendingScrollUpdates()`.
        fn safe_destroy(win: &tauri::WebviewWindow) {
            let _ = win.eval("window.stop()");
            if let Ok(url) = "about:blank".parse() {
                let _ = win.navigate(url);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = win.destroy();
        }

        let window = tauri::WebviewWindowBuilder::new(
            &handle,
            &label,
            tauri::WebviewUrl::External(parsed_url),
        )
        .title("__SKILL_LOADING__")
        .visible(false)
        .inner_size(1280.0, 720.0)
        .on_page_load(move |_wv, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let _ = load_tx.send(());
            }
        })
        .build()
        .context("webview creation failed")?;

        // ── Phase 1: Wait for initial DOM load (or timeout / cancel) ─
        let deadline = Instant::now() + Duration::from_secs(30);

        loop {
            if skill_headless::is_fetch_cancelled() {
                safe_destroy(&window);
                anyhow::bail!("cancelled by user");
            }
            if load_rx.try_recv().is_ok() {
                break;
            }
            if Instant::now() > deadline {
                safe_destroy(&window);
                anyhow::bail!("page load timeout (30s)");
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        // ── Phase 2: Wait for content to stabilise (SPA rendering) ───
        // SPAs fire PageLoadEvent::Finished when the shell loads, then
        // fetch data via XHR and render it.  We poll body text length
        // and wait until it stabilises (same length for 3 consecutive
        // checks 500ms apart) or 15s passes.
        let stability_js = r#"
            (function() {
                try {
                    document.title = '__SKILL_LEN__' + (document.body ? document.body.innerText.length : 0);
                } catch(e) { document.title = '__SKILL_LEN__0'; }
            })();
        "#;

        let settle_deadline = Instant::now() + Duration::from_secs(15);
        let mut last_len: Option<usize> = None;
        let mut stable_count = 0;

        loop {
            if skill_headless::is_fetch_cancelled() {
                safe_destroy(&window);
                anyhow::bail!("cancelled by user");
            }

            let _ = window.eval(stability_js);
            std::thread::sleep(Duration::from_millis(500));

            if let Ok(title) = window.title() {
                if let Some(len_str) = title.strip_prefix("__SKILL_LEN__") {
                    if let Ok(len) = len_str.parse::<usize>() {
                        if len > 100 {
                            // Minimum content threshold
                            if last_len == Some(len) {
                                stable_count += 1;
                                if stable_count >= 3 {
                                    break; // Content stabilised!
                                }
                            } else {
                                stable_count = 0;
                            }
                            last_len = Some(len);
                        }
                    }
                }
            }

            if Instant::now() > settle_deadline {
                break; // Timeout — extract what we have.
            }
        }

        if skill_headless::is_fetch_cancelled() {
            safe_destroy(&window);
            anyhow::bail!("cancelled by user");
        }

        // ── Phase 3: Extract visible text ────────────────────────────
        let extract_js = r#"
            (function() {
                try {
                    function extractText(node) {
                        if (!node) return '';
                        var tag = (node.tagName || '').toLowerCase();
                        if (tag === 'script' || tag === 'style' || tag === 'noscript'
                            || tag === 'svg' || tag === 'nav' || tag === 'footer'
                            || tag === 'header') return '';
                        var style = node.nodeType === 1 ? getComputedStyle(node) : null;
                        if (style && (style.display === 'none' || style.visibility === 'hidden')) return '';
                        if (node.nodeType === 3) return node.textContent;
                        var parts = [];
                        for (var i = 0; i < node.childNodes.length; i++) {
                            parts.push(extractText(node.childNodes[i]));
                        }
                        var text = parts.join(' ');
                        var block = style && (style.display === 'block' || style.display === 'flex'
                            || style.display === 'grid' || style.display === 'table'
                            || tag === 'br' || tag === 'p' || tag === 'div' || tag === 'li'
                            || tag === 'h1' || tag === 'h2' || tag === 'h3' || tag === 'h4'
                            || tag === 'h5' || tag === 'h6' || tag === 'tr');
                        return block ? '\n' + text + '\n' : text;
                    }
                    var raw = extractText(document.body || document.documentElement);
                    var clean = raw.replace(/[ \t]+/g, ' ').replace(/\n{3,}/g, '\n\n').trim();
                    document.title = '__SKILL_DONE__' + clean.substring(0, 100000);
                } catch(e) {
                    document.title = '__SKILL_DONE__' + (document.body ? document.body.innerText || '' : '');
                }
            })();
        "#;

        let _ = window.eval(extract_js);

        // Poll for the extraction result.
        let extract_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            std::thread::sleep(Duration::from_millis(100));

            if skill_headless::is_fetch_cancelled() {
                safe_destroy(&window);
                anyhow::bail!("cancelled by user");
            }

            if let Ok(title) = window.title() {
                if let Some(text) = title.strip_prefix("__SKILL_DONE__") {
                    safe_destroy(&window);
                    return Ok(text.to_string());
                }
            }

            if Instant::now() > extract_deadline {
                break;
            }
        }

        safe_destroy(&window);
        anyhow::bail!("content extraction timeout")
    });
}
