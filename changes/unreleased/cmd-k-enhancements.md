### Features

- Add recent/frequent command ranking to Cmd-K. Commands you use most often and most recently appear at the top in a "Recent" section, with frequency and recency boosts applied to search scoring.
- Add typo tolerance to Cmd-K search. Misspellings like "drak mode" or "calbriation" now find the right result using Damerau-Levenshtein edit distance as a fallback when fuzzy matching fails.
- Add contextual boosting to Cmd-K. Device, EEG, Calibration, and LSL commands are ranked higher when a device is connected; Devices settings are boosted when no device is connected.
- Add keyboard-driven toggles in Cmd-K. Toggleable settings (dark mode, high contrast) show an ON/OFF indicator and can be flipped with the Tab key without closing the palette.
- Add prefix-based command filtering to Cmd-K. Type `>` to show only system commands or `@` to show only settings.
- Add semantic search fallback to Cmd-K via fastembed embeddings. When fuzzy results are sparse and the query is 6+ characters, a debounced request to the daemon's `/v1/search/commands` endpoint returns semantically similar commands in a "Suggested" section. Handles natural language queries like "make text bigger" or "reduce eye strain".
- Add toggle dark/light mode command to the Cmd-K palette.

### Server

- Add `POST /v1/search/commands` endpoint for semantic command search. Accepts a query and candidate list, embeds both with nomic-embed-text-v1.5 via fastembed, and returns the top 5 results ranked by cosine similarity.
