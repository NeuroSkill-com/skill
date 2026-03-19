### Bugfixes

- **Disable GIF capture in periodic screenshot loop**: The normal app screenshot worker no longer produces animated GIFs via motion detection. GIF burst capture is now reserved exclusively for scripts. The `gif_encode` module and config fields are preserved for the script-level API.
