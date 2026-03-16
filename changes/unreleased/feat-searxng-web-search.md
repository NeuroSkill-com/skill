### Features

- **Configurable web search provider**: the `web_search` tool now supports three backends — **DuckDuckGo** (default, no API key), **Brave Search** (free tier: 2,000 queries/month with API key), and **SearXNG** (self-hosted instance URL). A new `WebSearchProvider` config struct holds the backend choice, Brave API key, and SearXNG URL. Each backend falls back to DuckDuckGo HTML scraping if it fails.
- **Search provider UI**: added a backend selector (DuckDuckGo / Brave / SearXNG) to the Tools settings tab, with conditional API key and URL inputs.

### Bugfixes

- **Remove broken public SearXNG instance scraping**: public SearXNG instances universally block automated API access with 429 rate limits or anti-bot captchas. Removed the background instance list fetcher and random instance selection. SearXNG now requires a user-provided self-hosted instance URL.

### i18n

- **Search provider strings**: added search provider selector, Brave API key, and SearXNG URL translations in en, de, fr, uk, and he.
