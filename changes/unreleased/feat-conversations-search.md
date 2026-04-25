### Features

- **Conversations table**: stores all AI coding assistant messages (Claude, Pi) with app, role (user/assistant/tool), text, cwd, timestamp, session ID, EEG focus/mood.
- **FTS5 full-text search**: SQLite virtual table for instant keyword search across all conversation text. `POST /v1/brain/search-conversations {"query":"JWT","mode":"fts"}`.
- **Fuzzy search**: LIKE-based substring matching for partial queries. `{"query":"rate limit","mode":"fuzzy"}`.
- **Structured search**: filter by app, role, time range. `{"mode":"structured","app":"claude","role":"user","since":...}`.
- **Semantic search**: user prompts embedded via fastembed (nomic-embed-text-v1.5, local, no API credits) and stored in HNSW label index. Searchable by meaning, not just keywords.
- **Generic embedding store**: `embeddings` table decoupled from specific data tables. Multi-model support — can re-embed with different models, store multiple vectors per item. Source tracking (source_type, source_id).
- **Code context HNSW index**: separate `code_context_index.hnsw` file for code-specific semantic search, keeping EEG label searches uncontaminated.
- **Conversation events**: VS Code extension sends `conversation_message` events to daemon for each Claude/Pi message. User prompts get embedded; assistant responses and tool calls get FTS-indexed only (saves compute).
- **Session tracking**: messages grouped by JSONL filename (session ID). Timestamps from app's own data (ISO 8601 UTC), not extension read time.
- **Embedding settings**: `neuroskill.embedding.maxInputLength` (default 1000 chars) and `neuroskill.embedding.enableConversations` (default true) in VS Code settings.

### Tauri UI

- **AI Conversations section** in Activity tab: timestamped message thread with role icons (user/assistant/tool), app name, EEG focus score, scrollable.

### VS Code Sidebar

- **Conversations card**: timestamped messages with role icons, app badge, collapsible.
- **AI Usage card**: Copilot/Codeium acceptance rate bar, suggestion count, source breakdown chips.
