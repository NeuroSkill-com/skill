// SPDX-License-Identifier: GPL-3.0-only
//! CDP-like command definitions.

use serde::{Deserialize, Serialize};

use crate::session::Cookie;

/// A command sent to the headless browser event loop.
///
/// Modeled after Chrome DevTools Protocol domains:
/// Page, Runtime, DOM, Network, Emulation, Storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    // ── Page ──────────────────────────────────────────────────────────────

    /// Navigate the webview to a URL (Page.navigate).
    Navigate { url: String },

    /// Reload the current page (Page.reload).
    Reload { ignore_cache: bool },

    /// Go back in history (Page.navigateToHistoryEntry).
    GoBack,

    /// Go forward in history.
    GoForward,

    /// Stop loading the current page.
    StopLoading,

    /// Get the current URL.
    GetUrl,

    /// Get the page title.
    GetTitle,

    /// Get the full rendered HTML (Outer HTML of the document element).
    GetContent,

    /// Capture a PNG screenshot of the current viewport (Page.captureScreenshot).
    Screenshot,

    /// Print the page to PDF (if supported by the webview backend).
    PrintToPdf,

    // ── Runtime ──────────────────────────────────────────────────────────

    /// Evaluate a JavaScript expression and return the result (Runtime.evaluate).
    EvalJs { script: String },

    /// Evaluate JS but don't wait for a return value (fire-and-forget).
    EvalJsNoReturn { script: String },

    /// Call a named function with JSON-serialized arguments.
    CallFunction { function: String, args: Vec<String> },

    // ── DOM ──────────────────────────────────────────────────────────────

    /// Inject CSS into the page (CSS.addRule / insertStyleSheet).
    InjectCss { css: String },

    /// Inject a `<script>` tag with the given source URL.
    InjectScriptUrl { url: String },

    /// Inject inline JavaScript (wrapped in <script>).
    InjectScriptContent { content: String },

    /// Query a CSS selector and return outer HTML of matching elements.
    QuerySelector { selector: String },

    /// Query a CSS selector and return the text content of matching elements.
    QuerySelectorText { selector: String },

    /// Query a CSS selector and return an attribute value.
    GetAttribute {
        selector: String,
        attribute: String,
    },

    /// Click on the element matching a CSS selector.
    Click { selector: String },

    /// Type text into the focused element or a selector.
    TypeText {
        selector: Option<String>,
        text: String,
    },

    /// Set a form input's value directly.
    SetValue { selector: String, value: String },

    /// Scroll by (x, y) pixels.
    ScrollBy { x: f64, y: f64 },

    /// Scroll to (x, y) absolute position.
    ScrollTo { x: f64, y: f64 },

    // ── Network / Storage ────────────────────────────────────────────────

    /// Set a cookie.
    SetCookie { cookie: Cookie },

    /// Get all cookies (or for a specific domain).
    GetCookies { domain: Option<String> },

    /// Delete cookies matching a name (and optional domain).
    DeleteCookies {
        name: String,
        domain: Option<String>,
    },

    /// Clear all cookies.
    ClearCookies,

    /// Read a localStorage key.
    GetLocalStorage { key: String },

    /// Write a localStorage key.
    SetLocalStorage { key: String, value: String },

    /// Remove a localStorage key.
    RemoveLocalStorage { key: String },

    /// Clear all localStorage.
    ClearLocalStorage,

    /// Read a sessionStorage key.
    GetSessionStorage { key: String },

    /// Write a sessionStorage key.
    SetSessionStorage { key: String, value: String },

    // ── Emulation ────────────────────────────────────────────────────────

    /// Set the User-Agent string for subsequent navigations.
    SetUserAgent { user_agent: String },

    /// Resize the virtual viewport.
    SetViewport { width: u32, height: u32 },

    /// Enable or disable JavaScript (where supported).
    SetJsEnabled { enabled: bool },

    // ── Cache ────────────────────────────────────────────────────────────

    /// Clear the webview's HTTP cache (if supported by backend).
    ClearCache,

    /// Clear all browsing data (cache + cookies + storage).
    ClearBrowsingData,

    // ── Session ──────────────────────────────────────────────────────────

    /// Wait for a CSS selector to appear in the DOM (polls with timeout).
    WaitForSelector {
        selector: String,
        timeout_ms: u64,
    },

    /// Wait for navigation to complete (waits for `load` event).
    WaitForNavigation { timeout_ms: u64 },

    /// Close the browser session and destroy the webview.
    Close,
}
