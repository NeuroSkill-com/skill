// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Public init function — spawns the actor and returns the shared state.

use std::sync::{Arc, Mutex, atomic::AtomicBool};

use serde_json::json;
use tokio::sync::mpsc;

use crate::config::LlmConfig;
use crate::event::LlmEventEmitter;
use crate::catalog::LlmCatalog;
use super::logging::{LlmLogBuffer, push_log};
use super::protocol::InferRequest;
use super::state::LlmServerState;
use super::actor::run_actor;

const LLM_LOG_DIR: &str = skill_constants::LLM_LOG_DIR;

/// Initialise the LLM server state.
///
/// Spawns the inference actor thread and returns the shared state used by the
/// axum router.  Returns `None` when:
/// - `config.enabled == false`
/// - No model is selected or the model file does not exist
pub fn init(
    config:    &LlmConfig,
    catalog:   &LlmCatalog,
    app: Arc<dyn LlmEventEmitter>,
    log_buf:   LlmLogBuffer,
    skill_dir: &std::path::Path,
) -> Option<Arc<LlmServerState>> {
    if !config.enabled {
        push_log(&app, &log_buf, "info", "LLM server disabled — skipping init");
        return None;
    }

    let model_path = catalog.active_model_path()
        .or_else(|| config.model_path.clone())
        .or_else(|| {
            push_log(&app, &log_buf, "warn", "no model selected — LLM server disabled");
            None
        })?;

    if !model_path.exists() {
        push_log(&app, &log_buf, "error",
            &format!("model file not found: {} — LLM server disabled", model_path.display()));
        return None;
    }

    // Resolve the mmproj path: explicit selection → auto-detect from catalog →
    // legacy config.mmproj field.
    let active_model_repo = catalog.active_model_entry().map(|e| e.repo.as_str());
    let mmproj_path = catalog
        .resolve_mmproj_path(config.autoload_mmproj)
        .or_else(|| config.mmproj.clone())
        .filter(|p| {
            let Some(model_repo) = active_model_repo else { return true; };

            let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or_default();
            let mmproj_repo = catalog.entries.iter()
                .find(|e| {
                    e.is_mmproj
                        && (e.local_path.as_ref().is_some_and(|lp| lp == p)
                            || e.filename == file_name)
                })
                .map(|e| e.repo.as_str());

            if let Some(mm_repo) = mmproj_repo {
                if mm_repo != model_repo {
                    push_log(&app, &log_buf, "warn",
                        &format!(
                            "mmproj/model repo mismatch — skipping vision projector: {} \
                             (mmproj repo: {}, model repo: {})",
                            p.display(), mm_repo, model_repo,
                        ));
                    return false;
                }
            }
            true
        })
        .filter(|p| {
            if p.exists() { return true; }
            push_log(&app, &log_buf, "warn",
                &format!("mmproj file not found (deleted?): {} — skipping vision", p.display()));
            false
        });

    let model_name  = model_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("llama.cpp-model")
        .to_owned();

    // ── Resolve effective context size ─────────────────────────────────────────
    // When the user hasn't explicitly set a context size (`ctx_size == None`),
    // use llmfit-based recommendation derived from the model's parameter count
    // and the system's available GPU/unified memory.  When the user *has* set
    // a value, cap it at the model's trained maximum context length.
    let mut config = config.clone();
    if let Some(entry) = catalog.active_model_entry() {
        // Carry model metadata into the config so the actor can estimate
        // memory for dynamic context resizing at runtime.
        config.params_b = entry.params_b;
        config.quant = entry.quant.clone();
        config.max_context_length = entry.max_context_length;

        if config.ctx_size.is_none() {
            let recommended = crate::catalog::recommend_ctx_size(entry);
            push_log(&app, &log_buf, "info",
                &format!("auto context size: {recommended} tokens \
                          (params={:.1}B, max_ctx={}, quant={})",
                    entry.params_b, entry.max_context_length, entry.quant));
            config.ctx_size = Some(recommended);
        } else if entry.max_context_length > 0 {
            // Cap user-set context at the model's trained maximum.
            let user_ctx = config.ctx_size.unwrap_or(entry.max_context_length);
            if user_ctx > entry.max_context_length {
                push_log(&app, &log_buf, "warn",
                    &format!("user ctx_size {} exceeds model max {} — capping",
                        user_ctx, entry.max_context_length));
                config.ctx_size = Some(entry.max_context_length);
            }
        }
    }

    push_log(&app, &log_buf, "info", &format!("starting LLM server — model: {model_name}"));

    // ── Per-session log file ──────────────────────────────────────────────────
    let ts_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let log_dir = skill_dir.join(LLM_LOG_DIR);
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join(format!("llm_{ts_secs}.txt"));
    push_log(&app, &log_buf, "info", &format!("session log → {}", log_path.display()));

    let (req_tx, req_rx) = mpsc::unbounded_channel::<InferRequest>();
    let ready_flag  = Arc::new(AtomicBool::new(false));
    let n_ctx_flag  = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let vision_flag = Arc::new(AtomicBool::new(false));
    let allowed_tools = Arc::new(Mutex::new(config.tools.clone()));
    let (abort_tx, _) = tokio::sync::watch::channel(0u64);

    let config2     = config.clone();
    let path2       = model_path.clone();
    let mmproj2     = mmproj_path.clone();
    let app2        = app.clone();
    let buf2        = log_buf.clone();
    let ready2      = ready_flag.clone();
    let n_ctx2      = n_ctx_flag.clone();
    let vision2     = vision_flag.clone();

    let join_handle = std::thread::Builder::new()
        .name("llm-actor".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || run_actor(req_rx, config2, path2, mmproj2, app2, buf2,
                                 Some(log_path), ready2, n_ctx2, vision2))
        .expect("failed to spawn llm-actor thread");

    app.emit_event("llm:status", json!({"status":"loading","model":model_name}));

    // Base scripts directory — subdirectories created lazily per tool invocation.
    let scripts_dir = skill_dir.join("chats").join("scripts");
    let _ = std::fs::create_dir_all(&scripts_dir);

    // Discover Agent Skills from all configured locations.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let bundled_skills_dir = exe_dir.as_ref()
        .map(|d| d.join(skill_constants::SKILLS_SUBDIR))
        .filter(|d| d.is_dir())
        .or_else(|| {
            let cwd = std::env::current_dir().ok()?;
            let p = cwd.join(skill_constants::SKILLS_SUBDIR);
            if p.is_dir() { Some(p) } else { None }
        });
    let skills_result = skill_skills::load_skills(&skill_skills::LoadSkillsOptions {
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        skill_dir: skill_dir.to_path_buf(),
        bundled_dir: bundled_skills_dir,
        skill_paths: Vec::new(),
        include_defaults: true,
    });
    let n_skills = skills_result.skills.len();
    for diag in &skills_result.diagnostics {
        push_log(&app, &log_buf, &diag.level, &format!("[skills] {}: {}", diag.path, diag.message));
    }
    if n_skills > 0 {
        let names: Vec<&str> = skills_result.skills.iter().map(|s| s.name.as_str()).collect();
        push_log(&app, &log_buf, "info", &format!("discovered {n_skills} skill(s): {}", names.join(", ")));
    }

    Some(Arc::new(LlmServerState {
        req_tx,
        model_name,
        api_key:      config.api_key.clone(),
        allowed_tools,
        cancelled_tool_calls: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        scripts_dir,
        skills:       Arc::new(skills_result.skills),
        ready:        ready_flag,
        n_ctx:        n_ctx_flag,
        vision_ready: vision_flag,
        abort_tx,
        join_handle:  Mutex::new(Some(join_handle)),
    }))
}
