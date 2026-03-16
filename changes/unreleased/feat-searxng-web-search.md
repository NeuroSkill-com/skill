### Features

- **SearXNG web search with background instance refresh**: the `web_search` tool now fetches the list of public SearXNG instances from `https://searx.space/data/instances.json` at app startup and refreshes it every hour in a background thread. Instances are filtered for HTTPS, normal network, HTTP 200 status, and < 1s median response time. On each search, up to 3 randomly-selected instances are tried with tight timeouts (2s connect / 3s read). If a user-configured SearXNG URL is set, it is tried first. DuckDuckGo HTML scraping remains the final fallback.

### i18n

- **SearXNG settings strings**: added SearXNG URL field label and description translations in en, de, fr, uk, and he.

### Dependencies

- **fastrand**: added `fastrand` dependency to `skill-tools` for random instance selection.
