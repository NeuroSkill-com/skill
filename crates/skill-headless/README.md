# skill-headless

Headless browser engine for NeuroSkill — a CDP-like command API over
**wry** (system WebView) and **tao** (windowing).

## Overview

A hidden system webview runs on a dedicated OS thread.  Callers interact
with it from any thread through a channel-based command/response protocol.
The command set mirrors the most-used Chrome DevTools Protocol domains:

| CDP Domain   | Commands                                                       |
|-------------|---------------------------------------------------------------|
| **Page**     | Navigate, Reload, GoBack, GoForward, StopLoading, GetUrl, GetTitle, GetContent, Screenshot |
| **Runtime**  | EvalJs, EvalJsNoReturn, CallFunction                          |
| **DOM**      | InjectCss, InjectScriptUrl, InjectScriptContent, QuerySelector, QuerySelectorText, GetAttribute, Click, TypeText, SetValue, ScrollBy, ScrollTo |
| **Network**  | SetCookie, GetCookies, DeleteCookies, ClearCookies            |
| **Storage**  | Get/Set/Remove/Clear LocalStorage, Get/Set SessionStorage     |
| **Emulation**| SetUserAgent (launch-time), SetViewport, SetJsEnabled         |
| **Cache**    | ClearCache, ClearBrowsingData                                 |
| **Session**  | WaitForSelector, WaitForNavigation, Close                     |

## Architecture

```
 ┌──────────────┐        Command (crossbeam)        ┌────────────────┐
 │  caller       │ ──────────────────────────────────▶│  event-loop    │
 │  (any thread) │ ◀──────────────────────────────────│  thread (tao)  │
 └──────────────┘        Response (crossbeam)        │  + wry webview │
                                                      └────────────────┘
```

## Modes

| Mode | Description |
|------|-------------|
| `Mode::Headless` (default) | Window is positioned off-screen. Nothing is ever shown to the user. Webview still gets real pixel dimensions. |
| `Mode::Headful` | Window is visible on-screen. Useful for debugging, demos, or interactive automation. |

## Quick Start

```rust
use skill_headless::{Browser, BrowserConfig, Command, Mode};

let browser = Browser::launch(BrowserConfig::default())?;

// Navigate
browser.send(Command::Navigate {
    url: "https://example.com".into(),
})?;

// Wait for page to load
browser.send(Command::WaitForNavigation { timeout_ms: 10_000 })?;

// Get the page title
let title = browser.send(Command::GetTitle)?;
println!("Title: {}", title.as_text().unwrap_or("?"));

// Get full rendered HTML
let html = browser.send(Command::GetContent)?;

// Execute JavaScript
let result = browser.send(Command::EvalJs {
    script: "document.querySelectorAll('a').length".into(),
})?;
println!("Links: {}", result.as_text().unwrap_or("0"));

// Click a link
browser.send(Command::Click {
    selector: "a.some-link".into(),
})?;

// Fill a form
browser.send(Command::SetValue {
    selector: "#search".into(),
    value: "hello world".into(),
})?;

// Inject custom CSS
browser.send(Command::InjectCss {
    css: "body { background: #111; color: #eee; }".into(),
})?;

// Work with cookies
use skill_headless::Cookie;
browser.send(Command::SetCookie {
    cookie: Cookie {
        name: "session".into(),
        value: "abc123".into(),
        domain: ".example.com".into(),
        path: "/".into(),
        ..Default::default()
    },
})?;

// LocalStorage
browser.send(Command::SetLocalStorage {
    key: "pref".into(),
    value: "dark".into(),
})?;

// Clean up
browser.send(Command::Close)?;
```

## Configuration

```rust
use skill_headless::Mode;

// Headless (default) — no visible window
let browser = Browser::launch(BrowserConfig {
    width: 1920,
    height: 1080,
    mode: Mode::Headless,
    user_agent: Some("SkillBot/1.0".into()),
    data_dir: Some("/tmp/skill-browser".into()),
    timeout: std::time::Duration::from_secs(60),
    devtools: false,
    initial_url: "https://example.com".into(),
    ..Default::default()
})?;

// Headful — visible window for debugging / demos
let browser = Browser::launch(BrowserConfig {
    mode: Mode::Headful,
    devtools: true,
    ..Default::default()
})?;
```

## Platform Notes

| Platform | WebView Engine | Notes |
|----------|---------------|-------|
| macOS    | WKWebView     | Works out of the box |
| Linux    | WebKitGTK     | Requires `libwebkit2gtk-4.1-dev`. In CI, use `xvfb-run`. |
| Windows  | WebView2 (Edge) | Requires Edge/WebView2 runtime |

## Limitations

- **No true headless mode** — a display server is required on Linux (use `xvfb-run` in CI).
- **Screenshot** requires injecting a canvas-based library (e.g. html2canvas).
- **PDF export** is not supported.
- **User-Agent** can only be set at launch time, not changed per-navigation.
- **Different engines per platform** — behavior may vary slightly between WebKit/WebView2.
- **Cookies** are limited to `document.cookie` (no HttpOnly access from JS).

For full CDP fidelity, consider `headless_chrome` or `chromiumoxide` instead.
This crate is ideal when you want to reuse the system webview already
available in Tauri apps without shipping Chromium.
