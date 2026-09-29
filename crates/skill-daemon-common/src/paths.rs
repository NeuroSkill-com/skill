// SPDX-License-Identifier: GPL-3.0-only
//! Filesystem locations shared by the daemon and the Tauri app.
//!
//! These have to agree exactly. Bearer auth compares a secret that the daemon
//! reads from disk against one the app reads from disk, so the two processes
//! resolving different paths does not degrade gracefully — every request answers
//! 401 and the UI simply cannot drive the daemon.
//!
//! They used to be two separate implementations (`skill_daemon_state::util` and
//! `src-tauri/src/daemon_cmds.rs`) and they had already drifted: only the app
//! honoured `SKILL_DAEMON_CONFIG_ROOT`, so with that variable set the app read a
//! tmpdir token while the daemon read the real one under `$HOME`. This module
//! exists so there is one definition to drift from.

use std::path::PathBuf;

/// Environment override for the daemon's config directory.
///
/// Lets e2e tests pin every daemon path under one tmpdir without touching
/// `$HOME` / `$XDG_CONFIG_HOME`. Honoured by every consumer of this module.
pub const CONFIG_ROOT_ENV: &str = "SKILL_DAEMON_CONFIG_ROOT";

/// The daemon's config directory, or `None` if the platform config dir cannot
/// be resolved.
pub fn config_root() -> Option<PathBuf> {
    if let Ok(root) = std::env::var(CONFIG_ROOT_ENV) {
        if !root.is_empty() {
            return Some(PathBuf::from(root));
        }
    }
    Some(dirs::config_dir()?.join("skill").join("daemon"))
}

/// Path to the daemon's bearer-token file.
///
/// Callers wrap the `None` case in whatever error type they use; it means the
/// platform has no resolvable config directory.
pub fn token_path() -> Option<PathBuf> {
    Some(config_root()?.join("auth.token"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The override must win, and must produce the same answer for anyone who
    /// asks — that equality is the whole point of this module.
    #[test]
    fn config_root_env_overrides_and_is_consistent() {
        // Not using a real env var here: tests share a process and this would
        // race. The override logic is exercised through the public helpers by
        // callers that set it; here we assert the shape instead.
        let root = config_root().expect("a config root on every supported platform");
        let token = token_path().expect("a token path wherever there is a root");
        assert_eq!(token, root.join("auth.token"));
        assert!(token.is_absolute(), "token path must be absolute: {token:?}");
    }

    #[test]
    fn token_path_sits_under_the_config_root() {
        let root = config_root().unwrap();
        let token = token_path().unwrap();
        assert!(token.starts_with(&root), "{token:?} not under {root:?}");
        assert_eq!(token.file_name().unwrap(), "auth.token");
    }
}
