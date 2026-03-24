### Refactor

- **ScreenshotsTab logic extraction**: extracted `screenshots-logic.ts` with rolling history buffer, microsecond/millisecond/ETA formatting, and SVG sparkline/area path generation — 16 unit tests.

- **InteractiveGraph3D logic extraction**: extracted `graph3d-logic.ts` with 3D vector math (`add3`, `scale3`, `normalize3`, `length3`), Fibonacci sphere layout, and Turbo colormap (`turbo`, `turboCss`, `turboHex`) — 16 unit tests.
