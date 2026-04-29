// SPDX-License-Identifier: GPL-3.0-only
/** DE — "validation" namespace. */
const validation: Record<string, string> = {
  "settingsTabs.validation": "Validierung",
  "validation.title": "Validierung & Forschung",
  "validation.intro":
    "Opt-in-Forschungsinstrumente, die den Pause-Coach und Fokus-Score gegen externe Maße kalibrieren. Keine ist erforderlich, um NeuroSkill zu verwenden.",
  "validation.disclaimer":
    "Nur Forschungswerkzeug — kein Medizinprodukt. Nicht von FDA, CE oder einer Aufsichtsbehörde zugelassen. Nicht für den klinischen Gebrauch.",

  "validation.master.title": "Globale Sperren",
  "validation.master.respectFlow": "Flow-Zustand respektieren",
  "validation.master.respectFlowDesc":
    "Wenn du in den Flow kommst, werden alle Eingabeaufforderungen unterdrückt. Standardmäßig aktiviert — lass es an.",
  "validation.master.quietBefore": "Ruhezeiten Beginn",
  "validation.master.quietAfter": "Ruhezeiten Ende",
  "validation.master.quietDesc":
    "Lokale Zeit. Außerhalb dieses Fensters keine Eingabeaufforderungen. Beginn = Ende deaktiviert Ruhezeiten.",

  "validation.kss.title": "Karolinska-Schläfrigkeitsskala (KSS)",
  "validation.kss.desc":
    "5-Sekunden-Selbstauskunft (1–9) zur momentanen Schläfrigkeit. Kalibriert den Pause-Coach gegen subjektives Empfinden.",
  "validation.kss.enabled": "KSS-Eingabeaufforderungen aktivieren",
  "validation.kss.maxPerDay": "Max. Eingabeaufforderungen pro Tag",
  "validation.kss.minInterval": "Min. Minuten zwischen Eingabeaufforderungen",
  "validation.kss.triggerBreakCoach": "Auslösen, wenn Pause-Coach Müdigkeit erkennt",
  "validation.kss.triggerRandom": "Gelegentliche zufällige Kontrollproben",
  "validation.kss.triggerRandomDesc": "Erforderlich für ROC/AUC — ohne Negativfälle sehen wir nur müde Zustände.",
  "validation.kss.randomWeight": "Gewicht zufälliger Proben (0–1)",

  "validation.tlx.title": "NASA-TLX (Arbeitsbelastung, 6 Skalen)",
  "validation.tlx.desc":
    "60-Sekunden-Selbstauskunft mit 6 Subskalen nach einer Arbeitseinheit. Misst Belastung — komplementär zur KSS-Schläfrigkeit.",
  "validation.tlx.enabled": "NASA-TLX-Eingabeaufforderungen aktivieren",
  "validation.tlx.maxPerDay": "Max. Eingabeaufforderungen pro Tag",
  "validation.tlx.minTaskMin": "Mindestaufgabenlänge (Min) zur Abfrage",
  "validation.tlx.endOfDay": "Tagesabschluss-Übersicht zur Belastung",

  "validation.tlx.form.title": "Bewerte die gerade beendete Aufgabe",
  "validation.tlx.form.subtitle": "Jede Skala 0–100. Schiebe den Regler auf die passende Position.",
  "validation.tlx.mental": "Geistige Anforderung",
  "validation.tlx.physical": "Körperliche Anforderung",
  "validation.tlx.temporal": "Zeitliche Anforderung",
  "validation.tlx.performance": "Leistung",
  "validation.tlx.effort": "Anstrengung",
  "validation.tlx.frustration": "Frustration",

  "validation.pvt.title": "Psychomotor Vigilance Task (PVT)",
  "validation.pvt.desc":
    "3-Minuten-Reaktionszeitaufgabe. Das objektive Vigilanzmaß — langsam zu erfassen, aber das stärkste Signal in der Literatur.",
  "validation.pvt.enabled": "Wöchentliche PVT-Erinnerungen",
  "validation.pvt.weeklyReminder": "Hinweis anzeigen, wenn diese Woche kein PVT lief",
  "validation.pvt.runNow": "PVT jetzt starten (3 Min)",

  "validation.pvt.task.title": "Psychomotor Vigilance Task",
  "validation.pvt.task.intro":
    "Wenn der Punkt erscheint, klicke (oder drücke eine beliebige Taste) so schnell wie möglich. Dauer: 3 Minuten.",
  "validation.pvt.task.start": "Starten",
  "validation.pvt.task.cancel": "Abbrechen",
  "validation.pvt.task.go": "Klicken / Taste drücken",
  "validation.pvt.task.wait": "Warten…",
  "validation.pvt.task.tooFast": "Zu schnell — warte auf den Punkt.",
  "validation.pvt.task.results": "PVT abgeschlossen",
  "validation.pvt.task.close": "Schließen",

  "validation.eeg.title": "EEG-Müdigkeitsindex (Jap et al. 2009)",
  "validation.eeg.desc":
    "Wird kontinuierlich aus dem Bandleistungsstrom berechnet, wenn ein NeuroSkill-Headset angeschlossen ist. Formel: (α + θ) / β. Passiv — kostenlos.",
  "validation.eeg.enabled": "EEG-Müdigkeitsindex berechnen",
  "validation.eeg.windowSecs": "Rollendes Fenster (Sekunden)",
  "validation.eeg.current": "Aktueller Wert",
  "validation.eeg.noHeadset": "Kein EEG-Headset gestreamt",

  "validation.calibrationWeek.title": "Kalibrierungswoche",
  "validation.calibrationWeek.desc":
    "Opt-in 7-Tage-Burst mit höherer Abtastrate. Erhöht KSS auf 8/Tag, fragt TLX nach jedem Flow-Block ≥ 20 Min, fordert ein PVT in der Wochenmitte. Setzt sich am 8. Tag automatisch zurück.",
  "validation.calibrationWeek.start": "Kalibrierungswoche starten",
  "validation.calibrationWeek.cancel": "Kalibrierung abbrechen",

  "validation.results.title": "Aktuelle Ergebnisse",
  "validation.results.kssCount": "{0} KSS-Antworten (letzte 7 Tage)",
  "validation.results.tlxCount": "{0} TLX-Antworten (letzte 7 Tage)",
  "validation.results.pvtCount": "{0} PVT-Läufe (letzte 7 Tage)",
  "validation.results.empty": "Noch keine Daten — opt-in oben, Eingabeaufforderungen erscheinen.",

  "validation.references.title": "Referenzen",
  "validation.save.saving": "Speichern…",
  "validation.save.saved": "Gespeichert",
  "validation.save.failed": "Speichern fehlgeschlagen: {0}",
};
export default validation;
