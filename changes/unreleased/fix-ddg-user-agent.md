### Bugfixes

- **Rotating browser User-Agents**: replaced bot-like User-Agent strings with a pool of 10 realistic browser UAs (Chrome, Firefox, Safari, Edge on Windows/macOS/Linux) rotated on each request to reduce fingerprinting.
- **Fix DuckDuckGo HTML search**: mimic real form submission by adding `Origin` header, correct `Referer`, and the `b=` submit-button field. Without these, DDG returns a captcha page instead of results.
