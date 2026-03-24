// SPDX-License-Identifier: GPL-3.0-only
//! Network interception tests — fetch/XHR capture, navigation events, URL blocking.

use skill_headless::{Browser, BrowserConfig, Command, Mode};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(30);

fn main() {
    let mut passed = 0u32;
    let mut failed = 0u32;

    macro_rules! test {
        ($name:expr, $body:block) => {{
            print!("[TEST] {:<60}", $name);
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
                Ok(_) => {
                    println!(" PASS");
                    passed += 1;
                }
                Err(e) => {
                    let msg = e
                        .downcast_ref::<String>()
                        .map(|s| s.as_str())
                        .or_else(|| e.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown panic");
                    println!(" FAIL: {msg}");
                    failed += 1;
                }
            }
        }};
    }

    println!("=== skill-headless interception tests ===\n");

    let browser = Browser::launch(BrowserConfig {
        width: 800,
        height: 600,
        mode: Mode::Headless,
        timeout: TIMEOUT,
        ..Default::default()
    })
    .expect("launch failed");
    std::thread::sleep(Duration::from_millis(500));

    // ══════════════════════════════════════════════════════════════════════
    // 1. ENABLE / DISABLE
    // ══════════════════════════════════════════════════════════════════════
    println!("── Enable / Disable ────────────────────────────────────────\n");

    test!("EnableInterception succeeds", {
        let resp = browser.send(Command::EnableInterception).unwrap();
        assert!(resp.is_ok(), "expected Ok, got: {:?}", resp);
    });

    test!("GetInterceptedRequests returns empty log initially", {
        let resp = browser
            .send(Command::GetInterceptedRequests { clear: false })
            .unwrap();
        let log = resp.as_network().expect("expected Network response");
        assert!(log.requests.is_empty(), "expected no requests");
        assert!(log.responses.is_empty(), "expected no responses");
    });

    // ══════════════════════════════════════════════════════════════════════
    // 2. FETCH INTERCEPTION
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── Fetch Interception ──────────────────────────────────────\n");

    test!("Intercepts fetch() GET request", {
        // Clear any prior data
        browser
            .send(Command::GetInterceptedRequests { clear: true })
            .unwrap();

        // Make a fetch call (will fail to connect, but we still intercept it)
        browser
            .send(Command::EvalJsNoReturn {
                script: "fetch('https://httpbin.org/get?test=1').catch(function(){});".into(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(1000));

        let resp = browser
            .send(Command::GetInterceptedRequests { clear: false })
            .unwrap();
        let log = resp.as_network().unwrap();
        println!(
            "  (reqs: {}, resps: {})",
            log.requests.len(),
            log.responses.len()
        );
        assert!(!log.requests.is_empty(), "expected at least 1 request");

        let req = &log.requests[0];
        assert_eq!(req.method, "GET");
        assert!(req.url.contains("httpbin.org/get"), "url: {}", req.url);
        assert!(req.url.contains("test=1"), "query param missing");
        assert!(req.timestamp_ms > 0.0);
    });

    test!("Intercepts fetch() POST request with body", {
        browser
            .send(Command::GetInterceptedRequests { clear: true })
            .unwrap();

        browser
            .send(Command::EvalJsNoReturn {
                script: r#"
                fetch('https://httpbin.org/post', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ hello: 'world' })
                }).catch(function(){});
            "#
                .into(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(1000));

        let log = browser
            .send(Command::GetInterceptedRequests { clear: false })
            .unwrap()
            .as_network()
            .unwrap()
            .clone();
        assert!(!log.requests.is_empty(), "expected POST request");

        let req = &log.requests[0];
        assert_eq!(req.method, "POST");
        assert!(req.body.contains("hello"), "body: {}", req.body);
        assert!(
            req.headers.contains("application/json"),
            "headers: {}",
            req.headers
        );
    });

    // ══════════════════════════════════════════════════════════════════════
    // 3. XMLHttpRequest INTERCEPTION
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── XHR Interception ────────────────────────────────────────\n");

    test!("Intercepts XMLHttpRequest", {
        browser
            .send(Command::GetInterceptedRequests { clear: true })
            .unwrap();

        browser
            .send(Command::EvalJsNoReturn {
                script: r#"
                var xhr = new XMLHttpRequest();
                xhr.open('GET', 'https://httpbin.org/anything?xhr=1');
                xhr.setRequestHeader('X-Custom', 'test-value');
                xhr.send();
            "#
                .into(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(1000));

        let log = browser
            .send(Command::GetInterceptedRequests { clear: false })
            .unwrap()
            .as_network()
            .unwrap()
            .clone();
        println!(
            "  (reqs: {}, resps: {})",
            log.requests.len(),
            log.responses.len()
        );
        assert!(!log.requests.is_empty(), "expected XHR request");

        let req = &log.requests[0];
        assert_eq!(req.method, "GET");
        assert!(req.url.contains("xhr=1"), "url: {}", req.url);
        assert!(req.headers.contains("X-Custom"), "headers: {}", req.headers);
    });

    // ══════════════════════════════════════════════════════════════════════
    // 4. NAVIGATION EVENTS
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── Navigation Events ───────────────────────────────────────\n");

    test!("Navigation events are captured", {
        browser
            .send(Command::GetInterceptedRequests { clear: true })
            .unwrap();

        // The custom protocol load should have generated navigation events.
        // Let's trigger one explicitly.
        browser
            .send(Command::Navigate {
                url: "skill://localhost/test-nav".into(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));

        let log = browser
            .send(Command::GetInterceptedRequests { clear: false })
            .unwrap()
            .as_network()
            .unwrap()
            .clone();
        println!("  (navs: {})", log.navigations.len());
        assert!(!log.navigations.is_empty(), "expected navigation events");

        let nav = log.navigations.iter().find(|n| n.url.contains("test-nav"));
        assert!(nav.is_some(), "expected test-nav in navigations");
        assert!(nav.unwrap().allowed, "expected navigation to be allowed");
    });

    // ══════════════════════════════════════════════════════════════════════
    // 5. URL BLOCKING
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── URL Blocking ────────────────────────────────────────────\n");

    test!("SetBlockedUrls blocks matching navigations", {
        browser
            .send(Command::GetInterceptedRequests { clear: true })
            .unwrap();

        browser
            .send(Command::SetBlockedUrls {
                patterns: vec!["blocked-domain.test".into()],
            })
            .unwrap();

        // Try to navigate to a blocked URL
        browser
            .send(Command::Navigate {
                url: "https://blocked-domain.test/page".into(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));

        let log = browser
            .send(Command::GetInterceptedRequests { clear: false })
            .unwrap()
            .as_network()
            .unwrap()
            .clone();

        let blocked_nav = log
            .navigations
            .iter()
            .find(|n| n.url.contains("blocked-domain"));
        assert!(blocked_nav.is_some(), "expected blocked nav event");
        assert!(
            !blocked_nav.unwrap().allowed,
            "expected navigation to be blocked"
        );
    });

    test!("Non-blocked URLs still pass through", {
        browser
            .send(Command::GetInterceptedRequests { clear: true })
            .unwrap();

        browser
            .send(Command::Navigate {
                url: "skill://localhost/allowed-page".into(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));

        let log = browser
            .send(Command::GetInterceptedRequests { clear: false })
            .unwrap()
            .as_network()
            .unwrap()
            .clone();

        let nav = log
            .navigations
            .iter()
            .find(|n| n.url.contains("allowed-page"));
        assert!(nav.is_some(), "expected allowed nav event");
        assert!(nav.unwrap().allowed, "expected navigation to be allowed");
    });

    test!("ClearBlockedUrls unblocks everything", {
        browser.send(Command::ClearBlockedUrls).unwrap();
        browser
            .send(Command::GetInterceptedRequests { clear: true })
            .unwrap();

        browser
            .send(Command::Navigate {
                url: "https://blocked-domain.test/after-clear".into(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));

        let log = browser
            .send(Command::GetInterceptedRequests { clear: false })
            .unwrap()
            .as_network()
            .unwrap()
            .clone();

        let nav = log
            .navigations
            .iter()
            .find(|n| n.url.contains("after-clear"));
        assert!(nav.is_some(), "expected nav after clear");
        assert!(
            nav.unwrap().allowed,
            "expected navigation allowed after clear"
        );
    });

    // ══════════════════════════════════════════════════════════════════════
    // 6. CLEAR ON RETRIEVAL
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── Clear on Retrieval ──────────────────────────────────────\n");

    test!("GetInterceptedRequests { clear: true } empties the log", {
        // Ensure there's some data
        browser
            .send(Command::Navigate {
                url: "skill://localhost/clear-test".into(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));

        let log1 = browser
            .send(Command::GetInterceptedRequests { clear: true })
            .unwrap()
            .as_network()
            .unwrap()
            .clone();
        assert!(!log1.navigations.is_empty(), "expected data before clear");

        let log2 = browser
            .send(Command::GetInterceptedRequests { clear: false })
            .unwrap()
            .as_network()
            .unwrap()
            .clone();
        assert!(
            log2.requests.is_empty(),
            "requests should be empty after clear"
        );
        assert!(
            log2.responses.is_empty(),
            "responses should be empty after clear"
        );
        assert!(
            log2.navigations.is_empty(),
            "navigations should be empty after clear"
        );
    });

    // ══════════════════════════════════════════════════════════════════════
    // 7. DISABLE INTERCEPTION
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── Disable Interception ────────────────────────────────────\n");

    test!("DisableInterception succeeds", {
        let resp = browser.send(Command::DisableInterception).unwrap();
        assert!(resp.is_ok());
    });

    // ── Cleanup ──────────────────────────────────────────────────────────
    let _ = browser.send(Command::Close);

    // ══════════════════════════════════════════════════════════════════════
    // SUMMARY
    // ══════════════════════════════════════════════════════════════════════
    println!("\n================================================================");
    println!("  PASSED: {passed}");
    println!("  FAILED: {failed}");
    println!("================================================================");

    if failed > 0 {
        std::process::exit(1);
    }
}
