# Screenshot Capture + Vision-Encoder Embedding System

## Overview

Every ~5 seconds (aligned with EEG embedding epoch cadence), capture a screenshot
of the **active application window**, encode it through a **vision embedding
model**, and store the raw embedding alongside metadata in SQLite + HNSW.
The shared `YYYYMMDDHHmmss` timestamp is the cross-modal join key to EEG
embeddings and text embeddings.

**Default encoder**: `fastembed::ImageEmbedding` (CLIP ViT-B/32, 512-dim).
No extra download — fastembed auto-fetches the ONNX weights into
`~/.skill/fastembed_cache/` on first use, the same cache the text embedder
already uses.

**Optional encoder**: the LLM's **mmproj vision projector** (already loaded
for multimodal chat).  Produces embeddings in the LLM's token-embedding space
so they can be directly compared with LLM text embeddings.  Selected in
Settings → Screenshots → Embedding Model.

When the user switches models, all existing screenshots can be **re-embedded**
in bulk with progress and a time estimate.

---

## Storage Layout

```
~/.skill/
  screenshots/
    20260315/
      20260315143025.webp
      20260315143030.webp
      …
  screenshots.sqlite            ← metadata + embedding blobs
  screenshots.hnsw              ← visual-similarity HNSW index (payload = timestamp)
```

---

## 1. Platform Window Capture (`screenshot.rs :: capture_active_window()`)

Captures **only the frontmost application window**, not the entire display.

| Platform    | Method | Details |
|-------------|--------|---------|
| **macOS**   | `screencapture -x -l <windowid> -t png /tmp/…` | Get window ID via `CGWindowListCopyWindowInfo` (CoreGraphics FFI). `-l <wid>` captures that single window. Falls back to full screen if window ID unavailable. |
| **Linux**   | `import -window "$(xdotool getactivewindow)" png:-` (X11) or `grim -g "$(swaymsg … \| jq …)" -` (Wayland) | Captures the active window geometry. Falls back to `scrot -u` on older X11. |
| **Windows** | Win32 `PrintWindow(hwnd)` or `BitBlt` from `GetForegroundWindow()` | Uses the same `hwnd` already obtained by `active_window.rs`. No extra permissions. |

Returns `Option<CapturedImage>`:

```rust
struct CapturedImage {
    /// Raw PNG/BMP bytes of the captured window.
    raw_bytes: Vec<u8>,
    /// Original width of the captured window.
    width:  u32,
    /// Original height of the captured window.
    height: u32,
}
```

---

## 2. Image Sizing — Configurable Intermediate Resolution

The raw window capture is resized to an **intermediate resolution** before:

- (a) saving to disk as WebP (for human review / thumbnails)
- (b) feeding to the vision encoder

The intermediate size is **configurable** in `ScreenshotConfig` and defaults
to a value matched to the active vision model:

| Model | Native input | Default `image_size` |
|---|---|---|
| fastembed CLIP ViT-B/32 | 224×224 | 224 |
| fastembed Nomic Embed Vision v1.5 | 384×384 | 384 |
| mmproj (SigLIP / LLaVA style) | 384×384 or 448×448 | 384 |

When the user selects a model, `image_size` is auto-updated to the model's
recommended value (the user can still override it).

The image is resized with **aspect-ratio-preserving fit** (Lanczos3) then
**center-padded** to `image_size × image_size` with black pixels so the
vision encoder always receives a fixed-size square.

---

## 3. Vision Embedding — Dual Backend

### 3a. Default: fastembed `ImageEmbedding` (CLIP ViT-B/32, 512-dim)

- Uses `fastembed::ImageEmbedding::try_new(ImageInitOptions::new(model).with_cache_dir(…))`
- Supports `ClipVitB32` (512-dim, default) and `NomicEmbedVisionV15` (768-dim)
- Model weights auto-download to `~/.skill/fastembed_cache/`
- Runs in the screenshot worker thread directly (no IPC needed — fastembed is
  `Send + Sync`)
