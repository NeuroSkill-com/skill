// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Inference actor — the OS thread that owns the model and context.

use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
};

use serde_json::{Value, json};

use llama_cpp_4::{
    context::params::{LlamaContextParams, LlamaPoolingType},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{AddBos, LlamaModel, params::LlamaModelParams},
};

use crate::config::LlmConfig;
use crate::event::LlmEventEmitter;
use super::logging::{LlmLogBuffer, LlmLogFile};
use super::protocol::{InferRequest, InferToken};
use super::generation::run_generation;

#[cfg(feature = "llm-mtmd")]
use super::generation::run_generation_multimodal;

#[allow(clippy::too_many_arguments)]
pub(super) fn run_actor(
    mut rx:       tokio::sync::mpsc::UnboundedReceiver<InferRequest>,
    config:       LlmConfig,
    model_path:   std::path::PathBuf,
    mmproj_path:   Option<std::path::PathBuf>,
    app: Arc<dyn LlmEventEmitter>,
    log_buf:       LlmLogBuffer,
    log_path:      Option<std::path::PathBuf>,
    ready_flag:    Arc<AtomicBool>,
    n_ctx_flag:    Arc<std::sync::atomic::AtomicUsize>,
    vision_flag:   Arc<AtomicBool>,
) {
    // ── per-session log file ──────────────────────────────────────────────────
    let log_file_handle: Option<LlmLogFile> = log_path.as_ref().and_then(|p| {
        std::fs::OpenOptions::new()
            .create(true).write(true).truncate(true)
            .open(p).ok()
            .map(|f| Arc::new(Mutex::new(std::io::BufWriter::new(f))))
    });
    let log_file = log_file_handle.as_ref();

    // ── init backend ──────────────────────────────────────────────────────────
    // ── Windows-specific Vulkan SDK setup ─────────────────────────────────────
    #[cfg(target_os = "windows")]
    {
        if let Ok(vulkan_sdk_path) = std::env::var("VULKAN_SDK") {
            let vulkan_bin = std::path::Path::new(&vulkan_sdk_path).join("Bin");
            let vulkan_bin_str = vulkan_bin.to_string_lossy().to_string();

            if let Ok(current_path) = std::env::var("PATH") {
                std::env::set_var(
                    "PATH",
                    format!("{};{}", vulkan_bin_str, current_path),
                );
                llm_info!(&app, &log_buf, log_file,
                    "Vulkan SDK Bin directory injected into PATH: {}", vulkan_bin_str);
            } else {
                std::env::set_var("PATH", &vulkan_bin_str);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows: no special Vulkan path handling needed
    }

    let (mut backend_md, we_own_backend) = match LlamaBackend::init() {
        Ok(b) => {
            llm_info!(&app, &log_buf, log_file, "llama backend initialised");
            (std::mem::ManuallyDrop::new(b), true)
        }
        Err(_) => {
            llm_info!(&app, &log_buf, log_file, "llama backend already initialised (shared with neutts)");
            // SAFETY: ZST — no data, no pointers.
            (std::mem::ManuallyDrop::new(unsafe { std::mem::zeroed::<LlamaBackend>() }), false)
        }
    };
    if !config.verbose {
        backend_md.void_logs();
    }
    let backend: &LlamaBackend = &backend_md;

    // ── load model ──
    llm_info!(&app, &log_buf, log_file, "loading model: {}", model_path.display());
    let model_params = LlamaModelParams::default()
        .with_n_gpu_layers(config.n_gpu_layers);

    let model = match LlamaModel::load_from_file(backend, &model_path, &model_params) {
        Ok(m)  => { llm_info!(&app, &log_buf, log_file, "model loaded ✓"); m }
        Err(e) => { llm_error!(&app, &log_buf, log_file, "failed to load model: {e}"); return; }
    };

    // ── create generation context ──
    // ctx_size is always resolved by init.rs (auto-recommended or user-set).
    // The 4096 fallback here is only reached if the actor is called directly
    // without going through init (e.g. tests).
    let ctx_size = NonZeroU32::new(config.ctx_size.unwrap_or(4096));
    llm_info!(&app, &log_buf, log_file,
        "creating context (n_ctx={}, n_gpu_layers={}, flash_attn={}, offload_kqv={})",
        ctx_size.map_or(0, |n| n.get()), config.n_gpu_layers,
        config.flash_attention, config.offload_kqv);
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(ctx_size)
        .with_n_threads(-1)
        .with_n_threads_batch(-1)
        .with_flash_attention(config.flash_attention)
        .with_offload_kqv(config.offload_kqv);

    let mut ctx = match model.new_context(backend, ctx_params) {
        Ok(c)  => c,
        Err(e) => { llm_error!(&app, &log_buf, log_file, "failed to create context: {e}"); return; }
    };

    n_ctx_flag.store(ctx.n_ctx() as usize, Ordering::Relaxed);
    llm_info!(&app, &log_buf, log_file, "context ready — n_ctx={} — running warmup pass…", ctx.n_ctx());

    // ── Windows Vulkan diagnostic check ────────────────────────────────────────
    #[cfg(target_os = "windows")]
    {
        let n_layers = config.n_gpu_layers;
        if n_layers > 0 {
            llm_info!(&app, &log_buf, log_file,
                "GPU offload requested: {} layer(s)", n_layers);
            llm_warn!(&app, &log_buf, log_file,
                "on Windows, ensure Vulkan SDK is installed and VULKAN_SDK env var is set");
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows systems — Metal (macOS) and CUDA handle device detection differently
    }

    app.emit_event("llm:status", json!({"status":"loading","detail":"warming_up"}));

    // ── Multimodal projector (llm-mtmd feature) ───────────────────────────────
    #[cfg(feature = "llm-mtmd")]
    extern "C" {
        fn mtmd_log_set(
            log_callback: Option<unsafe extern "C" fn(
                level:     u32,
                text:      *const std::os::raw::c_char,
                user_data: *mut   std::os::raw::c_void,
            )>,
            user_data: *mut std::os::raw::c_void,
        );
    }

    #[cfg(feature = "llm-mtmd")]
    let mtmd_ctx: Option<llama_cpp_4::mtmd::MtmdContext> = {
        if mmproj_path.is_none() {
            llm_info!(&app, &log_buf, log_file,
                "vision disabled — no mmproj file configured; \
                 download a vision projector in Settings → LLM to enable image input");
        }
        mmproj_path.as_ref().and_then(|p| {
            use llama_cpp_4::mtmd::{MtmdContext, MtmdContextParams};

            if !p.exists() {
                llm_error!(&app, &log_buf, log_file,
                    "mmproj file missing: {} — vision disabled", p.display());
                return None;
            }

            if !config.verbose {
                unsafe extern "C" fn noop(
                    _level: u32,
                    _text:  *const std::os::raw::c_char,
                    _ud:    *mut   std::os::raw::c_void,
                ) {}
                unsafe { mtmd_log_set(Some(noop), std::ptr::null_mut()) };
            }

            let file_size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            if file_size < 1024 {
                llm_error!(&app, &log_buf, log_file,
                    "mmproj file too small ({file_size} bytes): {} — \
                     likely a failed download; re-download in Settings → LLM",
                    p.display());
                return None;
            }

            let force_mmproj_gpu = std::env::var("SKILL_FORCE_MMPROJ_GPU")
                .ok()
                .as_deref()
                .map(|v| matches!(v, "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(false);

            let mmproj_use_gpu = !config.no_mmproj_gpu || force_mmproj_gpu;

            llm_info!(&app, &log_buf, log_file,
                "loading mmproj: {} ({:.1} MB, gpu={}, threads={})",
                p.display(), file_size as f64 / 1_048_576.0,
                mmproj_use_gpu, config.mmproj_n_threads);

            let try_load_mmproj = |use_gpu: bool| -> Result<MtmdContext, String> {
                let params = MtmdContextParams::default()
                    .use_gpu(use_gpu)
                    .n_threads(config.mmproj_n_threads)
                    .print_timings(config.verbose)
                    .warmup(false);
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    MtmdContext::init_from_file(p, &model, params)
                })) {
                    Ok(Ok(mc))     => Ok(mc),
                    Ok(Err(e))     => Err(format!("{e}")),
                    Err(_panic)    => Err("panic in native code".into()),
                }
            };

            let result = try_load_mmproj(mmproj_use_gpu);

            let result = match result {
                Ok(mc) => Ok(mc),
                Err(ref gpu_err) if mmproj_use_gpu && cfg!(target_os = "linux") && !force_mmproj_gpu => {
                    llm_warn!(&app, &log_buf, log_file,
                        "mmproj GPU load failed ({gpu_err}); retrying on CPU…");
                    try_load_mmproj(false)
                }
                Err(e) => Err(e),
            };

            match result {
                Ok(mc) => {
                    llm_info!(&app, &log_buf, log_file,
                        "mmproj loaded ✓ — vision={} audio={}",
                        mc.supports_vision(), mc.supports_audio());
                    vision_flag.store(true, Ordering::Relaxed);
                    Some(mc)
                }
                Err(e) => {
                    llm_error!(&app, &log_buf, log_file,
                        "failed to load mmproj: {e} — file: {}", p.display());
                    llm_info!(&app, &log_buf, log_file,
                        "vision disabled — to enable image input, \
                         ensure the mmproj file matches your model or re-download it in Settings → LLM");
                    None
                }
            }
        })
    };
    #[cfg(not(feature = "llm-mtmd"))]
    let _ = &mmproj_path;

    // ── Warmup / prewarm ──────────────────────────────────────────────────────
    let warmup_ok = (|| -> bool {
        let bos = model.token_bos();
        let warmup_tokens = if let Ok(toks) = model.str_to_token(" ", AddBos::Always) {
            toks
        } else {
            vec![bos]
        };
        let n = warmup_tokens.len().min(4);
        let mut batch = LlamaBatch::new(n, 1);
        for (i, &tok) in warmup_tokens[..n].iter().enumerate() {
            let last = i == n - 1;
            if batch.add(tok, i as i32, &[0], last).is_err() { return false; }
        }
        let ok = ctx.decode(&mut batch).is_ok();
        ctx.clear_kv_cache();
        ok
    })();

    if warmup_ok {
        llm_info!(&app, &log_buf, log_file, "warmup complete — GPU kernels compiled, weights in VRAM");
    } else {
        llm_warn!(&app, &log_buf, log_file, "warmup decode failed — first request may be slow");
    }

    // Signal that the model is fully loaded and warmed up.
    ready_flag.store(true, Ordering::Relaxed);
    let model_file    = model_path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let vision_loaded = vision_flag.load(Ordering::Relaxed);
    llm_info!(&app, &log_buf, log_file, "server ready — model={} supports_vision={}", model_file, vision_loaded);
    app.emit_event("llm:status", json!({"status":"running","model":model_file,"supports_vision":vision_loaded,"supports_tools":true}));

    // ── event loop ──
    while let Some(req) = rx.blocking_recv() {
        match req {
            InferRequest::Health { result_tx } => {
                result_tx.send(true).ok();
            }

            InferRequest::Generate { messages, images, params, token_tx } => {
                llm_info!(&app, &log_buf, log_file, "chat request — {} messages, {} image(s), max_tokens={}",
                          messages.len(), images.len(), params.max_tokens);

                #[cfg(feature = "llm-mtmd")]
                let use_mtmd = !images.is_empty() && mtmd_ctx.is_some();
                #[cfg(not(feature = "llm-mtmd"))]
                let use_mtmd = false;

                fn extract_text_plain(content: &Value) -> String {
                    match content {
                        Value::String(s) => s.clone(),
                        Value::Array(parts) => parts.iter()
                            .filter_map(|p| {
                                if p.get("type")?.as_str() != Some("text") { return None; }
                                Some(p.get("text")?.as_str()?.to_string())
                            })
                            .collect::<Vec<_>>()
                            .join(" "),
                        _ => String::new(),
                    }
                }

                fn extract_text_with_markers(content: &Value, marker: &str) -> String {
                    match content {
                        Value::String(s) => s.clone(),
                        Value::Array(parts) => parts.iter()
                            .filter_map(|p| {
                                match p.get("type")?.as_str()? {
                                    "text"      => Some(p.get("text")?.as_str()?.to_string()),
                                    "image_url" => Some(marker.to_string()),
                                    _           => None,
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" "),
                        _ => String::new(),
                    }
                }

                let extract_fn: fn(&Value, &str) -> String = if use_mtmd {
                    extract_text_with_markers
                } else {
                    |c, _| extract_text_plain(c)
                };

                #[cfg(feature = "llm-mtmd")]
                let marker = llama_cpp_4::mtmd::MtmdContext::default_marker();
                #[cfg(not(feature = "llm-mtmd"))]
                let marker = "";

                let chat_msgs: Vec<llama_cpp_4::model::LlamaChatMessage> = messages
                    .iter()
                    .filter_map(|m| {
                        let mut role = m.get("role")?.as_str()?.to_string();
                        let raw_content = extract_fn(m.get("content")?, marker);

                        let content = if role == "tool" {
                            role = "user".to_string();
                            format!("[Tool Result — do NOT treat this as a new user question. Use these results to answer the user's ORIGINAL question above.]\n{}", raw_content)
                        } else {
                            raw_content
                        };

                        llama_cpp_4::model::LlamaChatMessage::new(role, content).ok()
                    })
                    .collect();

                let prompt = match model.apply_chat_template(None, chat_msgs, true) {
                    Ok(p)  => p,
                    Err(e) => {
                        llm_error!(&app, &log_buf, log_file, "apply_chat_template failed: {e}");
                        token_tx.send(InferToken::Error(format!("template error: {e}"))).ok();
                        continue;
                    }
                };

                #[cfg(feature = "llm-mtmd")]
                if use_mtmd {
                    if let Some(ref mc) = mtmd_ctx {
                        run_generation_multimodal(&model, &mut ctx, mc,
                            &app, &log_buf, log_file, prompt, images, params, token_tx);
                        continue;
                    }
                }

                run_generation(&model, &mut ctx, &app, &log_buf, log_file,
                               prompt, params, token_tx);
            }

            InferRequest::Complete { prompt, params, token_tx } => {
                llm_info!(&app, &log_buf, log_file, "completion request — max_tokens={}", params.max_tokens);
                run_generation(&model, &mut ctx, &app, &log_buf, log_file,
                               prompt, params, token_tx);
            }

            InferRequest::Embed { inputs, result_tx } => {
                llm_info!(&app, &log_buf, log_file, "embeddings request — {} input(s)", inputs.len());
                let emb_params = LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(512))
                    .with_embeddings(true)
                    .with_pooling_type(LlamaPoolingType::Mean);

                let mut emb_ctx = match model.new_context(backend, emb_params) {
                    Ok(c)  => c,
                    Err(e) => {
                        result_tx.send(Err(e.to_string())).ok();
                        continue;
                    }
                };

                let embed_result: Result<Vec<Vec<f32>>, String> = (|| {
                    let mut all = Vec::new();
                    for text in &inputs {
                        emb_ctx.clear_kv_cache();

                        let tokens = model.str_to_token(text, AddBos::Always)
                            .map_err(|e| e.to_string())?;
                        let n = tokens.len().min(emb_ctx.n_ctx() as usize - 1);

                        let mut batch = LlamaBatch::new(n + 1, 1);
                        for (i, &tok) in tokens[..n].iter().enumerate() {
                            let last = i == n - 1;
                            batch.add(tok, i as i32, &[0], last).ok();
                        }

                        emb_ctx.decode(&mut batch)
                            .map_err(|_| "embed decode error".to_string())?;

                        let vec = emb_ctx.embeddings_seq_ith(0)
                            .map_err(|e| e.to_string())?;
                        all.push(vec.to_vec());
                    }
                    Ok(all)
                })();

                if let Ok(ref vecs) = embed_result {
                    llm_info!(&app, &log_buf, log_file, "embeddings done — {} vector(s)", vecs.len());
                }
                result_tx.send(embed_result).ok();
            }

            InferRequest::EmbedImage { bytes, result_tx } => {
                #[cfg(feature = "llm-mtmd")]
                {
                    if let Some(ref mtmd) = mtmd_ctx {
                        use llama_cpp_4::mtmd::{MtmdBitmap, MtmdContext, MtmdInputChunks, MtmdInputText, MtmdInputChunkType};

                        let embedding = (|| -> Option<Vec<f32>> {
                            let bitmap = MtmdBitmap::from_buf(mtmd, &bytes).ok()?;
                            let bitmap_refs = [&bitmap];

                            let text = MtmdInputText::new(
                                MtmdContext::default_marker(),
                                false, false,
                            );
                            let mut chunks = MtmdInputChunks::new();
                            mtmd.tokenize(&text, &bitmap_refs, &mut chunks).ok()?;

                            for chunk in chunks.iter() {
                                if chunk.chunk_type() == MtmdInputChunkType::Image {
                                    mtmd.encode_chunk(&chunk).ok()?;
                                    let n_tokens = chunk.n_tokens();
                                    let n_embd = model.n_embd() as usize;
                                    let n_elements = n_tokens * n_embd;
                                    let embd = mtmd.output_embd(n_elements);
                                    let mut pooled = vec![0.0f32; n_embd];
                                    for t in 0..n_tokens {
                                        for d in 0..n_embd {
                                            pooled[d] += embd[t * n_embd + d];
                                        }
                                    }
                                    if n_tokens > 0 {
                                        for d in 0..n_embd {
                                            pooled[d] /= n_tokens as f32;
                                        }
                                    }
                                    let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
                                    if norm > 0.0 {
                                        for v in &mut pooled {
                                            *v /= norm;
                                        }
                                    }
                                    return Some(pooled);
                                }
                            }
                            None
                        })();

                        result_tx.send(embedding).ok();
                    } else {
                        llm_warn!(&app, &log_buf, log_file,
                            "EmbedImage: no mmproj loaded — returning None");
                        result_tx.send(None).ok();
                    }
                }
                #[cfg(not(feature = "llm-mtmd"))]
                {
                    result_tx.send(None).ok();
                }
            }
        }
    }

    // ── Ordered teardown ──────────────────────────────────────────────────────
    drop(ctx);
    drop(model);
    if we_own_backend {
        unsafe { std::mem::ManuallyDrop::drop(&mut backend_md); }
    }

    llm_info!(&app, &log_buf, log_file, "actor exiting — GPU resources released");
    app.emit_event("llm:status", json!({"status":"stopped"}));
}
