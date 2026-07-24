// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Auto-download Hugging Face weights for `rlx-tts-bench` engines on first use.
//!
//! Prefers single-file RLX packs when the Hub ships them:
//! [`.rlxp`](https://github.com/MIT-RLX/rlx/blob/main/docs/rlxp.md) (sidecars) →
//! [`.rlx`](https://github.com/MIT-RLX/rlx/blob/main/docs/rlx-bake.md) (bake) →
//! `.gguf` → loose directory snapshot. Materializes under `~/.skill/models/<engine>/`
//! and sets the `RLX_*_DIR` / bundle env vars bench adapters resolve via weight hints.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use hf_hub::api::sync::ApiBuilder;

use crate::skill_dir;

/// Ensure weights for a bench engine id are on disk and env vars point at them.
///
/// `on_progress(label, downloaded, total)` is invoked around Hub fetches
/// (total may be 0 when unknown). No-op for engines handled elsewhere
/// (qwen3 / orpheus / kyutai / kitten) or engines with no known Hub source.
pub fn ensure_bench_engine_weights(engine_id: &str) -> Result<()> {
    ensure_bench_engine_weights_with_progress(engine_id, |_, _, _| {})
}

/// Like [`ensure_bench_engine_weights`] with a progress callback.
pub fn ensure_bench_engine_weights_with_progress(
    engine_id: &str,
    mut on_progress: impl FnMut(&str, u64, u64),
) -> Result<()> {
    on_progress(&format!("Preparing {engine_id} weights…"), 0, 0);
    let result = ensure_bench_engine_weights_inner(engine_id);
    if result.is_ok() {
        on_progress(&format!("{engine_id} ready"), 1, 1);
    }
    result
}

