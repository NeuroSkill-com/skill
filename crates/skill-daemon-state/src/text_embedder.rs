// SPDX-License-Identifier: GPL-3.0-only
//! Shared text embedder backed by `rlx-embed` (the RLX runtime).
//!
//! A single embedder instance is created lazily and shared across labels,
//! hooks, screenshot OCR, and screenshot search. This avoids loading the
//! model weights multiple times.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};

enum LoadedTextEmbedder {
    #[cfg(feature = "text-embeddings-rlx")]
    Rlx(Box<RlxTextEmbedding>),
    /// Compiled without any embedding backend — every embed call returns None.
    /// Keeps the surface compilable so callers don't need to gate their use sites.
    #[allow(dead_code)]
    None,
}

/// Shared, cheaply-cloneable handle to the text embedder.
///
/// The model is loaded **lazily** on first use (not at daemon startup) so the
/// GPU isn't hammered during init.
#[derive(Clone)]
pub struct SharedTextEmbedder {
    inner: Arc<Mutex<Option<LoadedTextEmbedder>>>,
    init: Arc<Once>,
    model_code: Arc<Mutex<String>>,
    rlx_device: Arc<Mutex<String>>,
    rlx_max_seq: Arc<Mutex<usize>>,
}

impl Default for SharedTextEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedTextEmbedder {
    /// Create a new handle **without** loading the model yet.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            init: Arc::new(Once::new()),
            model_code: Arc::new(Mutex::new("nomic-ai/nomic-embed-text-v1.5".into())),
            rlx_device: Arc::new(Mutex::new(default_rlx_device())),
            rlx_max_seq: Arc::new(Mutex::new(512)),
        }
    }

    /// Create a handle that never loads the model (always returns `None`).
    /// Useful for unit tests that don't need real embeddings.
    pub fn new_noop() -> Self {
        let s = Self::new();
        s.init.call_once(|| {});
        s
    }

    /// Set the model code to use. Call [`reload`] after to apply.
    pub fn set_model_code(&self, code: &str) {
        if let Ok(mut guard) = self.model_code.lock() {
            *guard = code.to_string();
        }
    }

    /// Get the current model code.
    pub fn model_code(&self) -> String {
        self.model_code.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn set_rlx_device(&self, device: &str) {
        if let Ok(mut guard) = self.rlx_device.lock() {
            *guard = device.to_string();
        }
    }

    pub fn rlx_device(&self) -> String {
        self.rlx_device
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| default_rlx_device())
    }

    pub fn set_rlx_max_seq(&self, max_seq: usize) {
        if let Ok(mut guard) = self.rlx_max_seq.lock() {
            *guard = max_seq.max(1);
        }
    }

    pub fn rlx_max_seq(&self) -> usize {
        self.rlx_max_seq.lock().map(|g| *g).unwrap_or(512)
    }

    /// Reload the model (e.g. after changing model_code).
    /// Blocks while loading weights. Returns false for unknown model codes.
    pub fn reload(&self) -> bool {
        let code = self.model_code();
        let loaded = load_rlx_embedder(&code, &self.rlx_device(), self.rlx_max_seq());
        let ok = loaded.is_ok();
        match &loaded {
            Ok(_) => eprintln!("[text-embedder] {code} loaded via rlx"),
            Err(e) => eprintln!("[text-embedder] failed to load {code} via rlx: {e:#}"),
        }
        if let Ok(mut guard) = self.inner.lock() {
            *guard = loaded.ok();
        }
        ok
    }

    /// Ensure the model is loaded (called at most once).
    fn ensure_loaded(&self) {
        let inner = self.inner.clone();
        let model_code = self.model_code.clone();
        let rlx_device = self.rlx_device.clone();
        let rlx_max_seq = self.rlx_max_seq.clone();
        self.init.call_once(move || {
            let code = model_code.lock().map(|g| g.clone()).unwrap_or_default();
            let rlx_device = rlx_device
                .lock()
                .map(|g| g.clone())
                .unwrap_or_else(|_| default_rlx_device());
            let rlx_max_seq = rlx_max_seq.lock().map(|g| *g).unwrap_or(512);
            let loaded = load_rlx_embedder(&code, &rlx_device, rlx_max_seq);
            match &loaded {
                Ok(_) => eprintln!("[text-embedder] {code} loaded via rlx"),
                Err(e) => eprintln!("[text-embedder] failed to load {code} via rlx: {e:#}"),
            }
            if let Ok(mut guard) = inner.lock() {
                *guard = loaded.ok();
            }
        });
    }

    /// Embed a single text string.  Returns `None` if the model is not loaded
    /// or embedding fails.
    pub fn embed(&self, text: &str) -> Option<Vec<f32>> {
        self.ensure_loaded();
        let mut guard = self.inner.lock().ok()?;
        let model = guard.as_mut()?;
        let mut vecs = embed_with_loaded(model, vec![text]).ok()?;
        if vecs.is_empty() {
            None
        } else {
            Some(vecs.remove(0))
        }
    }

    /// Embed multiple texts in a single batch.
    pub fn embed_batch(&self, texts: Vec<&str>) -> Option<Vec<Vec<f32>>> {
        self.ensure_loaded();
        let mut guard = self.inner.lock().ok()?;
        let model = guard.as_mut()?;
        embed_with_loaded(model, texts).ok()
    }

    /// Whether the active model expects nomic task-instruction prefixes
    /// (`search_query:` / `search_document:`). nomic-embed-text-v1.5 is trained
    /// with these and produces materially better retrieval when they're used.
    pub fn needs_task_prefix(&self) -> bool {
        self.model_code().to_ascii_lowercase().contains("nomic")
    }

    fn with_prefix(&self, text: &str, task: &str) -> String {
        if self.needs_task_prefix() {
            format!("{task}: {text}")
        } else {
            text.to_string()
        }
    }

    /// Embed a search **query** (prepends `search_query:` for nomic models).
    /// Use this for the text the user is searching *with*.
    pub fn embed_query(&self, text: &str) -> Option<Vec<f32>> {
        if text.trim().is_empty() {
            return None;
        }
        self.embed(&self.with_prefix(text, "search_query"))
    }

    /// Embed an indexed **document** (prepends `search_document:` for nomic
    /// models). Use this for text being stored/indexed for later retrieval.
    pub fn embed_document(&self, text: &str) -> Option<Vec<f32>> {
        if text.trim().is_empty() {
            return None;
        }
        self.embed(&self.with_prefix(text, "search_document"))
    }

    /// Batch variant of [`embed_document`].
    pub fn embed_documents(&self, texts: Vec<&str>) -> Option<Vec<Vec<f32>>> {
        self.embed_batch_prefixed(texts, "search_document")
    }

    /// Batch variant of [`embed_query`].
    pub fn embed_queries(&self, texts: Vec<&str>) -> Option<Vec<Vec<f32>>> {
        self.embed_batch_prefixed(texts, "search_query")
    }

    fn embed_batch_prefixed(&self, texts: Vec<&str>, task: &str) -> Option<Vec<Vec<f32>>> {
        if self.needs_task_prefix() {
            let prefixed: Vec<String> = texts.iter().map(|t| format!("{task}: {t}")).collect();
            self.embed_batch(prefixed.iter().map(String::as_str).collect())
        } else {
            self.embed_batch(texts)
        }
    }
}

