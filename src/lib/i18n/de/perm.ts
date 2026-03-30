// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** DE "perm" namespace translations. */
const perm: Record<string, string> = {
  "perm.intro":
    "{app} benötigt einige optionale Systemberechtigungen, um Funktionen wie Tastatur-/Maus-Aktivitätszeitstempel und Benachrichtigungen zu aktivieren. Alle Daten bleiben auf Ihrem Gerät.",
  "perm.granted": "Erteilt",
  "perm.denied": "Nicht erteilt",
  "perm.unknown": "Unbekannt",
  "perm.notRequired": "Nicht erforderlich",
  "perm.systemManaged": "Vom System verwaltet",
  "perm.accessibility": "Bedienungshilfen",
  "perm.accessibilityDesc":
    'Die Tastatur- und Maus-Aktivitätsverfolgung nutzt einen CGEventTap (macOS), um Zeitstempel der letzten Taste und Mausbewegung zu erfassen. Es werden keine Tastenanschläge oder Cursorpositionen gespeichert – nur Unix-Sekunden-Zeitstempel. Dafür ist auf macOS die Berechtigung "Bedienungshilfen" erforderlich.',
  "perm.accessibilityOk": "Berechtigung erteilt. Tastatur- und Maus-Aktivitätszeitstempel werden erfasst.",
  "perm.accessibilityPending": "Berechtigungsstatus wird geprüft…",
  "perm.howToGrant": "So erteilen Sie diese Berechtigung:",
  "perm.accessStep1": "Klicken Sie unten auf «Bedienungshilfen-Einstellungen öffnen».",
  "perm.accessStep2": "Suchen Sie {app} in der Liste (oder klicken Sie auf +, um die App hinzuzufügen).",
  "perm.accessStep3": "Schalten Sie den Schalter ein.",
  "perm.accessStep4": "Kehren Sie hierher zurück – der Status wird automatisch aktualisiert.",
  "perm.openAccessibilitySettings": "Bedienungshilfen-Einstellungen öffnen",
  "perm.bluetooth": "Bluetooth",
  "perm.bluetoothDesc":
    "Bluetooth wird verwendet, um eine Verbindung zu Ihrem BCI-Headset (Muse, MW75 Neuro, OpenBCI Ganglion, IDUN Guardian usw.) herzustellen. Auf macOS erscheint beim ersten Scan ein einmaliger Berechtigungs-Dialog. Unter Linux und Windows ist keine gesonderte Berechtigung erforderlich.",
  "perm.openBluetoothSettings": "Bluetooth-Einstellungen öffnen",
  "perm.notifications": "Benachrichtigungen",
  "perm.notificationsDesc":
    "Benachrichtigungen werden gesendet, wenn Sie Ihr tägliches Aufnahmeziel erreichen und wenn ein Software-Update verfügbar ist. Auf macOS und Windows erscheint beim ersten Senden einer Benachrichtigung ein Berechtigungs-Dialog.",
  "perm.openNotificationsSettings": "Benachrichtigungs-Einstellungen öffnen",
  "perm.matrix": "Berechtigungsübersicht",
  "perm.feature": "Funktion",
  "perm.matrixBluetooth": "Bluetooth (BCI-Gerät)",
  "perm.matrixKeyboardMouse": "Tastatur- & Maus-Zeitstempel",
  "perm.matrixActiveWindow": "Aktives Fenster verfolgen",
  "perm.matrixNotifications": "Benachrichtigungen",
  "perm.matrixNone": "Keine Berechtigung nötig",
  "perm.matrixAccessibility": "Bedienungshilfen erforderlich",
  "perm.matrixOsPrompt": "Systemdialog beim ersten Start",
  "perm.legendNone": "Keine Berechtigung nötig",
  "perm.legendRequired": "Systemberechtigung erforderlich – ohne sie deaktiviert sich die Funktion lautlos",
  "perm.legendPrompt": "Systemdialog beim ersten Aufruf",
  "perm.why": "Warum benötigt {app} diese Berechtigungen?",
  "perm.whyBluetooth": "Bluetooth",
  "perm.whyBluetoothDesc": "Um Ihr BCI-Headset über BLE zu erkennen und Daten daraus zu streamen.",
  "perm.whyAccessibility": "Bedienungshilfen",
  "perm.whyAccessibilityDesc":
    "Um Tastatur- und Mausereignisse mit einem Zeitstempel zu versehen und so Aktivitätskontext zu liefern. Nur der Zeitpunkt des Ereignisses wird gespeichert – nie was getippt oder wohin der Cursor bewegt wurde.",
  "perm.whyNotifications": "Benachrichtigungen",
  "perm.whyNotificationsDesc":
    "Um Sie zu benachrichtigen, wenn Sie Ihr tägliches Aufnahmeziel erreichen und wenn Updates bereitstehen.",
  "perm.privacyNote":
    "Alle Daten werden lokal auf Ihrem Gerät gespeichert und niemals übertragen. Sie können jede Funktion unter Einstellungen → Aktivitätsverfolgung deaktivieren.",
  "perm.screenRecording": "Bildschirmaufnahme",
  "perm.screenRecordingDesc":
    "Erforderlich, um andere Anwendungsfenster für das Screenshot-Embedding-System aufzuzeichnen. macOS schwärzt Fensterinhalte ohne diese Berechtigung.",
  "perm.screenRecordingOk":
    "Berechtigung für Bildschirmaufnahme wurde erteilt. Screenshot-Erfassung funktioniert korrekt.",
  "perm.screenRecordingStep1":
    "Systemeinstellungen → Datenschutz & Sicherheit → Bildschirm- & Systemtonaufnahme öffnen",
  "perm.screenRecordingStep2": "NeuroSkill™ in der Liste finden und aktivieren",
  "perm.screenRecordingStep3":
    "Möglicherweise müssen Sie die App beenden und neu starten, damit die Änderung wirksam wird",
  "perm.openScreenRecordingSettings": "Bildschirmaufnahme-Einstellungen öffnen",
  "perm.whyScreenRecording": "Bildschirmaufnahme",
  "perm.whyScreenRecordingDesc":
    "Zum Erfassen des aktiven Fensters für visuelle Ähnlichkeitssuche und cross-modale EEG-Korrelation. Nur manuell ausgelöste Screenshots werden gespeichert — keine Daueraufnahme.",
  "perm.matrixScreenRecording": "Screenshot-Erfassung",
  "perm.matrixScreenRecordingReq": "Bildschirmaufnahme erforderlich",
  "perm.calendar": "Kalender",
  "perm.calendarDesc":
    "Kalender-Tools können Termine lesen, um Planungskontext bereitzustellen. Die Berechtigung wird von macOS bei Bedarf angefordert.",
  "perm.requestCalendarPermission": "Kalenderberechtigung anfordern",
  "perm.openCalendarSettings": "Kalender-Datenschutzeinstellungen öffnen",
  "perm.location": "Ortungsdienste",
  "perm.locationDesc":
    "Auf macOS nutzen die Ortungsdienste CoreLocation (GPS/WLAN/Mobilfunk) für hochgenaue Positionierung. Unter Linux und Windows wird IP-basierte Geolokalisierung verwendet, die keine Berechtigung erfordert. Falls Ortungsdienste verweigert oder nicht verfügbar sind, wird automatisch auf IP-Geolokalisierung zurückgegriffen.",
  "perm.locationOk": "Ortungsberechtigung erteilt. CoreLocation wird für hochgenaue Standortbestimmung verwendet.",
  "perm.locationFallback": "Ortung nicht autorisiert — die App verwendet IP-basierte Geolokalisierung (Stadtgenauigkeit).",
  "perm.locationStep1": "Öffne Systemeinstellungen → Datenschutz & Sicherheit → Ortungsdienste",
  "perm.locationStep2": "Finde {app} in der Liste und aktiviere es",
  "perm.locationStep3": "Kehre hierher zurück — der Status aktualisiert sich automatisch",
  "perm.requestLocationPermission": "Ortungsberechtigung anfordern",
  "perm.openLocationSettings": "Ortungsdienste-Einstellungen öffnen",
  "perm.whyLocation": "Standort",
  "perm.whyLocationDesc":
    "Für präzisen Standortkontext im LLM und zur Speicherung von GPS-Daten neben Gesundheitsdaten. Bei Verweigerung wird IP-Geolokalisierung verwendet.",
  "perm.matrixLocation": "Standort (GPS / IP)",
  "perm.matrixLocationReq": "Ortungsdienste (optional — Rückfall auf IP)",
  "perm.openInputMonitoringSettings": "Eingabeüberwachungs-Einstellungen öffnen",
  "perm.openFocusSettings": "Fokus-Einstellungen öffnen",
  "perm.whyCalendar": "Kalender",
  "perm.whyCalendarDesc":
    "Um dem KI-Tool Planungskontext zu liefern, damit es auf Ihre bevorstehenden Termine verweisen kann.",
  "perm.matrixCalendar": "Kalendertermine",
  "perm.matrixCalendarReq": "Kalenderzugriff erforderlich",
};

export default perm;
