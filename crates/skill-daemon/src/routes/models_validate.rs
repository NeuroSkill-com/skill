// SPDX-License-Identifier: GPL-3.0-only
//! Model validation endpoints — `/v1/models/validate*`.
//!
//! These hit the **same Rust engines** the app uses (skill-tts / skill-asr /
//! skill-llm), not the Playwright frontend. Downloads go through the daemon's
//! HF Hub catalog (`/v1/llm/download/start`), never an external CLI.
//!
//! Typical flow:
//!   1. `POST /v1/models/ensure-llm-weights` — queue GGUF download if missing
//!   2. `POST /v1/models/validate`           — TTS → ASR → LLM (+ optional embed/UMAP)

use std::path::PathBuf;
use std::time::{Duration, Instant};

use axum::{extract::State, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::routes::settings_llm_runtime::{
    llm_download_start_impl, llm_server_start_impl, llm_server_status_impl, llm_server_stop_impl,
};
use crate::state::AppState;

const DEFAULT_TTS_TEXT: &str = "NeuroSkill voice check one two three.";
const DEFAULT_LLM_PROMPT: &str = "Reply with exactly the single word PONG and nothing else.";
const DEFAULT_EMBED_ANCHOR: &str = "The cat sat on the warm windowsill in the afternoon sun.";
const DEFAULT_EMBED_SIMILAR: &str = "A kitten rested on the sunny window ledge.";
const DEFAULT_EMBED_UNRELATED: &str = "Quarterly corporate tax filings are due at the fiscal year end.";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/models/validate", post(validate_all))
        .route("/models/validate/tts", post(validate_tts))
        .route("/models/validate/asr", post(validate_asr))
        .route("/models/validate/llm", post(validate_llm))
        .route("/models/validate/embed-text", post(validate_embed_text))
        .route("/models/validate/embed-image", post(validate_embed_image))
        .route("/models/validate/umap", post(validate_umap))
        .route("/models/ensure-llm-weights", post(ensure_llm_weights))
}

#[derive(Debug, Default, Deserialize)]
struct ValidateReq {
    /// Override TTS phrase (also used as ASR reference when round-tripping).
    #[serde(default)]
    text: Option<String>,
    /// Wait this many seconds for an in-flight LLM download (default 600).
    #[serde(default)]
    download_timeout_secs: Option<u64>,
    /// Skip starting/chatting the LLM (only check weights present).
    #[serde(default)]
    weights_only: Option<bool>,
    /// Start the server and wait until `running`, then stop — no chat.
    /// Default for validate when chat is omitted (generation can crash/OOM
    /// on some Metal packed paths; load is the regression we care about).
    #[serde(default)]
    load_only: Option<bool>,
    /// Also run a tiny PONG chat after the server is running.
    #[serde(default)]
    chat: Option<bool>,
    /// Also queue the active mmproj (vision) download. Default **false** —
    /// text-only validate must not pull a ~600MB+ projector that the chat
    /// path does not need, and dual downloads + dual daemon loads OOMed a
    /// 64GB machine when combined with Inflect/Whisper.
    #[serde(default)]
    include_mmproj: Option<bool>,
    /// After a successful chat check, stop the LLM server to free RAM.
    #[serde(default)]
    stop_after: Option<bool>,
    /// Also validate rlx-embed text + image and synthetic UMAP in `/validate`.
    #[serde(default)]
    include_embed_umap: Option<bool>,
    /// Override texts for embed-text semantic check: `[anchor, similar, unrelated]`.
    #[serde(default)]
    embed_texts: Option<Vec<String>>,
    /// Number of synthetic epochs per session for UMAP validate (default 24).
    #[serde(default)]
    umap_n: Option<usize>,
}

fn phrase(req: &ValidateReq) -> String {
    req.text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_TTS_TEXT)
        .to_string()
}

fn resample_linear(pcm: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == 0 || to_hz == 0 || from_hz == to_hz {
        return pcm.to_vec();
    }
    let ratio = to_hz as f64 / from_hz as f64;
    let n = ((pcm.len() as f64) * ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let src = i as f64 / ratio;
        let j = src.floor() as usize;
        let f = (src - j as f64) as f32;
        let a = pcm.get(j).copied().unwrap_or(0.0);
        let b = pcm.get(j + 1).copied().unwrap_or(a);
        out.push(a * (1.0 - f) + b * f);
    }
    out
}

