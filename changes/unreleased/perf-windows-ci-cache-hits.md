### Performance

- **Improve Windows CI build caching**: stabilize Rust cache reuse with a shared `rust-cache` key and enable persisted GitHub Actions `sccache` backend to increase cache-hit rates and reduce repeated full recompiles.
- **Add compile cache diagnostics**: print `sccache --show-stats` before and after Rust compile in the Windows release workflow to make cache effectiveness visible in job logs.
