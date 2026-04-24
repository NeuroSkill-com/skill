### Bugfixes

- **InteractiveGraph3D event listener leaks**: `pointerdown` and `dblclick` listeners on the WebGL canvas were never removed in `onDestroy`. Extracted into named refs with cleanup.
- **UmapViewer3D event listener leaks**: 4 canvas event listeners (pointermove, pointerleave, pointerdown, pointerup) added as anonymous functions were never removed on unmount. Extracted into named module-level refs with removal in `onDestroy`.
- **ActivityTab brain polling leak**: `startBrainPolling()` was called on mount but `stopBrainPolling()` was never called on destroy, leaving a 30-second interval running after leaving the tab.
- **Brain polling stopped by peer component**: `stopBrainPolling()` unconditionally killed the interval even when other components still needed it. Added reference counting so polling only stops when the last consumer unmounts.
- **Memory leak in terminal_command_end labeling**: `Box::leak` was used to format non-zero exit codes in EEG auto-labeling, permanently leaking memory for every failed command. Replaced with owned `String`.
- **Screenshot encode failures untracked**: `encode_webp` failures (disk full, I/O error) silently continued without incrementing the `capture_errors` counter. Now counted so metrics reflect actual failures.
- **401 auto-retry**: daemon HTTP client now retries once with a fresh token on 401 (stale token after daemon restart) instead of failing immediately.
