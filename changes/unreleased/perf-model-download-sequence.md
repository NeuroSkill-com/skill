### Performance

- **An already-downloaded model resolves instantly instead of being re-hashed.** Every call re-read the whole blob to verify its SHA-256 — 11.7 s for a 378 MB model, scaling linearly, so merely opening the download screen for a cached 5 GB model stalled for minutes. A blob that is exactly the expected size *and* already linked into the snapshot for this commit was hashed and promoted by a previous run, so it is now trusted (as `hf_hub` does): **11.7 s → 0.5 s**, measured. `SKILL_DOWNLOAD_VERIFY_CACHED=1` forces the old full re-check for a cache you suspect.
- **Verification reports progress instead of looking hung.** Hashing a freshly-downloaded file took ~12 s of a measured 48 s download with the bar frozen at 99 % behind a stale "378 / 378 MB · 10.6 MB/s · 0s left" — indistinguishable from a hang, at exactly the moment a user is most likely to force-quit and leave a partial behind. It now reports `Verifying… 42%`.

### Bugfixes

- **Transfers now retry instead of giving up on the first network hiccup.** A single dropped read aborted the whole download and waited for a human to press the button again — which, for a multi-gigabyte model on unreliable wifi, meant it often never finished. Up to five attempts with capped exponential backoff (1→8 s, ~15 s total). Each attempt re-reads the partial file and resumes from it, so nothing already transferred is re-fetched. Cancellations, pauses, HTTP 4xx and hash mismatches are **not** retried — they are answers that will not change. A cancel during a backoff is acted on within 200 ms rather than after the full wait.
- **The transfer is pinned to the commit its metadata came from.** Bytes were fetched from `resolve/main/` while the hash to verify against, the `.incomplete` filename and the snapshot directory all came from the resolved commit. If the repo moved in between — far more likely across a cancelled download and a resume days later — the *new* file's bytes were appended to the *old* file's partial, and only the final hash caught it, after paying for the transfer twice.
- **A short read is treated as an interruption, not a success.** A connection that closed early left `.incomplete` shorter than expected and fell through to the size check as a hard failure; it is now a retryable condition that resumes.

### CLI

- **`cargo run -p skill-llm --example download_probe`** drives the real downloader against a public repo and prints what the UI would show. It found both performance problems above; pass a megabyte count as the third argument to interrupt partway and verify that re-running resumes.
