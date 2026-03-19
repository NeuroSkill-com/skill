### Bugfixes

- **Devices are no longer auto-paired**: previously every device that connected was automatically added to the paired list, even without the user clicking "Pair". Now only explicitly paired devices are remembered. The single exception is first-time onboarding: if no devices are paired at all, the first successful connection auto-pairs as a convenience so new users can test immediately.
- **Auto-connect requires explicit pairing**: the scanner no longer auto-connects USB or other "trusted transport" devices without pairing. Only devices the user has explicitly paired (or first-time onboarding) trigger auto-connect.
- **Startup auto-connect skipped when no paired devices**: on first launch with no paired devices, the app no longer blindly scans and connects to the first device it finds. The user must discover and pair a device manually.
