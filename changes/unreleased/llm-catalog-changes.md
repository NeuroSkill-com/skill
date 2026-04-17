## [Unreleased]

### Features

- **LLM catalog**: Added Qwen3.6 35B-A3B (MoE), MiniMax M2.7, and GLM 5.1 models to the local LLM catalog.
- **HuggingFace GGUF search**: New UI section in Settings → LLM to search HuggingFace Hub for GGUF models. Browse repos by downloads/likes, expand to see individual quant files with sizes, and add models to the catalog with one click (optionally starting download immediately). Each repo shows a collapsible README (rendered as markdown with proper table and heading styling, YAML frontmatter stripped) and a direct link to the HuggingFace page. Two new daemon endpoints: `GET /llm/catalog/search` and `GET /llm/catalog/search/files` (the latter also returns the repo README, capped at 32 KB). Fully localized (9 languages).
