// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** FR "onboarding" namespace translations. */
const onboarding: Record<string, string> = {
  "onboarding.title": "Bienvenue dans {app}",
  "onboarding.step.welcome": "Bienvenue",
  "onboarding.step.bluetooth": "Bluetooth",
  "onboarding.step.fit": "Ajustement",
  "onboarding.step.calibration": "Calibration",
  "onboarding.step.models": "Modèles",
  "onboarding.step.tray": "Icône",
  "onboarding.step.permissions": "Permissions",
  "onboarding.step.extensions": "Extensions",
  "onboarding.step.enable_bluetooth": "Activer Bluetooth",
  "onboarding.step.done": "Terminé",
  "onboarding.newBadge": "Nouveau",
  "onboarding.fontSizeLabel": "Taille du texte",
  "onboarding.fontSizeDecrease": "Réduire la taille du texte",
  "onboarding.fontSizeIncrease": "Augmenter la taille du texte",
  "onboarding.welcomeBackTitle": "Bon retour dans {app}",
  "onboarding.whatsNewTitle": "Nouveautés depuis votre dernière configuration",
  "onboarding.whatsNewBody":
    "Quelques nouvelles étapes ont été ajoutées depuis votre dernière exécution de cet assistant. Votre configuration existante (Bluetooth, calibration, modèles) est inchangée — vous pouvez la parcourir rapidement. Les nouvelles étapes sont signalées ici et marquées « NOUVEAU » dans la barre de progression :",
  "onboarding.trayHint": "Trouvez l'icône de l'app dans la barre de menus / la zone de notification",
  "onboarding.permissionsHint": "Optionnel : autoriser la capture de l'app active, des fichiers et du presse-papiers",
  "onboarding.extensionsHint": "Optionnel : installer les helpers VS Code, navigateur et shell",
  "onboarding.welcomeTitle": "Bienvenue dans {app}",
  "onboarding.welcomeBody":
    "{app} enregistre, analyse et indexe vos données EEG depuis tout appareil BCI pris en charge. Configurons tout en quelques étapes rapides.",
  "onboarding.bluetoothHint": "Connecter votre appareil BCI",
  "onboarding.fitHint": "Vérifier la qualité du contact des capteurs",
  "onboarding.calibrationHint": "Lancer une session de calibration rapide",
  "onboarding.modelsHint": "Télécharger les modèles d'IA locaux recommandés",
  "onboarding.bluetoothTitle": "Connecter votre appareil BCI",
  "onboarding.bluetoothBody":
    "Allumez votre appareil BCI et portez-le. {app} recherchera automatiquement les appareils à proximité.",
  "onboarding.btConnected": "Connecté à {name}",
  "onboarding.btScanning": "Recherche...",
  "onboarding.btReady": "Prêt à scanner",
  "onboarding.btScan": "Scanner",
  "onboarding.btInstructions": "Comment se connecter",
  "onboarding.btStep1":
    "Allumez votre appareil BCI (maintenez le bouton, basculez l'interrupteur ou appuyez sur le bouton selon votre casque).",
  "onboarding.btStep2":
    "Placez le casque sur votre tête - les capteurs doivent reposer derrière vos oreilles et sur votre front.",
  "onboarding.btStep3":
    "Cliquez sur Scanner ci-dessus. {app} trouvera et se connectera automatiquement à l'appareil le plus proche.",
  "onboarding.btSuccess": "Casque connecté ! Vous pouvez continuer.",
  "onboarding.fitTitle": "Vérifier l'ajustement",
  "onboarding.fitBody":
    "Un bon contact des capteurs est essentiel pour des données EEG propres. Les quatre capteurs doivent être verts ou jaunes.",
  "onboarding.sensorQuality": "Qualité des capteurs en direct",
  "onboarding.quality.good": "Bon",
  "onboarding.quality.fair": "Moyen",
  "onboarding.quality.poor": "Faible",
  "onboarding.quality.no_signal": "Pas de signal",
  "onboarding.fitNeedsBt": "Connectez d'abord votre casque pour voir les données en direct.",
  "onboarding.fitTips": "Conseils pour un meilleur contact",
  "onboarding.fitTip1":
    "Capteurs d'oreille (TP9/TP10) : placez-les derrière et légèrement au-dessus des oreilles. Écartez les cheveux.",
  "onboarding.fitTip2":
    "Capteurs frontaux (AF7/AF8) : doivent reposer à plat sur une peau propre - essuyez avec un chiffon sec si nécessaire.",
  "onboarding.fitTip3":
    "En cas de mauvais contact, humidifiez légèrement les capteurs avec un doigt mouillé pour améliorer la conductivité.",
  "onboarding.fitGood": "Parfait ! Tous les capteurs ont un bon contact.",
  "onboarding.calibrationTitle": "Lancer la calibration",
  "onboarding.calibrationBody":
    "La calibration enregistre l'EEG étiqueté pendant que vous alternez entre deux états mentaux. Cela aide {app} à apprendre vos schémas cérébraux de base.",
  "onboarding.openCalibration": "Ouvrir la calibration",
  "onboarding.calibrationNeedsBt": "Connectez d'abord votre casque pour lancer la calibration.",
  "onboarding.calibrationSkip":
    "Vous pouvez sauter cette étape et calibrer plus tard depuis le menu ou les paramètres.",
  "onboarding.enableBluetoothTitle": "Activez le Bluetooth sur votre Mac",
  "onboarding.enableBluetoothBody":
    "{app} a besoin que l'adaptateur Bluetooth de votre Mac soit activé pour trouver et connecter votre appareil BCI. Veuillez activer Bluetooth dans les Réglages si il est désactivé.",
  "onboarding.enableBluetoothStatus": "Adaptateur Bluetooth",
  "onboarding.enableBluetoothHint":
    "Ouvrez Réglages → Bluetooth et activez Bluetooth. Si vous exécutez en développement via le Terminal, assurez-vous que l'adaptateur système est activé.",
  "onboarding.enableBluetoothOpen": "Ouvrir les réglages Bluetooth",
  "onboarding.modelsTitle": "Télécharger les modèles recommandés",
  "onboarding.modelsBody":
    "Pour la meilleure expérience locale, téléchargez maintenant ces modèles par défaut : Qwen3.5 4B (Q4_K_M), encodeur ZUNA, NeuTTS et Kitten TTS.",
  "onboarding.models.downloadAll": "Télécharger le lot recommandé",
  "onboarding.models.download": "Télécharger",
  "onboarding.models.downloading": "Téléchargement...",
  "onboarding.models.downloaded": "Téléchargé",
  "onboarding.models.qwenTitle": "Qwen3.5 4B (Q4_K_M)",
  "onboarding.models.qwenDesc":
    "Modèle de chat recommandé. Utilise Q4_K_M pour le meilleur équilibre qualité/vitesse sur la plupart des ordinateurs portables.",
  "onboarding.models.zunaTitle": "Encodeur EEG ZUNA",
  "onboarding.models.zunaDesc":
    "Nécessaire pour les embeddings EEG, l'historique sémantique et l'analyse en aval des états cérébraux.",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc":
    "Moteur vocal multilingue recommandé avec une meilleure qualité et prise en charge du clonage.",
  "onboarding.models.kittenTitle": "Kitten TTS",
  "onboarding.models.kittenDesc":
    "Backend vocal léger et rapide, utile comme solution de repli et pour les systèmes à faibles ressources.",
  "onboarding.trayTitle": "Retrouver l'app dans la barre",
  "onboarding.trayBody":
    "{app} fonctionne en arrière-plan. Après l'installation, l'icône dans la barre de menu (macOS) ou la zone de notification (Windows/Linux) est votre point d'accès.",
  "onboarding.tray.states": "L'icône change de couleur selon l'état :",
  "onboarding.tray.grey": "Gris - déconnecté",
  "onboarding.tray.amber": "Ambre - recherche ou connexion",
  "onboarding.tray.green": "Vert - connecté et enregistrement",
  "onboarding.tray.red": "Rouge - Bluetooth désactivé",
  "onboarding.tray.open": "Cliquez sur l'icône à tout moment pour afficher ou masquer le tableau de bord.",
  "onboarding.tray.menu":
    "Clic droit (ou clic gauche sous Windows/Linux) pour les actions rapides - connecter, étiqueter, calibrer, et plus.",
  "onboarding.extensionsTitle": "Extensions complémentaires",
  "onboarding.extensionsBody":
    "{app} peut récupérer du contexte supplémentaire depuis votre éditeur, navigateur et terminal. Chaque intégration est indépendante : vous pouvez l'installer ou l'ignorer séparément, aucune n'est requise pour le fonctionnement EEG.",
  "onboarding.extensionsPrivacy":
    "Même garantie de confidentialité que pour le reste : chaque extension communique avec le démon local via un port localhost, et ces données sont écrites dans activity.sqlite sur cet ordinateur. Rien n'est envoyé à NeuroSkill ni à qui que ce soit d'autre.",
  "onboarding.extensionsSkip":
    "Tout est facultatif. Vous pouvez installer, mettre à jour ou supprimer chacune de ces options plus tard dans Réglages → Extensions et Réglages → Terminal.",
  "onboarding.extensions.vscodeTitle": "Éditeur basé sur VS Code",
  "onboarding.extensions.vscodeDesc":
    "Ajoute le suivi d'édition par fichier, les suggestions IA en ligne et l'intégration avec la boucle de développement. Fonctionne avec VS Code, VSCodium, Cursor, Windsurf, Trae, Positron — tout fork installé est détecté automatiquement.",
  "onboarding.extensions.browserTitle": "Extension de navigateur",
  "onboarding.extensions.browserDesc":
    "Enregistre l'onglet actif, le temps de focus et les habitudes de lecture du navigateur. Sideload pris en charge pour Chrome, Firefox, Edge et Safari (Safari nécessite une étape de signature supplémentaire).",
  "onboarding.extensions.terminalTitle": "Hooks terminal / shell",
  "onboarding.extensions.terminalDesc":
    "Ajoute un petit hook preexec/precmd à votre shell pour que l'app puisse corréler le moment des commandes avec votre état de concentration. Choisissez zsh, bash, fish ou PowerShell — modifie votre fichier rc avec une seule ligne source, entièrement réversible plus tard.",

  "onboarding.permissionsTitle": "Suivi d'activité facultatif",
  "onboarding.permissionsBody":
    '{app} peut enregistrer ce sur quoi vous travailliez afin de corréler vos données EEG/concentration avec le contexte réel — "j\'ai perdu le focus en écrivant cette PR" plutôt que simplement "j\'ai perdu le focus à 15 h". Désactivé par défaut et entièrement facultatif.',
  "onboarding.permissionsPrivacy":
    "Tout reste sur cet ordinateur. L'activité enregistrée est écrite dans un fichier local activity.sqlite et n'est jamais envoyée à un serveur — ni à NeuroSkill, ni à personne. Vous pouvez désactiver chaque option à tout moment ; les données enregistrées restent sur le disque jusqu'à ce que vous les supprimiez.",
  "onboarding.permissionsSkip":
    "Tout désactivé par défaut. Vous pouvez activer chacune de ces options plus tard dans Réglages → Suivi d'activité.",
  "onboarding.permissionsActiveWindowDesc":
    "Capture l'application au premier plan, le titre de la fenêtre, l'onglet de navigateur actif et le chemin du fichier ouvert dans l'éditeur. macOS demandera un accès Accessibilité / Automatisation pour chaque navigateur et éditeur.",
  "onboarding.permissionsInputDesc":
    "N'enregistre que les horodatages d'utilisation du clavier/souris — jamais quelles touches, jamais les positions, jamais le contenu. Aucune permission OS requise.",
  "onboarding.permissionsFileDesc":
    "Surveille Documents, Bureau, Téléchargements et les dossiers de développement habituels pour les événements création/modification/suppression. N'enregistre que chemins et horodatages — le contenu des fichiers n'est jamais lu. macOS peut demander un Accès complet au disque.",
  "onboarding.permissionsScreenshotsDesc":
    "Capture l'écran à intervalle régulier, applique l'OCR au texte et indexe le tout pour la recherche visuelle et les requêtes type « qu'y avait-il à l'écran à 15 h ? ». macOS demande l'Enregistrement de l'écran. Réglez l'intervalle, la qualité et l'OCR dans Réglages → Captures.",
  "onboarding.permissionsLocationDesc":
    "Enregistre la localisation de l'appareil avec vos blocs de concentration (maison vs bureau vs café) pour corréler les changements de lieu avec votre état de focus. macOS demande les Services de localisation. Stocké localement ; jamais transmis.",
  "onboarding.permissionsCalendarDesc":
    "Lit les métadonnées des événements du calendrier (titre, heure, durée, nombre de participants) pour corréler la densité de réunions avec les baisses de concentration. macOS demande l'Accès au Calendrier à la première utilisation. Le contenu des événements n'est jamais transmis.",
  "onboarding.permissionsClipboardDesc":
    "Enregistre les changements du presse-papiers (quelle app, type de contenu, taille). Le contenu n'est jamais lu. macOS uniquement ; demandera un accès Automatisation.",
  "onboarding.downloadsComplete": "Tous les téléchargements terminés !",
  "onboarding.downloadsCompleteBody":
    "Les modèles recommandés sont téléchargés et prêts à l'emploi. Pour télécharger d'autres modèles ou en utiliser d'autres, ouvrez",
  "onboarding.downloadMoreSettings": "les paramètres de l'application",
  "onboarding.doneTitle": "Tout est prêt !",
  "onboarding.doneBody": "{app} fonctionne dans votre barre de menus. Voici quelques informations utiles :",
  "onboarding.doneTip.tray":
    "{app} réside dans le tray de votre barre de menus. Cliquez sur l'icône pour afficher/masquer le tableau de bord.",
  "onboarding.doneTip.shortcuts":
    "Utilisez ⌘K pour ouvrir la palette de commandes, ou ? pour voir tous les raccourcis clavier.",
  "onboarding.doneTip.help": "Ouvrez l'aide depuis le menu pour une référence complète de toutes les fonctionnalités.",
  "onboarding.back": "Retour",
  "onboarding.next": "Suivant",
  "onboarding.getStarted": "Commencer",
  "onboarding.finish": "Terminer",
  "onboarding.models.ocrTitle": "OCR Models",
  "onboarding.models.ocrDesc":
    "Text detection + recognition models for extracting text from screenshots. Enables text search across captured screens (~10 MB each).",
  "onboarding.screenRecTitle": "Autorisation d'enregistrement d'écran",
  "onboarding.screenRecDesc":
    "Requise sur macOS pour capturer les fenêtres d'autres applications pour le système de captures d'écran. Sans cette autorisation, les captures peuvent être vides.",
  "onboarding.screenRecOpen": "Ouvrir les réglages",
};

export default onboarding;
