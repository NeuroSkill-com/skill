### Features

- **Configurable tool context compression**: Added a new "Context compression" setting (Off / Normal / Aggressive) in Settings → LLM → Tools that controls how tool results are compressed before being injected into the conversation context. Normal mode caps web search results to 5, truncates long URLs, and compresses old tool results. Aggressive mode uses tighter limits for small context windows. Custom overrides for max search results and max result characters are available when compression is enabled.

### Bugfixes

- **Web search no longer stalls after returning URLs**: Improved `web_search` tool description to instruct the LLM to use `render=true` for factual/current-data queries (weather, prices, scores, news). When `render=false`, the tool result now includes a follow-up hint telling the model to fetch page content. Added a weather example to the system prompt so the model learns the correct pattern.
- **Context window no longer fills up after web search**: When compression is enabled (default), `web_search` now returns a compact text summary instead of verbose JSON — cutting result size by ~50%. `web_fetch` content is capped to the configured max-result-chars (2 K default, 1 K aggressive) instead of the previous hardcoded 12 K. Headless-rendered page text per URL reduced from 4 K to 2 K chars. Old tool results are further compressed in subsequent rounds. Combined, this leaves enough context for the LLM to continue with follow-up tool calls.

### UI

- **Tool context compression controls**: Added compression level selector and optional max-search-results / max-result-chars overrides to both the Settings → LLM → Tools tab and the inline chat tools panel.

### i18n

- **Context compression labels**: Added translations for context compression settings in English, German, French, Hebrew, and Ukrainian.
