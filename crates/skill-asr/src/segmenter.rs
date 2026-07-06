// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Pure utterance segmentation — no audio I/O, no models, no rlx.
//!
//! Turns a stream of per-frame speech probabilities (continuous VAD) or an
//! external talk gate (push-to-talk) into discrete utterance buffers. Kept free
//! of `cpal`/`rlx` so it compiles and unit-tests without the `asr` feature: the
//! engine ([`crate::engine`]) computes the probabilities with Silero and feeds
//! them here, then transcribes each emitted [`SegEvent::Utterance`] with Whisper.

use std::collections::VecDeque;

const TARGET_HZ: usize = 16_000;

/// Tunables for the continuous VAD state machine (push-to-talk ignores the
/// probability-driven fields and only honours `min_utt_samples`).
#[derive(Debug, Clone)]
pub struct SegmenterConfig {
    /// Samples per VAD frame (Silero @ 16 kHz → 512).
    pub frame_len: usize,
    /// Speech probability at/above which a frame is voiced.
    pub speech_threshold: f32,
    /// Consecutive voiced frames required to open an utterance.
    pub min_speech_frames: usize,
    /// Trailing silent frames that close an utterance.
    pub hangover_frames: usize,
    /// Frames of pre-onset audio prepended so the first phoneme isn't clipped.
    pub pre_roll_frames: usize,
    /// Minimum finalized utterance length; shorter buffers are dropped.
    pub min_utt_samples: usize,
}

impl SegmenterConfig {
    /// Defaults tuned for 16 kHz Silero frames (~32 ms each).
    pub fn defaults_16k(frame_len: usize) -> Self {
        Self {
            frame_len,
            speech_threshold: 0.5,
            min_speech_frames: 3,           // ~96 ms
            hangover_frames: 24,            // ~0.77 s
            pre_roll_frames: 8,             // ~256 ms
            min_utt_samples: TARGET_HZ / 4, // 0.25 s
        }
    }
}

/// Output of the segmenter as audio/gate is fed in.
#[derive(Debug, Clone, PartialEq)]
pub enum SegEvent {
    /// Speech onset (VAD-confirmed, or push-to-talk pressed).
    SpeechStart,
    /// Speech offset (hangover elapsed, or push-to-talk released).
    SpeechEnd,
    /// A finalized utterance buffer ready for ASR (≥ `min_utt_samples`).
    Utterance(Vec<f32>),
}

/// Stateful segmenter. One instance per voice session.
pub struct Segmenter {
    cfg: SegmenterConfig,
    // continuous-mode state
    speaking: bool,
    speech_frames: usize,
    silence_frames: usize,
    pre_roll: VecDeque<f32>,
    utt: Vec<f32>,
    // push-to-talk state
    recording: bool,
}

impl Segmenter {
    pub fn new(cfg: SegmenterConfig) -> Self {
        let pre_roll = VecDeque::with_capacity(cfg.pre_roll_frames * cfg.frame_len);
        Self {
            cfg,
            speaking: false,
            speech_frames: 0,
            silence_frames: 0,
            pre_roll,
            utt: Vec::new(),
            recording: false,
        }
    }

    /// Continuous mode: push one VAD frame with its speech probability.
    /// `frame` must be `cfg.frame_len` samples.
    pub fn push_vad_frame(&mut self, prob: f32, frame: &[f32]) -> Vec<SegEvent> {
        let mut out = Vec::new();
        let is_speech = prob >= self.cfg.speech_threshold;

        if is_speech {
            self.silence_frames = 0;
            self.speech_frames += 1;
            if self.speaking {
                self.utt.extend_from_slice(frame);
            } else if self.speech_frames >= self.cfg.min_speech_frames {
                self.speaking = true;
                self.utt.extend(self.pre_roll.drain(..));
                self.utt.extend_from_slice(frame);
                out.push(SegEvent::SpeechStart);
            } else {
                self.push_pre_roll(frame);
            }
        } else if self.speaking {
            self.utt.extend_from_slice(frame);
            self.silence_frames += 1;
            if self.silence_frames >= self.cfg.hangover_frames {
                self.finalize(&mut out);
            }
        } else {
            self.speech_frames = 0;
            self.push_pre_roll(frame);
        }
        out
    }

    /// Push-to-talk mode: `gate_open` is the talk-key state; `audio` is any new
    /// 16 kHz samples since the last call (empty slice on idle ticks).
    pub fn push_ptt(&mut self, gate_open: bool, audio: &[f32]) -> Vec<SegEvent> {
        let mut out = Vec::new();
        if gate_open && !self.recording {
            self.recording = true;
            self.utt.clear();
            out.push(SegEvent::SpeechStart);
        }
        if gate_open {
            self.utt.extend_from_slice(audio);
        } else if self.recording {
            self.recording = false;
            let took = std::mem::take(&mut self.utt);
            out.push(SegEvent::SpeechEnd);
            if took.len() >= self.cfg.min_utt_samples {
                out.push(SegEvent::Utterance(took));
            }
        }
        out
    }

    /// Flush a still-open utterance at shutdown. Returns the buffer if it's long
    /// enough to transcribe.
    pub fn flush(&mut self) -> Option<Vec<f32>> {
        if self.speaking || self.recording {
            self.speaking = false;
            self.recording = false;
            let took = std::mem::take(&mut self.utt);
            if took.len() >= self.cfg.min_utt_samples {
                return Some(took);
            }
        }
        None
    }

