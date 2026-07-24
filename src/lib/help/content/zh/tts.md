# On-device Voice (TTS + ASR)

## Overview
NeuroSkill™ speaks and listens fully on-device. **Text-to-speech (TTS)** announces calibration phases and can be triggered from scripts or chat. **Speech recognition (ASR)** powers the chat microphone (continuous or push-to-talk). After the first model download, synthesis and transcription run locally — no cloud round-trip.

## Choosing engines
Open **Settings → Voice** or the **LLM / Chat** voice sections. Engine chips come from the daemon catalog (`/v1/tts/engines`, `/v1/asr/engines`), which mirrors every backend Skill wires from rlx-models.

- **TTS** — KittenTTS (default, small ONNX), NeuTTS, RLX-TTS, Qwen3-TTS, Piper, StyleTTS2, and many experimental Hub engines (Orpheus, Kyutai, MetaVoice, MiraTTS, …). Chips marked **exp** may download large checkpoints; **bundle** engines need a one-time local export (Inflect-Nano / MeloTTS). Engines unavailable in this build (for example on Windows) are greyed out.
- **ASR** — Whisper (default), Qwen3-ASR, Voxtral, FunASR (**SenseVoice** default; Paraformer-zh optional), Nemotron-ASR, and RLX-ASR. First use auto-downloads Hub weights; chat shows download progress while loading.

## How TTS works
Text preprocessing → sentence chunking → engine-specific phonemisation / tokenizer → local inference → playback on the system default output. KittenTTS uses libespeak-ng + an INT8 ONNX model (~30 MB). Larger RLX engines pull HuggingFace packs (`.rlx` / `.rlxp` / GGUF) into `~/.skill/models/`.

## How ASR works
Microphone capture (cpal) → Silero VAD segmentation → selected ASR backend → transcript events on the daemon WebSocket. In voice-loop mode the daemon sends the transcript to the LLM and speaks the reply.

## API — say command
Trigger speech from any external script or agent. WebSocket: `{"command":"say","text":"your message"}`. HTTP: `POST /say` with `{"text":"your message"}`.

## Debug
Enable TTS logging in Settings → Voice. Use the widget below to test synthesis from this help window.
