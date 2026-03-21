### Bugfixes

- **Fix WebView2 window class unregistration error on Windows**: Stopped dropping the WebView early during Close command handling. The `Chrome_WidgetWin_0` class unregistration race (Win32 error 1412) occurred because the WebView was destroyed while the event loop and parent window were still alive. The WebView is now dropped naturally when `run_return` finishes, allowing Chromium's internal child windows to clean up before class unregistration.
