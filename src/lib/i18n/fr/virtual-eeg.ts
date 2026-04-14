// SPDX-License-Identifier: GPL-3.0-only
/** FR "virtual-eeg" namespace — Simulateur d'appareil EEG virtuel. */
const virtualEeg: Record<string, string> = {
  "settingsTabs.virtualEeg": "EEG Virtuel",

  "veeg.title": "Appareil EEG Virtuel",
  "veeg.desc":
    "Simulez un casque EEG pour les tests, les démonstrations et le développement. Génère des données synthétiques qui traversent l'ensemble du pipeline de traitement du signal.",

  "veeg.status": "État",
  "veeg.running": "En cours",
  "veeg.stopped": "Arrêté",
  "veeg.start": "Démarrer",
  "veeg.stop": "Arrêter",

  "veeg.channels": "Canaux",
  "veeg.channelsDesc": "Nombre d'électrodes EEG à simuler.",
  "veeg.sampleRate": "Fréquence d'échantillonnage (Hz)",
  "veeg.sampleRateDesc": "Échantillons par seconde par canal.",

  "veeg.template": "Modèle de signal",
  "veeg.templateDesc": "Choisissez le type de signal synthétique à générer.",
  "veeg.templateSine": "Ondes sinusoïdales",
  "veeg.templateSineDesc": "Ondes sinusoïdales propres aux bandes de fréquences standard (delta, thêta, alpha, bêta, gamma).",
  "veeg.templateGoodQuality": "EEG de bonne qualité",
  "veeg.templateGoodQualityDesc": "EEG réaliste au repos avec rythme alpha dominant et bruit rose en arrière-plan.",
  "veeg.templateBadQuality": "EEG de mauvaise qualité",
  "veeg.templateBadQualityDesc": "Signal bruité avec artefacts musculaires, bruit de ligne 50/60 Hz et sauts d'électrode.",
  "veeg.templateInterruptions": "Connexion intermittente",
  "veeg.templateInterruptionsDesc":
    "Bon signal avec des interruptions périodiques simulant des électrodes mal fixées ou des interférences sans fil.",
  "veeg.templateFile": "Depuis un fichier",
  "veeg.templateFileDesc": "Relire des échantillons à partir d'un fichier CSV ou EDF.",

  "veeg.quality": "Qualité du signal",
  "veeg.qualityDesc": "Ajustez le rapport signal/bruit. Plus élevé = signal plus propre.",
  "veeg.qualityPoor": "Médiocre",
  "veeg.qualityFair": "Passable",
  "veeg.qualityGood": "Bon",
  "veeg.qualityExcellent": "Excellent",

  "veeg.chooseFile": "Choisir un fichier",
  "veeg.noFile": "Aucun fichier sélectionné",
  "veeg.fileLoaded": "{name} ({channels} canaux, {samples} échantillons)",

  "veeg.advanced": "Avancé",
  "veeg.amplitudeUv": "Amplitude (µV)",
  "veeg.amplitudeDesc": "Amplitude crête à crête des signaux générés.",
  "veeg.noiseUv": "Plancher de bruit (µV)",
  "veeg.noiseDesc": "Amplitude efficace du bruit gaussien additif.",
  "veeg.lineNoise": "Bruit de ligne",
  "veeg.lineNoiseDesc": "Ajouter une interférence secteur de 50 Hz ou 60 Hz.",
  "veeg.lineNoise50": "50 Hz",
  "veeg.lineNoise60": "60 Hz",
  "veeg.lineNoiseNone": "Aucun",
  "veeg.dropoutProb": "Probabilité d'interruption",
  "veeg.dropoutDesc": "Probabilité de perte de signal par seconde (0 = aucune, 1 = constante).",

  "veeg.preview": "Aperçu du signal",
  "veeg.previewDesc": "Aperçu en temps réel des 4 premiers canaux.",

  // ── Fenêtre des appareils virtuels ────────────────────────────────────────────
  "window.title.virtualDevices": "{app} – Appareils Virtuels",

  "vdev.title": "Appareils Virtuels",
  "vdev.desc":
    "Testez NeuroSkill sans matériel EEG physique. Choisissez un modèle correspondant à un appareil réel ou configurez votre propre source de signal synthétique.",

  "vdev.presets": "Modèles d'appareils",
  "vdev.statusRunning": "Appareil virtuel en cours de diffusion",
  "vdev.statusStopped": "Aucun appareil virtuel actif",
  "vdev.selected": "Prêt",
  "vdev.configure": "Configurer",
  "vdev.customConfig": "Configuration personnalisée",

  "vdev.presetMuse": "Muse S",
  "vdev.presetMuseDesc": "Disposition bandeau 4 canaux — TP9, AF7, AF8, TP10.",
  "vdev.presetCyton": "OpenBCI Cyton",
  "vdev.presetCytonDesc": "Signal de recherche 8 canaux, montage frontal/central complet.",
  "vdev.presetCap32": "Bonnet EEG 32 canaux",
  "vdev.presetCap32Desc": "Système international 10-20 complet, 32 électrodes.",
  "vdev.presetAlpha": "Alpha intense",
  "vdev.presetAlphaDesc": "Rythme alpha prononcé à 10 Hz — ligne de base détendue yeux fermés.",
  "vdev.presetArtifact": "Test d'artefacts",
  "vdev.presetArtifactDesc": "Signal bruité avec artefacts musculaires et bruit de ligne 50 Hz.",
  "vdev.presetDropout": "Test d'interruptions",
  "vdev.presetDropoutDesc": "Perte de signal périodique simulant des électrodes mal fixées.",
  "vdev.presetMinimal": "Minimal (1 canal)",
  "vdev.presetMinimalDesc": "Onde sinusoïdale monocanal — charge la plus légère possible.",
  "vdev.presetCustom": "Personnalisé",
  "vdev.presetCustomDesc": "Définissez votre propre nombre de canaux, fréquence, modèle et niveau de bruit.",

  "vdev.lslSourceTitle": "Source LSL virtuelle",
  "vdev.lslRunning": "Diffusion d'EEG synthétique via LSL",
  "vdev.lslStopped": "Source LSL virtuelle arrêtée",
  "vdev.lslDesc": "Démarre une source locale Lab Streaming Layer pour tester la découverte et la connexion de flux LSL.",
  "vdev.lslHint":
    'Ouvrez Paramètres → onglet LSL et cliquez sur « Scanner le réseau » pour voir SkillVirtualEEG dans la liste des flux, puis connectez-vous.',
  "vdev.lslStarted": "La source LSL virtuelle diffuse maintenant sur le réseau local.",

  // Panneau d'état
  "vdev.statusSource": "Source LSL",
  "vdev.statusSession": "Session",
  "vdev.sessionConnected": "Connecté",
  "vdev.sessionConnecting": "Connexion en cours…",
  "vdev.sessionDisconnected": "Déconnecté",
  "vdev.startBtn": "Démarrer l'appareil virtuel",
  "vdev.stopBtn": "Arrêter l'appareil virtuel",
  "vdev.autoConnect": "Connexion automatique au tableau de bord",
  "vdev.autoConnectDesc": "Connecter le tableau de bord à cette source immédiatement après le démarrage.",

  // Aperçu
  "vdev.previewOffline": "Aperçu du signal (hors ligne)",
  "vdev.previewOfflineDesc":
    "Aperçu de la forme d'onde côté client — montre la forme du signal avant la connexion. Aucune donnée n'est encore diffusée.",

  // Modèle personnalisé — canaux / fréquence
  "vdev.cfgChannels": "Canaux",
  "vdev.cfgChannelsDesc": "Nombre d'électrodes EEG à simuler.",
  "vdev.cfgRate": "Fréquence d'échantillonnage",
  "vdev.cfgRateDesc": "Échantillons par seconde par canal.",

  // Modèle personnalisé — qualité du signal
  "vdev.cfgQuality": "Qualité du signal",
  "vdev.cfgQualityDesc": "Rapport signal/bruit. Plus élevé = signal plus propre.",

  // Modèle personnalisé — modèle
  "vdev.cfgTemplate": "Modèle de signal",
  "vdev.cfgTemplateSine": "Ondes sinusoïdales",
  "vdev.cfgTemplateSineDesc": "Ondes sinusoïdales pures aux fréquences delta, thêta, alpha, bêta et gamma.",
  "vdev.cfgTemplateGood": "EEG de bonne qualité",
  "vdev.cfgTemplateGoodDesc": "État de repos réaliste avec alpha dominant et bruit rose en arrière-plan.",
  "vdev.cfgTemplateBad": "EEG de mauvaise qualité",
  "vdev.cfgTemplateBadDesc": "Signal bruité avec artefacts musculaires, bruit de ligne et sauts d'électrode.",
  "vdev.cfgTemplateInterruptions": "Connexion intermittente",
  "vdev.cfgTemplateInterruptionsDesc": "Bon signal avec des interruptions périodiques simulant des électrodes mal fixées.",

  // Modèle personnalisé — avancé
  "vdev.cfgAdvanced": "Avancé",
  "vdev.cfgAmplitude": "Amplitude (µV)",
  "vdev.cfgAmplitudeDesc": "Amplitude crête à crête du signal simulé.",
  "vdev.cfgNoise": "Plancher de bruit (µV)",
  "vdev.cfgNoiseDesc": "Amplitude efficace du bruit gaussien additif de fond.",
  "vdev.cfgLineNoise": "Bruit de ligne",
  "vdev.cfgLineNoiseDesc": "Injecter une interférence secteur de 50 Hz ou 60 Hz.",
  "vdev.cfgLineNoiseNone": "Aucun",
  "vdev.cfgLineNoise50": "50 Hz",
  "vdev.cfgLineNoise60": "60 Hz",
  "vdev.cfgDropout": "Probabilité d'interruption",
  "vdev.cfgDropoutDesc": "Probabilité de perte de signal par seconde (0 = jamais, 1 = constante).",
};

export default virtualEeg;