fn default_rlx_device() -> String {
    // Apple Silicon: Metal is the fastest backend for these models — benchmarked
    // ~1144 docs/s (nomic-text) vs MLX ~833 and CPU ~296. See bench_text_embeddings.
    if cfg!(target_os = "macos") {
        "metal".into()
    } else {
        "cpu".into()
    }
}

fn embed_with_loaded(model: &mut LoadedTextEmbedder, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
    match model {
        #[cfg(feature = "text-embeddings-rlx")]
        LoadedTextEmbedder::Rlx(model) => model.embed(texts),
        LoadedTextEmbedder::None => Err(anyhow!(
            "no text-embedding backend compiled in (enable text-embeddings-rlx)"
        )),
    }
}

#[cfg(not(feature = "text-embeddings-rlx"))]
fn load_rlx_embedder(_code: &str, _device: &str, _max_seq: usize) -> Result<LoadedTextEmbedder> {
    Err(anyhow!(
        "RLX text embeddings requested but this build lacks the text-embeddings-rlx feature"
    ))
}

#[cfg(feature = "text-embeddings-rlx")]
fn load_rlx_embedder(code: &str, device: &str, max_seq: usize) -> Result<LoadedTextEmbedder> {
    Ok(LoadedTextEmbedder::Rlx(Box::new(RlxTextEmbedding::from_repo(
        code, device, max_seq,
    )?)))
}

#[cfg(feature = "text-embeddings-rlx")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RlxArch {
    Bert,
    NomicBert,
}

