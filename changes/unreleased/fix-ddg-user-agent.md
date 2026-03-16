### Bugfixes

- **Rotating browser User-Agents**: replaced bot-like User-Agent strings with a pool of 10 realistic browser UAs (Chrome, Firefox, Safari, Edge on Windows/macOS/Linux) rotated on each request to reduce fingerprinting.
- **Fix DuckDuckGo HTML search**: mimic real form submission by adding `Origin` header, correct `Referer`, and the `b=` submit-button field. Fixed HTML parser to split on `class="result results_links"` (the actual outer wrapper) instead of `class="result__body"` which is now a multi-class attribute and no longer matches.
- **Extracted `parse_ddg_html`**: separated HTML parsing from HTTP fetching for testability. Added offline unit tests for result parsing and DDG redirect URL unwrapping.
