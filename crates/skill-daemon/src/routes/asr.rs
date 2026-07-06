// SPDX-License-Identifier: GPL-3.0-only
//! Voice-input (ASR + VAD) routes.
//!
//! Drives the `skill-asr` engine (cpal mic capture → Silero VAD → Whisper) and
//! forwards every [`skill_asr::AsrEvent`] to websocket clients as `"asr"` events.
//! The chat window picks the mode per session; absent fields fall back to the
//! `settings.asr` defaults. In [`RoutingMode::VoiceLoop`], a finalized transcript
//! is sent through the LLM chat and the reply is spoken via `skill-tts`.

use std::sync::Arc;

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::state::AppState;
use skill_asr::{AsrEvent, AsrMode, EventSink, RoutingMode, TriggerMode};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/asr/start", post(asr_start))
        .route("/asr/stop", post(asr_stop))
        .route("/asr/status", get(asr_status))
        .route("/asr/ptt", post(asr_ptt))
        .route("/asr/speaking", post(asr_speaking))
        .route("/settings/asr", get(asr_settings_get).post(asr_settings_set))
}

/// Read the persisted `settings.asr` (the source of truth for voice defaults +
/// engine/model — replaces the UI's localStorage mirror).
async fn asr_settings_get(State(state): State<AppState>) -> Json<Value> {
    let s = crate::routes::settings_io::load_user_settings(&state);
    Json(json!({ "ok": true, "asr": s.asr }))
}

/// Write `settings.asr` back to settings.json (atomic load-modify-save).
async fn asr_settings_set(State(state): State<AppState>, Json(cfg): Json<skill_settings::AsrConfig>) -> Json<Value> {
    crate::routes::settings_io::patch_settings(&state, move |s| s.asr = cfg).await;
    Json(json!({ "ok": true }))
}

