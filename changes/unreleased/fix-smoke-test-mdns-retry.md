### Bugfixes

- **Smoke test mDNS retry**: Moved mDNS discovery from `smoke-test.sh` (bash `dns-sd`) into `test.ts` (bonjour-service). Discovery now retries indefinitely with a 3-second backoff until the Skill server appears or the user presses Ctrl-C, fixing the "could not resolve port" failure on slow startups.
