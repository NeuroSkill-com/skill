// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-asr` — voice input for the LLM chat: streaming **VAD** (`rlx-vad`,
//! Silero) gates the microphone and **ASR** (`rlx-whisper`) transcribes each
//! utterance.
//!
//! The engine is a process-wide singleton driven by free functions
//! ([`start`] / [`stop`] / [`set_ptt`] / [`status`]) — the same global-handle
//! shape as `skill-tts`. The daemon owns it; the UI is a thin client that calls
//! the REST routes and listens for [`AsrEvent`]s on the websocket.
//!
//! Two orthogonal mode axes (picked per session in the chat window, with a
//! default in settings):
//!
//! * **trigger** — [`TriggerMode::Continuous`] (hands-free, VAD-gated) or
//!   [`TriggerMode::PushToTalk`] (record while a key is held).
//! * **routing** — [`RoutingMode::VoiceLoop`] (auto-send the transcript to the
//!   LLM and speak the reply) or [`RoutingMode::TranscribeOnly`] (just emit the
//!   transcript). Routing is interpreted by the daemon; the engine itself only
//!   produces transcripts.
//!
//! When built without the `asr` feature (or on Windows) the public API is a set
//! of no-ops that report "unavailable" so callers compile unchanged.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
// `AtomicU64` is only used by the `asr_active`-gated session-generation counter.
#[cfg(asr_active)]
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

#[cfg(asr_active)]
mod engine;

// Pure segmentation state machine — always compiled so it unit-tests without the
// `asr` feature (no cpal / rlx dependency). Its only non-test consumer is the
// feature-gated `engine`, so allow dead code when the engine is compiled out.
#[cfg_attr(not(asr_active), allow(dead_code))]
mod segmenter;

// ─── Mode types ─────────────────────────────────────────────────────────────

/// How the microphone is gated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    /// Always listening; Silero VAD segments utterances automatically.
    #[default]
    Continuous,
    /// Record only while the caller holds the talk key (see [`set_ptt`]).
    PushToTalk,
}

/// What happens with a finalized transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoutingMode {
    /// Auto-send to the LLM chat and speak the reply (full voice loop).
    #[default]
    VoiceLoop,
    /// Only emit the transcript; the UI decides what to do with it.
    TranscribeOnly,
}

fn default_language() -> String {
    "en".to_string()
}

fn default_engine() -> String {
    "whisper".to_string()
}

fn default_model() -> String {
    skill_constants::WHISPER_ASR_HF_REPO.to_string()
}

/// A fully-resolved voice-mode selection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AsrMode {
    #[serde(default)]
    pub trigger: TriggerMode,
    #[serde(default)]
    pub routing: RoutingMode,
    #[serde(default = "default_language")]
    pub language: String,
    /// ASR engine id (`"whisper"` today; more backends land over time).
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Model id for the engine — typically a HuggingFace repo
    /// (e.g. `openai/whisper-small`).
    #[serde(default = "default_model")]
    pub model: String,
}

impl Default for AsrMode {
    fn default() -> Self {
        Self {
            trigger: TriggerMode::default(),
            routing: RoutingMode::default(),
            language: default_language(),
            engine: default_engine(),
            model: default_model(),
        }
    }
}

// ─── Events ─────────────────────────────────────────────────────────────────

/// Lifecycle + transcript events emitted by the engine. Serialized with a
/// `kind` tag and broadcast verbatim to websocket clients by the daemon.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AsrEvent {
    /// Model is downloading / initializing.
    Loading,
    /// Microphone is open; the engine is listening.
    Listening,
    /// VAD detected the onset of speech (or push-to-talk pressed).
    SpeechStart,
    /// VAD detected the end of speech (or push-to-talk released).
    SpeechEnd,
    /// A finalized utterance transcript.
    Transcript { text: String, is_final: bool },
    /// A recoverable or fatal error (the engine may still be running).
    Error { message: String },
    /// The engine has fully stopped.
    Stopped,
}

/// Callback invoked for every [`AsrEvent`]. Runs on the engine thread, so keep
/// it cheap (the daemon's sink just broadcasts and, for the voice loop, spawns
/// async work onto the tokio runtime).
pub type EventSink = Arc<dyn Fn(AsrEvent) + Send + Sync + 'static>;