#[derive(Debug, Default, Deserialize)]
struct AsrStartRequest {
    trigger: Option<TriggerMode>,
    routing: Option<RoutingMode>,
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PttRequest {
    active: bool,
}

async fn asr_start(State(state): State<AppState>, Json(req): Json<AsrStartRequest>) -> Json<Value> {
    if !skill_asr::is_available() {
        return Json(json!({ "ok": false, "error": "ASR not available in this build" }));
    }

    // Resolve the mode: explicit fields win, otherwise the settings defaults.
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let settings = skill_settings::load_settings(&skill_dir);
    let mode = AsrMode {
        trigger: req.trigger.unwrap_or(settings.asr.default_trigger),
        routing: req.routing.unwrap_or(settings.asr.default_routing),
        language: req.language.unwrap_or(settings.asr.language),
        engine: settings.asr.engine,
        model: settings.asr.model,
    };

    // Fresh conversation for each voice session.
    #[cfg(feature = "llm")]
    reset_voice_history();

    let rt = tokio::runtime::Handle::current();
    let sink = make_event_sink(state.clone(), mode.routing, rt);

    match skill_asr::start(mode.clone(), sink) {
        Ok(()) => Json(json!({ "ok": true, "mode": mode })),
        Err(e) => Json(json!({ "ok": false, "error": format!("{e:#}") })),
    }
}

/// Barge-in gate: the frontend brackets its own TTS playback with this so the
/// daemon-side mic doesn't transcribe the spoken reply (used when the daemon has
/// no TTS backend and the UI speaks the reply itself).
async fn asr_speaking(Json(req): Json<PttRequest>) -> Json<Value> {
    skill_asr::set_speaking(req.active);
    Json(json!({ "ok": true, "active": req.active }))
}

async fn asr_stop() -> Json<Value> {
    skill_asr::stop();
    Json(json!({ "ok": true }))
}

async fn asr_status() -> Json<Value> {
    Json(json!({ "ok": true, "status": skill_asr::status() }))
}

async fn asr_ptt(Json(req): Json<PttRequest>) -> Json<Value> {
    skill_asr::set_ptt(req.active);
    Json(json!({ "ok": true, "active": req.active }))
}

/// Build the engine callback: broadcast every event, and (voice-loop only) drive
/// a chat turn + spoken reply for each finalized transcript.
fn make_event_sink(state: AppState, routing: RoutingMode, rt: tokio::runtime::Handle) -> EventSink {
    Arc::new(move |evt: AsrEvent| {
        state.broadcast("asr", &evt);

        if routing == RoutingMode::VoiceLoop {
            if let AsrEvent::Transcript { text, is_final: true } = &evt {
                let text = text.clone();
                let state = state.clone();
                rt.spawn(async move {
                    voice_loop_turn(state, text).await;
                });
            }
        }
    })
}

// ── Voice-loop conversation state ────────────────────────────────────────────

#[cfg(feature = "llm")]
const VOICE_SYSTEM_PROMPT: &str = "You are a helpful voice assistant. Keep replies \
concise and natural to be spoken aloud — usually one to three short sentences. Do \
not use markdown, bullet lists, headings, or code blocks.";

/// Rolling multi-turn history for the active voice session, so the assistant has
/// context across utterances instead of answering each in isolation.
#[cfg(feature = "llm")]
fn voice_history() -> &'static std::sync::Mutex<Vec<Value>> {
    static H: std::sync::OnceLock<std::sync::Mutex<Vec<Value>>> = std::sync::OnceLock::new();
    H.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// Reset the conversation (called on `asr_start`): system prompt only.
#[cfg(feature = "llm")]
fn reset_voice_history() {
    if let Ok(mut h) = voice_history().lock() {
        h.clear();
        h.push(json!({ "role": "system", "content": VOICE_SYSTEM_PROMPT }));
    }
}

/// Keep the system prompt + the most recent turns so context stays bounded.
#[cfg(feature = "llm")]
fn cap_history(h: &mut Vec<Value>) {
    const MAX_MESSAGES: usize = 16; // after the system prompt
    if h.len() > MAX_MESSAGES + 1 {
        let drop = h.len() - (MAX_MESSAGES + 1);
        h.drain(1..1 + drop); // preserve index 0 (system)
    }
}

/// Serialize turns: while one transcript is being answered + spoken, drop further
/// transcripts. Combined with the engine's speaking-gate this gives clean
/// half-duplex turn-taking.
#[cfg(feature = "llm")]
fn turn_active() -> &'static std::sync::atomic::AtomicBool {
    static A: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    &A
}

/// One voice-loop turn: transcript → LLM chat (with history) → streamed spoken
/// reply, with the mic gated for the whole turn (barge-in guard).
#[cfg(feature = "llm")]
async fn voice_loop_turn(state: AppState, text: String) {
    use std::sync::atomic::Ordering;
    // Drop overlapping utterances while a turn is in flight.
    if turn_active().swap(true, Ordering::SeqCst) {
        return;
    }
    // Gate the mic for the whole turn (thinking + speaking) so we don't capture
    // and transcribe the assistant's own voice.
    skill_asr::set_speaking(true);
    voice_loop_turn_inner(&state, text).await;
    skill_asr::set_speaking(false);
    turn_active().store(false, Ordering::SeqCst);
}

#[cfg(feature = "llm")]
#[derive(Default)]
struct StreamState {
    raw: String,
    spoken: usize,
}

