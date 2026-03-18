### Bugfixes

- **Emotiv & IDUN dashboard UI incomplete**: Added `isEmotiv` / `isIdun` capability flags to the dashboard, device images, alt text, battery visibility for Emotiv, and device-specific scanning message for Emotiv Cortex API connections.
- **ElectrodeGuide missing Emotiv & IDUN tabs**: Added Emotiv EPOC (14-ch) and IDUN Guardian (1-ch) tabs with correct electrode positions to the 3D electrode guide.
- **MN8 earbuds not detected in frontend**: `deviceCapabilities()` was missing the `mn8` prefix for Emotiv MN8 earbuds.

### i18n

- **Emotiv scanning message**: Added `dashboard.connectingEmotiv` key in all five languages (en, de, fr, uk, he).
