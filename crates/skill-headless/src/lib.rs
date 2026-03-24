// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! # skill-headless
//!
//! Headless browser engine providing a CDP-like command interface over
//! wry + tao.  A hidden system webview executes real JavaScript, renders
//! pages, and exposes navigation / DOM / network / screenshot primitives
//! through a simple Rust API.
//!
//! ## Architecture
//!
//! ```text
//!  ┌──────────────┐        Command (crossbeam)        ┌────────────────┐
//!  │  caller       │ ──────────────────────────────────▶│  event-loop    │
//!  │  (any thread) │ ◀──────────────────────────────────│  thread (tao)  │
//!  └──────────────┘        Response (crossbeam)        │  + wry webview │
//!                                                       └────────────────┘
//! ```
//!
//! The event loop runs on a **dedicated thread** spawned by [`Browser::launch`].
//! Commands are sent via a typed channel and executed on the event-loop thread
//! where wry/tao calls are safe (single-threaded GUI requirement).
//!
//! ## Quick start
//!
//! ```ignore
//! use skill_headless::{Browser, BrowserConfig, Command, Mode};
//!
//! // Headless (default) — no visible window.
//! let browser = Browser::launch(BrowserConfig::default())?;
//!
//! // Headful — visible window for debugging.
//! // let browser = Browser::launch(BrowserConfig {
//! //     mode: Mode::Headful,
//! //     ..Default::default()
//! // })?;
//!
//! // Navigate
//! let nav = browser.send(Command::Navigate {
//!     url: "https://example.com".into(),
//! })?;
//!
//! // Wait a bit for page load, then grab HTML
//! std::thread::sleep(std::time::Duration::from_secs(2));
//! let resp = browser.send(Command::GetContent)?;
//! println!("{}", resp.as_text().unwrap_or_default());
//!
//! // Execute arbitrary JS
//! let resp = browser.send(Command::EvalJs {
//!     script: "document.title".into(),
//! })?;
//!
//! // Screenshot as PNG bytes
//! let resp = browser.send(Command::Screenshot)?;
//!
//! // Inject CSS
//! browser.send(Command::InjectCss {
//!     css: "body { background: red; }".into(),
//! })?;
//!
//! // Clean up
//! browser.send(Command::Close)?;
//! ```

mod command;
mod engine;
mod error;
mod intercept;
mod response;
mod session;

pub use command::Command;
pub use engine::{Browser, BrowserConfig, Mode, external_fetch_page, cancel_current_fetch, is_fetch_cancelled};
pub use error::HeadlessError;
pub use intercept::{
    InterceptStore, InterceptedRequest, InterceptedResponse, NavigationEvent, NetworkLog,
};
pub use response::Response;
pub use session::{Cookie, StorageEntry};