fn ensure_bench_engine_weights_inner(engine_id: &str) -> Result<()> {
    match engine_id {
        "rlx-tts" => ensure_packed(
            "eugenehp/rlx-tts",
            &["rlx-tts.rlxp", "rlx-tts.rlx", "rlx-tts.gguf"],
            "models/rlx-tts",
            "RLX_TTS_BUNDLE",
        ),
        "moss-nano" => ensure_packed(
            "eugenehp/moss-nano",
            &["moss-nano.rlxp", "moss-nano.rlx", "moss-nano.gguf"],
            "models/moss-nano",
            "RLX_MOSS_NANO_DIR",
        ),
        "soprano" => ensure_packed(
            "eugenehp/soprano",
            &["soprano.rlxp", "soprano.rlx", "soprano.gguf"],
            "models/soprano",
            "RLX_SOPRANO_DIR",
        ),
        "f5tts" => {
            ensure_repo_files(
                "huggingfacess/F5-TTS-ONNX",
                "models/f5tts",
                "RLX_F5TTS_DIR",
                &[
                    "F5_Preprocess.onnx",
                    "F5_Transformer.onnx",
                    "F5_Decode.onnx",
                    "vocab.txt",
                ],
                &["F5_Transformer.onnx", "vocab.txt"],
            )?;
            // vocab.txt may live only on the SWivid training repo.
            let dest = skill_dir().join("models/f5tts");
            if !dest.join("vocab.txt").is_file() {
                let _ = materialize_files(
                    "SWivid/F5-TTS",
                    &["F5TTS_v1_Base/vocab.txt"],
                    &dest,
                    "models/f5tts/hf-cache",
                    Some(&[("F5TTS_v1_Base/vocab.txt", "vocab.txt")]),
                );
            }
            Ok(())
        }
        "chatterbox" => ensure_repo_snapshot(
            "synath/chatterbox-ONNX",
            "models/chatterbox",
            "RLX_CHATTERBOX_DIR",
            &["weights.safetensors", "manifest.json", "native/t3_lm.safetensors"],
        ),
        "luxtts" => ensure_repo_snapshot(
            "YatharthS/LuxTTS",
            "models/luxtts",
            "RLX_LUXTTS_DIR",
            &["encoder_body.onnx", "fm_decoder.onnx", "tokens.txt"],
        ),
        "supertonic" => {
            let dest = skill_dir().join("models/supertonic");
            if ready(&dest, &["onnx/tts.json", "onnx/vocoder.onnx"]) {
                return set_env("RLX_SUPERTONIC_DIR", &dest);
            }
            materialize_files(
                "Supertone/supertonic-3",
                &[
                    "config.json",
                    "onnx/duration_predictor.onnx",
                    "onnx/text_encoder.onnx",
                    "onnx/vector_estimator.onnx",
                    "onnx/vocoder.onnx",
                    "onnx/tts.json",
                    "onnx/unicode_indexer.json",
                    "voice_styles/F1.json",
                    "voice_styles/F2.json",
                    "voice_styles/M1.json",
                    "voice_styles/M2.json",
                ],
                &dest,
                "models/supertonic/hf-cache",
                None,
            )?;
            set_env("RLX_SUPERTONIC_DIR", &dest)
        }
        "sesame" => {
            ensure_repo_snapshot(
                "unsloth/csm-1b",
                "models/sesame",
                "RLX_SESAME_DIR",
                &["config.json", "model.safetensors"],
            )?;
            ensure_mimi()
        }
        "zonos" => {
            ensure_repo_snapshot(
                "Zyphra/Zonos-v0.1-transformer",
                "models/zonos",
                "RLX_ZONOS_DIR",
                &["config.json"],
            )?;
            // Zonos uses the Parler 44.1 kHz DAC layout.
            ensure_repo_snapshot(
                "parler-tts/dac_44khz",
                "models/parler-dac",
                "RLX_DAC_DIR",
                &["config.json", "model.safetensors"],
            )
        }
        "gepard" => ensure_repo_snapshot(
            "nineninesix/gepard-1.0",
            "models/gepard",
            "RLX_GEPARD_DIR",
            &["model.safetensors", "nano_dec_1.89kbps.safetensors"],
        ),
        "metavoice" => ensure_repo_snapshot(
            "metavoiceio/metavoice-1B-v0.1",
            "models/metavoice",
            "RLX_METAVOICE_DIR",
            &[],
        ),
        "pocket-tts" => {
            let dest = skill_dir().join("models/pocket-tts");
            if ready(&dest, &["tokenizer.model", "tts_b6369a24.safetensors"]) {
                return set_env("RLX_POCKET_TTS_DIR", &dest);
            }
            materialize_files(
                "Verylicious/pocket-tts-ungated",
                &[
                    "tts_b6369a24.safetensors",
                    "tokenizer.model",
                    "embeddings/alba.safetensors",
                ],
                &dest,
                "models/pocket-tts/hf-cache",
                None,
            )?;
            set_env("RLX_POCKET_TTS_DIR", &dest)
        }
        "voxtral-tts" | "voxtral" => {
            let dest = skill_dir().join("models/voxtral-tts");
            if ready(&dest, &["consolidated.safetensors", "tekken.json"]) {
                return set_env("RLX_VOXTRAL_TTS_DIR", &dest);
            }
            let mut files: Vec<&str> = vec!["params.json", "tekken.json", "consolidated.safetensors"];
            // Preset voice embeddings (best-effort; missing voices are non-fatal).
            let voice_names = ["neutral_female", "neutral_male", "cheerful_female", "cheerful_male"];
            let voice_rels: Vec<String> = voice_names.iter().map(|v| format!("voice_embedding/{v}.pt")).collect();
            for r in &voice_rels {
                files.push(r.as_str());
            }
            let _ = materialize_files(
                "mistralai/Voxtral-4B-TTS-2603",
                &files,
                &dest,
                "models/voxtral-tts/hf-cache",
                None,
            );
            if !ready(&dest, &["consolidated.safetensors", "tekken.json"]) {
                anyhow::bail!(
                    "voxtral-tts weights incomplete under {} (need consolidated.safetensors + tekken.json)",
                    dest.display()
                );
            }
            set_env("RLX_VOXTRAL_TTS_DIR", &dest)
        }
        "piper" => {
            // One MIT English voice; native path still needs `rlx-split/` beside the ONNX.
            let dest = skill_dir().join("models/piper");
            let onnx = "en/en_US/en_US-lessac-medium/en_US-lessac-medium.onnx";
            let json = "en/en_US/en_US-lessac-medium/en_US-lessac-medium.onnx.json";
            if dest.join("en_US-lessac-medium.onnx").is_file() || dest.join(onnx).is_file() || ready_any_onnx(&dest) {
                return set_env("RLX_PIPER_DIR", &dest);
            }
            materialize_files(
                "rhasspy/piper-voices",
                &[onnx, json],
                &dest,
                "models/piper/hf-cache",
                Some(&[
                    (onnx, "en_US-lessac-medium.onnx"),
                    (json, "en_US-lessac-medium.onnx.json"),
                ]),
            )?;
            set_env("RLX_PIPER_DIR", &dest)
        }
        "styletts2" | "kokoro" => {
            // Kokoro ONNX hub bundle; native StyleTTS2 still needs `onnx/rlx-split/`.
            let dest = skill_dir().join("models/styletts2");
            if ready(&dest, &["tokenizer.json"]) || dest.join("voices").is_dir() {
                let _ = set_env("RLX_STYLETTS2_DIR", &dest);
                let _ = set_env("RLX_KOKORO_DIR", &dest);
                return Ok(());
            }
            ensure_repo_snapshot(
                "onnx-community/Kokoro-82M-v1.0-ONNX",
                "models/styletts2",
                "RLX_STYLETTS2_DIR",
                &["tokenizer.json"],
            )?;
            set_env("RLX_KOKORO_DIR", &skill_dir().join("models/styletts2"))
        }
        "parlertts" => {
            ensure_repo_snapshot(
                "parler-tts/parler-tts-mini-v1",
                "models/parlertts",
                "RLX_PARLERTTS_DIR",
                &["tokenizer.json"],
            )?;
            ensure_repo_snapshot(
                "parler-tts/dac_44khz",
                "models/parler-dac",
                "RLX_PARLER_DAC_DIR",
                &["config.json", "model.safetensors"],
            )
        }
        "miotts" => {
            ensure_repo_snapshot(
                "Aratako/MioTTS-0.6B",
                "models/miotts",
                "RLX_MIOTTS_DIR",
                &["model.safetensors"],
            )?;
            ensure_repo_snapshot(
                "Aratako/MioCodec-25Hz-24kHz",
                "models/miocodec",
                "RLX_MIOCODEC_DIR",
                &[],
            )
        }
        "miratts" => ensure_repo_snapshot(
            "YatharthS/MiraTTS",
            "models/miratts",
            "RLX_MIRATTS_DIR",
            &["tokenizer.json"],
        ),
        // melotts / inflect / tiny-tts: local exported bundles only.
        _ => Ok(()),
    }
}

