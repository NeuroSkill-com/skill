// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! TTS engine catalog for the UI / daemon.
//!
//! Single source of metadata (`ENGINE_META`) plus ids from
//! [`crate::engines::list_engine_ids`]. UI fetches via `/v1/tts/engines`.

use serde::Serialize;

/// One selectable TTS backend.
#[derive(Debug, Clone, Serialize)]
pub struct TtsEngineInfo {
    pub id: String,
    pub label: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub default_voice: String,
    pub voices: Vec<String>,
    pub experimental: bool,
    pub downloadable: bool,
    /// Needs a one-time local bundle export (no Hub pack) — greys out until present.
    pub needs_bundle: bool,
    /// False when this build cannot run the engine (e.g. Windows / feature off).
    pub available: bool,
}

#[derive(Clone, Copy)]
struct Meta {
    label: &'static str,
    experimental: bool,
    downloadable: bool,
    needs_bundle: bool,
    models: &'static [&'static str],
    default_model: &'static str,
    default_voice: &'static str,
}

fn meta_for(id: &str) -> Meta {
    match id {
        "kitten" => Meta {
            label: "KittenTTS",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "neutts" => Meta {
            label: "NeuTTS",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "rlx-tts" => Meta {
            label: "RLX-TTS",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "qwen3-tts" => Meta {
            label: "Qwen3-TTS",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &["Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice"],
            default_model: "Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice",
            default_voice: "vivian",
        },
        "orpheus" => Meta {
            label: "Orpheus",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "tara",
        },
        "kyutai-tts" | "kyutai" => Meta {
            label: "Kyutai-TTS",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "inflect-nano" => Meta {
            label: "Inflect-Nano",
            experimental: true,
            downloadable: false,
            needs_bundle: true,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "styletts2" => Meta {
            label: "StyleTTS2",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "piper" => Meta {
            label: "Piper",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "chatterbox" => Meta {
            label: "Chatterbox",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "f5tts" => Meta {
            label: "F5-TTS",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "luxtts" => Meta {
            label: "LuxTTS",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "moss-nano" => Meta {
            label: "MOSS-Nano",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "soprano" => Meta {
            label: "Soprano",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "supertonic" => Meta {
            label: "Supertonic",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "sesame" => Meta {
            label: "Sesame",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "zonos" => Meta {
            label: "Zonos",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "gepard" => Meta {
            label: "Gepard",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "metavoice" => Meta {
            label: "MetaVoice",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "pocket-tts" => Meta {
            label: "Pocket-TTS",
            experimental: false,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "parlertts" => Meta {
            label: "Parler-TTS",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "miotts" => Meta {
            label: "MioTTS",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "miratts" => Meta {
            label: "MiraTTS",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "melotts" => Meta {
            label: "MeloTTS",
            experimental: true,
            downloadable: false,
            needs_bundle: true,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        "voxtral-tts" | "voxtral" => Meta {
            label: "Voxtral-TTS",
            experimental: true,
            downloadable: true,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
        _other => Meta {
            label: "Experimental",
            experimental: true,
            downloadable: false,
            needs_bundle: false,
            models: &[],
            default_model: "",
            default_voice: "",
        },
    }
}

/// Normalize legacy / alias engine ids to the canonical catalog id.
pub fn normalize_engine_id(engine: &str) -> String {
    match engine.trim().to_ascii_lowercase().as_str() {
        "qwen3_tts" => "qwen3-tts".into(),
        "kyutai" => "kyutai-tts".into(),
        "inflect_nano" => "inflect-nano".into(),
        "moss_nano" => "moss-nano".into(),
        "f5_tts" | "f5-tts" => "f5tts".into(),
        "voxtral" => "voxtral-tts".into(),
        "pocket" => "pocket-tts".into(),
        "parler" => "parlertts".into(),
        "styletts" | "kokoro" => "styletts2".into(),
        "kittentts" => "kitten".into(),
        other => other.to_string(),
    }
}

/// Whether `engine` is a known (or aliasable) catalog id in this build.
pub fn is_known_engine(engine: &str) -> bool {
    let id = normalize_engine_id(engine);
    list_engines().iter().any(|e| e.id == id)
}

fn engine_available(id: &str) -> bool {
    // Pluggable RLX engines need `tts-engines`; kitten/neutts have their own cfgs.
    match id {
        "kitten" => cfg!(tts_kitten_active),
        "neutts" => cfg!(feature = "tts-neutts"),
        _ => cfg!(tts_engines_active),
    }
}

fn to_info(id: &str) -> TtsEngineInfo {
    let m = meta_for(id);
    let voices = {
        #[cfg(tts_engines_active)]
        {
            crate::engines::voices_for(id)
        }
        #[cfg(not(tts_engines_active))]
        {
            Vec::new()
        }
    };
    TtsEngineInfo {
        id: id.to_string(),
        label: m.label.to_string(),
        models: m.models.iter().map(|s| (*s).to_string()).collect(),
        default_model: m.default_model.to_string(),
        default_voice: m.default_voice.to_string(),
        voices,
        experimental: m.experimental,
        downloadable: m.downloadable,
        needs_bundle: m.needs_bundle,
        available: engine_available(id),
    }
}

/// Full TTS catalog available in this build.
pub fn list_engines() -> Vec<TtsEngineInfo> {
    #[cfg(tts_engines_active)]
    {
        crate::engines::list_engine_ids().into_iter().map(to_info).collect()
    }
    #[cfg(not(tts_engines_active))]
    {
        ["kitten", "neutts"].into_iter().map(to_info).collect()
    }
}

/// Whether any TTS engine can run in this binary.
pub fn voice_tts_available() -> bool {
    list_engines().iter().any(|e| e.available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_aliases() {
        assert_eq!(normalize_engine_id("kyutai"), "kyutai-tts");
        assert_eq!(normalize_engine_id("kittentts"), "kitten");
        assert_eq!(normalize_engine_id("f5-tts"), "f5tts");
        assert_eq!(normalize_engine_id("voxtral"), "voxtral-tts");
    }

    #[test]
    fn catalog_has_core_engines() {
        let ids: Vec<_> = list_engines().into_iter().map(|e| e.id).collect();
        assert!(ids.contains(&"kitten".to_string()) || !cfg!(tts_kitten_active));
        for e in list_engines() {
            assert!(!e.id.is_empty());
            assert!(!e.label.is_empty());
        }
    }

    /// Printed for `scripts/check-tts-catalog-parity.sh` when features are on.
    #[test]
    fn catalog_parity_ids() {
        let ids = list_engines().into_iter().map(|e| e.id).collect::<Vec<_>>().join(",");
        println!("PARITY_IDS {ids}");
    }
}