/// Snapshot of the engine for the `/v1/asr/status` route.
#[derive(Clone, Debug, Serialize)]
pub struct AsrStatus {
    pub running: bool,
    pub available: bool,
    pub trigger: Option<TriggerMode>,
    pub routing: Option<RoutingMode>,
    pub language: Option<String>,
    pub ptt_active: bool,
}

// ─── SKILL_DIR ──────────────────────────────────────────────────────────────

static SKILL_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Initialise `SKILL_DIR` and pre-create the model cache directory.
pub fn init_asr_dirs(dir: &Path) {
    let _ = SKILL_DIR.set(dir.to_path_buf());
    let _ = std::fs::create_dir_all(skill_dir().join("models/whisper/hf-cache"));
}

/// Resolve the skill directory (falls back to a platform default).
pub fn skill_dir() -> PathBuf {
    SKILL_DIR.get().cloned().unwrap_or_else(|| {
        dirs::data_local_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
            .join("NeuroSkill")
    })
}

/// Whether this build ships the real engine (feature `asr`, non-Windows).
pub const fn is_available() -> bool {
    cfg!(asr_active)
}

// ─── Barge-in / half-duplex gate ─────────────────────────────────────────────

fn speaking_flag() -> &'static AtomicBool {
    static SPEAKING: AtomicBool = AtomicBool::new(false);
    &SPEAKING
}

/// Mark that the assistant is currently speaking (TTS playback). While set, the
/// engine discards captured audio and resets its VAD state on release, so the
/// microphone doesn't pick up — and transcribe — the assistant's own voice
/// (half-duplex barge-in guard). The daemon's voice loop brackets TTS with this.
pub fn set_speaking(on: bool) {
    speaking_flag().store(on, Ordering::Relaxed);
}

/// Whether the assistant is currently speaking (capture is gated).
pub fn is_speaking() -> bool {
    speaking_flag().load(Ordering::Relaxed)
}

// ─── Offline transcription (no microphone) ──────────────────────────────────

/// Transcribe a 16 kHz mono PCM buffer through the same Silero-VAD segmentation +
/// Whisper path as the live engine. One transcript per detected utterance.
#[cfg(asr_active)]
pub use engine::{transcribe_pcm_16k, transcribe_wav};

/// Unavailable without the `asr` feature.
#[cfg(not(asr_active))]
pub fn transcribe_pcm_16k(_pcm16k: &[f32], _language: &str) -> anyhow::Result<Vec<String>> {
    anyhow::bail!("ASR engine not available in this build")
}

/// Unavailable without the `asr` feature.
#[cfg(not(asr_active))]
pub fn transcribe_wav(_path: &std::path::Path, _language: &str) -> anyhow::Result<Vec<String>> {
    anyhow::bail!("ASR engine not available in this build")
}

// ─── Global control handle ──────────────────────────────────────────────────

struct Control {
    #[allow(dead_code)]
    stop_tx: std::sync::mpsc::Sender<()>,
    ptt: Arc<AtomicBool>,
    mode: AsrMode,
    #[allow(dead_code)]
    gen: u64,
}

fn control() -> &'static Mutex<Option<Control>> {
    static CONTROL: OnceLock<Mutex<Option<Control>>> = OnceLock::new();
    CONTROL.get_or_init(|| Mutex::new(None))
}

