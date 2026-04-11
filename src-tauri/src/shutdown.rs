// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Graceful shutdown: flush settings, stop LLM server, tear down TTS.

use crate::helpers::save_settings_now;
use crate::tts::tts_shutdown;

pub(crate) static EXIT_SHUTDOWN_STARTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(crate) fn run_blocking_exit_shutdown(app: &tauri::AppHandle) {
    if EXIT_SHUTDOWN_STARTED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }

    // Flush any pending debounced settings to disk before exit.
    save_settings_now(app);

    #[cfg(feature = "llm")]
    {
        let _ = crate::daemon_cmds::llm_server_stop();
    }

    tts_shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn exit_shutdown_guard_prevents_double_entry() {
        // Reset to known state for this test.
        EXIT_SHUTDOWN_STARTED.store(false, Ordering::SeqCst);

        // First swap should return false (was not started).
        let was_started = EXIT_SHUTDOWN_STARTED.swap(true, Ordering::AcqRel);
        assert!(!was_started, "first call should proceed");

        // Second swap should return true (already started).
        let was_started = EXIT_SHUTDOWN_STARTED.swap(true, Ordering::AcqRel);
        assert!(was_started, "second call should be suppressed");

        // Clean up for other tests that might run in the same process.
        EXIT_SHUTDOWN_STARTED.store(false, Ordering::SeqCst);
    }
}
