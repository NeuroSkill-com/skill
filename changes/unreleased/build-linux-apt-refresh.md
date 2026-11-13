### Build

- **Linux release setup recovers from a bad apt install instead of only reporting it**: the cached install is now non-fatal, `pkg-config` decides whether glib, GTK, libsoup and WebKit are really present, and if not they are installed directly with up to three retries. The step fails, with a clear message, only if that also fails. The package list now lives in one file shared by both paths.
