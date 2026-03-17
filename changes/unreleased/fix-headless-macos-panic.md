### Bugfixes

- **Headless browser no longer crashes the app on macOS**: On macOS, tao requires the event loop on the main thread, but Tauri already owns it. The headless browser launch (used by `web_search render=true` and `web_fetch render=true`) panicked with "EventLoop must be created on the main thread!", crashing the entire app. Now `Browser::launch` is wrapped in `catch_unwind` and both tools gracefully fall back to plain HTTP fetch with HTML tag stripping when the headless browser is unavailable. The fallback produces clean text content suitable for LLM consumption.
