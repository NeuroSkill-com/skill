//! HuggingFace Hub model downloader and NeuCodec checkpoint converter.
//!
//! Downloads (or reuses cached copies of) the GGUF backbone from HuggingFace,
//! then constructs and returns a [`NeuTTS`](crate::model::NeuTTS).
//!
//! Files are cached under `~/.cache/huggingface/hub`; subsequent calls return
//! immediately from cache without a network request.

#![allow(dead_code)]

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use hf_hub::{api::sync::Api, api::Progress, Cache, Repo};

use crate::model::NeuTTS;

// ─────────────────────────────────────────────────────────────────────────────
// Model registry
// ─────────────────────────────────────────────────────────────────────────────

/// Metadata for a single backbone repository.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub repo: &'static str,
    pub name: &'static str,
    pub language: &'static str,
    pub params: &'static str,
    pub is_gguf: bool,
    pub size_mb: u32,
    pub pros: &'static str,
    pub cons: &'static str,
    pub recommended: bool,
}

pub const BACKBONE_MODELS: &[ModelInfo] = &[
    ModelInfo {
        repo: "neuphonic/neutts-nano-q4-gguf",
        name: "NeuTTS Nano Q4",
        language: "en-us",
        params: "0.2B",
        is_gguf: true,
        size_mb: 135,
        pros: "Fast CPU inference · small download · low RAM usage",
        cons: "Slightly lower quality than Q8; may clip on complex sentences",
        recommended: true,
    },
    ModelInfo {
        repo: "neuphonic/neutts-nano-q8-gguf",
        name: "NeuTTS Nano Q8",
        language: "en-us",
        params: "0.2B",
        is_gguf: true,
        size_mb: 230,
        pros: "Better voice quality than Q4 · still fast on modern CPUs",
        cons: "2× larger download than Q4; needs ~500 MB RAM",
        recommended: false,
    },
    ModelInfo {
        repo: "neuphonic/neutts-nano",
        name: "NeuTTS Nano (full fp16)",
        language: "en-us",
        params: "0.2B",
        is_gguf: false,
        size_mb: 430,
        pros: "Reference-quality for Nano; best baseline for fine-tuning",
        cons: "Slowest of the Nano variants; requires FP16 build",
        recommended: false,
    },
    ModelInfo {
        repo: "neuphonic/neutts-air-q4-gguf",
        name: "NeuTTS Air Q4",
        language: "en-us",
        params: "0.7B",
        is_gguf: true,
        size_mb: 430,
        pros: "High naturalness · richer prosody than Nano · voice cloning",
        cons: "3× heavier than Nano Q4; slower on older hardware; ~900 MB RAM",
        recommended: false,
    },
    ModelInfo {
        repo: "neuphonic/neutts-air-q8-gguf",
        name: "NeuTTS Air Q8",
        language: "en-us",
        params: "0.7B",
        is_gguf: true,
        size_mb: 820,
        pros: "Near-lossless quality for the 0.7B model",
        cons: "Large download (~820 MB); needs ~1.5 GB RAM",
        recommended: false,
    },
    ModelInfo {
        repo: "neuphonic/neutts-air",
        name: "NeuTTS Air (full fp16)",
        language: "en-us",
        params: "0.7B",
        is_gguf: false,
        size_mb: 1450,
        pros: "Highest possible quality for on-device English TTS",
        cons: "Very large (~1.5 GB); slow on CPU; requires FP16 build",
        recommended: false,
    },
    ModelInfo {
        repo: "neuphonic/neutts-nano-german-q4-gguf",
        name: "NeuTTS Nano German Q4",
        language: "de",
        params: "0.2B",
        is_gguf: true,
        size_mb: 135,
        pros: "Compact German TTS · fast CPU inference",
        cons: "Q4 quantisation; lower quality than Q8",
        recommended: true,
    },
    ModelInfo {
        repo: "neuphonic/neutts-nano-german-q8-gguf",
        name: "NeuTTS Nano German Q8",
        language: "de",
        params: "0.2B",
        is_gguf: true,
        size_mb: 230,
        pros: "Better German voice quality than Q4",
        cons: "2× larger download",
        recommended: false,
    },
    ModelInfo {
        repo: "neuphonic/neutts-nano-french-q4-gguf",
        name: "NeuTTS Nano French Q4",
        language: "fr-fr",
        params: "0.2B",
        is_gguf: true,
        size_mb: 135,
        pros: "Compact French TTS · fast CPU inference",
        cons: "Q4 quantisation; lower quality than Q8",
        recommended: true,
    },
    ModelInfo {
        repo: "neuphonic/neutts-nano-french-q8-gguf",
        name: "NeuTTS Nano French Q8",
        language: "fr-fr",
        params: "0.2B",
        is_gguf: true,
        size_mb: 230,
        pros: "Better French voice quality than Q4",
        cons: "2× larger download",
        recommended: false,
    },
    ModelInfo {
        repo: "neuphonic/neutts-nano-spanish-q4-gguf",
        name: "NeuTTS Nano Spanish Q4",
        language: "es",
        params: "0.2B",
        is_gguf: true,
        size_mb: 135,
        pros: "Compact Spanish TTS · fast CPU inference",
        cons: "Q4 quantisation; lower quality than Q8",
        recommended: true,
    },
    ModelInfo {
        repo: "neuphonic/neutts-nano-spanish-q8-gguf",
        name: "NeuTTS Nano Spanish Q8",
        language: "es",
        params: "0.2B",
        is_gguf: true,
        size_mb: 230,
        pros: "Better Spanish voice quality than Q4",
        cons: "2× larger download",
        recommended: false,
    },
];

