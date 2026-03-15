# skill-tray

System tray icon helpers — progress-ring overlay, shortcut formatting, and dedup fingerprints.

## Overview

Pure-`std` utility crate with zero external dependencies. Provides the pixel-level logic for rendering a circular progress ring onto an RGBA icon buffer, keyboard-shortcut label formatting, and helpers for avoiding redundant tray icon rebuilds.

## Public API

### Progress ring

| Function | Description |
|---|---|
| `overlay_progress_bar(buf, w, h, progress)` | Composites a circular progress arc onto a raw RGBA buffer. Decoupled from `tauri::Image` — the Tauri layer converts the output. |

### Shortcut formatting

| Function | Description |
|---|---|
| `shortcut_suffix(shortcut)` | Returns the platform-aware display string (replaces `CmdOrCtrl` with `⌘` / `Ctrl`) |
| `with_shortcut(label, shortcut)` | Appends the shortcut suffix to a menu label |

### Text helpers

| Function | Description |
|---|---|
| `ellipsize_middle(text, max_chars)` | Middle-truncates long strings with `…` |

### Deduplication

| Function | Description |
|---|---|
| `progress_bucket(progress)` | Quantize progress to a bucket for icon-update dedup |
| `progress_percent(progress)` | Convert progress to integer percent |

### Constants

| Constant | Description |
|---|---|
| `MENU_REBUILD_MIN_MS` | Minimum interval between tray menu rebuilds (300 ms) |

## Dependencies

None — pure `std`.
