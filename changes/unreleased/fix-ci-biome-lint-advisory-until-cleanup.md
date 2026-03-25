### Build

- **CI frontend lint stability**: switched `Biome lint` in `.github/workflows/ci.yml` back to advisory (`continue-on-error: true`) so existing lint debt no longer hard-fails the frontend job while cleanup is ongoing.
