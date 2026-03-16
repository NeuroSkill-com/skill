### Bugfixes

- **Fix LINUX.md path in packaging scripts**: Updated `package-linux-dist.sh` and `package-linux-system-bundles.sh` to reference `docs/LINUX.md` instead of the non-existent root-level `LINUX.md`, fixing a `cp: cannot stat` error on Linux CI.
