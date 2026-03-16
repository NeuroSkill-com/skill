### Features

- **SearXNG web search support**: added optional SearXNG instance URL to the LLM tool configuration. When set, the `web_search` tool queries the SearXNG JSON API first, falling back to DuckDuckGo HTML scraping if the instance is unreachable or returns no results. Configurable in the LLM settings UI under the Web Search toggle.

### i18n

- **SearXNG settings strings**: added translations for the SearXNG URL field label and description in en, de, fr, uk, and he.