#[cfg(feature = "llm")]
async fn voice_loop_turn_inner(state: &AppState, text: String) {
    let srv = match state.llm_state_cell.lock().ok().and_then(|g| g.clone()) {
        Some(s) => s,
        None => {
            state.broadcast("asr", &json!({ "kind": "error", "message": "LLM server not running" }));
            return;
        }
    };

    // Append the user turn and snapshot the conversation.
    let messages = {
        let Ok(mut h) = voice_history().lock() else {
            return;
        };
        if h.is_empty() {
            h.push(json!({ "role": "system", "content": VOICE_SYSTEM_PROMPT }));
        }
        h.push(json!({ "role": "user", "content": text }));
        h.clone()
    };

    let params = skill_llm::GenParams {
        thinking_budget: Some(0), // direct spoken answer, no <think>
        ..Default::default()
    };

    // Stream sentences to a speaker task: start speaking sentence 1 while the
    // model is still generating sentence 2.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let speaker = tokio::spawn(async move {
        while let Some(sentence) = rx.recv().await {
            speak_in_daemon(&sentence).await;
        }
    });

    let stream = std::sync::Arc::new(std::sync::Mutex::new(StreamState::default()));
    let stream_cb = stream.clone();
    let tx_cb = tx.clone();
    let on_delta = move |d: &str| {
        if let Ok(mut s) = stream_cb.lock() {
            s.raw.push_str(d);
            let visible = strip_think(&s.raw);
            if s.spoken > visible.len() {
                s.spoken = visible.len();
            }
            while let Some(end) = next_sentence_end(&visible, s.spoken) {
                let sentence = visible[s.spoken..end].trim().to_string();
                s.spoken = end;
                if !sentence.is_empty() {
                    let _ = tx_cb.send(sentence);
                }
            }
        }
    };

    let result = skill_llm::run_chat_with_builtin_tools(&srv, messages, params, Vec::new(), on_delta, |_evt| {}).await;

    // Flush the trailing partial sentence, then close + await the speaker.
    if let Ok(mut s) = stream.lock() {
        let visible = strip_think(&s.raw);
        if s.spoken < visible.len() {
            let tail = visible[s.spoken..].trim().to_string();
            if !tail.is_empty() {
                let _ = tx.send(tail);
            }
            s.spoken = visible.len();
        }
    }
    drop(tx);
    let _ = speaker.await;

    match result {
        Ok((reply, ..)) => {
            let answer = strip_think(&reply);
            if let Ok(mut h) = voice_history().lock() {
                h.push(json!({ "role": "assistant", "content": answer }));
                cap_history(&mut h);
            }
            // `spoken` tells the UI whether it still needs to speak the text (only
            // when this daemon has no TTS backend).
            state.broadcast(
                "asr",
                &json!({ "kind": "assistant", "text": answer, "spoken": DAEMON_TTS }),
            );
        }
        Err(e) => state.broadcast("asr", &json!({ "kind": "error", "message": format!("chat: {e:#}") })),
    }
}

