#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Transcribe TTS-engine WAVs with the external openai-whisper and score coverage.

Usage: python3 scripts/whisper_validate_tts.py <reference_text> <wav> [<wav> ...]
"""
import re
import sys

import whisper


def words(text: str):
    return [w for w in re.split(r"[^a-z0-9]+", text.lower()) if len(w) > 2]


def main() -> int:
    reference = sys.argv[1]
    wavs = sys.argv[2:]
    ref_words = words(reference)

    model = whisper.load_model("base.en")
    ok = True
    print(f"REFERENCE: {reference}\n")
    for wav in wavs:
        r = model.transcribe(wav, language="en", fp16=False)
        transcript = r["text"].strip()
        heard = words(transcript)
        hits = sum(1 for w in ref_words if any(w == h or w in h for h in heard))
        ratio = hits / max(1, len(ref_words))
        verdict = "PASS" if ratio >= 0.6 else "FAIL"
        if ratio < 0.6:
            ok = False
        print(f"[{verdict}] {wav}")
        print(f"   heard:    {transcript!r}")
        print(f"   coverage: {hits}/{len(ref_words)} = {ratio:.0%}\n")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
