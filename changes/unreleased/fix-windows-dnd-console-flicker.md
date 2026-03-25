### Bugfixes

- **Hide Windows DND helper console windows**: run `reg query` / `reg add` with `CREATE_NO_WINDOW` so background DND polling no longer flashes a terminal window every 5 seconds on Windows.
