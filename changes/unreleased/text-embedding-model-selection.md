### Features

- Add text embedding model selection. Settings → Embeddings now shows 16 fastembed models (nomic, BGE, MiniLM, E5, MxBAI, GTE) with dimensions and descriptions. Selecting a model downloads weights and hot-swaps the ONNX runtime in the daemon. Persisted to settings and loaded on startup. Backed by `GET/POST /v1/models/text-embedding` daemon API. Unknown model codes are rejected.

### UI

- Move Embeddings tab directly below EXG in the settings sidebar.
