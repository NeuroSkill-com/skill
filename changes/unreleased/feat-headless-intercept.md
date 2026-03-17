### Features

- **skill-headless: network interception**: Added request/response interception support. `EnableInterception` monkey-patches `fetch()` and `XMLHttpRequest` to capture all HTTP traffic. Navigation events are recorded via wry's navigation handler. `SetBlockedUrls` blocks navigations matching URL substring patterns. `GetInterceptedRequests` retrieves the full network log (requests, responses, navigations) with optional clear-on-read. Includes 11 tests covering fetch GET/POST, XHR with custom headers, navigation capture, URL blocking, and log clearing.
