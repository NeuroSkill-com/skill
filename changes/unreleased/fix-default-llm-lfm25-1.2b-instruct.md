### Bugfixes

- **Default LLM switched to LFM2.5 1.2B Instruct**: first-run chat/bootstrap now prefers `LFM2.5 1.2B Instruct` (Q4_K_M first) instead of `LFM2.5-VL 1.6B`.
- **Default activation on bootstrap**: when no local text model exists, the selected default is set as active immediately and downloaded automatically before server start.