fn ensure_mimi() -> Result<()> {
    let dest = skill_dir().join("models/mimi");
    if ready(&dest, &["config.json", "model.safetensors"]) {
        return set_env("RLX_MIMI_DIR", &dest);
    }
    materialize_files(
        "kyutai/mimi",
        &["config.json", "model.safetensors", "preprocessor_config.json"],
        &dest,
        "models/mimi/hf-cache",
        None,
    )?;
    set_env("RLX_MIMI_DIR", &dest)
}

fn ensure_packed(repo: &str, filenames: &[&str], models_subdir: &str, env_key: &str) -> Result<()> {
    let dest_dir = skill_dir().join(models_subdir);
    // Already have any preferred pack on disk?
    for name in filenames {
        if dest_dir.join(name).is_file() {
            return set_env(env_key, &dest_dir);
        }
    }
    if let Ok(existing) = std::env::var(env_key) {
        let p = PathBuf::from(existing.trim());
        if filenames.iter().any(|n| p.join(n).is_file())
            || (p.is_file() && filenames.iter().any(|n| p.file_name().is_some_and(|f| f == *n)))
        {
            return Ok(());
        }
        // Loose dir with markers still OK (loader resolves .rlxp/.gguf inside).
        if p.is_dir() {
            return Ok(());
        }
    }
    std::fs::create_dir_all(&dest_dir).with_context(|| format!("create {}", dest_dir.display()))?;
    let cache = dest_dir.join("hf-cache");
    let mut last_err: Option<anyhow::Error> = None;
    for name in filenames {
        match fetch_hf_file(repo, name, &cache) {
            Ok(src) => {
                let dest = dest_dir.join(name);
                if src != dest {
                    std::fs::copy(&src, &dest)
                        .with_context(|| format!("copy {} → {}", src.display(), dest.display()))?;
                }
                tts_log!("tts", "downloaded {repo}/{name} → {}", dest.display());
                return set_env(env_key, &dest_dir);
            }
            Err(e) => {
                tts_log!("tts", "pack miss {repo}/{name}: {e:#}");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no pack files {:?} in {repo}", filenames)))
}

fn ensure_repo_snapshot(repo: &str, models_subdir: &str, env_key: &str, markers: &[&str]) -> Result<()> {
    if let Ok(existing) = std::env::var(env_key) {
        let p = PathBuf::from(existing.trim());
        if ready(&p, markers) {
            return Ok(());
        }
    }
    let flat = skill_dir().join(models_subdir);
    if ready(&flat, markers) {
        return set_env(env_key, &flat);
    }
    let snap = download_repo_snapshot(repo, &format!("{models_subdir}/hf-cache"))?;
    if !markers.is_empty() && !ready(&snap, markers) {
        // Still point at the snapshot — some markers are optional (e.g. gepard nano_dec).
        tts_log!(
            "tts",
            "warning: {repo} snapshot at {} missing markers {markers:?}",
            snap.display()
        );
    }
    set_env(env_key, &snap)
}

fn ensure_repo_files(
    repo: &str,
    models_subdir: &str,
    env_key: &str,
    files: &[&str],
    ready_markers: &[&str],
) -> Result<()> {
    let dest = skill_dir().join(models_subdir);
    if ready(&dest, ready_markers) {
        return set_env(env_key, &dest);
    }
    materialize_files(repo, files, &dest, &format!("{models_subdir}/hf-cache"), None)?;
    set_env(env_key, &dest)
}

fn download_repo_snapshot(repo_id: &str, cache_subdir: &str) -> Result<PathBuf> {
    let cache = skill_dir().join(cache_subdir);
    std::fs::create_dir_all(&cache).ok();
    let api = ApiBuilder::new()
        .with_cache_dir(cache)
        .build()
        .context("init hf-hub api")?;
    let repo = api.model(repo_id.to_string());
    let info = repo.info().with_context(|| format!("hf info for {repo_id}"))?;
    let mut snap: Option<PathBuf> = None;
    for sib in &info.siblings {
        let path = repo
            .get(&sib.rfilename)
            .with_context(|| format!("download {repo_id}/{}", sib.rfilename))?;
        if snap.is_none() {
            // Walk up until we leave nested paths — prefer root of snapshot.
            snap = Some(snapshot_root(&path, &sib.rfilename));
        }
    }
    snap.ok_or_else(|| anyhow::anyhow!("no files in {repo_id}"))
}

fn snapshot_root(file_path: &Path, rfilename: &str) -> PathBuf {
    let mut p = file_path.to_path_buf();
    let depth = Path::new(rfilename).components().count().saturating_sub(1);
    for _ in 0..depth {
        if let Some(parent) = p.parent() {
            p = parent.to_path_buf();
        }
    }
    p.parent().unwrap_or(&p).to_path_buf()
}

fn materialize_files(
    repo_id: &str,
    files: &[&str],
    dest_dir: &Path,
    cache_subdir: &str,
    rename: Option<&[(&str, &str)]>,
) -> Result<()> {
    std::fs::create_dir_all(dest_dir).with_context(|| format!("create {}", dest_dir.display()))?;
    let cache = skill_dir().join(cache_subdir);
    std::fs::create_dir_all(&cache).ok();
    let api = ApiBuilder::new()
        .with_cache_dir(cache)
        .build()
        .context("init hf-hub api")?;
    let repo = api.model(repo_id.to_string());
    let mut any = false;
    for f in files {
        let out_rel = rename
            .and_then(|pairs| pairs.iter().find(|(src, _)| src == f).map(|(_, d)| *d))
            .unwrap_or(f);
        let out = dest_dir.join(out_rel);
        if out.is_file() {
            any = true;
            continue;
        }
        match repo.get(f) {
            Ok(src) => {
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::copy(&src, &out).with_context(|| format!("copy {repo_id}/{f} → {}", out.display()))?;
                any = true;
            }
            Err(e) => {
                tts_log!("tts", "skip {repo_id}/{f}: {e:#}");
            }
        }
    }
    if !any {
        anyhow::bail!("could not download any files from {repo_id}");
    }
    Ok(())
}

fn fetch_hf_file(repo_id: &str, filename: &str, cache_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(cache_dir).ok();
    let api = ApiBuilder::new()
        .with_cache_dir(cache_dir.to_path_buf())
        .build()
        .context("init hf-hub api")?;
    api.model(repo_id.to_string())
        .get(filename)
        .with_context(|| format!("download {repo_id}/{filename}"))
}

fn ready(dir: &Path, markers: &[&str]) -> bool {
    if !dir.is_dir() && !dir.is_file() {
        return false;
    }
    if markers.is_empty() {
        return dir.is_dir();
    }
    markers.iter().any(|m| dir.join(m).is_file())
}

fn ready_any_onnx(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.filter_map(|e| e.ok())
        .any(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("onnx")))
}

fn set_env(key: &str, path: &Path) -> Result<()> {
    // SAFETY: TTS worker is single-threaded during engine init.
    unsafe { std::env::set_var(key, path.as_os_str()) };
    Ok(())
}
