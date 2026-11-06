### Build

- **`npm run release -- --stable` ships a clean `x.y.z` build.** Promoting an RC flips its pre-release flag, so the shipped app, the updater manifest and the GitHub release all still say `x.y.z-rc.N`. `--stable` bumps to the bare version instead, tags it and lets the release workflows build it fresh as a regular release; its notes are the RCs' notes merged. Run it again once the builds are green to merge the release PR. Plain `npm run release` still promotes the tested RC byte-for-byte.