- `embed_bytes(&[raw_png_or_jpeg_bytes])` → `Vec<Vec<f32>>`

### 3b. Optional: mmproj via llama-cpp-4 mtmd

Selected in Settings when the user has a multimodal LLM + mmproj loaded.

The mmproj / `MtmdContext` is **not thread-safe** — it lives inside the LLM
actor thread.  The screenshot worker sends embed requests through a dedicated
channel:

```
ScreenshotWorker thread              LLM Actor thread
─────────────────────                ─────────────────
capture window → bytes
resize → intermediate_bytes
                          ──────►   InferRequest::EmbedImage { bytes, reply_tx }
                                    mtmd_ctx.tokenize + encode_chunk + output_embd
                          ◄──────   reply_tx.send(Some(Vec<f32>))
save WebP + insert SQLite + HNSW
```

mmproj encode flow:

```
raw_bytes → MtmdBitmap::from_buf(mtmd_ctx, &bytes)
          → mtmd_ctx.tokenize(text="", bitmaps=[&bitmap], &mut chunks)
          → find the image chunk in chunks.iter()
          → mtmd_ctx.encode_chunk(&image_chunk)
          → mtmd_ctx.output_embd(n_elements)   // raw f32 slice
          → copy into Vec<f32>                  // the embedding
```

Embedding dimensionality depends on the loaded model (typically 2048–4096),
discovered at runtime from `chunk.n_tokens() * model.n_embd()`.

### 3c. Fallback Behaviour

When the selected encoder is unavailable (e.g. mmproj chosen but LLM disabled,
or fastembed model download failed):

- Screenshots are **still captured and saved** to disk and SQLite.
- The `embedding` column is `NULL`, `hnsw_id` is `NULL`.
- The `model_id` column is set to `""` (empty string).
- A **rebuild command** can retroactively embed all un-embedded screenshots
  once a model becomes available.

---

## 4. SQLite Schema (`screenshots.sqlite`)

```sql
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS screenshots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,

    -- ── Temporal keys ──────────────────────────────────────────────────────
    timestamp       INTEGER NOT NULL,     -- YYYYMMDDHHmmss UTC (join key to eeg.sqlite)
    unix_ts         INTEGER NOT NULL,     -- unix seconds (for range queries)

    -- ── File reference ─────────────────────────────────────────────────────
    filename        TEXT    NOT NULL,     -- relative path: "20260315/20260315143025.webp"
    width           INTEGER NOT NULL,     -- saved image width  (after resize)
    height          INTEGER NOT NULL,     -- saved image height (after resize)
    file_size       INTEGER NOT NULL,     -- bytes on disk

    -- ── Embedding ──────────────────────────────────────────────────────────
    hnsw_id         INTEGER,              -- row index in screenshots.hnsw (NULL if not embedded)
    embedding       BLOB,                 -- f32 LE × dim (NULL if model unavailable)
    embedding_dim   INTEGER NOT NULL DEFAULT 0,  -- vector length (512 for CLIP, varies for mmproj)

    -- ── Model provenance (for reconciliation / re-embedding) ──────────────
    model_backend   TEXT NOT NULL DEFAULT '',  -- "fastembed" | "mmproj" | ""
    model_id        TEXT NOT NULL DEFAULT '',  -- e.g. "Qdrant/clip-ViT-B-32-vision",
                                               --      "mmproj:gemma3-mmproj-bf16.gguf"
    image_size      INTEGER NOT NULL DEFAULT 0,  -- intermediate resize used (e.g. 224, 384)
    quality         INTEGER NOT NULL DEFAULT 0,  -- WebP quality (0–100)

    -- ── Context (from active-window tracker — free metadata) ──────────────
    app_name        TEXT NOT NULL DEFAULT '',  -- active window app at capture time
    window_title    TEXT NOT NULL DEFAULT ''   -- active window title at capture time
);

CREATE INDEX IF NOT EXISTS idx_ss_ts       ON screenshots (timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_ss_unix     ON screenshots (unix_ts DESC);
CREATE INDEX IF NOT EXISTS idx_ss_model    ON screenshots (model_backend, model_id);
```

