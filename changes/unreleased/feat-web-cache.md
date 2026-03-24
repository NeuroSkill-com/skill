### Features

- **Persistent web cache for tool results**: Added a disk-backed cache (`skill_dir/web_cache/`) for `web_search` and `web_fetch` results. Avoids redundant network calls when the LLM re-fetches the same URL or repeats the same query within/across conversations. Entries are keyed by SHA-256 hash and expire via configurable TTLs. Expired entries are evicted on startup.

- **Configurable TTL and per-domain overrides**: `WebCacheConfig` in `LlmToolConfig` with `search_ttl_secs` (default 30 min), `fetch_ttl_secs` (default 2 hours), and `domain_ttl_overrides` map for fine-grained control (e.g. shorter TTL for news sites).

### UI

- **Web cache management panel**: New section in Settings > Tools (under web search) with:
  - Enable/disable toggle
  - Search TTL and fetch TTL button-group selectors (5/15/30/60 min and 30/60/120/240 min)
  - Live stats (entry count, total size)
  - Scrollable entry list with kind badge, domain, label, age, and size
  - Per-entry remove button, per-domain remove button, and clear-all button

### CLI

- **Tauri commands**: `web_cache_stats`, `web_cache_list`, `web_cache_clear`, `web_cache_remove_domain`, `web_cache_remove_entry` for frontend access to the cache.

### i18n

- **Web cache strings**: Added `llm.tools.webCache*` keys to all five locales (en, de, fr, he, uk).

### Dependencies

- **sha2**: Added `sha2 = "0.10"` to `skill-tools` for cache key hashing.