/// Byte index just past the next sentence terminator (`.`/`!`/`?`/newline) at or
/// after `from` that's followed by whitespace or end-of-text. ASCII-only match,
/// so the returned index is always a UTF-8 char boundary.
#[cfg(feature = "llm")]
fn next_sentence_end(s: &str, from: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = from;
    while i < s.len() {
        match b[i] {
            b'.' | b'!' | b'?' | b'\n' => {
                let end = i + 1;
                if end >= s.len() || b[end].is_ascii_whitespace() {
                    return Some(end);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(not(feature = "llm"))]
async fn voice_loop_turn(state: AppState, _text: String) {
    state.broadcast(
        "asr",
        &json!({ "kind": "error", "message": "LLM not built into this daemon" }),
    );
}

/// Whether this daemon was built with a TTS backend (speaks the reply itself).
#[cfg(feature = "llm")]
const DAEMON_TTS: bool = cfg!(feature = "voice-tts");

/// Speak the reply from the daemon (KittenTTS). Best-effort: logs and continues
/// if no audio device is available (e.g. headless CI).
#[cfg(all(feature = "llm", feature = "voice-tts"))]
async fn speak_in_daemon(text: &str) {
    if !text.trim().is_empty() {
        skill_tts::tts_speak(text.to_string(), None).await;
    }
}

/// No daemon TTS backend compiled in — the UI speaks the broadcast text instead.
#[cfg(all(feature = "llm", not(feature = "voice-tts")))]
async fn speak_in_daemon(_text: &str) {}

/// Strip `<think>…</think>` reasoning blocks so only the spoken answer is sent
/// to TTS. Tolerates an unterminated trailing `<think>`.
#[cfg(feature = "llm")]
fn strip_think(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find("<think>") {
        out.push_str(&rest[..open]);
        match rest[open..].find("</think>") {
            Some(close) => rest = &rest[open + close + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(all(test, feature = "llm"))]
mod tests {
    use super::*;

    // ── next_sentence_end (streaming-TTS splitter) ───────────────────────────

    /// Drive the splitter exactly as the streaming callback does and collect the
    /// complete sentences it would dispatch (mirrors the `while let Some(end)` loop).
    fn sentences(s: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut spoken = 0;
        while let Some(end) = next_sentence_end(s, spoken) {
            out.push(s[spoken..end].trim().to_string());
            spoken = end;
        }
        out
    }

    #[test]
    fn splits_on_terminators() {
        assert_eq!(
            sentences("Hello world. How are you?"),
            vec!["Hello world.", "How are you?"]
        );
    }

    #[test]
    fn does_not_split_decimals() {
        // The '.' in 3.14 is followed by a digit, not whitespace → not a boundary.
        assert_eq!(
            next_sentence_end("It is 3.14 today.", 0),
            Some("It is 3.14 today.".len())
        );
    }

    #[test]
    fn splits_at_blank_line_and_handles_no_terminator() {
        // A terminator counts only when followed by whitespace/end (uniformly,
        // incl. '\n'): '.' before a newline splits, and a blank line ('\n'+'\n')
        // is a boundary, but a bare newline mid-line (followed by a letter) is not.
        assert_eq!(next_sentence_end("Done.\nNext", 0), Some(5)); // '.' then '\n'
        assert_eq!(next_sentence_end("one\n\ntwo", 0), Some(4)); // blank line
        assert_eq!(next_sentence_end("Line one\nLine two", 0), None); // bare newline
        assert_eq!(next_sentence_end("incomplete tail", 0), None); // no terminator
    }

    #[test]
    fn ascii_terminator_index_is_utf8_safe() {
        // Multibyte content before the terminator must not corrupt the slice index.
        let s = "café costs €5. Yes.";
        let segs = sentences(s);
        assert_eq!(segs, vec!["café costs €5.", "Yes."]);
    }

    // ── strip_think ──────────────────────────────────────────────────────────

    #[test]
    fn strips_think_blocks() {
        assert_eq!(strip_think("<think>reasoning</think>Paris."), "Paris.");
        assert_eq!(strip_think("plain answer"), "plain answer");
        assert_eq!(strip_think("a<think>x</think>b<think>y</think>c"), "abc");
    }

    #[test]
    fn strips_unterminated_think_tail() {
        assert_eq!(strip_think("The answer.<think>still thinking"), "The answer.");
    }

    // ── cap_history ──────────────────────────────────────────────────────────

    #[test]
    fn cap_history_preserves_system_and_bounds_length() {
        let mut h = vec![json!({"role": "system", "content": "sys"})];
        for i in 0..30 {
            h.push(json!({"role": "user", "content": format!("u{i}")}));
        }
        cap_history(&mut h);
        assert!(h.len() <= 17, "system + 16 messages max, got {}", h.len());
        assert_eq!(h[0]["role"], "system", "system prompt must be preserved");
        // Most recent turn is retained.
        assert_eq!(h.last().unwrap()["content"], "u29");
    }

    #[test]
    fn cap_history_leaves_short_conversation_untouched() {
        let mut h = vec![
            json!({"role": "system", "content": "sys"}),
            json!({"role": "user", "content": "hi"}),
        ];
        cap_history(&mut h);
        assert_eq!(h.len(), 2);
    }
}