    fn finalize(&mut self, out: &mut Vec<SegEvent>) {
        let took = std::mem::take(&mut self.utt);
        self.speaking = false;
        self.speech_frames = 0;
        self.silence_frames = 0;
        out.push(SegEvent::SpeechEnd);
        if took.len() >= self.cfg.min_utt_samples {
            out.push(SegEvent::Utterance(took));
        }
    }

    fn push_pre_roll(&mut self, frame: &[f32]) {
        self.pre_roll.extend(frame.iter().copied());
        let cap = self.cfg.pre_roll_frames * self.cfg.frame_len;
        while self.pre_roll.len() > cap {
            self.pre_roll.pop_front();
        }
    }
}

/// Average interleaved channels down to mono. `channels <= 1` is a passthrough.
pub fn downmix(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks(channels)
        .map(|f| f.iter().copied().sum::<f32>() / channels as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: usize = 512;

    fn cfg() -> SegmenterConfig {
        SegmenterConfig::defaults_16k(FRAME)
    }

    fn frame(val: f32) -> Vec<f32> {
        vec![val; FRAME]
    }

    fn count_utterances(events: &[SegEvent]) -> usize {
        events.iter().filter(|e| matches!(e, SegEvent::Utterance(_))).count()
    }

    #[test]
    fn downmix_mono_is_passthrough() {
        let mono = vec![0.1, -0.2, 0.3];
        assert_eq!(downmix(&mono, 1), mono);
    }

    #[test]
    fn downmix_stereo_averages_channels() {
        // L/R interleaved: (1,-1) -> 0, (0.5,0.5) -> 0.5
        let stereo = vec![1.0, -1.0, 0.5, 0.5];
        assert_eq!(downmix(&stereo, 2), vec![0.0, 0.5]);
    }

    #[test]
    fn continuous_below_min_speech_never_opens() {
        let mut seg = Segmenter::new(cfg());
        // 2 voiced frames (< min_speech_frames = 3), then silence.
        let mut events = Vec::new();
        events.extend(seg.push_vad_frame(0.9, &frame(0.5)));
        events.extend(seg.push_vad_frame(0.9, &frame(0.5)));
        events.extend(seg.push_vad_frame(0.0, &frame(0.0)));
        assert!(events.is_empty(), "no utterance should start: {events:?}");
        assert!(seg.flush().is_none(), "nothing buffered to flush");
    }

    #[test]
    fn continuous_emits_one_utterance_with_start_and_end() {
        let mut seg = Segmenter::new(cfg());
        let mut events = Vec::new();

        // 5 voiced frames — SpeechStart fires on the 3rd.
        for _ in 0..5 {
            events.extend(seg.push_vad_frame(0.95, &frame(0.4)));
        }
        assert_eq!(
            events.iter().filter(|e| **e == SegEvent::SpeechStart).count(),
            1,
            "exactly one SpeechStart"
        );

        // Hangover worth of silence closes the utterance.
        for _ in 0..cfg().hangover_frames {
            events.extend(seg.push_vad_frame(0.0, &frame(0.0)));
        }

        assert_eq!(count_utterances(&events), 1, "one finalized utterance");
        assert!(events.contains(&SegEvent::SpeechEnd));
        assert!(seg.flush().is_none(), "closed after hangover — nothing to flush");

        // The utterance covers pre-roll(2) + onset/speech(3..5) + trailing silence.
        let utt = events
            .iter()
            .find_map(|e| match e {
                SegEvent::Utterance(b) => Some(b),
                _ => None,
            })
            .unwrap();
        let expected = (5 + cfg().hangover_frames) * FRAME;
        assert_eq!(utt.len(), expected, "utterance length matches fed frames");
    }

    #[test]
    fn push_to_talk_open_then_close_yields_utterance() {
        let mut seg = Segmenter::new(cfg());
        let chunk = vec![0.3f32; 3000];

        let e1 = seg.push_ptt(true, &chunk); // press + first audio
        assert_eq!(e1, vec![SegEvent::SpeechStart]);
        let e2 = seg.push_ptt(true, &chunk); // more audio
        assert!(e2.is_empty());
        let e3 = seg.push_ptt(false, &[]); // release

        assert_eq!(e3.len(), 2);
        assert_eq!(e3[0], SegEvent::SpeechEnd);
        match &e3[1] {
            SegEvent::Utterance(b) => assert_eq!(b.len(), 6000),
            other => panic!("expected Utterance, got {other:?}"),
        }
        assert!(seg.flush().is_none(), "released — nothing left to flush");
    }

    #[test]
    fn push_to_talk_too_short_drops_utterance() {
        let mut seg = Segmenter::new(cfg());
        seg.push_ptt(true, &vec![0.3f32; 100]); // below min_utt_samples
        let e = seg.push_ptt(false, &[]);
        assert_eq!(e, vec![SegEvent::SpeechEnd], "short clip emits no Utterance");
    }

    #[test]
    fn flush_returns_open_continuous_utterance() {
        let mut seg = Segmenter::new(cfg());
        for _ in 0..10 {
            seg.push_vad_frame(0.95, &frame(0.4));
        }
        let flushed = seg.flush().expect("open utterance flushed");
        assert!(flushed.len() >= cfg().min_utt_samples);
        assert!(seg.flush().is_none(), "nothing left after flush");
    }
}