/// Monotonic session id so a thread that exits only clears the handle if it
/// still owns the current session (a newer `start` may have replaced it).
#[cfg(asr_active)]
fn next_generation() -> u64 {
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

/// Called by the engine thread when it exits: clear the handle so
/// [`is_running`]/[`status`] report `false` after an error or natural stop —
/// but only if this thread still owns the active session.
#[cfg(asr_active)]
pub(crate) fn mark_stopped(gen: u64) {
    set_speaking(false);
    if let Ok(mut g) = control().lock() {
        if g.as_ref().is_some_and(|c| c.gen == gen) {
            *g = None;
        }
    }
}

/// Is a voice session currently active?
pub fn is_running() -> bool {
    control().lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Set the push-to-talk flag (no-op outside [`TriggerMode::PushToTalk`]).
pub fn set_ptt(active: bool) {
    if let Ok(g) = control().lock() {
        if let Some(c) = g.as_ref() {
            c.ptt.store(active, Ordering::Relaxed);
        }
    }
}

/// Current engine status.
pub fn status() -> AsrStatus {
    let guard = control().lock().ok();
    match guard.as_ref().and_then(|g| g.as_ref()) {
        Some(c) => AsrStatus {
            running: true,
            available: is_available(),
            trigger: Some(c.mode.trigger),
            routing: Some(c.mode.routing),
            language: Some(c.mode.language.clone()),
            ptt_active: c.ptt.load(Ordering::Relaxed),
        },
        None => AsrStatus {
            running: false,
            available: is_available(),
            trigger: None,
            routing: None,
            language: None,
            ptt_active: false,
        },
    }
}

/// Stop the active voice session (idempotent).
pub fn stop() {
    set_speaking(false);
    if let Ok(mut g) = control().lock() {
        if let Some(c) = g.take() {
            // Best-effort: the engine thread exits on the next loop tick and
            // emits `Stopped` itself.
            let _ = c.stop_tx.send(());
        }
    }
}

/// Start a voice session with the given mode. Restarts any session already
/// running. Readiness and errors arrive asynchronously via `on_event`.
#[cfg(asr_active)]
pub fn start(mode: AsrMode, on_event: EventSink) -> anyhow::Result<()> {
    stop();
    set_speaking(false);
    let ptt = Arc::new(AtomicBool::new(false));
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let gen = next_generation();
    engine::spawn(mode.clone(), on_event, ptt.clone(), stop_rx, gen)?;
    if let Ok(mut g) = control().lock() {
        *g = Some(Control {
            stop_tx,
            ptt,
            mode,
            gen,
        });
    }
    Ok(())
}

/// No-op fallback when the engine isn't compiled in.
#[cfg(not(asr_active))]
pub fn start(_mode: AsrMode, on_event: EventSink) -> anyhow::Result<()> {
    on_event(AsrEvent::Error {
        message: "ASR engine not available in this build".into(),
    });
    anyhow::bail!("ASR engine not available in this build")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_mode_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(TriggerMode::Continuous).unwrap(),
            serde_json::json!("continuous")
        );
        assert_eq!(
            serde_json::to_value(TriggerMode::PushToTalk).unwrap(),
            serde_json::json!("push_to_talk")
        );
    }

    #[test]
    fn routing_mode_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(RoutingMode::VoiceLoop).unwrap(),
            serde_json::json!("voice_loop")
        );
        assert_eq!(
            serde_json::to_value(RoutingMode::TranscribeOnly).unwrap(),
            serde_json::json!("transcribe_only")
        );
    }

    #[test]
    fn mode_defaults_are_continuous_voice_loop_en() {
        let m = AsrMode::default();
        assert_eq!(m.trigger, TriggerMode::Continuous);
        assert_eq!(m.routing, RoutingMode::VoiceLoop);
        assert_eq!(m.language, "en");
    }

    #[test]
    fn mode_deserializes_partial_with_defaults() {
        // The chat window may send only the field it's overriding.
        let m: AsrMode = serde_json::from_str(r#"{"trigger":"push_to_talk"}"#).unwrap();
        assert_eq!(m.trigger, TriggerMode::PushToTalk);
        assert_eq!(m.routing, RoutingMode::VoiceLoop); // defaulted
        assert_eq!(m.language, "en"); // defaulted
    }

    #[test]
    fn event_is_tagged_with_kind() {
        let v = serde_json::to_value(AsrEvent::Transcript {
            text: "hello".into(),
            is_final: true,
        })
        .unwrap();
        assert_eq!(v["kind"], "transcript");
        assert_eq!(v["text"], "hello");
        assert_eq!(v["is_final"], true);

        let listening = serde_json::to_value(AsrEvent::Listening).unwrap();
        assert_eq!(listening["kind"], "listening");
    }

    #[test]
    fn status_when_idle_reports_not_running() {
        // No session started in this unit-test context.
        let s = status();
        assert!(!s.running);
        assert_eq!(s.available, is_available());
        assert!(!s.ptt_active);
    }

    #[test]
    fn availability_matches_build_cfg() {
        assert_eq!(is_available(), cfg!(asr_active));
    }
}
