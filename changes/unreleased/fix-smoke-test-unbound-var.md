### Bugfixes

- **Fix smoke-test unbound variable**: `scripts/smoke-test.sh` failed with `unbound variable` when invoked without arguments due to `set -u` and bare `${*}`. Changed to `${*:-}` to default to empty string.