### Why the extra columns?

- `model_backend` + `model_id` — know exactly which encoder produced each
  embedding.  When the user switches models, we can identify which rows need
  re-embedding vs which are already up to date.
- `image_size` + `quality` — the resize dimensions and WebP quality used when
  this row was written.  If the user changes `image_size`, re-embedding can
  optionally re-encode from the original capture (if still on disk) at the new
  resolution.
- `embedding_dim` — stored explicitly because dimensionality varies by model
  (512 for CLIP, 768 for Nomic, 2048+ for mmproj).  Avoids guessing during
  reads.

---

## 5. HNSW Index (`screenshots.hnsw`)

- `fast_hnsw::labeled::LabeledIndex<Cosine, i64>` — payload = `YYYYMMDDHHmmss` timestamp (i64)
- `M = 16`, `ef_construction = 200` (same as EEG indexes)
- Saved every `SCREENSHOT_HNSW_SAVE_EVERY = 10` insertions
- **Rebuilt from SQLite** on startup if the `.hnsw` file is missing or corrupt
  (same pattern as `eeg_global.hnsw`)
- **Fully rebuilt** when the embedding model changes (all vectors must come
  from the same model / dimensionality for cosine similarity to be meaningful)

---

## 6. Settings Configuration (`ScreenshotConfig` in `settings.rs`)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScreenshotConfig {
    /// Master enable — opt-in only (default: false).
    #[serde(default)]
    pub enabled: bool,

    /// Capture interval in seconds (default: 5, aligned with EEG epoch).
    #[serde(default = "default_screenshot_interval")]
    pub interval_secs: u32,

    /// Intermediate image size in pixels (square).
    /// The captured window image is resized to fit within this square
    /// (aspect-ratio-preserving + center-pad) before saving and embedding.
    /// Auto-set to the model's recommended value when model changes;
    /// user can override.
    /// Default: 224 (CLIP ViT-B/32 native input).
    #[serde(default = "default_screenshot_image_size")]
    pub image_size: u32,

    /// WebP quality for saved thumbnails (0–100). Lower = smaller files.
    /// Default: 60.
    #[serde(default = "default_screenshot_quality")]
    pub quality: u8,

    /// Only capture during active EEG sessions (default: true).
    /// When false, captures whenever the app is running and enabled.
    #[serde(default = "default_screenshot_session_only")]
    pub session_only: bool,

    /// Embedding backend: "fastembed" (default) or "mmproj".
    #[serde(default = "default_screenshot_embed_backend")]
    pub embed_backend: String,

    /// fastembed model code (used when embed_backend == "fastembed").
    /// Supported: "clip-vit-b-32" (default, 512-dim),
    ///            "nomic-embed-vision-v1.5" (768-dim).
    #[serde(default = "default_screenshot_fastembed_model")]
    pub fastembed_model: String,
}