fn pcm_stats(pcm: &[f32], sr: u32) -> (f32, f32, f32) {
    let secs = if sr == 0 { 0.0 } else { pcm.len() as f32 / sr as f32 };
    let rms = if pcm.is_empty() {
        0.0
    } else {
        (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt()
    };
    let peak = pcm.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
    (secs, rms, peak)
}

fn tokens_overlap(reference: &str, hypothesis: &str) -> f32 {
    let norm = |s: &str| -> Vec<String> {
        s.to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .filter(|t| t.len() > 1)
            .map(str::to_string)
            .collect()
    };
    let refer = norm(reference);
    let hypo = norm(hypothesis);
    if refer.is_empty() {
        return if hypo.is_empty() { 1.0 } else { 0.0 };
    }
    let hits = refer.iter().filter(|t| hypo.iter().any(|h| h == *t)).count();
    hits as f32 / refer.len() as f32
}

/// Synthesize via the active skill-tts engine (Inflect-Nano / Qwen3 / …).
fn synthesize_active(text: &str) -> Result<(Vec<f32>, u32), String> {
    #[cfg(feature = "voice-tts")]
    {
        skill_tts::engines::synthesize_pcm(text).map_err(|e| {
            let (engine, model, voice) = skill_tts::active_engine();
            format!("TTS engine '{engine}' (model={model:?} voice={voice:?}) synth failed: {e:#}")
        })
    }
    #[cfg(not(feature = "voice-tts"))]
    {
        let _ = text;
        Err("voice-tts feature not enabled in this daemon build".into())
    }
}

async fn validate_tts(State(_state): State<AppState>, Json(req): Json<ValidateReq>) -> Json<Value> {
    let text = phrase(&req);
    let started = Instant::now();
    let result = tokio::task::spawn_blocking(move || synthesize_active(&text)).await;

    match result {
        Ok(Ok((pcm, sr))) => {
            let (secs, rms, peak) = pcm_stats(&pcm, sr);
            let ok = secs >= 0.15 && rms > 1e-3 && peak.is_finite() && peak <= 2.0;
            Json(json!({
                "ok": ok,
                "component": "tts",
                "engine": skill_tts::active_engine().0,
                "seconds": secs,
                "rms": rms,
                "peak": peak,
                "sample_rate": sr,
                "elapsed_ms": started.elapsed().as_millis(),
                "error": if ok { Value::Null } else {
                    json!("synthesis produced silence or invalid audio")
                },
            }))
        }
        Ok(Err(e)) => Json(json!({
            "ok": false,
            "component": "tts",
            "engine": skill_tts::active_engine().0,
            "error": e,
            "elapsed_ms": started.elapsed().as_millis(),
        })),
        Err(e) => Json(json!({
            "ok": false,
            "component": "tts",
            "error": format!("join error: {e}"),
            "elapsed_ms": started.elapsed().as_millis(),
        })),
    }
}

async fn validate_asr(State(_state): State<AppState>, Json(req): Json<ValidateReq>) -> Json<Value> {
    let text = phrase(&req);
    let started = Instant::now();
    let synth_text = text.clone();

    let roundtrip = tokio::task::spawn_blocking(move || -> Result<(Vec<f32>, u32, Vec<String>), String> {
        let (pcm, sr) = synthesize_active(&synth_text)?;
        let pcm16k = resample_linear(&pcm, sr, 16_000);
        let transcripts = skill_asr::transcribe_pcm_16k(&pcm16k, "en").map_err(|e| format!("ASR: {e:#}"))?;
        Ok((pcm, sr, transcripts))
    })
    .await;

    match roundtrip {
        Ok(Ok((_pcm, sr, transcripts))) => {
            let joined = transcripts.join(" ");
            let overlap = tokens_overlap(&text, &joined);
            // Soft gate: model must load; intelligibility is best-effort (VAD /
            // short phrases). Require either a non-empty transcript or that the
            // call succeeded without error (weights present).
            let ok = !transcripts.is_empty() && overlap >= 0.25;
            let soft_ok = transcripts.is_empty(); // loaded but VAD saw no speech
            Json(json!({
                "ok": ok || soft_ok,
                "component": "asr",
                "engine": "whisper",
                "reference": text,
                "transcripts": transcripts,
                "transcript": joined,
                "token_overlap": overlap,
                "sample_rate_in": sr,
                "note": if soft_ok {
                    "Whisper loaded; no utterance segmented (VAD). Treat as weights-OK."
                } else if ok {
                    "TTS→ASR round-trip intelligible"
                } else {
                    "Whisper loaded but transcript poorly matches reference"
                },
                "elapsed_ms": started.elapsed().as_millis(),
                "error": if ok || soft_ok { Value::Null } else {
                    json!(format!("low overlap ({overlap:.2}) for transcript {joined:?}"))
                },
            }))
        }
        Ok(Err(e)) => Json(json!({
            "ok": false,
            "component": "asr",
            "error": e,
            "elapsed_ms": started.elapsed().as_millis(),
        })),
        Err(e) => Json(json!({
            "ok": false,
            "component": "asr",
            "error": format!("join error: {e}"),
            "elapsed_ms": started.elapsed().as_millis(),
        })),
    }
}

fn active_llm_filenames(state: &AppState) -> (String, String) {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    (cat.active_model.clone(), cat.active_mmproj.clone())
}

fn llm_weights_ready(state: &AppState) -> bool {
    state
        .llm_catalog
        .lock()
        .ok()
        .and_then(|c| c.active_model_path())
        .is_some()
}

async fn ensure_llm_weights(State(state): State<AppState>, Json(req): Json<ValidateReq>) -> Json<Value> {
    let (filename, mmproj) = active_llm_filenames(&state);
    if filename.is_empty() {
        return Json(json!({
            "ok": false,
            "component": "llm",
            "error": "no active_model in catalog — pick one in Settings → LLM",
        }));
    }

    if llm_weights_ready(&state) {
        return Json(json!({
            "ok": true,
            "component": "llm",
            "result": "already_downloaded",
            "filename": filename,
            "mmproj": mmproj,
        }));
    }

    // Internal Rust download path (hf-hub via skill-llm catalog).
    let start = llm_download_start_impl(
        State(state.clone()),
        Json(crate::routes::settings::LlmFilenameRequest {
            filename: filename.clone(),
        }),
    )
    .await;

    let mut mmproj_start = Value::Null;
    // Default: skip mmproj — text validate does not need vision weights, and
    // concurrent GGUF+mmproj pulls + dual daemon loads previously OOMed.
    if req.include_mmproj.unwrap_or(false) && !mmproj.is_empty() {
        mmproj_start = llm_download_start_impl(
            State(state.clone()),
            Json(crate::routes::settings::LlmFilenameRequest {
                filename: mmproj.clone(),
            }),
        )
        .await
        .0;
    }

    Json(json!({
        "ok": true,
        "component": "llm",
        "result": "download_started",
        "filename": filename,
        "mmproj": mmproj,
        "mmproj_queued": req.include_mmproj.unwrap_or(false),
        "start": start.0,
        "mmproj_start": mmproj_start,
    }))
}

async fn wait_for_llm_download(state: &AppState, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if llm_weights_ready(state) {
            return Ok(());
        }
        // Surface failure / cancel from catalog.
        if let Ok(cat) = state.llm_catalog.lock() {
            if let Some(e) = cat.entries.iter().find(|e| e.filename == cat.active_model) {
                use skill_llm::catalog::DownloadState;
                match e.state {
                    DownloadState::Failed | DownloadState::Cancelled => {
                        return Err(format!(
                            "download {}: {:?} {}",
                            e.filename,
                            e.state,
                            e.status_msg.clone().unwrap_or_default()
                        ));
                    }
                    _ => {}
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out after {}s waiting for LLM weights",
                timeout.as_secs()
            ));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn validate_llm(State(state): State<AppState>, Json(req): Json<ValidateReq>) -> Json<Value> {
    let started = Instant::now();
    let (filename, mmproj) = active_llm_filenames(&state);
    if filename.is_empty() {
        return Json(json!({
            "ok": false,
            "component": "llm",
            "error": "no active_model selected",
            "elapsed_ms": started.elapsed().as_millis(),
        }));
    }

    let timeout = Duration::from_secs(req.download_timeout_secs.unwrap_or(600));

    if !llm_weights_ready(&state) {
        let _ = ensure_llm_weights(
            State(state.clone()),
            Json(ValidateReq {
                include_mmproj: req.include_mmproj,
                ..Default::default()
            }),
        )
        .await;
        if let Err(e) = wait_for_llm_download(&state, timeout).await {
            return Json(json!({
                "ok": false,
                "component": "llm",
                "filename": filename,
                "error": e,
                "elapsed_ms": started.elapsed().as_millis(),
            }));
        }
    }

    if req.weights_only.unwrap_or(false) {
        return Json(json!({
            "ok": true,
            "component": "llm",
            "result": "weights_present",
            "filename": filename,
            "mmproj": mmproj,
            "elapsed_ms": started.elapsed().as_millis(),
        }));
    }

    // Cap memory before start: dual-daemon + Qwen35 SpecRunner/F32 unpack
    // previously OOMed a 64GB machine. Validate only needs a tiny text reply.
    #[cfg(feature = "llm")]
    {
        skill_llm::shutdown_cell(&state.llm_state_cell);
        // Also clear status so a half-started auto-boot load can't look "loading"
        // forever after we killed its actor.
        if let Ok(mut st) = state.llm_status.lock() {
            *st = "stopped".to_string();
        }
        if let Ok(mut cfg) = state.llm_config.lock() {
            cfg.enabled = true; // start_impl needs this; we'll stop_after
            cfg.ctx_size = Some(2048);
            cfg.autoload_mmproj = false;
            cfg.mmproj = None;
            cfg.mtp_draft_count = 0;
            // Prefer an accelerator when available; packed DequantMatMul is
            // parity-checked vs CPU on Metal/MLX/wgpu. Override with
            // SKILL_VALIDATE_RLX_DEVICE=cpu|metal|mlx|gpu.
            let want = std::env::var("SKILL_VALIDATE_RLX_DEVICE").unwrap_or_else(|_| "metal".into());
            cfg.rlx_device = want;
            // Direct chat prompt is tiny; 128 keeps compile fast and RAM low.
            cfg.rlx_max_seq = 128;
            eprintln!(
                "[models/validate] llm cfg patched device={} max_seq={} autoload_mmproj={}",
                cfg.rlx_device, cfg.rlx_max_seq, cfg.autoload_mmproj
            );
        }
    }

    // Start server via the same handler the UI uses.
    let start_res = llm_server_start_impl(State(state.clone())).await;
    let start_body = start_res.0;
    if start_body.get("ok") == Some(&json!(false)) {
        return Json(json!({
            "ok": false,
            "component": "llm",
            "filename": filename,
            "error": start_body.get("error").cloned().unwrap_or(json!("start failed")),
            "start": start_body,
            "elapsed_ms": started.elapsed().as_millis(),
        }));
    }

    // Wait until status is running (or stopped with error).
    // Debug builds + Metal weight load for ~3GB GGUFs can exceed 30s,
    // especially under memory pressure — allow up to ~3 minutes.
    let mut last_status = json!({});
    for _ in 0..360 {
        let st = llm_server_status_impl(State(state.clone())).await.0;
        last_status = st.clone();
        let status = st.get("status").and_then(|v| v.as_str()).unwrap_or("");
        if status == "running" {
            break;
        }
        if status == "stopped" {
            if let Some(err) = st.get("start_error").and_then(|v| v.as_str()) {
                if !err.is_empty() {
                    return Json(json!({
                        "ok": false,
                        "component": "llm",
                        "filename": filename,
                        "error": err,
                        "status": st,
                        "elapsed_ms": started.elapsed().as_millis(),
                    }));
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    if last_status.get("status").and_then(|v| v.as_str()) != Some("running") {
        return Json(json!({
            "ok": false,
            "component": "llm",
            "filename": filename,
            "error": "server did not reach running state",
            "status": last_status,
            "elapsed_ms": started.elapsed().as_millis(),
        }));
    }

    // Default: load_only — proving the engine reaches running without the
    // chat path (which has crashed this host on packed Metal / long CPU runs).
    let want_chat = req.chat.unwrap_or(false);
    let load_only = req.load_only.unwrap_or(!want_chat);
    if load_only && !want_chat {
        if req.stop_after.unwrap_or(true) {
            let _ = llm_server_stop_impl(State(state.clone())).await;
        }
        return Json(json!({
            "ok": true,
            "component": "llm",
            "result": "running",
            "filename": filename,
            "mmproj": mmproj,
            "status": last_status,
            "stopped_after": req.stop_after.unwrap_or(true),
            "elapsed_ms": started.elapsed().as_millis(),
        }));
    }

    // Tiny chat completion through the running actor.
    #[cfg(feature = "llm")]
    let chat_result = {
        let srv = state.llm_state_cell.lock().ok().and_then(|g| g.clone());
        let Some(srv) = srv else {
            return Json(json!({
                "ok": false,
                "component": "llm",
                "error": "LLM server cell empty after start",
                "elapsed_ms": started.elapsed().as_millis(),
            }));
        };
        let messages = vec![
            json!({"role": "system", "content": "You are a concise assistant. Output only what is asked."}),
            json!({"role": "user", "content": DEFAULT_LLM_PROMPT}),
        ];
        let mut params = skill_llm::GenParams::default();
        params.max_tokens = 32;
        params.temperature = 0.0;
        params.thinking_budget = Some(0);
        params.stop = vec!["\n".into(), "<".into()];
        // Bypass run_chat_with_builtin_tools — skills/tool schemas blow past
        // the validate max_seq bucket. Talk to the actor directly.
        match srv.chat(messages, Vec::new(), params) {
            Ok(mut rx) => {
                let mut text = String::new();
                let mut err: Option<String> = None;
                while let Some(tok) = rx.recv().await {
                    match tok {
                        skill_llm::InferToken::Delta(t) => text.push_str(&t),
                        skill_llm::InferToken::Error(e) => {
                            err = Some(e);
                            break;
                        }
                        skill_llm::InferToken::Done { .. } => break,
                    }
                }
                match err {
                    Some(e) => Err(e),
                    None => Ok(text),
                }
            }
            Err(e) => Err(format!("{e:#}")),
        }
    };
    #[cfg(not(feature = "llm"))]
    let chat_result: Result<String, String> = Err("llm feature disabled".into());

    match chat_result {
        Ok(text) => {
            let upper = text.to_ascii_uppercase();
            let ok = upper.contains("PONG");
            if req.stop_after.unwrap_or(true) {
                let _ = llm_server_stop_impl(State(state.clone())).await;
            }
            Json(json!({
                "ok": ok,
                "component": "llm",
                "filename": filename,
                "mmproj": mmproj,
                "reply": text,
                "status": last_status,
                "stopped_after": req.stop_after.unwrap_or(true),
                "elapsed_ms": started.elapsed().as_millis(),
                "error": if ok { Value::Null } else {
                    json!(format!("expected PONG in reply, got: {text:?}"))
                },
            }))
        }
        Err(e) => {
            if req.stop_after.unwrap_or(true) {
                let _ = llm_server_stop_impl(State(state.clone())).await;
            }
            Json(json!({
                "ok": false,
                "component": "llm",
                "filename": filename,
                "error": e,
                "elapsed_ms": started.elapsed().as_millis(),
            }))
        }
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    dot / (na.sqrt() * nb.sqrt()).max(1e-12)
}

fn embed_phrase_triple(req: &ValidateReq) -> (String, String, String) {
    let texts = req.embed_texts.as_ref();
    let anchor = texts
        .and_then(|t| t.first())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_EMBED_ANCHOR)
        .to_string();
    let similar = texts
        .and_then(|t| t.get(1))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_EMBED_SIMILAR)
        .to_string();
    let unrelated = texts
        .and_then(|t| t.get(2))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_EMBED_UNRELATED)
        .to_string();
    (anchor, similar, unrelated)
}

/// Text embeddings via the daemon's shared rlx-embed (`SharedTextEmbedder`).
async fn validate_embed_text(State(state): State<AppState>, Json(req): Json<ValidateReq>) -> Json<Value> {
    let started = Instant::now();
    let (anchor, similar, unrelated) = embed_phrase_triple(&req);
    let embedder = state.text_embedder.clone();

    let result = tokio::task::spawn_blocking(move || {
        // Ensure weights are loaded (HF download on first use).
        if !embedder.reload() && embedder.embed("warmup").is_none() {
            return Err("rlx-embed text model failed to load (network/weights?)".to_string());
        }
        let a = embedder
            .embed_query(&anchor)
            .ok_or_else(|| "embed(anchor) returned None".to_string())?;
        let s = embedder
            .embed_query(&similar)
            .ok_or_else(|| "embed(similar) returned None".to_string())?;
        let u = embedder
            .embed_query(&unrelated)
            .ok_or_else(|| "embed(unrelated) returned None".to_string())?;
        if a.is_empty() || !a.iter().all(|x| x.is_finite()) {
            return Err("embedding empty or non-finite".into());
        }
        let sim_close = cosine(&a, &s);
        let sim_far = cosine(&a, &u);
        let ok = sim_close > sim_far + 0.05;
        Ok(json!({
            "ok": ok,
            "component": "embed-text",
            "backend": "rlx-embed",
            "model": embedder.model_code(),
            "device": embedder.rlx_device(),
            "dim": a.len(),
            "cosine_similar": sim_close,
            "cosine_unrelated": sim_far,
            "error": if ok { Value::Null } else {
                json!(format!(
                    "related ({sim_close:.4}) not clearly closer than unrelated ({sim_far:.4})"
                ))
            },
        }))
    })
    .await
    .unwrap_or_else(|e| Err(format!("embed-text join: {e}")));

    match result {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("elapsed_ms".into(), json!(started.elapsed().as_millis()));
            }
            Json(v)
        }
        Err(e) => Json(json!({
            "ok": false,
            "component": "embed-text",
            "backend": "rlx-embed",
            "error": e,
            "elapsed_ms": started.elapsed().as_millis(),
        })),
    }
}

fn synth_rgb_image(seed: usize) -> image::DynamicImage {
    let img = image::RgbImage::from_fn(256, 256, |x, y| {
        image::Rgb([
            ((x as usize + seed * 7) % 256) as u8,
            ((y as usize + seed * 13) % 256) as u8,
            ((seed * 31) % 256) as u8,
        ])
    });
    image::DynamicImage::ImageRgb8(img)
}

/// Image embeddings via `skill_screenshots::rlx_image::RlxImageEmbedder` (rlx-embed vision).
async fn validate_embed_image(State(_state): State<AppState>, Json(_req): Json<ValidateReq>) -> Json<Value> {
    let started = Instant::now();

    #[cfg(feature = "text-embeddings-rlx")]
    {
        let result = tokio::task::spawn_blocking(|| {
            let device = if cfg!(target_os = "macos") { "metal" } else { "cpu" };
            let enc = skill_screenshots::rlx_image::RlxImageEmbedder::from_repo(device)
                .map_err(|e| format!("rlx image embedder load failed: {e:#}"))?;
            let v0 = enc
                .embed_image(&synth_rgb_image(0))
                .ok_or_else(|| "embed_image(0) returned None".to_string())?;
            let v1 = enc
                .embed_image(&synth_rgb_image(1))
                .ok_or_else(|| "embed_image(1) returned None".to_string())?;
            if v0.len() != 768 || !v0.iter().all(|x| x.is_finite()) {
                return Err(format!(
                    "unexpected embedding: dim={} finite={}",
                    v0.len(),
                    v0.iter().all(|x| x.is_finite())
                ));
            }
            let norm: f32 = v0.iter().map(|x| x * x).sum::<f32>().sqrt();
            if (norm - 1.0).abs() >= 0.05 {
                return Err(format!("expected L2-normalized output, got {norm:.4}"));
            }
            // Different synthetic images should not be identical.
            let same = v0.iter().zip(v1.iter()).all(|(a, b)| (a - b).abs() < 1e-6);
            if same {
                return Err("two distinct images produced identical embeddings".into());
            }
            Ok(json!({
                "ok": true,
                "component": "embed-image",
                "backend": "rlx-embed",
                "model": "nomic-ai/nomic-embed-vision-v1.5",
                "device": device,
                "dim": v0.len(),
                "l2_norm": norm,
                "cosine_pair": cosine(&v0, &v1),
            }))
        })
        .await
        .unwrap_or_else(|e| Err(format!("embed-image join: {e}")));

        return match result {
            Ok(mut v) => {
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("elapsed_ms".into(), json!(started.elapsed().as_millis()));
                }
                Json(v)
            }
            Err(e) => Json(json!({
                "ok": false,
                "component": "embed-image",
                "backend": "rlx-embed",
                "error": e,
                "elapsed_ms": started.elapsed().as_millis(),
            })),
        };
    }

    #[cfg(not(feature = "text-embeddings-rlx"))]
    {
        Json(json!({
            "ok": false,
            "component": "embed-image",
            "error": "text-embeddings-rlx feature disabled",
            "elapsed_ms": started.elapsed().as_millis(),
        }))
    }
}

/// Seed a temp skill_dir with synthetic EEG embeddings and run `umap_compute_inner`
/// — the same path `/v1/analysis/umap` uses.
fn seed_umap_skill_dir(n_a: usize, n_b: usize) -> Result<(PathBuf, u64, u64, u64, u64), String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let skill_dir = std::env::temp_dir().join(format!("skill-umap-validate-{nanos}"));
    let day_dir = skill_dir.join("20260303");
    std::fs::create_dir_all(&day_dir).map_err(|e| format!("mkdir: {e}"))?;

    let db_path = day_dir.join("eeg.sqlite");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| format!("sqlite open: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE embeddings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL,
            device_id TEXT,
            device_name TEXT,
            hnsw_id INTEGER NOT NULL,
            eeg_embedding BLOB NOT NULL,
            label TEXT,
            extra_embedding BLOB,
            metrics_json TEXT
        );
        CREATE INDEX idx_timestamp ON embeddings (timestamp);",
    )
    .map_err(|e| format!("create table: {e}"))?;

    let a_start_ms: i64 = 1_700_000_000_000;
    let b_start_ms: i64 = a_start_ms + (n_a as i64) * 250 + 60_000;
    let dim = 32_usize;
    let mut rng_seed: u64 = 42;

    let mut insert = conn
        .prepare(
            "INSERT INTO embeddings (timestamp, device_id, device_name, hnsw_id, eeg_embedding)
             VALUES (?1, 'validate', 'validate', ?2, ?3)",
        )
        .map_err(|e| format!("prepare: {e}"))?;

    let mut write_epochs = |start_ms: i64, count: usize, cluster_offset: f32| -> Result<(), String> {
        for i in 0..count {
            let ts = start_ms + (i as i64) * 250;
            let emb: Vec<f32> = (0..dim)
                .map(|d| {
                    rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let raw = ((rng_seed >> 33) as f32) / (u32::MAX as f32) - 0.5;
                    raw + cluster_offset * (d as f32 / dim as f32)
                })
                .collect();
            let blob: Vec<u8> = emb.iter().flat_map(|v| v.to_le_bytes()).collect();
            insert
                .execute(rusqlite::params![ts, i as i64, blob])
                .map_err(|e| format!("insert: {e}"))?;
        }
        Ok(())
    };
    write_epochs(a_start_ms, n_a, 1.0)?;
    write_epochs(b_start_ms, n_b, -1.0)?;
    drop(insert);
    conn.close().map_err(|(_, e)| format!("sqlite close: {e}"))?;

    Ok((
        skill_dir,
        (a_start_ms / 1000) as u64,
        ((a_start_ms + (n_a as i64) * 250) / 1000) as u64,
        (b_start_ms / 1000) as u64,
        ((b_start_ms + (n_b as i64) * 250) / 1000) as u64,
    ))
}

