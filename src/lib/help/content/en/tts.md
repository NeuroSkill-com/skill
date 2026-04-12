# On-device Voice Guidance (TTS)

## On-device Voice Guidance (TTS)
NeuroSkill™ includes a fully on-device English text-to-speech engine. It announces calibration phases aloud (action labels, breaks, completion) and can be triggered remotely from any script via the WebSocket or HTTP API. All synthesis runs locally — no internet is needed after the ~30 MB model is downloaded once.

## How It Works
Text preprocessing → sentence chunking (≤400 chars) → phonemisation via libespeak-ng (C library, in-process, en-us voice) → tokenisation (IPA → integer IDs) → ONNX inference (KittenTTS model: input_ids + style + speed → f32 waveform) → 1 s silence pad → rodio plays on the system default audio output.

## Model
KittenML/kitten-tts-mini-0.8 from HuggingFace Hub. Voice: Jasper (English en-us). Sample rate: 24 000 Hz mono float32. Quantised INT8 ONNX — CPU-only, no GPU required. Cached in ~/.cache/huggingface/hub/ after first download.

## Requirements
espeak-ng must be installed and on PATH — it provides in-process IPA phonemisation (linked as a C library, not spawned as a subprocess). macOS: brew install espeak-ng. Ubuntu/Debian: apt install libespeak-ng-dev. Alpine: apk add espeak-ng-dev. Fedora: dnf install espeak-ng-devel.

## Calibration Integration
When a calibration session starts the engine is pre-warmed in the background (downloading the model if needed). At each phase the calibration window calls tts_speak with the action label, break announcement, completion message, or cancellation notice. Speech never blocks calibration — all TTS calls are fire-and-forget.

## API — say command
Trigger speech from any external script, automation tool, or LLM agent. The command returns immediately while audio plays. WebSocket: {"command":"say","text":"your message"}. HTTP: POST /say with body {"text":"your message"}. CLI (curl): curl -X POST http://localhost:<port>/say -d '{"text":"hello"}' -H 'Content-Type: application/json'.

## Debug Logging
Enable TTS synthesis logging in Settings → Voice to write events (spoken text, sample count, inference latency) to the NeuroSkill™ log file. Useful for measuring latency and diagnosing issues.

## Test It Here
Use the widget below to test the TTS engine directly from this help window.
