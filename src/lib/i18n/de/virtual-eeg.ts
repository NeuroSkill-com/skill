// SPDX-License-Identifier: GPL-3.0-only
/** DE "virtual-eeg" namespace — Virtueller EEG-Gerätesimulator. */
const virtualEeg: Record<string, string> = {
  "settingsTabs.virtualEeg": "Virtuelles EEG",

  "veeg.title": "Virtuelles EEG-Gerät",
  "veeg.desc":
    "Simulieren Sie ein EEG-Headset für Tests, Demos und Entwicklung. Erzeugt synthetische Daten, die die gesamte Signalverarbeitung durchlaufen.",

  "veeg.status": "Status",
  "veeg.running": "Läuft",
  "veeg.stopped": "Gestoppt",
  "veeg.start": "Starten",
  "veeg.stop": "Stoppen",

  "veeg.channels": "Kanäle",
  "veeg.channelsDesc": "Anzahl der zu simulierenden EEG-Elektroden.",
  "veeg.sampleRate": "Abtastrate (Hz)",
  "veeg.sampleRateDesc": "Abtastungen pro Sekunde pro Kanal.",

  "veeg.template": "Signalvorlage",
  "veeg.templateDesc": "Wählen Sie die Art des zu erzeugenden synthetischen Signals.",
  "veeg.templateSine": "Sinuswellen",
  "veeg.templateSineDesc": "Saubere Sinuswellen in Standard-Frequenzbändern (Delta, Theta, Alpha, Beta, Gamma).",
  "veeg.templateGoodQuality": "Gute EEG-Qualität",
  "veeg.templateGoodQualityDesc": "Realistisches Ruhe-EEG mit dominantem Alpha-Rhythmus und rosa Rauschen im Hintergrund.",
  "veeg.templateBadQuality": "Schlechte EEG-Qualität",
  "veeg.templateBadQualityDesc": "Verrauschtes Signal mit Muskelartefakten, 50/60-Hz-Netzbrummen und Elektrodenknacken.",
  "veeg.templateInterruptions": "Unterbrochene Verbindung",
  "veeg.templateInterruptionsDesc":
    "Gutes Signal mit periodischen Aussetzern zur Simulation lockerer Elektroden oder Funkstörungen.",
  "veeg.templateFile": "Aus Datei",
  "veeg.templateFileDesc": "Abtastwerte aus einer CSV- oder EDF-Datei wiedergeben.",

  "veeg.quality": "Signalqualität",
  "veeg.qualityDesc": "Signal-Rausch-Verhältnis anpassen. Höher = saubereres Signal.",
  "veeg.qualityPoor": "Schlecht",
  "veeg.qualityFair": "Mäßig",
  "veeg.qualityGood": "Gut",
  "veeg.qualityExcellent": "Ausgezeichnet",

  "veeg.chooseFile": "Datei wählen",
  "veeg.noFile": "Keine Datei ausgewählt",
  "veeg.fileLoaded": "{name} ({channels} Kanäle, {samples} Abtastungen)",

  "veeg.advanced": "Erweitert",
  "veeg.amplitudeUv": "Amplitude (µV)",
  "veeg.amplitudeDesc": "Spitze-zu-Spitze-Amplitude der erzeugten Signale.",
  "veeg.noiseUv": "Grundrauschen (µV)",
  "veeg.noiseDesc": "Effektivwert des additiven Gaußschen Rauschens.",
  "veeg.lineNoise": "Netzbrummen",
  "veeg.lineNoiseDesc": "50-Hz- oder 60-Hz-Netzstörungen hinzufügen.",
  "veeg.lineNoise50": "50 Hz",
  "veeg.lineNoise60": "60 Hz",
  "veeg.lineNoiseNone": "Keines",
  "veeg.dropoutProb": "Ausfallwahrscheinlichkeit",
  "veeg.dropoutDesc": "Wahrscheinlichkeit eines Signalausfalls pro Sekunde (0 = keiner, 1 = dauerhaft).",

  "veeg.preview": "Signalvorschau",
  "veeg.previewDesc": "Live-Vorschau der ersten 4 Kanäle.",

  // ── Virtuelle Geräte — Fenster ────────────────────────────────────────────────
  "window.title.virtualDevices": "{app} – Virtuelle Geräte",

  "vdev.title": "Virtuelle Geräte",
  "vdev.desc":
    "Testen Sie NeuroSkill ohne physische EEG-Hardware. Wählen Sie eine Vorlage, die einem realen Gerät entspricht, oder konfigurieren Sie Ihre eigene synthetische Signalquelle.",

  "vdev.presets": "Gerätevorlagen",
  "vdev.statusRunning": "Virtuelles Gerät streamt",
  "vdev.statusStopped": "Kein virtuelles Gerät aktiv",
  "vdev.selected": "Bereit",
  "vdev.configure": "Konfigurieren",
  "vdev.customConfig": "Benutzerdefinierte Konfiguration",

  "vdev.presetMuse": "Muse S",
  "vdev.presetMuseDesc": "4-Kanal-Stirnband-Layout — TP9, AF7, AF8, TP10.",
  "vdev.presetCyton": "OpenBCI Cyton",
  "vdev.presetCytonDesc": "8-Kanal-Forschungssignal, vollständige frontale/zentrale Montage.",
  "vdev.presetCap32": "32-Kanal-EEG-Kappe",
  "vdev.presetCap32Desc": "Vollständiges internationales 10-20-System, 32 Elektroden.",
  "vdev.presetAlpha": "Starkes Alpha",
  "vdev.presetAlphaDesc": "Ausgeprägter 10-Hz-Alpha-Rhythmus — entspannte Augen-geschlossen-Baseline.",
  "vdev.presetArtifact": "Artefakt-Test",
  "vdev.presetArtifactDesc": "Verrauschtes Signal mit Muskelartefakten und 50-Hz-Netzbrummen.",
  "vdev.presetDropout": "Ausfall-Test",
  "vdev.presetDropoutDesc": "Periodischer Signalverlust zur Simulation lockerer Elektroden.",
  "vdev.presetMinimal": "Minimal (1 Kanal)",
  "vdev.presetMinimalDesc": "Einkanal-Sinuswelle — geringstmögliche Last.",
  "vdev.presetCustom": "Benutzerdefiniert",
  "vdev.presetCustomDesc": "Definieren Sie eigene Kanalanzahl, Abtastrate, Vorlage und Rauschpegel.",

  "vdev.lslSourceTitle": "Virtuelle LSL-Quelle",
  "vdev.lslRunning": "Synthetisches EEG wird über LSL gestreamt",
  "vdev.lslStopped": "Virtuelle LSL-Quelle gestoppt",
  "vdev.lslDesc": "Startet eine lokale Lab-Streaming-Layer-Quelle, um die LSL-Stream-Erkennung und -Verbindung zu testen.",
  "vdev.lslHint":
    'Öffnen Sie Einstellungen → LSL-Tab und klicken Sie auf „Netzwerk scannen", um SkillVirtualEEG in der Stream-Liste zu sehen, und verbinden Sie sich dann damit.',
  "vdev.lslStarted": "Die virtuelle LSL-Quelle streamt jetzt im lokalen Netzwerk.",

  // Status-Panel
  "vdev.statusSource": "LSL-Quelle",
  "vdev.statusSession": "Sitzung",
  "vdev.sessionConnected": "Verbunden",
  "vdev.sessionConnecting": "Verbindung wird hergestellt …",
  "vdev.sessionDisconnected": "Getrennt",
  "vdev.startBtn": "Virtuelles Gerät starten",
  "vdev.stopBtn": "Virtuelles Gerät stoppen",
  "vdev.autoConnect": "Automatisch mit Dashboard verbinden",
  "vdev.autoConnectDesc": "Dashboard sofort nach dem Start mit dieser Quelle verbinden.",

  // Vorschau
  "vdev.previewOffline": "Signalvorschau (offline)",
  "vdev.previewOfflineDesc":
    "Clientseitige Wellenformvorschau — zeigt die Signalform vor dem Verbinden. Es werden noch keine Daten gestreamt.",

  // Benutzerdefinierte Vorlage — Kanäle / Abtastrate
  "vdev.cfgChannels": "Kanäle",
  "vdev.cfgChannelsDesc": "Anzahl der zu simulierenden EEG-Elektroden.",
  "vdev.cfgRate": "Abtastrate",
  "vdev.cfgRateDesc": "Abtastungen pro Sekunde pro Kanal.",

  // Benutzerdefinierte Vorlage — Signalqualität
  "vdev.cfgQuality": "Signalqualität",
  "vdev.cfgQualityDesc": "Signal-Rausch-Verhältnis. Höher = saubereres Signal.",

  // Benutzerdefinierte Vorlage — Vorlage
  "vdev.cfgTemplate": "Signalvorlage",
  "vdev.cfgTemplateSine": "Sinuswellen",
  "vdev.cfgTemplateSineDesc": "Reine Sinuswellen bei Delta-, Theta-, Alpha-, Beta- und Gamma-Frequenzen.",
  "vdev.cfgTemplateGood": "Gute EEG-Qualität",
  "vdev.cfgTemplateGoodDesc": "Realistischer Ruhezustand mit dominantem Alpha und rosa Rauschen im Hintergrund.",
  "vdev.cfgTemplateBad": "Schlechte EEG-Qualität",
  "vdev.cfgTemplateBadDesc": "Verrauschtes Signal mit Muskelartefakten, Netzbrummen und Elektrodenknacken.",
  "vdev.cfgTemplateInterruptions": "Unterbrochene Verbindung",
  "vdev.cfgTemplateInterruptionsDesc": "Gutes Signal mit periodischen Aussetzern zur Simulation lockerer Elektroden.",

  // Benutzerdefinierte Vorlage — Erweitert
  "vdev.cfgAdvanced": "Erweitert",
  "vdev.cfgAmplitude": "Amplitude (µV)",
  "vdev.cfgAmplitudeDesc": "Spitze-zu-Spitze-Amplitude des simulierten Signals.",
  "vdev.cfgNoise": "Grundrauschen (µV)",
  "vdev.cfgNoiseDesc": "Effektivwert des additiven Gaußschen Hintergrundrauschens.",
  "vdev.cfgLineNoise": "Netzbrummen",
  "vdev.cfgLineNoiseDesc": "50-Hz- oder 60-Hz-Netzstörungen einspeisen.",
  "vdev.cfgLineNoiseNone": "Keines",
  "vdev.cfgLineNoise50": "50 Hz",
  "vdev.cfgLineNoise60": "60 Hz",
  "vdev.cfgDropout": "Ausfallwahrscheinlichkeit",
  "vdev.cfgDropoutDesc": "Wahrscheinlichkeit eines Signalausfalls pro Sekunde (0 = nie, 1 = dauerhaft).",
};

export default virtualEeg;
