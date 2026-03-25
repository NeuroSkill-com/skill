### Bugfixes

- **Auto-bootstrap chat LLM on first start**: when starting the chat server with no downloaded text model, the app now downloads the smallest `LFM2.5-VL 1.6B` variant first and then starts the server once download completes.
- **Onboarding LLM priority aligned to bootstrap**: first-run model targeting now prefers the smallest `LFM2.5-VL 1.6B` option before other recommendations.
