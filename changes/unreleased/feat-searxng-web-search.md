### Features

- **SearXNG web search support**: the `web_search` tool now automatically fetches the list of public SearXNG instances from searx.space, filters for fast HTTPS instances (< 1s median response time), and queries up to 3 randomly-selected instances with a 2s connect / 3s read timeout. If a user-configured SearXNG URL is set, it is tried first. DuckDuckGo HTML scraping remains the final fallback. The public instance list is cached for 1 hour.

### i18n

- **SearXNG settings strings**: added SearXNG URL field label and description translations in en, de, fr, uk, and he.

### Dependencies

- **fastrand**: added `fastrand` dependency to `skill-tools` for random instance selection.
