### Bugfixes

- **LLM screenshot tool commands**: Added `search_screenshots`, `screenshots_around`, `screenshots_for_eeg`, and `eeg_for_screenshots` to the `skill` tool's command enum, description, and alias resolution. The LLM can now invoke screenshot search commands correctly instead of failing with validation errors. Also maps CLI names (`search-images`, `screenshots-around`, etc.) to their WebSocket API equivalents.
