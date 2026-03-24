### Bugfixes

- **Smoke test port discovery**: Fixed `smoke-test.sh` failing because `test.ts` tried its own 5-second mDNS browse which raced and failed. The script now resolves the port via `dns-sd -L` after the browse succeeds and passes it explicitly to `test.ts`, eliminating the double-discovery race condition.
