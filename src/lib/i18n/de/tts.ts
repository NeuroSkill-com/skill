// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** DE "tts" namespace translations. */
const tts: Record<string, string> = {
  "ttsTab.backendSection": "Sprachsynthese-Engine",
  "ttsTab.backendKitten": "KittenTTS",
  "ttsTab.backendKittenTag": "ONNX · Englisch · ~30 MB",
  "ttsTab.backendKittenDesc": "Kompaktes ONNX-Modell, schnell auf jeder CPU, nur Englisch.",
  "ttsTab.backendNeutts": "NeuTTS",
  "ttsTab.backendNeuttsTag": "GGUF · Stimmklonen · Mehrsprachig",
  "ttsTab.backendNeuttsDesc":
    "GGUF-LLM-Backbone mit NeuCodec-Decoder. Klont jede Stimme; unterstützt Englisch, Deutsch, Französisch, Spanisch.",
  "ttsTab.statusSection": "Engine-Status",
  "ttsTab.statusReady": "Bereit",
  "ttsTab.statusLoading": "Wird geladen…",
  "ttsTab.statusIdle": "Inaktiv",
  "ttsTab.statusUnloaded": "Nicht geladen",
  "ttsTab.statusError": "Fehler",
  "ttsTab.preloadButton": "Vorladen",
  "ttsTab.retryButton": "Wiederholen",
  "ttsTab.preloadOnStartup": "Engine beim Start vorladen",
  "ttsTab.preloadOnStartupDesc": "Lädt die aktive Engine beim App-Start im Hintergrund vor",
  "ttsTab.unloadButton": "Entladen",
  "ttsTab.errorTitle": "Ladefehler",
  "ttsTab.requirements": "Benötigt espeak-ng im PATH",
  "ttsTab.requirementsDesc": "macOS: brew install espeak-ng · Ubuntu: apt install espeak-ng",
  "ttsTab.kittenConfigSection": "KittenTTS-Einstellungen",
  "ttsTab.kittenVoiceLabel": "Stimme",
  "ttsTab.kittenModelInfo": "KittenML/kitten-tts-mini-0.8 · 24 kHz · ~30 MB",
  "ttsTab.neuttsConfigSection": "NeuTTS-Einstellungen",
  "ttsTab.neuttsModelLabel": "Backbone-Modell",
  "ttsTab.neuttsModelDesc":
    "Kleineres GGUF = schneller; größeres = natürlicher. Q4 wird für die meisten Systeme empfohlen.",
  "ttsTab.neuttsVoiceSection": "Referenzstimme",
  "ttsTab.neuttsVoiceDesc": "Wähle eine voreingestellte Stimme oder lade einen WAV-Clip für das Stimmklonen hoch.",
  "ttsTab.neuttsPresetLabel": "Voreingestellte Stimmen",
  "ttsTab.neuttsCustomOption": "Eigene WAV…",
  "ttsTab.neuttsRefWavLabel": "Referenz-WAV",
  "ttsTab.neuttsRefWavNone": "Keine Datei ausgewählt",
  "ttsTab.neuttsRefWavBrowse": "Durchsuchen…",
  "ttsTab.neuttsRefTextLabel": "Transkript",
  "ttsTab.neuttsRefTextPlaceholder": "Gib genau ein, was im WAV-Clip gesagt wird",
  "ttsTab.neuttsSaveButton": "Speichern",
  "ttsTab.neuttsSaved": "Gespeichert",
  "ttsTab.voiceJo": "Jo",
  "ttsTab.voiceDave": "Dave",
  "ttsTab.voiceGreta": "Greta",
  "ttsTab.voiceJuliette": "Juliette",
  "ttsTab.voiceMateo": "Mateo",
  "ttsTab.voiceCustom": "Eigene…",
  "ttsTab.testSection": "Stimme testen",
  "ttsTab.testDesc": "Gib einen beliebigen Text ein und drücke Sprechen, um die aktive Engine zu hören.",
  "ttsTab.startupSection": "Start",
  "ttsTab.loggingSection": "Debug-Protokollierung",
  "ttsTab.loggingLabel": "TTS-Synthese-Protokollierung",
  "ttsTab.loggingDesc": "Schreibt Syntheseereignisse (Text, Sampleanzahl, Latenz) in die Protokolldatei.",
  "ttsTab.apiSection": "API",
  "ttsTab.apiDesc": "Sprache über die WebSocket- oder HTTP-API aus einem beliebigen Skript oder Tool auslösen:",
  "ttsTab.apiExampleWs": 'WebSocket:  {"command":"say","text":"Augen geschlossen."}',
  "ttsTab.apiExampleHttp": 'HTTP (curl): POST /say  body: {"text":"Augen geschlossen."}',

  "helpTts.overviewTitle": "Sprachausgabe auf dem Gerät (TTS)",
  "helpTts.overviewBody":
    "NeuroSkill™ enthält eine vollständig lokale englische Text-to-Speech-Engine. Sie kündigt Kalibrierungsphasen laut an (Aktionslabels, Pausen, Abschluss) und kann per WebSocket- oder HTTP-API ferngesteuert werden. Die gesamte Synthese läuft lokal — nach dem einmaligen Download des ~30-MB-Modells wird kein Internet benötigt.",
  "helpTts.howItWorksTitle": "So funktioniert es",
  "helpTts.howItWorksBody":
    "Textvorverarbeitung → Satzaufteilung (≤400 Zeichen) → Phonemisierung über libespeak-ng (C-Bibliothek, prozessintern, en-us-Stimme) → Tokenisierung (IPA → Ganzzahl-IDs) → ONNX-Inferenz (KittenTTS-Modell: input_ids + style + speed → f32-Wellenform) → 1 s Stille → rodio-Wiedergabe über den Standard-Audioausgang.",
  "helpTts.modelTitle": "Model",
  "helpTts.modelBody":
    "KittenML/kitten-tts-mini-0.8 von HuggingFace Hub. Stimme: Jasper (Englisch en-us). Abtastrate: 24.000 Hz Mono Float32. Quantisiertes INT8-ONNX — nur CPU, keine GPU erforderlich. Nach dem ersten Download in ~/.cache/huggingface/hub/ zwischengespeichert.",
  "helpTts.requirementsTitle": "Voraussetzungen",
  "helpTts.requirementsBody":
    "espeak-ng muss installiert und im PATH verfügbar sein — es liefert die prozessinterne IPA-Phonemisierung (als C-Bibliothek gelinkt, kein Subprozess). macOS: brew install espeak-ng. Ubuntu/Debian: apt install libespeak-ng-dev. Alpine: apk add espeak-ng-dev. Fedora: dnf install espeak-ng-devel.",
  "helpTts.calibrationTitle": "Kalibrierungsintegration",
  "helpTts.calibrationBody":
    "Wenn eine Kalibrierungssitzung beginnt, wird die Engine im Hintergrund vorgewärmt (bei Bedarf wird das Modell heruntergeladen). In jeder Phase ruft das Kalibrierungsfenster tts_speak mit dem Aktionslabel, der Pausenansage, der Abschlussmeldung oder dem Abbruchhinweis auf. Sprache blockiert nie die Kalibrierung — alle TTS-Aufrufe sind Fire-and-Forget.",
  "helpTts.apiTitle": "API — say-Befehl",
  "helpTts.apiBody":
    'Sprachausgabe von jedem externen Skript, Automatisierungstool oder LLM-Agenten auslösen. Der Befehl kehrt sofort zurück, während Audio abgespielt wird. WebSocket: {"command":"say","text":"Ihre Nachricht"}. HTTP: POST /say mit Body {"text":"Ihre Nachricht"}. CLI (curl): curl -X POST http://localhost:<port>/say -d \'{"text":"hallo"}\' -H \'Content-Type: application/json\'.',
  "helpTts.loggingTitle": "Debug-Protokollierung",
  "helpTts.loggingBody":
    "Aktivieren Sie die TTS-Synthese-Protokollierung unter Einstellungen → Sprache, um Ereignisse (gesprochener Text, Sampleanzahl, Inferenzlatenz) in die NeuroSkill™-Protokolldatei zu schreiben. Nützlich zur Latenzmessung und Fehlerdiagnose.",
  "helpTts.testTitle": "Hier testen",
  "helpTts.testBody": "Verwenden Sie das Widget unten, um die TTS-Engine direkt aus diesem Hilfefenster zu testen.",
};

export default tts;
