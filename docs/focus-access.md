# Focus (Do Not Disturb) DB Access on macOS

## Why this matters

Some features/tools read:

`~/Library/DoNotDisturb/DB/Assertions.json`

On modern macOS, this path is protected by TCC. Without **Full Disk Access**, reads fail with `Operation not permitted`.

## What cannot be automated

NeuroSkill cannot grant Full Disk Access programmatically. macOS requires explicit user action in **System Settings**.

## Helper in this repo

- `scripts/check_focus_db.sh` — checks whether the current process can read the Focus DB and prints diagnostics.
- The script also attempts to open the relevant Privacy settings pane.

## How to enable access

1. Run:
   ```bash
   ./scripts/check_focus_db.sh
   ```
2. If output says **Read succeeded**, you are done.
3. If output shows **Operation not permitted**:
   - Open **System Settings → Privacy & Security → Full Disk Access**
   - Add the app that needs access:
     - **Terminal** (if running via terminal)
     - or the packaged **`.app`** bundle (if running installed app)
   - Quit and reopen the app/Terminal
   - Re-run `./scripts/check_focus_db.sh`

## Developer notes

- There is no entitlement/Info.plist key that bypasses this TCC requirement.
- For user-facing flows, provide explicit Full Disk Access instructions when Focus DB reads fail.
