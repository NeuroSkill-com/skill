// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** DE "calibration" namespace translations. */
const calibration: Record<string, string> = {
  "calibration.profiles": "Kalibrierungsprofile",
  "calibration.newProfile": "Neues Profil",
  "calibration.editProfile": "Profil bearbeiten",
  "calibration.profileName": "Profilname",
  "calibration.profileNamePlaceholder": "z. B. Augen offen / geschlossen",
  "calibration.addAction": "Aktion hinzufügen",
  "calibration.actionLabel": "Aktionsbezeichnung…",
  "calibration.breakLabel": "Pause",
  "calibration.selectProfile": "Profil",
  "calibration.descriptionN": "Dieses Protokoll führt {actions} aus, wiederholt <strong>{count}</strong> Mal.",
  "calibration.timingDescN": "{loops} Schleifen · {actions} Aktionen · {breakSecs}s Pause zwischen jeder",
  "calibration.notifActionBody": "Schleife {loop} von {total}",
  "calibration.notifBreakBody": "Nächste: {next}",
  "calibration.notifDoneBody": "Alle {n} Schleifen abgeschlossen.",
  "calibration.title": "Kalibrierung",
  "calibration.recording": "● Aufnahme",
  "calibration.neverCalibrated": "Nie kalibriert",
  "calibration.lastAgo": "Zuletzt: {ago}",
  "calibration.eegCalibration": "EEG-Kalibrierung",
  "calibration.description":
    'Diese Aufgabe wechselt zwischen <strong class="text-blue-600 dark:text-blue-400">{action1}</strong> und <strong class="text-violet-600 dark:text-violet-400">{action2}</strong> mit Pausen, wiederholt <strong>{count}</strong> Mal.',
  "calibration.timingDesc":
    "Jede Aktion dauert {actionSecs}s mit einer {breakSecs}s Pause. Labels werden automatisch gespeichert.",
  "calibration.startCalibration": "Kalibrierung starten",
  "calibration.complete": "Kalibrierung abgeschlossen",
  "calibration.completeDesc":
    "Alle {n} Iterationen erfolgreich abgeschlossen. Labels wurden für jede Phase gespeichert.",
  "calibration.runAgain": "Erneut ausführen",
  "calibration.iteration": "Durchlauf",
  "calibration.break": "Pause",
  "calibration.nextAction": "Nächste: {action}",
  "calibration.secondsRemaining": "Sekunden verbleibend",
  "calibration.ready": "Bereit",
  "calibration.lastCalibrated": "Zuletzt kalibriert",
  "calibration.lastAtAgo": "Zuletzt: {date} ({ago})",
  "calibration.noPrevious": "Keine vorherige Kalibrierung aufgezeichnet",
  "calibration.footer": "Esc zum Schließen · Ereignisse werden über WebSocket gesendet",
  "calibration.presets": "Schnellvoreinstellungen",
  "calibration.presetsDesc":
    "Wählen Sie eine Kalibrierungskonfiguration basierend auf Ihrem Ziel, Alter und Anwendungsfall.",
  "calibration.applyPreset": "Anwenden",
  "calibration.orCustom": "Oder manuell konfigurieren:",
  "calibration.preset.baseline": "Augen auf / zu",
  "calibration.preset.baselineDesc": "Klassisches Baseline: ruhende Augen auf vs. Augen zu. Empfohlen für Einsteiger.",
  "calibration.preset.focus": "Fokus / Entspannung",
  "calibration.preset.focusDesc": "Neurofeedback: Kopfrechnen vs. ruhiges Atmen. Allgemeine Verwendung.",
  "calibration.preset.meditation": "Meditation",
  "calibration.preset.meditationDesc": "Aktives Denken vs. Achtsamkeitsmeditation. Für Meditierende.",
  "calibration.preset.sleep": "Vor dem Schlafen / Schläfrigkeit",
  "calibration.preset.sleepDesc": "Waches Bewusstsein vs. Schläfrigkeit. Für Schlafforschung.",
  "calibration.preset.gaming": "Gaming / Leistung",
  "calibration.preset.gamingDesc":
    "Hochanspruchsvolle Aufgabe vs. passive Ruhe. Für E-Sport und Höchstleistungs-Biofeedback.",
  "calibration.preset.children": "Kurze Aufmerksamkeit",
  "calibration.preset.childrenDesc":
    "Kürzere Phasen (10 s) für Kinder oder Nutzer mit eingeschränkter Konzentrationsdauer.",
  "calibration.preset.clinical": "Klinisch / Forschung",
  "calibration.preset.clinicalDesc":
    "Erweitertes 5-Iterationen-Protokoll mit langen Aktionsphasen für Forschung oder klinische Baseline.",
  "calibration.preset.stress": "Stress / Angst",
  "calibration.preset.stressDesc":
    "Ruhige Entspannung vs. leichter kognitiver Stressor. Für Angst- und Stress-Tracking.",
  "calibration.moveUp": "Bewegen Sie sich nach oben",
  "calibration.moveDown": "Bewegen Sie sich nach unten",
  "calibration.removeAction": "Aktion entfernen",
};

export default calibration;
