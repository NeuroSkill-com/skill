### Bugfixes

- **Lazy embedder retry**: When the text embedding model fails to initialise at startup (e.g. missing download, network error), semantic label search, interactive search, and re-embed commands now retry initialisation on demand instead of permanently returning "embedder not initialized".
