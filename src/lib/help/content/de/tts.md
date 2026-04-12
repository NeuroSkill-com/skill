# Sprachausgabe auf dem Gerät (TTS)

## Sprachausgabe auf dem Gerät (TTS)
NeuroSkill™ enthält eine vollständig lokale englische Text-to-Speech-Engine. Sie kündigt Kalibrierungsphasen laut an (Aktionslabels, Pausen, Abschluss) und kann per WebSocket- oder HTTP-API ferngesteuert werden. Die gesamte Synthese läuft lokal — nach dem einmaligen Download des ~30-MB-Modells wird kein Internet benötigt.

## So funktioniert es
Textvorverarbeitung → Satzaufteilung (≤400 Zeichen) → Phonemisierung über libespeak-ng (C-Bibliothek, prozessintern, en-us-Stimme) → Tokenisierung (IPA → Ganzzahl-IDs) → ONNX-Inferenz (KittenTTS-Modell: input_ids + style + speed → f32-Wellenform) → 1 s Stille → rodio-Wiedergabe über den Standard-Audioausgang.

## Model
KittenML/kitten-tts-mini-0.8 von HuggingFace Hub. Stimme: Jasper (Englisch en-us). Abtastrate: 24.000 Hz Mono Float32. Quantisiertes INT8-ONNX — nur CPU, keine GPU erforderlich. Nach dem ersten Download in ~/.cache/huggingface/hub/ zwischengespeichert.

## Voraussetzungen
espeak-ng muss installiert und im PATH verfügbar sein — es liefert die prozessinterne IPA-Phonemisierung (als C-Bibliothek gelinkt, kein Subprozess). macOS: brew install espeak-ng. Ubuntu/Debian: apt install libespeak-ng-dev. Alpine: apk add espeak-ng-dev. Fedora: dnf install espeak-ng-devel.

## Kalibrierungsintegration
Wenn eine Kalibrierungssitzung beginnt, wird die Engine im Hintergrund vorgewärmt (bei Bedarf wird das Modell heruntergeladen). In jeder Phase ruft das Kalibrierungsfenster tts_speak mit dem Aktionslabel, der Pausenansage, der Abschlussmeldung oder dem Abbruchhinweis auf. Sprache blockiert nie die Kalibrierung — alle TTS-Aufrufe sind Fire-and-Forget.

## API — say-Befehl
Sprachausgabe von jedem externen Skript, Automatisierungstool oder LLM-Agenten auslösen. Der Befehl kehrt sofort zurück, während Audio abgespielt wird. WebSocket: {"command":"say","text":"Ihre Nachricht"}. HTTP: POST /say mit Body {"text":"Ihre Nachricht"}. CLI (curl): curl -X POST http://localhost:<port>/say -d '{"text":"hallo"}' -H 'Content-Type: application/json'.

## Debug-Protokollierung
Aktivieren Sie die TTS-Synthese-Protokollierung unter Einstellungen → Sprache, um Ereignisse (gesprochener Text, Sampleanzahl, Inferenzlatenz) in die NeuroSkill™-Protokolldatei zu schreiben. Nützlich zur Latenzmessung und Fehlerdiagnose.

## Hier testen
Verwenden Sie das Widget unten, um die TTS-Engine direkt aus diesem Hilfefenster zu testen.