fn default_screenshot_interval()        -> u32    { 5 }
fn default_screenshot_image_size()      -> u32    { 224 }
fn default_screenshot_quality()         -> u8     { 60 }
fn default_screenshot_session_only()    -> bool   { true }
fn default_screenshot_embed_backend()   -> String { "fastembed".into() }
fn default_screenshot_fastembed_model() -> String { "clip-vit-b-32".into() }
```

Persisted in `settings.json`:

```json
{
  "screenshot": {
    "enabled": false,
    "interval_secs": 5,
    "image_size": 224,
    "quality": 60,
    "session_only": true,
    "embed_backend": "fastembed",
    "fastembed_model": "clip-vit-b-32"
  }
}
```

---

## 7. Re-Embedding on Model Change

When the user changes `embed_backend` or `fastembed_model` (or the mmproj
file changes), existing embeddings become stale — they were produced by a
different model and cannot be meaningfully compared with new ones.

### 7a. Detection

On every config save, compare the **new** `(embed_backend, model_id)` tuple
against the **previous** one.  If they differ, prompt the user:

> **Embedding model changed.**
> You have 4,320 screenshots embedded with the previous model
> (CLIP ViT-B/32).  To search across all screenshots consistently,
> re-embed them with the new model.
>
> **Estimated time:** ~18 minutes (4,320 images × ~0.25 s each)
>
> \[ Re-embed now \]  \[ Keep old — re-embed later \]  \[ Skip \]

### 7b. Time Estimation

Before starting, measure the encoder's throughput:

```rust
// Embed 3 sample images, take the median wall-clock time.
let sample_ms = bench_embed_3_images(&encoder, &sample_bytes);
let per_image_ms = sample_ms / 3;
let total_rows = sqlite.count_embedded_rows();
let estimate_secs = (total_rows as f64 * per_image_ms as f64) / 1000.0;
```

The estimate is shown in the UI and updated live as re-embedding progresses
(actual throughput may differ from the initial benchmark).

### 7c. Re-Embed Procedure

```
1.  Load the new encoder (fastembed model or mmproj).
2.  Query: SELECT id, filename, image_size FROM screenshots
           WHERE embedding IS NOT NULL
           ORDER BY id
3.  For each row:
      a. Read the WebP file from disk.
      b. If file missing → skip, set embedding = NULL.
      c. Resize to current config.image_size (may differ from original).
      d. Embed → Vec<f32>.
      e. UPDATE screenshots SET embedding = ?, embedding_dim = ?,
              model_backend = ?, model_id = ?, image_size = ?
              WHERE id = ?
      f. Emit progress: { done, total, elapsed_secs, eta_secs }
4.  Drop the old HNSW index.
5.  Rebuild HNSW from all non-NULL embedding rows.
6.  Save the new .hnsw file.
```

Also handles rows with `embedding IS NULL` (captured but never embedded):

```
SELECT id, filename FROM screenshots WHERE embedding IS NULL
```

These are embedded for the first time using the current model.

### 7d. Tauri Commands

```rust
/// Count screenshots needing re-embedding and estimate time.
#[tauri::command]
fn estimate_screenshot_reembed() -> ReembedEstimate {
    // { total: 4320, stale: 4320, unembedded: 12,
    //   per_image_ms: 250, eta_secs: 1083 }
}

