// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Inference actor — the OS thread that owns the model and context.

use anyhow::Context;
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use serde_json::{json, Value};

use llama_cpp_4::{
    context::params::{LlamaContextParams, LlamaPoolingType},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
};

use super::generation::{run_generation, GpuMemoryGuard};
use super::logging::{LlmLogBuffer, LlmLogFile};
use super::protocol::{InferRequest, InferToken};
use crate::config::LlmConfig;
use crate::event::LlmEventEmitter;

#[cfg(feature = "llm-mtmd")]
use super::generation::run_generation_multimodal;

#[allow(clippy::too_many_arguments)]
pub(super) fn run_actor(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<InferRequest>,
    config: LlmConfig,
    model_path: std::path::PathBuf,
    mmproj_path: Option<std::path::PathBuf>,
    app: Arc<dyn LlmEventEmitter>,
    log_buf: LlmLogBuffer,
    log_path: Option<std::path::PathBuf>,
    ready_flag: Arc<AtomicBool>,
    n_ctx_flag: Arc<std::sync::atomic::AtomicUsize>,
    vision_flag: Arc<AtomicBool>,
) {
    // ── per-session log file ──────────────────────────────────────────────────
    let log_file_handle: Option<LlmLogFile> = log_path.as_ref().and_then(|p| {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(p)
            .ok()
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
                std::env::set_var("PATH", format!("{};{}", vulkan_bin_str, current_path));
                llm_info!(
                    &app,
                    &log_buf,
                    log_file,
                    "Vulkan SDK Bin directory injected into PATH: {}",
                    vulkan_bin_str
                );
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
            llm_info!(
                &app,
                &log_buf,
                log_file,
                "llama backend already initialised (shared with neutts)"
            );
            // SAFETY: `LlamaBackend` is effectively a ZST handle (no heap
            // pointers, no Drop). When we don't own the backend (neutts already
            // initialised it), we create a zero-filled placeholder that is never
            // dropped (ManuallyDrop) and never used for initialization.
            // SAFETY: `LlamaBackend` is a plain struct of integer handles /
            // pointers with no Drop impl. A zeroed instance is valid (null
            // handles) and is wrapped in ManuallyDrop so it is never dropped.
            (
                std::mem::ManuallyDrop::new(unsafe { std::mem::zeroed::<LlamaBackend>() }),
                false,
            )
        }
    };
    if !config.verbose {
        backend_md.void_logs();
    }
    let backend: &LlamaBackend = &backend_md;

    // ── load model ──
    let model_file_name = model_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("model");
    app.emit_event(
        "llm:status",
        json!({"status":"loading","detail":"loading_model","model":model_file_name}),
    );
    llm_info!(
        &app,
        &log_buf,
        log_file,
        "loading model: {}",
        model_path.display()
    );
    let model_params = LlamaModelParams::default().with_n_gpu_layers(config.n_gpu_layers);

    let model = match LlamaModel::load_from_file(backend, &model_path, &model_params) {
        Ok(m) => {
            llm_info!(&app, &log_buf, log_file, "model loaded ✓");
            m
        }
        Err(e) => {
            llm_error!(&app, &log_buf, log_file, "failed to load model: {e}");
            app.emit_event(
                "llm:status",
                json!({"status":"stopped","error":format!("failed to load model: {e}")}),
            );
            return;
        }
    };

    // ── create generation context ──
    // ctx_size is always resolved by init.rs (auto-recommended or user-set).
    // The 4096 fallback here is only reached if the actor is called directly
    // without going through init (e.g. tests).
    let ctx_size = NonZeroU32::new(config.ctx_size.unwrap_or(4096));
    app.emit_event(
        "llm:status",
        json!({"status":"loading","detail":"creating_context","model":model_file_name}),
    );
    llm_info!(
        &app,
        &log_buf,
        log_file,
        "creating context (n_ctx={}, n_gpu_layers={}, flash_attn={}, offload_kqv={})",
        ctx_size.map_or(0, std::num::NonZero::get),
        config.n_gpu_layers,
        config.flash_attention,
        config.offload_kqv
    );
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(ctx_size)
        .with_n_threads(-1)
        .with_n_threads_batch(-1)
        .with_flash_attention(config.flash_attention)
        .with_offload_kqv(config.offload_kqv);

    let mut ctx = match model.new_context(backend, ctx_params) {
        Ok(c) => c,
        Err(e) => {
            llm_error!(&app, &log_buf, log_file, "failed to create context: {e}");
            app.emit_event(
                "llm:status",
                json!({"status":"stopped","error":format!("failed to create context: {e}")}),
            );
            return;
        }
    };

    n_ctx_flag.store(ctx.n_ctx() as usize, Ordering::Relaxed);
    llm_info!(
        &app,
        &log_buf,
        log_file,
        "context ready — n_ctx={} — running warmup pass…",
        ctx.n_ctx()
    );

    // ── Windows Vulkan diagnostic check ────────────────────────────────────────
    #[cfg(target_os = "windows")]
    {
        let n_layers = config.n_gpu_layers;
        if n_layers > 0 {
            llm_info!(
                &app,
                &log_buf,
                log_file,
                "GPU offload requested: {} layer(s)",
                n_layers
            );
            llm_warn!(
                &app,
                &log_buf,
                log_file,
                "on Windows, ensure Vulkan SDK is installed and VULKAN_SDK env var is set"
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows systems — Metal (macOS) and CUDA handle device detection differently
    }

    app.emit_event(
        "llm:status",
        json!({"status":"loading","detail":"warming_up","model":model_file_name}),
    );

    // ── Multimodal projector (llm-mtmd feature) ───────────────────────────────
    #[cfg(feature = "llm-mtmd")]
    extern "C" {
        fn mtmd_log_set(
            log_callback: Option<
                unsafe extern "C" fn(
                    level: u32,
                    text: *const std::os::raw::c_char,
                    user_data: *mut std::os::raw::c_void,
                ),
            >,
            user_data: *mut std::os::raw::c_void,
        );
    }

    #[cfg(feature = "llm-mtmd")]
    let mtmd_ctx: Option<llama_cpp_4::mtmd::MtmdContext> = {
        if mmproj_path.is_none() {
            llm_info!(
                &app,
                &log_buf,
                log_file,
                "vision disabled — no mmproj file configured; \
                 download a vision projector in Settings → LLM to enable image input"
            );
        }
        mmproj_path.as_ref().and_then(|p| {
            use llama_cpp_4::mtmd::{MtmdContext, MtmdContextParams};
            app.emit_event("llm:status", json!({"status":"loading","detail":"loading_vision","model":model_file_name}));

            if !p.exists() {
                llm_error!(&app, &log_buf, log_file,
                    "mmproj file missing: {} — vision disabled", p.display());
                return None;
            }

            if !config.verbose {
                // SAFETY: `noop` is a valid C-calling-convention function that
                // ignores all arguments. `mtmd_log_set` stores the callback
                // globally — `noop` has 'static lifetime (it's a function item).
                unsafe extern "C" fn noop(
                    _level: u32,
                    _text:  *const std::os::raw::c_char,
                    _ud:    *mut   std::os::raw::c_void,
                ) {}
                // SAFETY: `noop` has a 'static lifetime (function item) and
                // matches the expected C callback signature. null user-data is valid.
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

            let try_load_mmproj = |use_gpu: bool| -> anyhow::Result<MtmdContext> {
                let params = MtmdContextParams::default()
                    .use_gpu(use_gpu)
                    .n_threads(config.mmproj_n_threads)
                    .print_timings(config.verbose)
                    .warmup(false);
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    MtmdContext::init_from_file(p, &model, params)
                })) {
                    Ok(Ok(mc))     => Ok(mc),
                    Ok(Err(e))     => Err(anyhow::anyhow!("{e}")),
                    Err(_panic)    => Err(anyhow::anyhow!("panic in native code")),
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

    // Build GPU memory guard from config thresholds.
    let gpu_guard = GpuMemoryGuard {
        decode_threshold: config.gpu_memory_threshold,
        gen_threshold: config.gpu_memory_gen_threshold,
    };

    // ── Warmup / prewarm ──────────────────────────────────────────────────────
    let warmup_ok =
        (|| -> bool {
            // Pre-check GPU memory to avoid Metal abort() during warmup.
            let (mem_ok, free_gb) = super::generation::gpu_memory_check(gpu_guard.decode_threshold);
            if !mem_ok {
                llm_warn!(&app, &log_buf, log_file,
                "skipping warmup — insufficient GPU memory ({:.2} GB free < {:.2} GB threshold)",
                free_gb.unwrap_or(0.0), gpu_guard.decode_threshold);
                return false;
            }

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
                if batch.add(tok, i as i32, &[0], last).is_err() {
                    return false;
                }
            }
            let ok = ctx.decode(&mut batch).is_ok();
            ctx.clear_kv_cache();
            if ok {
                return true;
            }

            // Retry once after a brief delay (transient Metal failures).
            llm_warn!(
                &app,
                &log_buf,
                log_file,
                "warmup decode failed — retrying after 200ms"
            );
            std::thread::sleep(std::time::Duration::from_millis(200));
            let mut batch2 = LlamaBatch::new(n, 1);
            for (i, &tok) in warmup_tokens[..n].iter().enumerate() {
                let last = i == n - 1;
                if batch2.add(tok, i as i32, &[0], last).is_err() {
                    return false;
                }
            }
            let ok2 = ctx.decode(&mut batch2).is_ok();
            ctx.clear_kv_cache();
            ok2
        })();

    if warmup_ok {
        llm_info!(
            &app,
            &log_buf,
            log_file,
            "warmup complete — GPU kernels compiled, weights in VRAM"
        );
    } else {
        llm_warn!(
            &app,
            &log_buf,
            log_file,
            "warmup decode failed — first request may be slow"
        );
    }

    // Signal that the model is fully loaded and warmed up.
    ready_flag.store(true, Ordering::Relaxed);
    let model_file = model_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");
    let vision_loaded = vision_flag.load(Ordering::Relaxed);
    llm_info!(
        &app,
        &log_buf,
        log_file,
        "server ready — model={} supports_vision={}",
        model_file,
        vision_loaded
    );
    app.emit_event("llm:status", json!({"status":"running","model":model_file,"supports_vision":vision_loaded,"supports_tools":true}));

    // ── event loop ──
    while let Some(req) = rx.blocking_recv() {
        match req {
            InferRequest::Health { result_tx } => {
                result_tx.send(true).ok();
            }

            InferRequest::Generate {
                messages,
                images,
                params,
                token_tx,
            } => {
                llm_info!(
                    &app,
                    &log_buf,
                    log_file,
                    "chat request — {} messages, {} image(s), max_tokens={}",
                    messages.len(),
                    images.len(),
                    params.max_tokens
                );

                #[cfg(feature = "llm-mtmd")]
                let use_mtmd = !images.is_empty() && mtmd_ctx.is_some();
                #[cfg(not(feature = "llm-mtmd"))]
                let use_mtmd = false;

                fn extract_text_plain(content: &Value) -> String {
                    match content {
                        Value::String(s) => s.clone(),
                        Value::Array(parts) => parts
                            .iter()
                            .filter_map(|p| {
                                if p.get("type")?.as_str() != Some("text") {
                                    return None;
                                }
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
                        Value::Array(parts) => parts
                            .iter()
                            .filter_map(|p| match p.get("type")?.as_str()? {
                                "text" => Some(p.get("text")?.as_str()?.to_string()),
                                "image_url" => Some(marker.to_string()),
                                _ => None,
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

                // ── Build chat messages ──────────────────────────────────────────
                let build_chat_msgs =
                    |msgs: &[serde_json::Value]| -> Vec<llama_cpp_4::model::LlamaChatMessage> {
                        msgs.iter()
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
                        .collect()
                    };

                // ── Fit prompt to context: grow context or trim history ──────────
                // Strategy:
                //   1. Tokenize the full prompt.
                //   2. If it fits in the current n_ctx (with 25% reserved for
                //      generation) → proceed.
                //   3. Otherwise, try to grow the context window up to the model's
                //      max_context_length, as long as estimated VRAM usage allows.
                //   4. If we still can't fit → trim oldest middle messages.
                let mut trimmed_messages = messages.clone();
                let prompt: Option<String> = 'build_prompt: {
                    loop {
                        let chat_msgs = build_chat_msgs(&trimmed_messages);
                        let p = match model.apply_chat_template(None, chat_msgs, true) {
                            Ok(p) => p,
                            Err(e) => {
                                llm_error!(
                                    &app,
                                    &log_buf,
                                    log_file,
                                    "apply_chat_template failed: {e}"
                                );
                                token_tx
                                    .send(InferToken::Error(format!("template error: {e}")))
                                    .ok();
                                break 'build_prompt None;
                            }
                        };

                        let Ok(tokens) = model.str_to_token(&p, llama_cpp_4::model::AddBos::Always)
                        else {
                            break 'build_prompt Some(p);
                        };

                        let n_ctx_cur = ctx.n_ctx() as usize;
                        let reserve = n_ctx_cur / 4;
                        let budget = n_ctx_cur.saturating_sub(reserve);

                        if tokens.len() < budget {
                            break 'build_prompt Some(p); // fits fine
                        }

                        // ── Try to grow the context window ──────────────────────
                        // We need at least tokens.len() * 4/3 (to keep 25% headroom).
                        let needed_ctx = ((tokens.len() as f64) * 4.0 / 3.0).ceil() as u32 + 64;
                        let max_ctx = if config.max_context_length > 0 {
                            config.max_context_length
                        } else {
                            n_ctx_cur as u32 // no metadata → can't grow
                        };

                        if needed_ctx > n_ctx_cur as u32 && needed_ctx <= max_ctx {
                            // Check if memory allows the larger context.
                            let can_afford = if config.params_b > 0.0 {
                                let gpu = skill_data::gpu_stats::read();
                                let available_gb: f64 = gpu
                                    .as_ref()
                                    .and_then(|g| {
                                        if g.is_unified_memory {
                                            g.free_memory_bytes
                                                .map(|b| b as f64 / (1024.0 * 1024.0 * 1024.0))
                                        } else {
                                            g.total_memory_bytes
                                                .map(|b| b as f64 / (1024.0 * 1024.0 * 1024.0))
                                        }
                                    })
                                    .unwrap_or(0.0);
                                let mem_budget = available_gb * 0.70;
                                let estimated = crate::catalog::estimate_memory_gb(
                                    config.params_b,
                                    &config.quant,
                                    needed_ctx,
                                );
                                estimated <= mem_budget
                            } else {
                                false // no model metadata → don't risk OOM
                            };

                            if can_afford {
                                // Round up to next power-of-two-ish standard size for cleanliness.
                                let new_ctx = [4096u32, 8192, 16384, 32768, 65536, 131072]
                                    .iter()
                                    .copied()
                                    .find(|&c| c >= needed_ctx && c <= max_ctx)
                                    .unwrap_or(needed_ctx.min(max_ctx));

                                llm_info!(
                                    &app,
                                    &log_buf,
                                    log_file,
                                    "prompt needs {} tokens, growing context {} -> {} (max={})",
                                    tokens.len(),
                                    n_ctx_cur,
                                    new_ctx,
                                    max_ctx
                                );

                                let new_ctx_params = LlamaContextParams::default()
                                    .with_n_ctx(NonZeroU32::new(new_ctx))
                                    .with_n_threads(-1)
                                    .with_n_threads_batch(-1)
                                    .with_flash_attention(config.flash_attention)
                                    .with_offload_kqv(config.offload_kqv);

                                match model.new_context(backend, new_ctx_params) {
                                    Ok(new_c) => {
                                        ctx = new_c;
                                        n_ctx_flag.store(ctx.n_ctx() as usize, Ordering::Relaxed);
                                        llm_info!(
                                            &app,
                                            &log_buf,
                                            log_file,
                                            "context resized to n_ctx={}",
                                            ctx.n_ctx()
                                        );
                                        // Re-check with new budget
                                        let new_budget = (ctx.n_ctx() as usize)
                                            .saturating_sub(ctx.n_ctx() as usize / 4);
                                        if tokens.len() < new_budget {
                                            break 'build_prompt Some(p);
                                        }
                                        // Still doesn't fit — fall through to trimming
                                    }
                                    Err(e) => {
                                        llm_warn!(&app, &log_buf, log_file,
                                            "failed to grow context to {}: {e} — will trim messages instead",
                                            new_ctx);
                                    }
                                }
                            }
                        }

                        // ── Fall back: trim oldest middle messages ───────────────
                        if trimmed_messages.len() <= 2 {
                            llm_warn!(
                                &app,
                                &log_buf,
                                log_file,
                                "prompt still too long after trimming all history ({} >= {})",
                                tokens.len(),
                                budget
                            );
                            break 'build_prompt Some(p); // let generation.rs emit the error
                        }
                        llm_info!(&app, &log_buf, log_file,
                            "prompt too long ({} tokens >= {} budget, n_ctx={}), dropping message at index 1 ({} messages remaining)",
                            tokens.len(), budget, n_ctx_cur, trimmed_messages.len() - 1);
                        trimmed_messages.remove(1);
                    }
                };
                let Some(prompt) = prompt else { continue };

                #[cfg(feature = "llm-mtmd")]
                if use_mtmd {
                    if let Some(ref mc) = mtmd_ctx {
                        run_generation_multimodal(
                            &model, &mut ctx, mc, &app, &log_buf, log_file, prompt, images, params,
                            token_tx, gpu_guard,
                        );
                        continue;
                    }
                }

                run_generation(
                    &model, &mut ctx, &app, &log_buf, log_file, prompt, params, token_tx, gpu_guard,
                );
            }

            InferRequest::Complete {
                prompt,
                params,
                token_tx,
            } => {
                llm_info!(
                    &app,
                    &log_buf,
                    log_file,
                    "completion request — max_tokens={}",
                    params.max_tokens
                );
                run_generation(
                    &model, &mut ctx, &app, &log_buf, log_file, prompt, params, token_tx, gpu_guard,
                );
            }

            InferRequest::Embed { inputs, result_tx } => {
                llm_info!(
                    &app,
                    &log_buf,
                    log_file,
                    "embeddings request — {} input(s)",
                    inputs.len()
                );
                let emb_params = LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(512))
                    .with_embeddings(true)
                    .with_pooling_type(LlamaPoolingType::Mean);

                let mut emb_ctx = match model.new_context(backend, emb_params) {
                    Ok(c) => c,
                    Err(e) => {
                        result_tx.send(Err(anyhow::anyhow!("{e}"))).ok();
                        continue;
                    }
                };

                let embed_result: anyhow::Result<Vec<Vec<f32>>> = (|| {
                    let mut all = Vec::new();
                    for text in &inputs {
                        emb_ctx.clear_kv_cache();

                        let tokens = model.str_to_token(text, AddBos::Always)?;
                        let n = tokens.len().min(emb_ctx.n_ctx() as usize - 1);

                        let mut batch = LlamaBatch::new(n + 1, 1);
                        for (i, &tok) in tokens[..n].iter().enumerate() {
                            let last = i == n - 1;
                            batch.add(tok, i as i32, &[0], last).ok();
                        }

                        emb_ctx.decode(&mut batch).context("embed decode error")?;

                        let vec = emb_ctx.embeddings_seq_ith(0)?;
                        all.push(vec.to_vec());
                    }
                    Ok(all)
                })();

                if let Ok(ref vecs) = embed_result {
                    llm_info!(
                        &app,
                        &log_buf,
                        log_file,
                        "embeddings done — {} vector(s)",
                        vecs.len()
                    );
                }
                result_tx.send(embed_result).ok();
            }

            InferRequest::EmbedImage { bytes, result_tx } => {
                #[cfg(feature = "llm-mtmd")]
                {
                    if let Some(ref mtmd) = mtmd_ctx {
                        use llama_cpp_4::mtmd::{
                            MtmdBitmap, MtmdContext, MtmdInputChunkType, MtmdInputChunks,
                            MtmdInputText,
                        };

                        let embedding = (|| -> Option<Vec<f32>> {
                            let bitmap = MtmdBitmap::from_buf(mtmd, &bytes).ok()?;
                            let bitmap_refs = [&bitmap];

                            let text =
                                MtmdInputText::new(MtmdContext::default_marker(), false, false);
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
                                        for (d, p) in pooled.iter_mut().enumerate().take(n_embd) {
                                            *p += embd[t * n_embd + d];
                                        }
                                    }
                                    if n_tokens > 0 {
                                        for p in pooled.iter_mut().take(n_embd) {
                                            *p /= n_tokens as f32;
                                        }
                                    }
                                    let norm: f32 =
                                        pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
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
                        llm_warn!(
                            &app,
                            &log_buf,
                            log_file,
                            "EmbedImage: no mmproj loaded — returning None"
                        );
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
        // SAFETY: `backend_md` was created by `LlamaBackend::init()` (not
        // zeroed). `ctx` and `model` have already been dropped above, so no
        // live references to the backend remain. We drop exactly once.
        unsafe {
            std::mem::ManuallyDrop::drop(&mut backend_md);
        }
    }

    llm_info!(
        &app,
        &log_buf,
        log_file,
        "actor exiting — GPU resources released"
    );
    app.emit_event("llm:status", json!({"status":"stopped"}));
}
