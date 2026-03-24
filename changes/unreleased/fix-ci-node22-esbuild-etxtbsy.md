### Build

- **Node 22 in CI**: Bumped all workflows from Node 20 to Node 22, satisfying `camera-controls@3.1.2` engine requirement and aligning with current LTS.
- **esbuild ETXTBSY retry**: Added `npm ci || npm ci` retry in ci.yml to handle transient ETXTBSY race condition during esbuild postinstall on Linux runners.
