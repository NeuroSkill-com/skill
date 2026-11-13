### Build

- **`npm run bump` no longer prints the whole dependency graph**: its Cargo.lock refresh runs `cargo metadata` only for the lockfile update, but inherited stdout and dumped several MB of JSON into the terminal. stdout is now discarded; cargo's progress and errors (stderr) still show.
