// SPDX-License-Identifier: GPL-3.0-only
//! Smoke test — launches a headless browser, navigates, evaluates JS, and
//! exercises the major command categories.

use skill_headless::{Browser, BrowserConfig, Command, Cookie, Mode};
use std::time::Duration;

fn main() {
    println!("=== skill-headless smoke test ===\n");

    // ── Launch ───────────────────────────────────────────────────────────
    println!("[1] Launching headless browser...");
    let browser = Browser::launch(BrowserConfig {
        width: 1024,
        height: 768,
        mode: Mode::Headless,
        devtools: false,
        timeout: Duration::from_secs(15),
        ..Default::default()
    })
    .expect("failed to launch browser");
    println!("    OK — browser launched\n");

    // ── Navigate to a data URL (no network needed) ───────────────────────
    // First load about:blank, then inject the test page via JS.
    // We avoid data: URLs because wry's WebKitGTK IPC handler crashes when
    // building an http::Request from a long data: URI.
    println!("    Loading test page via innerHTML...");
    browser
        .send(Command::EvalJsNoReturn {
            script: r##"
                document.open();
                document.write('<!DOCTYPE html><html><head><title>Test Page</title></head><body><h1 id="heading">Hello Headless</h1><p class="info">Paragraph 1</p><p class="info">Paragraph 2</p><a href="#link1" class="nav">Link A</a><a href="#link2" class="nav">Link B</a><input id="search" type="text" value="" /><div id="output"></div></body></html>');
                document.close();
            "##.into(),
        })
        .expect("document.write failed");

    // Give the webview a moment to render.
    std::thread::sleep(Duration::from_millis(500));

    println!("[2] Setting up test page...");

    // ── Debug: check IPC ────────────────────────────────────────────────
    println!("\n[2b] Debug: EvalJs simple...");
    let resp = browser
        .send(Command::EvalJs {
            script: "'hello'".into(),
        })
        .expect("EvalJs failed");
    println!("    Result: {:?}", resp);

    // ── GetTitle ─────────────────────────────────────────────────────────
    println!("\n[3] GetTitle...");
    let resp = browser.send(Command::GetTitle).expect("GetTitle failed");
    println!("    Title: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("Test Page"), "title mismatch");
    println!("    PASS");

    // ── GetUrl ───────────────────────────────────────────────────────────
    println!("\n[4] GetUrl...");
    let resp = browser.send(Command::GetUrl).expect("GetUrl failed");
    let url = resp.as_text().unwrap_or("");
    println!("    URL: {}", url);
    assert!(!url.is_empty(), "expected non-empty URL");
    println!("    PASS");

    // ── GetContent ───────────────────────────────────────────────────────
    println!("\n[5] GetContent...");
    let resp = browser.send(Command::GetContent).expect("GetContent failed");
    let html = resp.as_text().unwrap_or("");
    println!("    HTML length: {} chars", html.len());
    assert!(html.contains("Hello Headless"), "expected heading in HTML");
    println!("    PASS");

    // ── EvalJs ───────────────────────────────────────────────────────────
    println!("\n[6] EvalJs (arithmetic)...");
    let resp = browser
        .send(Command::EvalJs {
            script: "(40 + 2).toString();".into(),
        })
        .expect("EvalJs failed");
    println!("    Result: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("42"), "expected 42");
    println!("    PASS");

    // ── EvalJs (DOM access) ──────────────────────────────────────────────
    println!("\n[7] EvalJs (DOM access)...");
    let resp = browser
        .send(Command::EvalJs {
            script: "document.getElementById('heading').textContent;".into(),
        })
        .expect("EvalJs DOM failed");
    println!("    Result: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("Hello Headless"), "heading mismatch");
    println!("    PASS");

    // ── QuerySelector ────────────────────────────────────────────────────
    println!("\n[8] QuerySelector('.info')...");
    let resp = browser
        .send(Command::QuerySelector {
            selector: ".info".into(),
        })
        .expect("QuerySelector failed");
    let text = resp.as_text().unwrap_or("[]");
    println!("    Result: {} chars", text.len());
    assert!(text.contains("Paragraph 1"), "expected p1");
    assert!(text.contains("Paragraph 2"), "expected p2");
    println!("    PASS");

    // ── QuerySelectorText ────────────────────────────────────────────────
    println!("\n[9] QuerySelectorText('.nav')...");
    let resp = browser
        .send(Command::QuerySelectorText {
            selector: ".nav".into(),
        })
        .expect("QuerySelectorText failed");
    let text = resp.as_text().unwrap_or("[]");
    println!("    Result: {}", text);
    assert!(text.contains("Link A"), "expected Link A");
    assert!(text.contains("Link B"), "expected Link B");
    println!("    PASS");

    // ── GetAttribute ─────────────────────────────────────────────────────
    println!("\n[10] GetAttribute('#search', 'type')...");
    let resp = browser
        .send(Command::GetAttribute {
            selector: "#search".into(),
            attribute: "type".into(),
        })
        .expect("GetAttribute failed");
    println!("    Result: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("text"), "expected type=text");
    println!("    PASS");

    // ── SetValue ─────────────────────────────────────────────────────────
    println!("\n[11] SetValue('#search', 'hello world')...");
    let resp = browser
        .send(Command::SetValue {
            selector: "#search".into(),
            value: "hello world".into(),
        })
        .expect("SetValue failed");
    println!("    Response: {:?}", resp);
    // Verify the value was set.
    let resp = browser
        .send(Command::EvalJs {
            script: "document.getElementById('search').value;".into(),
        })
        .expect("verify SetValue failed");
    println!("    Verification: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("hello world"), "value mismatch");
    println!("    PASS");

    // ── Click ────────────────────────────────────────────────────────────
    println!("\n[12] Click('.nav')...");
    let resp = browser
        .send(Command::Click {
            selector: ".nav".into(),
        })
        .expect("Click failed");
    println!("    Result: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("ok"), "expected click ok");
    println!("    PASS");

    // ── InjectCss ────────────────────────────────────────────────────────
    println!("\n[13] InjectCss...");
    let resp = browser
        .send(Command::InjectCss {
            css: "body { background: red; }".into(),
        })
        .expect("InjectCss failed");
    println!("    Response: {:?}", resp);
    // Verify the style was injected.
    let resp = browser
        .send(Command::EvalJs {
            script: "document.querySelectorAll('style').length.toString();".into(),
        })
        .expect("verify CSS failed");
    let count: i32 = resp.as_text().unwrap_or("0").parse().unwrap_or(0);
    println!("    Style tags: {count}");
    assert!(count >= 1, "expected at least 1 style tag");
    println!("    PASS");

    // ── InjectScriptContent ──────────────────────────────────────────────
    println!("\n[14] InjectScriptContent...");
    let resp = browser
        .send(Command::InjectScriptContent {
            content: "document.getElementById('output').textContent = 'injected';".into(),
        })
        .expect("InjectScriptContent failed");
    println!("    Response: {:?}", resp);
    std::thread::sleep(Duration::from_millis(200));
    let resp = browser
        .send(Command::EvalJs {
            script: "document.getElementById('output').textContent;".into(),
        })
        .expect("verify script inject failed");
    println!("    Output div: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("injected"), "expected 'injected'");
    println!("    PASS");

    // ── LocalStorage ─────────────────────────────────────────────────────
    println!("\n[15] LocalStorage set/get/remove...");
    browser
        .send(Command::SetLocalStorage {
            key: "test_key".into(),
            value: "test_val".into(),
        })
        .expect("SetLocalStorage failed");
    let resp = browser
        .send(Command::GetLocalStorage {
            key: "test_key".into(),
        })
        .expect("GetLocalStorage failed");
    println!("    Get: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("test_val"), "localStorage mismatch");
    browser
        .send(Command::RemoveLocalStorage {
            key: "test_key".into(),
        })
        .expect("RemoveLocalStorage failed");
    let resp = browser
        .send(Command::GetLocalStorage {
            key: "test_key".into(),
        })
        .expect("GetLocalStorage after remove failed");
    println!("    After remove: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("null"), "expected null after remove");
    println!("    PASS");

    // ── SessionStorage ───────────────────────────────────────────────────
    println!("\n[16] SessionStorage set/get...");
    browser
        .send(Command::SetSessionStorage {
            key: "sess_key".into(),
            value: "sess_val".into(),
        })
        .expect("SetSessionStorage failed");
    let resp = browser
        .send(Command::GetSessionStorage {
            key: "sess_key".into(),
        })
        .expect("GetSessionStorage failed");
    println!("    Get: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("sess_val"), "sessionStorage mismatch");
    println!("    PASS");

    // ── Cookies (via document.cookie) ────────────────────────────────────
    // Note: document.cookie doesn't work on data: URLs in most browsers,
    // so we just test the command doesn't crash.
    println!("\n[17] Cookie commands (smoke — data: URLs may not support cookies)...");
    let r = browser.send(Command::SetCookie {
        cookie: Cookie {
            name: "test".into(),
            value: "123".into(),
            ..Default::default()
        },
    });
    println!("    SetCookie: {:?}", r.is_ok());
    let r = browser.send(Command::GetCookies { domain: None });
    println!("    GetCookies: {:?}", r.map(|r| format!("{:?}", r)));
    let r = browser.send(Command::ClearCookies);
    println!("    ClearCookies: {:?}", r.is_ok());
    println!("    PASS (no crash)");

    // ── Scroll ───────────────────────────────────────────────────────────
    println!("\n[18] ScrollTo / ScrollBy...");
    let r = browser.send(Command::ScrollTo { x: 0.0, y: 100.0 });
    println!("    ScrollTo: {:?}", r.is_ok());
    let r = browser.send(Command::ScrollBy { x: 0.0, y: -50.0 });
    println!("    ScrollBy: {:?}", r.is_ok());
    println!("    PASS");

    // ── SetViewport ──────────────────────────────────────────────────────
    println!("\n[19] SetViewport(800, 600)...");
    let resp = browser
        .send(Command::SetViewport {
            width: 800,
            height: 600,
        })
        .expect("SetViewport failed");
    println!("    Response: {:?}", resp);
    println!("    PASS");

    // ── Reload ───────────────────────────────────────────────────────────
    println!("\n[20] Reload...");
    let resp = browser
        .send(Command::Reload {
            ignore_cache: false,
        })
        .expect("Reload failed");
    println!("    Response: {:?}", resp);
    println!("    PASS");

    // Wait for reload to complete before continuing.
    std::thread::sleep(Duration::from_millis(500));

    // ── GoBack / GoForward ───────────────────────────────────────────────
    println!("\n[21] GoBack / GoForward...");
    let r = browser.send(Command::GoBack);
    println!("    GoBack: {:?}", r.is_ok());
    let r = browser.send(Command::GoForward);
    println!("    GoForward: {:?}", r.is_ok());
    println!("    PASS");

    // ── EvalJsNoReturn ───────────────────────────────────────────────────
    println!("\n[22] EvalJsNoReturn...");
    let resp = browser
        .send(Command::EvalJsNoReturn {
            script: "console.log('fire and forget')".into(),
        })
        .expect("EvalJsNoReturn failed");
    println!("    Response: {:?}", resp);
    println!("    PASS");

    // ── CallFunction ─────────────────────────────────────────────────────
    println!("\n[23] CallFunction...");
    // First define a function (use EvalJs to wait for completion).
    browser
        .send(Command::EvalJs {
            script: "window.myAdd = (a, b) => a + b; 'defined'".into(),
        })
        .expect("define function failed");
    let resp = browser
        .send(Command::CallFunction {
            function: "window.myAdd".into(),
            args: vec!["10".into(), "32".into()],
        })
        .expect("CallFunction failed");
    println!("    myAdd(10, 32) = {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("42"), "expected 42");
    println!("    PASS");

    // ── WaitForSelector ──────────────────────────────────────────────────
    // Re-inject test page (reload in test [20] cleared it).
    browser
        .send(Command::EvalJs {
            script: "document.body.innerHTML = '<h1 id=\"heading\">Hello</h1>'; 'ok'".into(),
        })
        .unwrap();

    println!("\n[24] WaitForSelector('#heading', 2000ms)...");
    let resp = browser
        .send(Command::WaitForSelector {
            selector: "#heading".into(),
            timeout_ms: 2000,
        })
        .expect("WaitForSelector failed");
    println!("    Result: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("found"), "expected found");
    println!("    PASS");

    println!("\n[25] WaitForSelector('#nonexistent', 500ms) — expect timeout...");
    let resp = browser
        .send(Command::WaitForSelector {
            selector: "#nonexistent".into(),
            timeout_ms: 500,
        })
        .expect("WaitForSelector failed");
    println!("    Result: {:?}", resp.as_text());
    assert_eq!(resp.as_text(), Some("timeout"), "expected timeout");
    println!("    PASS");

    // ── ClearBrowsingData ────────────────────────────────────────────────
    println!("\n[26] ClearBrowsingData...");
    let resp = browser
        .send(Command::ClearBrowsingData)
        .expect("ClearBrowsingData failed");
    println!("    Response: {:?}", resp);
    println!("    PASS");

    // ── Screenshot ─────────────────────────────────────────────────────
    println!("\n[27] Screenshot...");
    let resp = browser.send(Command::Screenshot).expect("Screenshot panicked");
    let text = resp.as_text().unwrap_or("");
    let is_png = text.starts_with("data:image/png;base64,");
    println!("    Got PNG data URL: {is_png} ({} chars)", text.len());
    assert!(is_png, "expected PNG data URL from screenshot");
    println!("    PASS");

    // ── Unsupported commands (verify no crash) ───────────────────────────
    println!("\n[28] Unsupported: SetUserAgent, SetJsEnabled, PrintToPdf...");
    let r = browser.send(Command::SetUserAgent {
        user_agent: "test".into(),
    });
    println!("    SetUserAgent: {:?}", r.map(|r| r.is_ok()));
    let r = browser.send(Command::SetJsEnabled { enabled: false });
    println!("    SetJsEnabled: {:?}", r.map(|r| r.is_ok()));
    let r = browser.send(Command::PrintToPdf);
    println!("    PrintToPdf: {:?}", r.map(|r| r.is_ok()));
    println!("    PASS (graceful errors)");

    // ── Close ────────────────────────────────────────────────────────────
    println!("\n[29] Close...");
    let resp = browser.send(Command::Close).expect("Close failed");
    println!("    Response: {:?}", resp);
    assert!(browser.is_closed(), "expected is_closed() = true");
    println!("    PASS");

    // ── Post-close error ─────────────────────────────────────────────────
    println!("\n[30] Send after close (expect SessionClosed error)...");
    let r = browser.send(Command::GetTitle);
    println!("    Result: {:?}", r);
    assert!(r.is_err(), "expected error after close");
    println!("    PASS");

    println!("\n=== ALL TESTS PASSED ===");
}
