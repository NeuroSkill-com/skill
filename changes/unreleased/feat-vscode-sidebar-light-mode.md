### UI

- **Sidebar webview now renders correctly in light themes** (`extensions/vscode/src/sidebar.ts`). Three theme-fragile spots fixed:
  - `.metric-card` background switched from `--vscode-sideBar-background` (which is the same colour as the surrounding sidebar — cards collapsed into a faint border in light mode) to `--vscode-editorWidget-background`, with `--vscode-input-background` and a translucent grey as cascading fallbacks.
  - `.ai-bar` track switched from `--vscode-panel-border` to `--vscode-progressBar-background` so the empty bar is visible against a white background.
  - Row-divider opacity bumped from 0.08 to 0.18; added `:last-child { border-bottom: none }` on commit and AI-metric rows so the last row doesn't double up against the disclaimer footer.
- **State colours stay hard-coded** — red ring for stuck, green for flow, amber for warning. They're semantic, not chrome, and they read fine on both themes.