/// Re-embed all screenshots with the current model.  Streams progress events.
#[tauri::command]
fn rebuild_screenshot_embeddings() -> ReembedResult {
    // { embedded: 4320, skipped: 3, elapsed_secs: 1041 }
}
```

---

## 8. New Files

| File | Purpose |
|------|---------|
| `src-tauri/src/screenshot.rs` | Platform capture (`capture_active_window`), background worker (`run_screenshot_worker`), resize/pad helpers |
| `src-tauri/src/screenshot_store.rs` | `ScreenshotStore` — SQLite wrapper (open, insert, query, count, rebuild) |

---

## 9. Integration Points

| Where | Change |
|-------|--------|
| `lib.rs` mod list | Add `mod screenshot; mod screenshot_store;` |
| `lib.rs :: AppState` | Add `screenshot_config: ScreenshotConfig`, `screenshot_store: Option<Arc<ScreenshotStore>>` |
| `lib.rs :: run() setup` | Spawn `screenshot::run_screenshot_worker` thread |
| `constants.rs` | Add `SCREENSHOTS_DIR`, `SCREENSHOTS_SQLITE`, `SCREENSHOTS_HNSW`, `SCREENSHOT_HNSW_SAVE_EVERY` |
| `settings.rs` | Add `ScreenshotConfig` struct + `screenshot` field on the persisted settings |
| `settings_cmds.rs` | Add `get_screenshot_config` / `set_screenshot_config` / `estimate_screenshot_reembed` / `rebuild_screenshot_embeddings` Tauri commands |
| `llm/mod.rs` | Add `InferRequest::EmbedImage { bytes, reply_tx }` variant + handler in actor loop (for mmproj backend only) |
| `Cargo.toml` | Add `image = { version = "0.25", default-features = false, features = ["png", "webp", "jpeg"] }` |

---

## 10. Background Worker Flow

```
thread "screenshot-worker":

  // ── One-time init ──
  let fastembed_model = load_fastembed_if_configured(&config)
  let store = ScreenshotStore::open(skill_dir)
  let hnsw  = load_or_rebuild_hnsw(skill_dir, &store)

  loop:
      sleep(config.interval_secs)

      // ── Gate checks ──
      if !config.enabled { continue }
      if config.session_only && session_start_utc.is_none() { continue }

      // ── Capture active window ──
      let captured = capture_active_window()?
      let (resized, w, h) = resize_fit_pad(&captured.raw_bytes, config.image_size)

      // ── Save to disk ──
      let ts       = yyyymmddhhmmss_utc()
      let date_dir = screenshots_dir / &ts[..8]
      create_dir_all(date_dir)
      let filename = format!("{}/{}.webp", &ts[..8], ts)
      encode_webp(&resized, config.quality, &path)
      let file_size = path.metadata().len()

      // ── Active window context (already tracked, free) ──
      let (app_name, window_title) = read_current_active_window(app_state)

      // ── Embed ──
      let (embedding, model_backend, model_id) = match config.embed_backend {
          "fastembed" => {
              if let Some(ref fe) = fastembed_model {
                  let emb = fe.embed_bytes(&[&resized_png])?[0].clone();
                  (Some(emb), "fastembed", &config.fastembed_model)
              } else {
                  (None, "", "")
              }
          }
          "mmproj" => {
              if vision_ready {
                  let (tx, rx) = oneshot::channel();
                  llm_tx.send(InferRequest::EmbedImage { bytes: resized_png, reply_tx: tx });
                  let emb = rx.recv_timeout(30s).ok().flatten();
                  (emb, "mmproj", &mmproj_filename)
              } else {
                  (None, "", "")
              }
          }
      };

      // ── HNSW insert ──
      let hnsw_id = if let Some(ref emb) = embedding {
          let id = hnsw.insert(emb, ts_i64);
          maybe_save_hnsw(every 10);
          Some(id)
      } else {
          None
      };

      // ── SQLite insert ──
      store.insert(ScreenshotRow {
          timestamp: ts_i64,
          unix_ts,
          filename,
          width: w, height: h, file_size,
          hnsw_id,
          embedding: embedding.as_deref(),
          embedding_dim: embedding.as_ref().map_or(0, |e| e.len()),
          model_backend, model_id,
          image_size: config.image_size,
          quality: config.quality,
          app_name, window_title,
      });

      // ── Notify frontend ──
      emit("screenshot-captured", { ts, filename });
```

---

## 11. Tauri Commands + Query API

```rust
/// Get current screenshot configuration.
#[tauri::command]
fn get_screenshot_config() -> ScreenshotConfig

/// Update screenshot configuration.  If the embedding model changed,
/// returns a flag so the frontend can prompt re-embedding.
#[tauri::command]
fn set_screenshot_config(config: ScreenshotConfig) -> ConfigChangeResult

/// Find screenshots visually similar to a query image.
/// Embeds the query image with the current model, then searches HNSW.
#[tauri::command]
fn search_screenshots_by_image(image_bytes: Vec<u8>, k: usize) -> Vec<ScreenshotResult>

/// Find screenshots by timestamp range (for EEG correlation).
#[tauri::command]
fn get_screenshots_around(timestamp: i64, window_secs: i32) -> Vec<ScreenshotResult>

