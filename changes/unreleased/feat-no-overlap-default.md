### Features

- **No overlap by default**: Changed the default embedding epoch overlap from 2.5 s to 0.0 s, so consecutive epochs no longer overlap. Users can still configure overlap via settings. This doubles the effective epoch interval from 2.5 s to 5.0 s, reducing redundant GPU work.
