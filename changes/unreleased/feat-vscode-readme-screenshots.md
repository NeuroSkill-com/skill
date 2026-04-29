### UI

- **Theme-aware sidebar screenshots in the README**: every screenshot now ships in two variants (`*-dark.png`, `*-light.png`) and is embedded via a `<picture>` element with `prefers-color-scheme` sources. GitHub serves the variant matching the reader's OS theme; the marketplace gets the dark fallback. Six states captured: in-flow, stuck, fatigued, low-focus / off-peak, daemon-disconnected, status-bar strip.
- **Sidebar mock library + Playwright generator**: HTML stubs in `extensions/vscode/media/preview/` render the sidebar webview offline (no daemon required). A shared `_styles.css` drives both themes via a `:root.light` override; `npm run screenshots` (Playwright) toggles `<html class="light">` between captures and writes 12 PNGs to `media/screenshots/`.
- **Marketplace README pipeline**: `scripts/build-marketplace-readme.mjs` rewrites every relative `media/...` path (including `<source srcset>`, which `vsce` doesn't touch) to absolute GitHub raw URLs at package time. The source `README.md` keeps relative paths for GitHub + offline viewing; the generated `.marketplace.readme.md` is what `vsce package --readme-path` ships.

### Bugfixes

- **Equal padding on screenshot edges**: viewport width was 360px while the body was 320px wide, leaving an asymmetric 40px right gutter. Matched viewport to body width so left and right gutters are now identical.