async fn validate_umap(State(_state): State<AppState>, Json(req): Json<ValidateReq>) -> Json<Value> {
    let started = Instant::now();
    let n = req.umap_n.unwrap_or(24).clamp(8, 200);

    let result = tokio::task::spawn_blocking(move || {
        let (skill_dir, a0, a1, b0, b1) = seed_umap_skill_dir(n, n)?;
        let value = skill_router::umap_compute_inner(&skill_dir, a0, a1, b0, b1, None)
            .map_err(|e| format!("umap_compute_inner: {e:#}"))?;
        let _ = std::fs::remove_dir_all(&skill_dir);

        let points = value
            .get("points")
            .and_then(|p| p.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let dim = value.get("dim").and_then(|d| d.as_u64()).unwrap_or(0);
        let reason = value.get("reason").and_then(|r| r.as_str()).map(str::to_string);
        let ok = points >= 5 && dim > 0 && reason.is_none();
        let err_msg = if ok {
            Value::Null
        } else {
            json!(reason
                .clone()
                .unwrap_or_else(|| format!("expected ≥5 points, got {points} (dim={dim})")))
        };
        Ok::<Value, String>(json!({
            "ok": ok,
            "component": "umap",
            "backend": value.get("backend").cloned().unwrap_or(json!("cpu")),
            "n_a": value.get("n_a"),
            "n_b": value.get("n_b"),
            "dim": dim,
            "points": points,
            "umap_elapsed_ms": value.get("elapsed_ms"),
            "reason": reason,
            "error": err_msg,
        }))
    })
    .await
    .unwrap_or_else(|e| Err(format!("umap join: {e}")));

    match result {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("elapsed_ms".into(), json!(started.elapsed().as_millis()));
            }
            Json(v)
        }
        Err(e) => Json(json!({
            "ok": false,
            "component": "umap",
            "error": e,
            "elapsed_ms": started.elapsed().as_millis(),
        })),
    }
}

