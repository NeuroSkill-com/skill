# skill-autostart

Platform-specific launch-at-login (autostart) registration.

## Overview

Abstracts the OS-level mechanism for starting NeuroSkill automatically at login. Each platform has its own backend:

| Platform | Mechanism |
|---|---|
| **macOS** | `SMAppService` / Login Items |
| **Linux** | XDG autostart `.desktop` file in `~/.config/autostart/` |
| **Windows** | Registry key under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |

## Public API

| Function | Description |
|---|---|
| `is_enabled(app_name)` | Check whether autostart is currently registered |
| `set_enabled(app_name, enabled)` | Register or remove the autostart entry |

## Dependencies

- `skill-constants` — app identifier constants