/// Find screenshots by raw embedding vector (pre-computed).
#[tauri::command]
fn search_screenshots_by_vector(vector: Vec<f32>, k: usize) -> Vec<ScreenshotResult>

/// Count screenshots needing (re-)embedding and estimate wall-clock time.
#[tauri::command]
fn estimate_screenshot_reembed() -> ReembedEstimate

/// Re-embed all screenshots with the current model.
/// Emits "screenshot-reembed-progress" events: { done, total, elapsed_secs, eta_secs }
#[tauri::command]
fn rebuild_screenshot_embeddings() -> ReembedResult
```

### Return Types

```rust
struct ScreenshotResult {
    timestamp:    i64,
    unix_ts:      u64,
    filename:     String,
    app_name:     String,
    window_title: String,
    similarity:   f32,      // cosine similarity (0–1) — only for search results
}

struct ReembedEstimate {
    total:          usize,  // all screenshots with embeddings (may be stale)
    stale:          usize,  // embedded with a different model than current
    unembedded:     usize,  // captured but never embedded (NULL embedding)
    per_image_ms:   u64,    // measured from 3-image benchmark
    eta_secs:       u64,    // estimated wall-clock time for full re-embed
}

struct ReembedResult {
    embedded:     usize,  // rows successfully re-embedded
    skipped:      usize,  // rows skipped (file missing, etc.)
    elapsed_secs: f64,
}

struct ConfigChangeResult {
    model_changed: bool,       // true if embed_backend or model_id changed
    stale_count:   usize,      // screenshots embedded with the old model
}
```

---

## 12. Cross-Modal Search (timestamp join key)

The `YYYYMMDDHHmmss` timestamp is the universal join key across:

- `eeg.sqlite` → `embeddings.timestamp`
- `screenshots.sqlite` → `screenshots.timestamp`
- `labels.sqlite` → `labels.eeg_start` / `labels.eeg_end`

### Example queries

**"What was I looking at when I was most focused?"**
```
① eeg_global.hnsw → top-K EEG timestamps with highest engagement scores
② screenshots.sqlite WHERE unix_ts BETWEEN ts-3 AND ts+3
③ Return: screenshot filename + EEG metrics + window title
```

**"Find moments that looked like this screenshot"**
```
① Embed query image via current model → Vec<f32>
② screenshots.hnsw → top-K timestamps + similarity
③ For each timestamp → eeg.sqlite → brain state at that moment
```

**"What brain state produces this visual context?"**
```
① screenshots text search (app_name LIKE '%vscode%' OR window_title LIKE '%…%')
② Gather EEG embeddings at matching timestamps
③ Average / cluster → characteristic brain-state signature for that activity
```

---

## 13. Privacy & Disk Budget

- **Opt-in only** — `enabled: false` by default.
- **Session-gated** by default — only captures during active EEG recording.
- 224×224 WebP @ quality 60 ≈ **8–15 KB/image** → ~12 images/min × 60 min ≈ **6–11 MB/hour**.
- 384×384 WebP @ quality 60 ≈ **15–25 KB/image** → ~12 images/min × 60 min ≈ **11–18 MB/hour**.
- Active window capture only — not recording other displays or hidden windows.
- `screenshots/` folder and `screenshots.sqlite` can be deleted at any time.
- HNSW auto-rebuilds from SQLite embedding blobs on next startup.
- All data stays local — never transmitted.

---

## 14. Cargo Dependencies

```toml
image = { version = "0.25", default-features = false, features = ["png", "webp", "jpeg"] }
```

- `fastembed::ImageEmbedding` — already a dependency (`fastembed = "5.11.0"`),
  `ImageEmbeddingModel::ClipVitB32` and `NomicEmbedVisionV15` available out
  of the box.
- `llama-cpp-4` mtmd — already a dependency (feature `llm-mtmd`).
- No new model downloads for the default (fastembed auto-fetches on first use).
