### Features

- **Configurable tool context compression**: Added a new "Context compression" setting (Off / Normal / Aggressive) in Settings → LLM → Tools that controls how tool results are compressed before being injected into the conversation context. Normal mode caps web search results to 5, truncates long URLs, and compresses old tool results. Aggressive mode uses tighter limits for small context windows. Custom overrides for max search results and max result characters are available when compression is enabled.

### Bugfixes

- **Web search no longer stalls after returning URLs**: Improved `web_search` tool description to instruct the LLM to use `render=true` for factual/current-data queries (weather, prices, scores, news). When `render=false`, the tool result now includes a follow-up hint telling the model to fetch page content. Added a weather example to the system prompt so the model learns the correct pattern.
- **Context window no longer fills up during multi-step tool chains**: The orchestration loop now condenses prior-round tool results to one-line summaries after each round (e.g. `[location: Boston, MA, US (America/New_York)]`, `[web_search: 5 results for "weather Boston"]`). The model already consumed those results and chose its next action, so the full content is no longer needed. This frees ~200-500 tokens per prior round, allowing 3-4 step chains (location → search → fetch → answer) to complete even on 4 K context models. Additionally, `web_search` returns compact text instead of verbose JSON, `web_fetch` is capped to configured limits, and headless-rendered page text is reduced from 4 K to 2 K chars per URL.

### UI

- **Tool context compression controls**: Added compression level selector and optional max-search-results / max-result-chars overrides to both the Settings → LLM → Tools tab and the inline chat tools panel.

### i18n

- **Context compression labels**: Added translations for context compression settings in English, German, French, Hebrew, and Ukrainian.