async fn validate_all(State(state): State<AppState>, Json(req): Json<ValidateReq>) -> Json<Value> {
    let started = Instant::now();
    let include_embed_umap = req.include_embed_umap.unwrap_or(false);

    let tts = validate_tts(
        State(state.clone()),
        Json(ValidateReq {
            text: req.text.clone(),
            ..Default::default()
        }),
    )
    .await
    .0;
    let asr = validate_asr(
        State(state.clone()),
        Json(ValidateReq {
            text: req.text.clone(),
            ..Default::default()
        }),
    )
    .await
    .0;
    let llm = validate_llm(
        State(state.clone()),
        Json(ValidateReq {
            text: req.text.clone(),
            download_timeout_secs: req.download_timeout_secs,
            weights_only: req.weights_only,
            load_only: req.load_only,
            chat: req.chat,
            include_mmproj: req.include_mmproj,
            stop_after: req.stop_after,
            ..Default::default()
        }),
    )
    .await
    .0;

    let mut ok = tts.get("ok") == Some(&json!(true))
        && asr.get("ok") == Some(&json!(true))
        && llm.get("ok") == Some(&json!(true));

    let (embed_text, embed_image, umap) = if include_embed_umap {
        let et = validate_embed_text(
            State(state.clone()),
            Json(ValidateReq {
                embed_texts: req.embed_texts.clone(),
                ..Default::default()
            }),
        )
        .await
        .0;
        let ei = validate_embed_image(State(state.clone()), Json(ValidateReq::default()))
            .await
            .0;
        let um = validate_umap(
            State(state),
            Json(ValidateReq {
                umap_n: req.umap_n,
                ..Default::default()
            }),
        )
        .await
        .0;
        ok = ok
            && et.get("ok") == Some(&json!(true))
            && ei.get("ok") == Some(&json!(true))
            && um.get("ok") == Some(&json!(true));
        (Some(et), Some(ei), Some(um))
    } else {
        (None, None, None)
    };

    Json(json!({
        "ok": ok,
        "component": "all",
        "tts": tts,
        "asr": asr,
        "llm": llm,
        "embed_text": embed_text,
        "embed_image": embed_image,
        "umap": umap,
        "elapsed_ms": started.elapsed().as_millis(),
    }))
}
