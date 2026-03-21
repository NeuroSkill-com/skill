### Features

- **Status command enriched with usage analytics**: The `status` command now returns most-used apps (all-time, 24h, 7d), most frequent label texts (all-time, 24h, 7d), screenshot/OCR summary counts (total, with embedding, with OCR, with OCR embedding), top screenshot apps (all-time, 24h), and label text-embedding count. New fields: `apps`, `screenshots`, and expanded `labels` section in the JSON response.

### LLM

- **Status tool results formatted as readable text**: When the internal LLM calls the `status` command, the result is now converted from raw JSON to a human-readable text block with clear section headers (Device, Session, EEG Embeddings, Labels, Most Used Apps, Screenshots, Signal Quality, Current Scores, Hooks, Sleep, Recording History). This makes status output in the Chat window much easier to read for both the user and the model.