#[cfg(feature = "text-embeddings-rlx")]
struct RlxTextEmbedding {
    tokenizer: tokenizers::Tokenizer,
    compiled: rlx::runtime::CompiledGraph,
    arch: RlxArch,
    hidden_size: usize,
    pooling: rlx_models::Pooling,
    compiled_bs: (usize, usize),
    config_path: PathBuf,
    weights_path: String,
    device: rlx::Device,
    max_seq: usize,
}

#[cfg(feature = "text-embeddings-rlx")]
impl RlxTextEmbedding {
    fn from_repo(repo_id: &str, device: &str, max_seq: usize) -> Result<Self> {
        let repo = hf_hub::api::sync::ApiBuilder::new()
            .with_progress(true)
            .build()?
            .model(repo_id.to_string());
        let config_path = repo.get("config.json")?;
        let tokenizer_path = repo.get("tokenizer.json")?;
        let weights_path = repo.get("model.safetensors")?;
        let tokenizer =
            tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|e| anyhow!("loading tokenizer.json: {e}"))?;
        let arch = detect_rlx_arch(&config_path)?;
        let pooling = default_pooling(repo_id);
        let device = parse_rlx_device(device)?;
        if !rlx::runtime::is_available(device) {
            return Err(anyhow!("RLX device '{}' is not available in this build", device.name()));
        }
        let weights_path = weights_path
            .to_str()
            .ok_or_else(|| anyhow!("non-utf8 weights path"))?
            .to_string();
        let (hidden_size, compiled) = compile_rlx_embedder(arch, &config_path, &weights_path, 1, 1, device)?;

        Ok(Self {
            tokenizer,
            compiled,
            arch,
            hidden_size,
            pooling,
            compiled_bs: (1, 1),
            config_path,
            weights_path,
            device,
            max_seq: max_seq.max(1),
        })
    }

    fn embed(&mut self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut ids_rows = Vec::with_capacity(texts.len());
        for text in texts {
            let enc = self
                .tokenizer
                .encode(text, true)
                .map_err(|e| anyhow!("tokenizing text: {e}"))?;
            let mut ids = enc.get_ids().iter().map(|&id| id as f32).collect::<Vec<_>>();
            ids.truncate(self.max_seq);
            if ids.is_empty() {
                ids.push(0.0);
            }
            ids_rows.push(ids);
        }

        let batch = ids_rows.len();
        let seq = ids_rows.iter().map(Vec::len).max().unwrap_or(1).min(self.max_seq);
        self.ensure_compiled(batch, seq)?;

        let mut input_ids = vec![0.0f32; batch * seq];
        let mut attention_mask = vec![0.0f32; batch * seq];
        let token_type_ids = vec![0.0f32; batch * seq];
        let mut position_ids = vec![0.0f32; batch * seq];
        let mut lengths = Vec::with_capacity(batch);

        for (row_idx, ids) in ids_rows.iter().enumerate() {
            let n = ids.len().min(seq);
            lengths.push(n);
            let base = row_idx * seq;
            input_ids[base..base + n].copy_from_slice(&ids[..n]);
            for i in 0..seq {
                position_ids[base + i] = i as f32;
            }
            for i in 0..n {
                attention_mask[base + i] = 1.0;
            }
        }

        let mut owned_inputs: Vec<(&str, &[f32])> = vec![
            ("input_ids", input_ids.as_slice()),
            ("attention_mask", attention_mask.as_slice()),
            ("token_type_ids", token_type_ids.as_slice()),
        ];
        if matches!(self.arch, RlxArch::Bert) {
            owned_inputs.push(("position_ids", position_ids.as_slice()));
        }

        let outputs = self.compiled.run(&owned_inputs);
        let hidden = outputs
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("RLX embedder returned no output"))?;
        let mut result = Vec::with_capacity(batch);
        #[allow(clippy::needless_range_loop)]
        for row in 0..batch {
            let mut pooled = match self.pooling {
                rlx_models::Pooling::Cls => {
                    let start = row * seq * self.hidden_size;
                    hidden[start..start + self.hidden_size].to_vec()
                }
                rlx_models::Pooling::Mean => {
                    let n = lengths[row].max(1);
                    let mut v = vec![0.0f32; self.hidden_size];
                    for pos in 0..n {
                        let start = (row * seq + pos) * self.hidden_size;
                        for d in 0..self.hidden_size {
                            v[d] += hidden[start + d];
                        }
                    }
                    for x in &mut v {
                        *x /= n as f32;
                    }
                    v
                }
            };
            l2_normalize(&mut pooled);
            result.push(pooled);
        }
        Ok(result)
    }

    fn ensure_compiled(&mut self, batch: usize, seq: usize) -> Result<()> {
        if self.compiled_bs == (batch, seq) {
            return Ok(());
        }
        let (hidden_size, compiled) = compile_rlx_embedder(
            self.arch,
            &self.config_path,
            &self.weights_path,
            batch,
            seq,
            self.device,
        )?;
        self.hidden_size = hidden_size;
        self.compiled = compiled;
        self.compiled_bs = (batch, seq);
        Ok(())
    }
}

