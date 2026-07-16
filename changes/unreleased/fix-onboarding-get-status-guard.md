### Bugfixes

- **Fix Onboarding Get Status Guard**: guard the onboarding wizard's daemon status call with try/catch so a daemon hiccup on first run can no longer freeze the entire wizard (status listener, calibration load, and TTS init never registered when the unguarded call threw).
