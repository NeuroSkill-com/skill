### Performance

- **Shared text embedder — eliminate 3 redundant model copies (~390 MB RAM saved)**: consolidated four independent `fastembed::TextEmbedding` instances into a single app-wide `Arc<Mutex<TextEmbedding>>`.

- **GPU optimization across all model systems**: enabled flash attention and KQV offloading in LLM; GPU-first mmproj loading on Linux; DirectML on Windows and CUDA on Linux for screenshot embeddings.