#[cfg(feature = "text-embeddings-rlx")]
fn detect_rlx_arch(config_path: &std::path::Path) -> Result<RlxArch> {
    let data = std::fs::read_to_string(config_path)?;
    let json: serde_json::Value = serde_json::from_str(&data)?;
    if json.get("img_size").is_some() && json.get("patch_size").is_some() {
        return Err(anyhow!("RLX text embeddings do not support vision embedding configs"));
    }
    if json.get("rotary_emb_base").is_some() || json.get("rotary_emb_fraction").is_some() {
        Ok(RlxArch::NomicBert)
    } else {
        Ok(RlxArch::Bert)
    }
}

#[cfg(feature = "text-embeddings-rlx")]
fn compile_rlx_embedder(
    arch: RlxArch,
    config_path: &std::path::Path,
    weights_path: &str,
    batch: usize,
    seq: usize,
    device: rlx::Device,
) -> Result<(usize, rlx::runtime::CompiledGraph)> {
    let mut wm = rlx_models::WeightMap::from_file(weights_path)?;
    let (graph, params, hidden_size) = match arch {
        RlxArch::Bert => {
            let cfg = rlx_models::BertConfig::from_file(config_path)?;
            let hidden_size = cfg.hidden_size;
            let (graph, params) = rlx_models::build_bert_graph_sized(&cfg, &mut wm, batch, seq)?;
            (graph, params, hidden_size)
        }
        RlxArch::NomicBert => {
            let cfg = rlx_models::NomicBertConfig::from_file(config_path)?;
            let hidden_size = cfg.hidden_size;
            let (graph, params) = rlx_models::nomic::build_nomic_graph_sized(&cfg, &mut wm, batch, seq)?;
            (graph, params, hidden_size)
        }
    };
    let session = rlx::runtime::Session::new_with_precision(device, rlx::runtime::Precision::F16);
    let mut compiled = session.compile(graph);
    for (name, data) in &params {
        compiled.set_param(name, data);
    }
    Ok((hidden_size, compiled))
}

#[cfg(feature = "text-embeddings-rlx")]
fn default_pooling(repo_id: &str) -> rlx_models::Pooling {
    let lower = repo_id.to_ascii_lowercase();
    if lower.contains("bge") || lower.contains("nomic") {
        rlx_models::Pooling::Cls
    } else {
        rlx_models::Pooling::Mean
    }
}

#[cfg(feature = "text-embeddings-rlx")]
fn parse_rlx_device(tag: &str) -> Result<rlx::Device> {
    match tag.to_ascii_lowercase().as_str() {
        "cpu" => Ok(rlx::Device::Cpu),
        "metal" => Ok(rlx::Device::Metal),
        "mlx" => Ok(rlx::Device::Mlx),
        "gpu" | "wgpu" => Ok(rlx::Device::Gpu),
        "cuda" => Ok(rlx::Device::Cuda),
        "rocm" => Ok(rlx::Device::Rocm),
        "tpu" => Ok(rlx::Device::Tpu),
        other => Err(anyhow!("unsupported RLX device '{other}'")),
    }
}

#[cfg(feature = "text-embeddings-rlx")]
fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomic_models_get_task_prefixes() {
        let te = SharedTextEmbedder::new();
        te.set_model_code("nomic-ai/nomic-embed-text-v1.5");
        assert!(te.needs_task_prefix());
        assert_eq!(te.with_prefix("hello", "search_query"), "search_query: hello");
        assert_eq!(te.with_prefix("a doc", "search_document"), "search_document: a doc");
    }

    #[test]
    fn non_nomic_models_get_no_prefix() {
        let te = SharedTextEmbedder::new();
        te.set_model_code("BAAI/bge-small-en-v1.5");
        assert!(!te.needs_task_prefix());
        assert_eq!(te.with_prefix("hello", "search_query"), "hello");
    }
}