pub fn find_model(repo: &str) -> Option<&'static ModelInfo> {
    BACKBONE_MODELS.iter().find(|m| m.repo == repo)
}

fn backbone_language(repo: &str) -> &'static str {
    find_model(repo).map(|m| m.language).unwrap_or("en-us")
}

// ─────────────────────────────────────────────────────────────────────────────
// Progress reporting
// ─────────────────────────────────────────────────────────────────────────────

/// Progress event emitted during model loading.
#[derive(Debug, Clone)]
pub enum LoadProgress {
    Fetching {
        step: u32,
        total: u32,
        file: String,
        repo: String,
        size_mb: Option<u32>,
    },
    Downloading {
        step: u32,
        total: u32,
        downloaded: u64,
        total_bytes: u64,
    },
    Loading {
        step: u32,
        total: u32,
        component: String,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Download helpers
// ─────────────────────────────────────────────────────────────────────────────

struct HfProgress<F: FnMut(u64, u64)> {
    on_bytes: F,
    downloaded: u64,
    total: u64,
}

impl<F: FnMut(u64, u64)> Progress for HfProgress<F> {
    fn init(&mut self, size: usize, _filename: &str) {
        self.total = size as u64;
        (self.on_bytes)(0, self.total);
    }
    fn update(&mut self, size: usize) {
        self.downloaded += size as u64;
        (self.on_bytes)(self.downloaded, self.total);
    }
    fn finish(&mut self) {
        (self.on_bytes)(self.total, self.total);
    }
}

fn hf_download_cb<F: FnMut(u64, u64)>(api: &Api, repo_id: &str, filename: &str, mut on_bytes: F) -> Result<PathBuf> {
    let cache_repo = Cache::from_env().repo(Repo::model(repo_id.to_string()));
    if let Some(path) = cache_repo.get(filename) {
        on_bytes(1, 1);
        return Ok(path);
    }
    let api_repo = api.model(repo_id.to_string());
    let progress = HfProgress {
        on_bytes,
        downloaded: 0,
        total: 0,
    };
    api_repo
        .download_with_progress(filename, progress)
        .with_context(|| format!("Failed to download '{filename}' from '{repo_id}'"))
}

fn hf_download(api: &Api, repo_id: &str, filename: &str) -> Result<PathBuf> {
    hf_download_cb(api, repo_id, filename, |_, _| {})
}

fn hf_list_files(api: &Api, repo_id: &str) -> Result<Vec<String>> {
    let repo = api.model(repo_id.to_string());
    let info = repo
        .info()
        .with_context(|| format!("Failed to fetch repo info for '{repo_id}'"))?;
    Ok(info.siblings.into_iter().map(|s| s.rfilename).collect())
}

fn hf_download_by_extension(api: &Api, repo_id: &str, extensions: &[&str]) -> Result<PathBuf> {
    let files = hf_list_files(api, repo_id)?;
    for ext in extensions {
        if let Some(fname) = files.iter().find(|f| f.ends_with(ext)) {
            return hf_download(api, repo_id, fname);
        }
    }
    bail!(
        "No file with extension {:?} found in '{}'.\nAvailable files: {:?}",
        extensions,
        repo_id,
        files
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Public constants
// ─────────────────────────────────────────────────────────────────────────────

pub const CODEC_DECODER_REPO: &str = "neuphonic/neucodec";
pub const CODEC_SOURCE_FILE: &str = "pytorch_model.bin";
pub const CODEC_DECODER_FILE: &str = "neucodec_decoder.safetensors";
pub const CODEC_DECODER_LOCAL: &str = "models/neucodec_decoder.safetensors";
pub const CODEC_DECODER_SIZE_MB: u32 = 1_100;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Download and load a [`NeuTTS`] model from HuggingFace Hub with progress.
pub fn load_from_hub_cb<F>(backbone_repo: &str, gguf_file: Option<&str>, mut on_progress: F) -> Result<NeuTTS>
where
    F: FnMut(LoadProgress),
{
    let api = Api::new().context("Failed to initialise HuggingFace Hub client")?;

    let backbone_size_mb = find_model(backbone_repo).map(|m| m.size_mb);
    let file_label = gguf_file.unwrap_or("*.gguf").to_string();
    on_progress(LoadProgress::Fetching {
        step: 1,
        total: 3,
        file: file_label,
        repo: backbone_repo.into(),
        size_mb: backbone_size_mb,
    });

    let resolved_gguf: String = match gguf_file {
        Some(fname) => fname.to_string(),
        None => {
            let files = hf_list_files(&api, backbone_repo)
                .with_context(|| format!("Failed to list files in '{backbone_repo}'"))?;
            files
                .into_iter()
                .find(|f| f.ends_with(".gguf"))
                .with_context(|| format!("No .gguf file found in '{backbone_repo}'"))?
        }
    };
    let backbone_path = hf_download_cb(&api, backbone_repo, &resolved_gguf, |dl, tot| {
        on_progress(LoadProgress::Downloading {
            step: 1,
            total: 3,
            downloaded: dl,
            total_bytes: tot,
        });
    })
    .with_context(|| format!("Failed to download '{resolved_gguf}' from '{backbone_repo}'"))?;

    let local_decoder = std::path::Path::new(CODEC_DECODER_LOCAL);
    let decoder_path: PathBuf = if local_decoder.exists() {
        on_progress(LoadProgress::Fetching {
            step: 2,
            total: 3,
            file: CODEC_DECODER_FILE.into(),
            repo: "(local cache)".into(),
            size_mb: None,
        });
        local_decoder.to_path_buf()
    } else {
        on_progress(LoadProgress::Fetching {
            step: 2,
            total: 3,
            file: CODEC_SOURCE_FILE.into(),
            repo: CODEC_DECODER_REPO.into(),
            size_mb: Some(CODEC_DECODER_SIZE_MB),
        });
        let bin_path = hf_download_cb(&api, CODEC_DECODER_REPO, CODEC_SOURCE_FILE, |dl, tot| {
            on_progress(LoadProgress::Downloading {
                step: 2,
                total: 3,
                downloaded: dl,
                total_bytes: tot,
            });
        })
        .with_context(|| format!("Failed to download '{CODEC_SOURCE_FILE}' from '{CODEC_DECODER_REPO}'"))?;
        on_progress(LoadProgress::Loading {
            step: 2,
            total: 3,
            component: format!("converting {CODEC_SOURCE_FILE} → {CODEC_DECODER_FILE}"),
        });
        convert_checkpoint(&bin_path, local_decoder).context("Failed to convert NeuCodec checkpoint to safetensors")?;
        local_decoder.to_path_buf()
    };

    on_progress(LoadProgress::Loading {
        step: 3,
        total: 3,
        component: "backbone + NeuCodec decoder".into(),
    });
    let language = backbone_language(backbone_repo).to_string();
    NeuTTS::load_with_decoder(&backbone_path, &decoder_path, &language)
}

/// Download and load a [`NeuTTS`] model from HuggingFace Hub (no progress).
pub fn load_from_hub(backbone_repo: &str) -> Result<NeuTTS> {
    load_from_hub_cb(backbone_repo, None, |_| {})
}

/// Load the default NeuTTS-Nano Q4 model.
pub fn load_default() -> Result<NeuTTS> {
    load_from_hub("neuphonic/neutts-nano-q4-gguf")
}

pub fn list_gguf_files(backbone_repo: &str) -> Result<Vec<String>> {
    let api = Api::new().context("Failed to initialise HuggingFace Hub client")?;
    let files = hf_list_files(&api, backbone_repo)?;
    Ok(files.into_iter().filter(|f| f.ends_with(".gguf")).collect())
}

pub fn supported_backbone_repos() -> Vec<&'static str> {
    BACKBONE_MODELS.iter().map(|m| m.repo).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Checkpoint conversion (pytorch_model.bin → safetensors, no PyTorch needed)
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a `pytorch_model.bin` ZIP archive to a safetensors file (pure Rust).
pub fn convert_neucodec_checkpoint(
    bin_path: &std::path::Path,
    out_path: &std::path::Path,
    n_heads: u32,
    repo: &str,
) -> Result<()> {
    convert_checkpoint_inner(bin_path, out_path, n_heads, repo)
}

fn convert_checkpoint(bin_path: &std::path::Path, out_path: &std::path::Path) -> Result<()> {
    convert_checkpoint_inner(bin_path, out_path, 16, CODEC_DECODER_REPO)
}

fn convert_checkpoint_inner(
    bin_path: &std::path::Path,
    out_path: &std::path::Path,
    n_heads: u32,
    repo: &str,
) -> Result<()> {
    use safetensors::tensor::TensorView;
    use std::io::Read;
    use zip::ZipArchive;

    println!(
        "[neutts] Converting {} → {} (this runs once) …",
        bin_path.display(),
        out_path.display()
    );

    let file = std::fs::File::open(bin_path).with_context(|| format!("Cannot open {}", bin_path.display()))?;
    let mut zip = ZipArchive::new(file).context("Not a valid PyTorch ZIP archive")?;

    let prefix = {
        let first = zip.by_index(0).context("Empty ZIP archive")?;
        first.name().split('/').next().unwrap_or("archive").to_string()
    };

    let pkl_bytes = {
        let mut pkl = zip
            .by_name(&format!("{prefix}/data.pkl"))
            .with_context(|| format!("data.pkl not found in archive (prefix='{prefix}')"))?;
        let mut buf = Vec::new();
        pkl.read_to_end(&mut buf)?;
        buf
    };

    let tensors = parse_pickle_metadata(&pkl_bytes).context("Failed to parse pickle tensor metadata")?;

    println!(
        "[neutts] Checkpoint: {} tensors; extracting decoder subset …",
        tensors.len()
    );

    let decoder_prefixes = ["generator.", "fc_post_a."];
    let mut st_map: std::collections::BTreeMap<String, Vec<u8>> = std::collections::BTreeMap::new();
    let mut shapes_map: std::collections::BTreeMap<String, Vec<usize>> = std::collections::BTreeMap::new();

    for (name, meta) in &tensors {
        if !decoder_prefixes.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let data_path = format!("{prefix}/data/{}", meta.storage_key);
        let raw_bytes = {
            let mut entry = zip
                .by_name(&data_path)
                .with_context(|| format!("Storage file '{data_path}' not in archive"))?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            buf
        };

        let f32_bytes = if meta.is_bf16 {
            raw_bytes
                .chunks_exact(2)
                .map(|b| {
                    let bits = u16::from_le_bytes([b[0], b[1]]);
                    f32::from_bits((bits as u32) << 16)
                })
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<u8>>()
        } else {
            let elem_bytes = 4usize;
            let start = meta.storage_offset * elem_bytes;
            let numel: usize = meta.shape.iter().product();
            let end = start + numel * elem_bytes;
            raw_bytes[start..end.min(raw_bytes.len())].to_vec()
        };

        shapes_map.insert(name.clone(), meta.shape.clone());
        st_map.insert(name.clone(), f32_bytes);
    }

    if st_map.is_empty() {
        bail!("No decoder tensors found in checkpoint — unexpected checkpoint structure");
    }
    println!("[neutts] Extracted {} decoder tensors", st_map.len());

    let hidden_dim = shapes_map
        .get("generator.backbone.embed.weight")
        .map(|s| s[0])
        .unwrap_or(1024);
    let out_dim = shapes_map
        .get("generator.head.out.weight")
        .map(|s| s[0])
        .unwrap_or(1922);
    let hop_length = (out_dim - 2) / 4;
    let depth = tensors
        .keys()
        .filter(|k| k.starts_with("generator.backbone.transformers.") && k.ends_with(".att_norm.weight"))
        .count();

    let mut views: Vec<(&str, TensorView<'_>)> = Vec::new();
    let entries: Vec<(String, Vec<u8>)> = st_map.into_iter().collect();
    for (name, bytes) in &entries {
        let shape = shapes_map[name].clone();
        let view = TensorView::new(safetensors::tensor::Dtype::F32, shape, bytes)
            .with_context(|| format!("TensorView failed for '{name}'"))?;
        views.push((name.as_str(), view));
    }

    let mut metadata = std::collections::HashMap::new();
    metadata.insert("hidden_dim".to_string(), hidden_dim.to_string());
    metadata.insert("depth".to_string(), depth.to_string());
    metadata.insert("n_heads".to_string(), n_heads.to_string());
    metadata.insert("hop_length".to_string(), hop_length.to_string());
    metadata.insert("source".to_string(), repo.to_string());

    std::fs::create_dir_all(out_path.parent().unwrap_or(std::path::Path::new(".")))
        .context("Cannot create models/ directory")?;
    safetensors::serialize_to_file(views.iter().map(|(n, v)| (*n, v)), &Some(metadata), out_path)
        .with_context(|| format!("Failed to write {}", out_path.display()))?;

    let size_mb = std::fs::metadata(out_path)?.len() / 1_048_576;
    println!("[neutts] Saved {} MB → {}", size_mb, out_path.display());
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Minimal pickle parser (reconstructs tensor metadata only)
// ─────────────────────────────────────────────────────────────────────────────

struct TensorMeta {
    storage_key: String,
    storage_offset: usize,
    shape: Vec<usize>,
    is_bf16: bool,
}

impl Clone for TensorMeta {
    fn clone(&self) -> Self {
        TensorMeta {
            storage_key: self.storage_key.clone(),
            storage_offset: self.storage_offset,
            shape: self.shape.clone(),
            is_bf16: self.is_bf16,
        }
    }
}

fn parse_pickle_metadata(pkl: &[u8]) -> Result<std::collections::BTreeMap<String, TensorMeta>> {
    use std::collections::BTreeMap;

    const MARK: u8 = b'(';
    const STOP: u8 = b'.';
    const POP: u8 = b'0';
    const POP_MARK: u8 = b'1';
    const DUP: u8 = b'2';
    const FLOAT: u8 = b'F';
    const INT: u8 = b'I';
    const LONG: u8 = b'L';
    const NONE: u8 = b'N';
    const REDUCE: u8 = b'R';
    const STRING: u8 = b'S';
    const UNICODE: u8 = b'V';
    const APPEND: u8 = b'a';
    const BUILD: u8 = b'b';
    const GLOBAL: u8 = b'c';
    const DICT: u8 = b'd';
    const EMPTY_DICT: u8 = b'}';
    const APPENDS: u8 = b'e';
    const GET: u8 = b'g';
    const BINGET: u8 = b'h';
    const LONG_BINGET: u8 = b'j';
    const INST: u8 = b'i';
    const LIST: u8 = b'l';
    const EMPTY_LIST: u8 = b']';
    const OBJ: u8 = b'o';
    const PUT: u8 = b'p';
    const BINPUT: u8 = b'q';
    const LONG_BINPUT: u8 = b'r';
    const SETITEM: u8 = b's';
    const TUPLE: u8 = b't';
    const SETITEMS: u8 = b'u';
    const EMPTY_TUPLE: u8 = b')';
    const PROTO: u8 = 0x80;
    const NEWOBJ: u8 = 0x81;
    const TUPLE1: u8 = 0x85;
    const TUPLE2: u8 = 0x86;
    const TUPLE3: u8 = 0x87;
    const NEWTRUE: u8 = 0x88;
    const NEWFALSE: u8 = 0x89;
    const SHORT_BINUNICODE: u8 = 0x8c;
    const BININT1: u8 = b'K';
    const BININT2: u8 = b'M';
    const BININT: u8 = b'J';
    const LONG1: u8 = 0x8a;
    const LONG4: u8 = 0x8b;
    const BINUNICODE: u8 = b'X';
    const EMPTY_SET: u8 = 0x8f;
    const FROZENSET: u8 = 0x91;
    const NEWOBJ_EX: u8 = 0x92;
    const STACK_GLOBAL: u8 = 0x93;
    const MEMOIZE: u8 = 0x94;
    const FRAME: u8 = 0x95;

    #[derive(Clone, Debug)]
    #[allow(dead_code)]
    enum Val {
        None,
        Bool(bool),
        Int(i64),
        Float(f64),
        Str(String),
        List(Vec<Val>),
        Tuple(Vec<Val>),
        Dict(Vec<(Val, Val)>),
        Global(String, String),
        Storage(String, bool),
        Tensor(TensorMetaInner),
        Opaque,
    }

    #[derive(Clone, Debug)]
    struct TensorMetaInner {
        storage_key: String,
        storage_offset: usize,
        shape: Vec<usize>,
        is_bf16: bool,
    }

    let mut stack: Vec<Val> = Vec::new();
    let mut mark_stack: Vec<usize> = Vec::new();
    let mut memo: BTreeMap<u64, Val> = BTreeMap::new();
    let mut pos = 0usize;
    let mut result: BTreeMap<String, TensorMeta> = BTreeMap::new();

    macro_rules! read_byte {
        () => {{
            let b = pkl[pos];
            pos += 1;
            b
        }};
    }
    macro_rules! read_u16 {
        () => {{
            let v = u16::from_le_bytes([pkl[pos], pkl[pos + 1]]);
            pos += 2;
            v
        }};
    }
    macro_rules! read_i32 {
        () => {{
            let v = i32::from_le_bytes(pkl[pos..pos + 4].try_into().unwrap());
            pos += 4;
            v
        }};
    }
    macro_rules! read_u32 {
        () => {{
            let v = u32::from_le_bytes(pkl[pos..pos + 4].try_into().unwrap());
            pos += 4;
            v
        }};
    }
    macro_rules! read_u64 {
        () => {{
            let v = u64::from_le_bytes(pkl[pos..pos + 8].try_into().unwrap());
            pos += 8;
            v
        }};
    }
    macro_rules! read_line {
        () => {{
            let start = pos;
            while pos < pkl.len() && pkl[pos] != b'\n' {
                pos += 1;
            }
            let s = std::str::from_utf8(&pkl[start..pos]).unwrap_or("").to_string();
            pos += 1;
            s
        }};
    }
    macro_rules! read_bytes {
        ($n:expr) => {{
            let n = $n as usize;
            let slice = &pkl[pos..pos + n];
            pos += n;
            slice
        }};
    }

    fn apply_global(func: Val, args: Val) -> Val {
        match (&func, &args) {
            (Val::Global(m, n), Val::Tuple(a)) => {
                let is_bf16 = n == "BFloat16Storage";
                if m.starts_with("torch") && (n.ends_with("Storage") || n == "storage") {
                    return Val::Storage(String::new(), is_bf16);
                }
                if (m == "torch._utils" || m == "torch") && n == "_rebuild_tensor_v2" {
                    if let (Some(Val::Storage(key, bf16)), Some(Val::Int(off)), Some(Val::Tuple(sz)), _) =
                        (a.first(), a.get(1), a.get(2), a.get(3))
                    {
                        let shape: Vec<usize> = sz
                            .iter()
                            .filter_map(|v| if let Val::Int(i) = v { Some(*i as usize) } else { None })
                            .collect();
                        return Val::Tensor(TensorMetaInner {
                            storage_key: key.clone(),
                            storage_offset: *off as usize,
                            shape,
                            is_bf16: *bf16,
                        });
                    }
                }
                if n == "_rebuild_parameter" || n == "_rebuild_parameter_with_state" {
                    if let Some(t @ Val::Tensor(_)) = a.first() {
                        return t.clone();
                    }
                }
                Val::Opaque
            }
            _ => Val::Opaque,
        }
    }

    while pos < pkl.len() {
        let op = read_byte!();
        match op {
            PROTO => {
                read_byte!();
            }
            FRAME => {
                read_u64!();
            }
            NONE => stack.push(Val::None),
            NEWTRUE => stack.push(Val::Bool(true)),
            NEWFALSE => stack.push(Val::Bool(false)),
            BININT1 => {
                let v = read_byte!() as i64;
                stack.push(Val::Int(v));
            }
            BININT2 => {
                let v = read_u16!() as i64;
                stack.push(Val::Int(v));
            }
            BININT => {
                let v = read_i32!() as i64;
                stack.push(Val::Int(v));
            }
            LONG1 => {
                let n = read_byte!() as usize;
                let bs = read_bytes!(n);
                let mut v = 0i64;
                for (i, &b) in bs.iter().enumerate() {
                    v |= (b as i64) << (8 * i);
                }
                stack.push(Val::Int(v));
            }
            LONG4 => {
                let n = read_i32!() as usize;
                let bs = read_bytes!(n);
                let mut v = 0i64;
                for (i, &b) in bs.iter().enumerate() {
                    v |= (b as i64) << (8 * i);
                }
                stack.push(Val::Int(v));
            }
            INT | LONG => {
                let s = read_line!();
                let v: i64 = s.trim_end_matches('L').parse().unwrap_or(0);
                stack.push(Val::Int(v));
            }
            FLOAT => {
                let s = read_line!();
                stack.push(Val::Float(s.parse().unwrap_or(0.0)));
            }
            BINUNICODE => {
                let n = read_u32!() as usize;
                let bs = read_bytes!(n);
                stack.push(Val::Str(String::from_utf8_lossy(bs).into()));
            }
            SHORT_BINUNICODE => {
                let n = read_byte!() as usize;
                let bs = read_bytes!(n);
                stack.push(Val::Str(String::from_utf8_lossy(bs).into()));
            }
            STRING | UNICODE => {
                let s = read_line!();
                stack.push(Val::Str(s.trim_matches('\'').to_string()));
            }
            b'T' => {
                let n = read_i32!() as usize;
                let bs = read_bytes!(n);
                stack.push(Val::Str(String::from_utf8_lossy(bs).into()));
            }
            b'U' => {
                let n = read_byte!() as usize;
                let bs = read_bytes!(n);
                stack.push(Val::Str(String::from_utf8_lossy(bs).into()));
            }
            GLOBAL => {
                let m = read_line!();
                let n = read_line!();
                stack.push(Val::Global(m, n));
            }
            STACK_GLOBAL => {
                let name = stack.pop().unwrap_or(Val::None);
                let module = stack.pop().unwrap_or(Val::None);
                if let (Val::Str(m), Val::Str(n)) = (module, name) {
                    stack.push(Val::Global(m, n));
                } else {
                    stack.push(Val::Opaque);
                }
            }
            b'P' => {
                let s = read_line!();
                let parts: Vec<&str> = s.split(',').collect();
                let key = parts.get(2).unwrap_or(&"0").to_string();
                let tp = parts.get(1).unwrap_or(&"FloatStorage").to_string();
                let is_bf16 = tp == "BFloat16Storage";
                stack.push(Val::Storage(key, is_bf16));
            }
            b'Q' => {
                let pid = stack.pop().unwrap_or(Val::None);
                let storage = match &pid {
                    Val::Tuple(parts) => {
                        let key = parts
                            .get(2)
                            .and_then(|v| if let Val::Str(s) = v { Some(s.clone()) } else { None })
                            .unwrap_or_default();
                        let is_bf16 = parts
                            .get(1)
                            .map(|v| {
                                if let Val::Global(_, n) = v {
                                    n.contains("BFloat16")
                                } else {
                                    false
                                }
                            })
                            .unwrap_or(false);
                        Val::Storage(key, is_bf16)
                    }
                    _ => Val::Opaque,
                };
                stack.push(storage);
            }
            EMPTY_TUPLE => stack.push(Val::Tuple(vec![])),
            TUPLE1 => {
                let a = stack.pop().unwrap_or(Val::None);
                stack.push(Val::Tuple(vec![a]));
            }
            TUPLE2 => {
                let b = stack.pop().unwrap_or(Val::None);
                let a = stack.pop().unwrap_or(Val::None);
                stack.push(Val::Tuple(vec![a, b]));
            }
            TUPLE3 => {
                let c = stack.pop().unwrap_or(Val::None);
                let b = stack.pop().unwrap_or(Val::None);
                let a = stack.pop().unwrap_or(Val::None);
                stack.push(Val::Tuple(vec![a, b, c]));
            }
            TUPLE => {
                let mark = mark_stack.pop().unwrap_or(0);
                let items: Vec<Val> = stack.drain(mark..).collect();
                stack.push(Val::Tuple(items));
            }
            EMPTY_LIST => stack.push(Val::List(vec![])),
            LIST => {
                let mark = mark_stack.pop().unwrap_or(0);
                let items: Vec<Val> = stack.drain(mark..).collect();
                stack.push(Val::List(items));
            }
            APPEND => {
                let v = stack.pop().unwrap_or(Val::None);
                if let Some(Val::List(ref mut l)) = stack.last_mut() {
                    l.push(v);
                }
            }
            APPENDS => {
                let mark = mark_stack.pop().unwrap_or(0);
                let items: Vec<Val> = stack.drain(mark..).collect();
                if let Some(Val::List(ref mut l)) = stack.last_mut() {
                    l.extend(items);
                }
            }
            EMPTY_DICT | EMPTY_SET => stack.push(Val::Dict(vec![])),
            DICT => {
                let mark = mark_stack.pop().unwrap_or(0);
                let items: Vec<Val> = stack.drain(mark..).collect();
                let pairs = items
                    .chunks(2)
                    .map(|c| (c[0].clone(), c.get(1).cloned().unwrap_or(Val::None)))
                    .collect();
                stack.push(Val::Dict(pairs));
            }
            SETITEM => {
                let v = stack.pop().unwrap_or(Val::None);
                let k = stack.pop().unwrap_or(Val::None);
                if let (Val::Str(name), Val::Tensor(meta)) = (&k, &v) {
                    result.insert(
                        name.clone(),
                        TensorMeta {
                            storage_key: meta.storage_key.clone(),
                            storage_offset: meta.storage_offset,
                            shape: meta.shape.clone(),
                            is_bf16: meta.is_bf16,
                        },
                    );
                }
                if let Some(Val::Dict(ref mut d)) = stack.last_mut() {
                    d.push((k, v));
                }
            }
            SETITEMS => {
                let mark = mark_stack.pop().unwrap_or(0);
                let items: Vec<Val> = stack.drain(mark..).collect();
                for chunk in items.chunks(2) {
                    let k = chunk[0].clone();
                    let v = chunk.get(1).cloned().unwrap_or(Val::None);
                    if let (Val::Str(name), Val::Tensor(meta)) = (&k, &v) {
                        result.insert(
                            name.clone(),
                            TensorMeta {
                                storage_key: meta.storage_key.clone(),
                                storage_offset: meta.storage_offset,
                                shape: meta.shape.clone(),
                                is_bf16: meta.is_bf16,
                            },
                        );
                    }
                    if let Some(Val::Dict(ref mut d)) = stack.last_mut() {
                        d.push((k, v));
                    }
                }
            }
            REDUCE => {
                let args = stack.pop().unwrap_or(Val::None);
                let func = stack.pop().unwrap_or(Val::None);
                stack.push(apply_global(func, args));
            }
            NEWOBJ | NEWOBJ_EX => {
                let args = stack.pop().unwrap_or(Val::None);
                let cls = stack.pop().unwrap_or(Val::None);
                stack.push(apply_global(cls, args));
            }
            BUILD => {
                let _state = stack.pop();
            }
            INST | OBJ => {
                let mark = mark_stack.pop().unwrap_or(0);
                let _items: Vec<Val> = stack.drain(mark..).collect();
                stack.push(Val::Opaque);
            }
            MEMOIZE => {
                let key = memo.len() as u64;
                if let Some(v) = stack.last() {
                    memo.insert(key, v.clone());
                }
            }
            PUT => {
                let _k = read_line!();
            }
            BINPUT => {
                let k = read_byte!() as u64;
                if let Some(v) = stack.last() {
                    memo.insert(k, v.clone());
                }
            }
            LONG_BINPUT => {
                let k = read_u32!() as u64;
                if let Some(v) = stack.last() {
                    memo.insert(k, v.clone());
                }
            }
            GET => {
                let k: u64 = read_line!().parse().unwrap_or(0);
                stack.push(memo.get(&k).cloned().unwrap_or(Val::None));
            }
            BINGET => {
                let k = read_byte!() as u64;
                stack.push(memo.get(&k).cloned().unwrap_or(Val::None));
            }
            LONG_BINGET => {
                let k = read_u32!() as u64;
                stack.push(memo.get(&k).cloned().unwrap_or(Val::None));
            }
            MARK => mark_stack.push(stack.len()),
            POP => {
                stack.pop();
            }
            POP_MARK => {
                let mark = mark_stack.pop().unwrap_or(0);
                stack.truncate(mark);
            }
            DUP => {
                if let Some(v) = stack.last() {
                    stack.push(v.clone());
                }
            }
            STOP => break,
            FROZENSET => stack.push(Val::Dict(vec![])),
            _ => {}
        }
    }

    fn scan_val(val: &Val, out: &mut BTreeMap<String, TensorMeta>) {
        match val {
            Val::Dict(pairs) => {
                for (k, v) in pairs {
                    if let (Val::Str(name), Val::Tensor(meta)) = (k, v) {
                        out.entry(name.clone()).or_insert_with(|| TensorMeta {
                            storage_key: meta.storage_key.clone(),
                            storage_offset: meta.storage_offset,
                            shape: meta.shape.clone(),
                            is_bf16: meta.is_bf16,
                        });
                    }
                    scan_val(v, out);
                }
            }
            Val::List(items) | Val::Tuple(items) => {
                for item in items {
                    scan_val(item, out);
                }
            }
            _ => {}
        }
    }
    for v in &stack {
        scan_val(v, &mut result);
    }

    Ok(result)
}
