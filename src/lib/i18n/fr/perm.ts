// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** FR "perm" namespace translations. */
const perm: Record<string, string> = {
  "perm.intro":
    "{app} utilise quelques autorisations système optionnelles pour activer des fonctionnalités telles que les horodatages d'activité clavier/souris et les notifications. Toutes les données restent sur votre appareil.",
  "perm.granted": "Accordée",
  "perm.denied": "Non accordée",
  "perm.unknown": "Inconnue",
  "perm.notRequired": "Non requise",
  "perm.systemManaged": "Gérée par le système",
  "perm.accessibility": "Accessibilité",
  "perm.accessibilityDesc":
    "Le suivi de l'activité clavier et souris utilise un CGEventTap (macOS) pour enregistrer les horodatages du dernier appui de touche et du dernier mouvement de souris. Aucune frappe ni position du curseur n'est stockée - uniquement des horodatages Unix. Cela nécessite l'autorisation Accessibilité sur macOS.",
  "perm.accessibilityOk": "Autorisation accordée. Les horodatages d'activité clavier et souris sont enregistrés.",
  "perm.accessibilityPending": "Vérification du statut de l'autorisation...",
  "perm.howToGrant": "Comment accorder cette autorisation :",
  "perm.accessStep1": "Cliquez sur « Ouvrir les réglages Accessibilité » ci-dessous.",
  "perm.accessStep2": "Trouvez {app} dans la liste (ou cliquez sur + pour l'ajouter).",
  "perm.accessStep3": "Activez l'interrupteur.",
  "perm.accessStep4": "Revenez ici - le statut se met à jour automatiquement.",
  "perm.openAccessibilitySettings": "Ouvrir les réglages Accessibilité",
  "perm.bluetooth": "Bluetooth",
  "perm.bluetoothDesc":
    "Le Bluetooth est utilisé pour se connecter à votre casque BCI (Muse, MW75 Neuro, OpenBCI Ganglion, IDUN Guardian, etc.). Sur macOS, le système affiche une invite de permission unique lors du premier scan. Sous Linux et Windows, aucune permission spécifique n'est requise.",
  "perm.openBluetoothSettings": "Ouvrir les réglages Bluetooth",
  "perm.notifications": "Notifications",
  "perm.notificationsDesc":
    "Les notifications sont envoyées lorsque vous atteignez votre objectif d'enregistrement quotidien et lorsqu'une mise à jour logicielle est disponible. Sur macOS et Windows, le système demande la permission lors de l'envoi de la première notification.",
  "perm.openNotificationsSettings": "Ouvrir les réglages Notifications",
  "perm.matrix": "Récapitulatif des autorisations",
  "perm.feature": "Fonctionnalité",
  "perm.matrixBluetooth": "Bluetooth (appareil BCI)",
  "perm.matrixKeyboardMouse": "Horodatages clavier & souris",
  "perm.matrixActiveWindow": "Suivi de la fenêtre active",
  "perm.matrixNotifications": "Notifications",
  "perm.matrixNone": "Aucune permission requise",
  "perm.matrixAccessibility": "Accessibilité requise",
  "perm.matrixOsPrompt": "Invite système au premier usage",
  "perm.legendNone": "Aucune permission requise",
  "perm.legendRequired": "Permission OS requise - se désactive silencieusement si absente",
  "perm.legendPrompt": "Invite système au premier appel",
  "perm.why": "Pourquoi {app} a-t-il besoin de ces autorisations ?",
  "perm.whyBluetooth": "Bluetooth",
  "perm.whyBluetoothDesc": "Pour détecter votre casque BCI et diffuser ses données via BLE.",
  "perm.whyAccessibility": "Accessibilité",
  "perm.whyAccessibilityDesc":
    "Pour horodater les événements clavier et souris et fournir un contexte d'activité. Seul le moment de l'événement est enregistré - jamais ce qui a été tapé ni où se trouvait le curseur.",
  "perm.whyNotifications": "Notifications",
  "perm.whyNotificationsDesc":
    "Pour vous avertir lorsque vous atteignez votre objectif quotidien et lorsque des mises à jour sont disponibles.",
  "perm.privacyNote":
    "Toutes les données sont stockées localement sur votre appareil et ne sont jamais transmises à un serveur. Vous pouvez désactiver n'importe quelle fonctionnalité dans Réglages → Suivi d'activité.",
  "perm.screenRecording": "Enregistrement d'écran",
  "perm.screenRecordingDesc":
    "Requis pour capturer les fenêtres d'autres applications pour le système d'embedding de captures. macOS masque le contenu des fenêtres sans cette autorisation.",
  "perm.screenRecordingOk":
    "L'autorisation d'enregistrement d'écran est accordée. La capture d'écran fonctionnera correctement.",
  "perm.screenRecordingStep1":
    "Ouvrir Réglages système → Confidentialité et sécurité → Enregistrement de l'écran et audio système",
  "perm.screenRecordingStep2": "Trouver NeuroSkill™ dans la liste et l'activer",
  "perm.screenRecordingStep3":
    "Vous devrez peut-être quitter et relancer l'application pour que la modification prenne effet",
  "perm.openScreenRecordingSettings": "Ouvrir les réglages d'enregistrement d'écran",
  "perm.whyScreenRecording": "Enregistrement d'écran",
  "perm.whyScreenRecordingDesc":
    "Pour capturer la fenêtre active pour la recherche de similarité visuelle et la corrélation EEG cross-modale. Seules les captures manuelles sont stockées — jamais d'enregistrement continu.",
  "perm.matrixScreenRecording": "Capture d'écran",
  "perm.matrixScreenRecordingReq": "Enregistrement d'écran requis",
  "perm.calendar": "Calendrier",
  "perm.calendarDesc":
    "Les outils de calendrier peuvent lire les événements pour fournir du contexte de planification. L'autorisation est demandée par macOS au besoin.",
  "perm.requestCalendarPermission": "Demander l'accès au calendrier",
  "perm.openCalendarSettings": "Ouvrir les réglages de confidentialité du calendrier",
  "perm.location": "Services de localisation",
  "perm.locationDesc":
    "Sur macOS, les services de localisation utilisent CoreLocation (GPS/Wi-Fi/cellulaire) pour un positionnement précis. Sur Linux et Windows, l’app utilise la géolocalisation IP sans autorisation nécessaire. Si la localisation est refusée, l’app bascule automatiquement sur la géolocalisation IP.",
  "perm.locationOk": "Autorisation de localisation accordée. CoreLocation sera utilisé pour une haute précision.",
  "perm.locationFallback":
    "Localisation non autorisée — l’app utilisera la géolocalisation IP (précision au niveau de la ville).",
  "perm.locationStep1": "Ouvrez Réglages Système → Confidentialité et sécurité → Services de localisation",
  "perm.locationStep2": "Trouvez {app} dans la liste et activez-le",
  "perm.locationStep3": "Revenez ici — le statut se mettra à jour automatiquement",
  "perm.requestLocationPermission": "Demander l’autorisation de localisation",
  "perm.openLocationSettings": "Ouvrir les réglages de localisation",
  "perm.whyLocation": "Localisation",
  "perm.whyLocationDesc":
    "Pour fournir un contexte de localisation précis au LLM et stocker les données GPS aux côtés des données de santé. Bascule sur la géolocalisation IP en cas de refus.",
  "perm.matrixLocation": "Localisation (GPS / IP)",
  "perm.matrixLocationReq": "Services de localisation (optionnel — bascule sur IP)",
  "perm.openInputMonitoringSettings": "Ouvrir les réglages de surveillance des entrées",
  "perm.openFocusSettings": "Ouvrir les réglages Concentration",
  "perm.whyCalendar": "Calendrier",
  "perm.whyCalendarDesc":
    "Pour fournir du contexte de planification aux outils IA afin que l'assistant puisse référencer vos événements à venir.",
  "perm.matrixCalendar": "Événements du calendrier",
  "perm.matrixCalendarReq": "Accès au calendrier requis",
  "perm.fullDiskAccess": "Accès complet au disque",
  "perm.fullDiskAccessDesc":
    "Requis pour la détection directe du mode Concentration via la base de données système. Sans cette autorisation, l'app bascule sur une méthode héritée plus lente. Recommandé pour une intégration fiable du mode Ne pas déranger.",
  "perm.fullDiskAccessStep1": "Ouvrir System Settings → Privacy & Security → Full Disk Access",
  "perm.fullDiskAccessStep2": "Trouver NeuroSkill™ (ou le terminal exécutant le daemon) dans la liste et l'activer",
  "perm.fullDiskAccessStep3":
    "Vous devrez peut-être quitter et relancer l'application pour que la modification prenne effet",
  "perm.openFullDiskAccessSettings": "Ouvrir les réglages d'accès complet au disque",
};

export default perm;
