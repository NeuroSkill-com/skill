### Dependencies

- **Frontend dependency update** (#94): SvelteKit 3.1, adapter-static 4, Vitest 5, Biome 2.6, maplibre-gl 6, tailwind-variants 4, three 0.187, sharp 0.35, Playwright 1.64, svelte-check 4.7 and 18 smaller bumps.

### Refactor

- **SvelteKit 3 migration.** SvelteKit 3 no longer reads `svelte.config.js`, so its settings (static adapter with the `index.html` fallback, Vite preprocessing) moved into the `sveltekit()` plugin in `vite.config.js`. It also removed the `$lib` import alias: the 907 `$lib/…` imports are now `#lib/…`, mapped in `package.json` `"imports"` (with a matching TypeScript `paths` entry, since TypeScript does not add file extensions to `"imports"` targets). `tsconfig.json` now extends `$app/tsconfig`, the error page reads `page` from `$app/state` instead of the removed `$app/stores`, and the search page updates its URL with a shallow `goto` instead of the deprecated `replaceState`.
- **Biome 2.6 configuration.** `biome.json` was migrated to the 2.6 schema (`"preset": "recommended"`). The new `useDomNodeTextContent` rule is off for the Playwright tests, where it misreads `locator.innerText()` as the DOM property and its fix would change what the tests assert.

### Build

- **The Homebrew cask follows releases again** (#80). The workflow that bumps `Casks/neuroskill.rb` after a stable release opened a pull request, which GitHub Actions is not allowed to do in this repository, so it failed and the cask stayed at 0.0.129 after 0.1.0 shipped. It now commits the bump (version and the release asset's sha256 digest) straight to `main`, and the cask is updated to 0.1.0.
- **`npm run setup` installs CMake on macOS and uses `npm ci`** (#78). CMake is needed to build `sentencepiece-sys` and `highs-sys` but was only checked on Linux. Dependencies now install with `npm ci` when `package-lock.json` exists, the way CI installs them, instead of `npm install`, which re-resolves versions and can fail on peer ranges the lockfile already settled (#81).
- **Building the frontend needs Node.js 22.17 or newer**, up from 20. SvelteKit 3 requires 22.17 and Vitest 5 requires 22.12, so on Node 20 the dependencies install but the frontend does not build or test. `package.json` `engines`, CONTRIBUTING and the development guide now say 22.17, and `npm run setup` checks the version, which it used to print as fine whatever it was. When Node is missing or older, setup now offers to install the latest Node 22 LTS from nodejs.org into `~/.local` (checksum-verified against the release's `SHASUMS256.txt`, no sudo needed), links `node`, `npm` and `npx` into `~/.local/bin`, and prints the line to add to your shell profile if that directory is not on your `PATH`. A new `.nvmrc` pins Node 22 for nvm, fnm and similar version managers. CI already runs Node 22.

### Docs

- **The README's download links work** (#82). They pointed at unversioned file names under `releases/latest/download/` that no release has ever had, and promised a Linux AppImage that isn't built. They now link the latest release page and name the real files: the macOS `.dmg`, the Windows `-setup.exe`, and the Linux `.deb`, `.rpm` or portable `.tar.gz`.
