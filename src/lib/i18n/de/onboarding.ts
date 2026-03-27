// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** DE "onboarding" namespace translations. */
const onboarding: Record<string, string> = {
  "onboarding.title": "Willkommen bei {app}",
  "onboarding.step.welcome": "Willkommen",
  "onboarding.step.bluetooth": "Bluetooth",
  "onboarding.step.fit": "Sitz prüfen",
  "onboarding.step.calibration": "Kalibrierung",
  "onboarding.step.models": "Modelle",
  "onboarding.step.tray": "Tray",
  "onboarding.step.enable_bluetooth": "Bluetooth aktivieren",
  "onboarding.step.done": "Fertig",
  "onboarding.welcomeTitle": "Willkommen bei {app}",
  "onboarding.welcomeBody": "{app} zeichnet EEG-Daten deines BCI-Geräts auf, analysiert und indexiert sie.",
  "onboarding.bluetoothHint": "BCI-Gerät verbinden",
  "onboarding.fitHint": "Sensorkontakt prüfen",
  "onboarding.calibrationHint": "Schnelle Kalibrierung durchführen",
  "onboarding.modelsHint": "Empfohlene lokale KI-Modelle herunterladen",
  "onboarding.bluetoothTitle": "BCI-Gerät verbinden",
  "onboarding.bluetoothBody":
    "Schalte dein BCI-Gerät ein und setze es auf. {app} sucht automatisch nach Geräten in der Nähe.",
  "onboarding.btConnected": "Verbunden mit {name}",
  "onboarding.btScanning": "Suche…",
  "onboarding.btReady": "Bereit zum Scannen",
  "onboarding.btScan": "Scannen",
  "onboarding.btInstructions": "So verbindest du",
  "onboarding.btStep1":
    "Schalte dein BCI-Gerät ein (Ein-/Aus-Knopf halten, Schalter umlegen oder Taste drücken, je nach Headset).",
  "onboarding.btStep2": "Setze das Headset auf — die Sensoren sollen hinter den Ohren und auf der Stirn aufliegen.",
  "onboarding.btStep3": "Klicke oben auf Scannen. {app} findet und verbindet sich automatisch mit dem nächsten Gerät.",
  "onboarding.btSuccess": "Headset verbunden! Du kannst fortfahren.",
  "onboarding.fitTitle": "Sitz des Headsets prüfen",
  "onboarding.fitBody":
    "Guter Sensorkontakt ist entscheidend für saubere EEG-Daten. Alle vier Sensoren sollten grün oder gelb anzeigen.",
  "onboarding.sensorQuality": "Live-Sensorqualität",
  "onboarding.quality.good": "Gut",
  "onboarding.quality.fair": "Mittel",
  "onboarding.quality.poor": "Schlecht",
  "onboarding.quality.no_signal": "Kein Signal",
  "onboarding.fitNeedsBt": "Verbinde zuerst dein Headset, um Live-Sensordaten zu sehen.",
  "onboarding.fitTips": "Tipps für besseren Kontakt",
  "onboarding.fitTip1":
    "Ohrsensoren (TP9/TP10): hinter und leicht über die Ohren schieben. Haare von den Sensoren streichen.",
  "onboarding.fitTip2":
    "Stirnsensoren (AF7/AF8): sollten flach auf sauberer Haut aufliegen — bei Bedarf mit einem trockenen Tuch abwischen.",
  "onboarding.fitTip3":
    "Bei schlechtem Kontakt die Sensoren leicht mit einem feuchten Finger befeuchten. Das verbessert die Leitfähigkeit.",
  "onboarding.fitGood": "Perfekter Sitz! Alle Sensoren haben guten Kontakt.",
  "onboarding.calibrationTitle": "Kalibrierung starten",
  "onboarding.calibrationBody":
    "Die Kalibrierung zeichnet gelabeltes EEG auf, während du zwischen zwei mentalen Zuständen wechselst. Das hilft {app}, deine Gehirn-Basismuster zu lernen.",
  "onboarding.openCalibration": "Kalibrierung öffnen",
  "onboarding.calibrationNeedsBt": "Verbinde zuerst dein Headset, um die Kalibrierung zu starten.",
  "onboarding.calibrationSkip":
    "Du kannst dies überspringen und später über das Tray-Menü oder die Einstellungen kalibrieren.",
  "onboarding.enableBluetoothTitle": "Bluetooth auf Ihrem Mac aktivieren",
  "onboarding.enableBluetoothBody": "{app} benötigt, dass der Bluetooth-Adapter Ihres Mac eingeschaltet ist, um Ihr BCI-Gerät zu finden und zu verbinden. Bitte aktivieren Sie Bluetooth in den Systemeinstellungen, falls es ausgeschaltet ist.",
  "onboarding.enableBluetoothStatus": "Bluetooth-Adapter",
  "onboarding.enableBluetoothHint": "Öffnen Sie die Systemeinstellungen → Bluetooth und schalten Sie Bluetooth ein. Wenn Sie in Entwicklung über Terminal ausführen, stellen Sie sicher, dass der Systemadapter aktiviert ist.",
  "onboarding.enableBluetoothOpen": "Bluetooth-Einstellungen öffnen",
  "onboarding.modelsTitle": "Empfohlene Modelle herunterladen",
  "onboarding.modelsBody":
    "Für die beste lokale Erfahrung lade jetzt diese Standardmodelle herunter: Qwen3.5 4B (Q4_K_M), ZUNA-Encoder, NeuTTS und Kitten TTS.",
  "onboarding.models.downloadAll": "Empfohlenes Set herunterladen",
  "onboarding.models.download": "Herunterladen",
  "onboarding.models.downloading": "Wird heruntergeladen…",
  "onboarding.models.downloaded": "Heruntergeladen",
  "onboarding.models.qwenTitle": "Qwen3.5 4B (Q4_K_M)",
  "onboarding.models.qwenDesc":
    "Empfohlenes Chat-Modell. Verwendet Q4_K_M für die beste Qualitäts-/Geschwindigkeitsbalance auf den meisten Laptops.",
  "onboarding.models.zunaTitle": "ZUNA-EEG-Encoder",
  "onboarding.models.zunaDesc":
    "Benötigt für EEG-Embeddings, semantische Historie und nachgeschaltete Gehirnzustands-Analytik.",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc": "Empfohlene mehrsprachige Sprachengine mit besserer Qualität und Klon-Unterstützung.",
  "onboarding.models.kittenTitle": "Kitten TTS",
  "onboarding.models.kittenDesc":
    "Leichtgewichtiges schnelles Sprach-Backend, nützlich als schneller Fallback und für ressourcenarme Systeme.",
  "onboarding.models.ocrTitle": "OCR-Modelle",
  "onboarding.models.ocrDesc":
    "Texterkennungsmodelle für die Extraktion von Text aus Screenshots. Ermöglicht Textsuche über erfasste Bildschirme (~10 MB pro Modell).",
  "onboarding.screenRecTitle": "Bildschirmaufnahme-Berechtigung",
  "onboarding.screenRecDesc":
    "Auf macOS erforderlich, um Fenster anderer Anwendungen für das Screenshot-System zu erfassen.",
  "onboarding.screenRecOpen": "Einstellungen öffnen",
  "onboarding.trayTitle": "Die App im Tray finden",
  "onboarding.trayBody":
    "{app} läuft im Hintergrund. Nach dem Setup ist das Symbol in deiner Menüleiste (macOS) bzw. im Infobereich (Windows/Linux) dein Einstiegspunkt.",
  "onboarding.tray.states": "Das Symbol ändert die Farbe je nach Status:",
  "onboarding.tray.grey": "Grau — getrennt",
  "onboarding.tray.amber": "Gelb — sucht oder verbindet",
  "onboarding.tray.green": "Grün — verbunden und zeichnet auf",
  "onboarding.tray.red": "Rot — Bluetooth ist ausgeschaltet",
  "onboarding.tray.open": "Klicke jederzeit auf das Tray-Symbol, um das Dashboard ein- oder auszublenden.",
  "onboarding.tray.menu":
    "Rechtsklick (oder Linksklick unter Windows/Linux) öffnet das Schnellmenü — verbinden, beschriften, kalibrieren und mehr.",
  "onboarding.downloadsComplete": "Alle Downloads abgeschlossen!",
  "onboarding.downloadsCompleteBody":
    "Die empfohlenen Modelle sind heruntergeladen und einsatzbereit. Um mehr Modelle herunterzuladen oder zu anderen zu wechseln, öffnen Sie",
  "onboarding.downloadMoreSettings": "App-Einstellungen",
  "onboarding.doneTitle": "Alles bereit!",
  "onboarding.doneBody": "{app} läuft in deiner Menüleiste. Hier sind ein paar Hinweise:",
  "onboarding.doneTip.tray":
    "{app} lebt in deinem Menüleisten-Tray. Klicke auf das Symbol, um das Dashboard ein-/auszublenden.",
  "onboarding.doneTip.shortcuts": "Verwende ⌘K für die Befehlspalette oder ? für alle Tastenkürzel.",
  "onboarding.doneTip.help": "Öffne die Hilfe über das Tray-Menü für eine vollständige Referenz aller Funktionen.",
  "onboarding.back": "Zurück",
  "onboarding.next": "Weiter",
  "onboarding.getStarted": "Los geht's",
  "onboarding.finish": "Fertig",
};

export default onboarding;
