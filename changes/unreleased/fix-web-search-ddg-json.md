### Bugfixes

- **Remove broken DuckDuckGo JSON API search**: the DuckDuckGo Instant Answer API (`api.duckduckgo.com`) has been deprecated and returns empty results for most queries, adding unnecessary latency. Removed it and now use HTML scraping directly as the sole search strategy.
