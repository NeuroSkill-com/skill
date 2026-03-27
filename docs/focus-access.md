Title: Focus (Do Not Disturb) DB access — Full Disk Access instructions

Why this matters
Many apps and tools query the Focus / Do Not Disturb database at
~/Library/DoNotDisturb/DB/Assertions.json. On modern macOS this file is
protected by TCC (Full Disk Access / privacy) and some processes will
get "Operation not permitted" when trying to read it.

What we cannot do for you
- We cannot programmatically grant Full Disk Access to an app or to
  Terminal. macOS requires explicit user action in System Settings to
  add an app to Full Disk Access.

What this repo adds
- scripts/check_focus_db.sh — small script that checks whether the
  running process can read the Focus DB and prints diagnostics. It will
  also try to open the Privacy & Security preference pane so you can add
  the app.

How to use
1) Run the checker from Terminal (this runs as your current user):
   ./scripts/check_focus_db.sh

2) If you see "Read succeeded" — nothing else is needed.

3) If you see "Operation not permitted" or the script prints a failure:
   a) Open System Settings → Privacy & Security → Full Disk Access
   b) Click the plus (+) and add Terminal (if you run the app via
      Terminal) or add the built app (the .app bundle) if you run the
      packaged macOS app.
   c) After adding, quit and re-open the app (or restart Terminal). On
      some macOS versions you must log out and back in.
   d) Re-run ./scripts/check_focus_db.sh — it should now show "Read
      succeeded".

Notes for developers
- There is no entitlement or Info.plist key that bypasses Full Disk
  Access. TCC requires the user to explicitly add the app to Full Disk
  Access.
- If your app is distributed via signed installer and you can instruct
  users, provide clear steps in your app Settings linking to this
  documentation or run the check_focus_db.sh and display its output.

If you want, I can:
- Add a small UI in the application settings that runs this check and
  shows instructions and a button to open System Settings.
- Modify the app packaging instructions to call out that Full Disk
  Access must be granted for Focus DB access.

