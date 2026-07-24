// SPDX-License-Identifier: GPL-3.0-only
//! HTTP route modules for `skill-daemon`.
//!
//! ## Extraction status
//!
//! Routes are moving here incrementally so the daemon binary crate stays
//! thinner and crates compile in parallel. Prefer modules that depend only on
//! [`skill_daemon_state::AppState`] plus leaf crates (`skill-iroh`, …).
//!
//! Still blocked in `skill-daemon` (do not move yet):
//! - `settings_exg` / embed-coupled settings handlers
//! - modules that call `crate::handlers` directly (`core`, pairing, devices)
//! - large monoliths (`search`, `settings`, `brain`) until further decoupled
//!
//! Mount extracted routers from `skill-daemon` with `.merge(...)` under `/v1`.

pub mod iroh;
