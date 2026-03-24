### Bugfixes

- **Fix smoke-test false-positive mDNS match**: The `dns-sd -B` header line already contains "skill", so the grep matched immediately before any service was actually discovered. Changed pattern to `Add.*_skill._tcp` which only matches a real service registration event.
