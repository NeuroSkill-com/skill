### Refactor

- **UmapViewer3D logic extraction**: extracted `umap-viewer-logic.ts` with pure functions for point-cloud normalization, random positions, Turbo colormap, RGB-to-hex conversion, and color array building — 14 unit tests.

- **DevicesTab logic extraction**: extracted `devices-logic.ts` with fuzzy matching, device image resolution (Muse, Emotiv, IDUN, OpenBCI, Hermes), OpenBCI channel labeling, and relative-time formatting — 21 unit tests.
