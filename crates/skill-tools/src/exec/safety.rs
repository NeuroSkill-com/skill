// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Safety checks, user-approval dialogs, and bash-edit hook for tool operations.

use std::sync::{Arc, Mutex};
use serde_json::json;

// ── Pluggable bash-edit callback ──────────────────────────────────────────────

/// Callback signature for bash command editing.
///
/// Receives the original command and returns:
/// - `Some(edited_command)` — the (possibly modified) command to execute
/// - `None` — the user cancelled; do not execute
pub type BashEditHook = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

static BASH_EDIT_HOOK: Mutex<Option<BashEditHook>> = Mutex::new(None);

/// Register a callback that is invoked before every LLM-generated bash command.
///
/// The callback runs on a blocking thread.  It should display the command to the
/// user, allow editing, and return `Some(final_command)` or `None` to cancel.
///
/// Call this once at app startup (e.g. from `setup()`).
pub fn set_bash_edit_hook(hook: BashEditHook) {
    *BASH_EDIT_HOOK.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(hook);
}

/// Clear the bash-edit hook (tests / shutdown).
#[cfg_attr(not(test), allow(dead_code))]
pub fn clear_bash_edit_hook() {
    *BASH_EDIT_HOOK.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

/// Present a bash command for user review/editing.
///
/// Returns `Some(command)` (possibly edited) or `None` if cancelled.
pub(crate) async fn request_bash_edit(command: &str) -> Option<String> {
    let hook = BASH_EDIT_HOOK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();

    match hook {
        Some(f) => {
            let cmd = command.to_string();
            tokio::task::spawn_blocking(move || f(&cmd))
                .await
                .unwrap_or_else(|e| {
                    crate::tool_log!("tool:bash", "[edit] hook panicked: {}", e);
                    None
                })
        }
        None => {
            // No hook registered — fall through to execute unmodified.
            Some(command.to_string())
        }
    }
}

/// Patterns that indicate a potentially dangerous bash command.
const DANGEROUS_BASH_PATTERNS: &[&str] = &[
    "rm ", "rm\t", "rmdir", "shred",
    "mkfs", "dd if=", "dd of=",
    "sudo ", "su -", "su\t",
    "> /dev/", "chmod", "chown",
    "kill ", "killall", "pkill",
    "shutdown", "reboot", "halt", "poweroff",
    "systemctl stop", "systemctl disable",
    ":(){ :|:& };:", // fork bomb
    "/etc/", "/boot/", "/usr/", "/var/", "/sys/", "/proc/",
];

/// Sensitive path prefixes that require approval for file write/edit.
const SENSITIVE_PATH_PREFIXES: &[&str] = &[
    "/etc/", "/boot/", "/usr/", "/var/", "/sys/", "/proc/",
    "/bin/", "/sbin/", "/lib/", "/opt/",
];

/// Characters that act as word boundaries before a dangerous pattern.
/// A match is only flagged if the pattern appears at the start of the string
/// or is preceded by one of these characters.  This prevents false positives
/// like "skill" matching "kill".
const BOUNDARY_CHARS: &[char] = &[
    ' ', '\t', '\n', '\r', ';', '|', '&', '(', ')', '{', '}', '`', '$', '/',
];

/// Check if a bash command looks dangerous and return a human-readable reason.
pub fn check_bash_safety(command: &str) -> Option<String> {
    let lower = command.to_lowercase();
    for pat in DANGEROUS_BASH_PATTERNS {
        // Find all occurrences and check word-boundary before each one.
        let mut start = 0;
        while let Some(pos) = lower[start..].find(pat) {
            let abs_pos = start + pos;
            let at_boundary = abs_pos == 0
                || lower[..abs_pos]
                    .chars()
                    .next_back()
                    .is_none_or(|c| BOUNDARY_CHARS.contains(&c));
            if at_boundary {
                return Some(format!("Command contains `{}`", pat.trim()));
            }
            start = abs_pos + 1;
        }
    }
    None
}

/// Check if a file path is in a sensitive location.
pub fn check_path_safety(path: &std::path::Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    for prefix in SENSITIVE_PATH_PREFIXES {
        if path_str.starts_with(prefix) {
            return Some(format!("Path is in sensitive location `{}`", prefix));
        }
    }
    None
}

/// Show a blocking approval dialog for a dangerous tool operation.
/// Returns `true` if the user approves, `false` if they deny.
pub async fn request_tool_approval(tool_name: &str, reason: &str, detail: &str) -> bool {
    let message = format!(
        "The LLM wants to use the {} tool.\n\n\u{26a0}\u{fe0f} {}\n\n{}\n\nAllow this operation?",
        tool_name, reason, detail
    );

    tokio::task::spawn_blocking(move || {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("NeuroSkill \u{2014} Tool Approval Required")
            .set_description(&message)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show() == rfd::MessageDialogResult::Yes
    }).await.unwrap_or_else(|e| {
        crate::tool_log!("tool", "[safety] approval dialog failed: {}", e);
        false
    })
}

// ── Helper for logging blocked operations ─────────────────────────────────────

/// Log and return a JSON error for a blocked tool invocation.
#[allow(dead_code)]
pub(crate) fn blocked_json(tool_name: &str, reason: &str) -> serde_json::Value {
    json!({ "ok": false, "tool": tool_name, "error": reason })
}
