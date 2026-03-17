### Features

- **CLI: screenshot search commands**: Added `search-images` and `screenshots-around` CLI commands. `search-images "query"` searches screenshots by OCR text in semantic (embedding HNSW) or substring (SQL LIKE) mode. `screenshots-around --at <utc>` finds screenshots near a given timestamp within a configurable window. Both commands support `--json`, `--full`, and `--k` flags. Also added the corresponding `search_screenshots` and `screenshots_around` WebSocket/HTTP commands to the server dispatcher.
