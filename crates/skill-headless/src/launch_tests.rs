// SPDX-License-Identifier: GPL-3.0-only
#[cfg(test)]
mod tests {
    use crate::{Browser, BrowserConfig};

    /// On macOS, `launch` must refuse up front rather than spawn a thread that
    /// is guaranteed to panic.
    ///
    /// tao requires its `EventLoop` on the process main thread, and `launch`
    /// always builds the loop on a freshly spawned one — so on macOS it cannot
    /// succeed, with or without a host event loop. It used to try anyway: the
    /// thread panicked with "On macOS, `EventLoop` must be created on the main
    /// thread!", the proxy channel dropped, and the caller got the unrelated
    /// `event loop did not start in time`. The daemon logged that panic on
    /// every `web_search` with `render: true` before falling back to plain
    /// HTTP, which made a platform limitation look like a crash.
    ///
    /// This test running at all is most of the point: if `launch` regresses to
    /// spawning the loop, the panic unwinds on that thread while this one sees
    /// the misleading timeout — so assert on the message, which names the real
    /// cause and the way out.
    #[test]
    #[cfg(target_os = "macos")]
    fn launch_refuses_on_macos_with_an_actionable_reason() {
        // `Browser` is not `Debug`, so unwrap the Result by hand.
        let msg = match Browser::launch(BrowserConfig::default()) {
            Ok(_) => panic!("launch unexpectedly succeeded on macOS off the main thread"),
            Err(e) => e.to_string(),
        };
        assert!(
            msg.contains("main thread"),
            "error should name the main-thread requirement: {msg}"
        );
        assert!(
            msg.contains("set_external_renderer") || msg.contains("external renderer"),
            "error should point at the supported alternative: {msg}"
        );
        assert!(
            !msg.contains("did not start in time"),
            "that message describes a timeout we no longer wait for: {msg}"
        );
    }

    /// The opt-in escape hatch still works for hosts that own the main loop.
    #[test]
    fn an_unavailable_host_gets_a_clear_error() {
        Browser::set_unavailable();
        let msg = match Browser::launch(BrowserConfig::default()) {
            Ok(_) => panic!("launch succeeded despite set_unavailable()"),
            Err(e) => e.to_string(),
        };
        assert!(!msg.is_empty(), "error must say something");
    }
}
