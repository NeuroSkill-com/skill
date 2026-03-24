// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** FR "calibration" namespace translations. */
const calibration: Record<string, string> = {
  "calibration.profiles": "Profils de calibration",
  "calibration.newProfile": "Nouveau profil",
  "calibration.editProfile": "Modifier le profil",
  "calibration.profileName": "Nom du profil",
  "calibration.profileNamePlaceholder": "ex. Yeux ouverts / fermés",
  "calibration.addAction": "Ajouter une action",
  "calibration.actionLabel": "Libellé de l'action...",
  "calibration.breakLabel": "pause",
  "calibration.selectProfile": "Profil",
  "calibration.descriptionN": "Ce protocole exécute {actions}, répété <strong>{count}</strong> fois.",
  "calibration.timingDescN": "{loops} boucles · {actions} actions · {breakSecs}s de pause entre chaque",
  "calibration.notifActionBody": "Boucle {loop} sur {total}",
  "calibration.notifBreakBody": "Suivant : {next}",
  "calibration.notifDoneBody": "Toutes les {n} boucles terminées.",
  "calibration.title": "Calibration",
  "calibration.recording": "● Enregistrement",
  "calibration.neverCalibrated": "Jamais calibré",
  "calibration.lastAgo": "Dernier : {ago}",
  "calibration.eegCalibration": "Calibration EEG",
  "calibration.description":
    'Cette tâche alterne entre <strong class="text-blue-600 dark:text-blue-400">{action1}</strong> et <strong class="text-violet-600 dark:text-violet-400">{action2}</strong> avec des pauses, répétée <strong>{count}</strong> fois.',
  "calibration.timingDesc":
    "Chaque action dure {actionSecs}s avec une pause de {breakSecs}s. Les labels sont enregistrés automatiquement.",
  "calibration.startCalibration": "Démarrer la calibration",
  "calibration.complete": "Calibration terminée",
  "calibration.completeDesc":
    "Les {n} itérations ont été complétées avec succès. Les labels ont été enregistrés pour chaque phase.",
  "calibration.runAgain": "Relancer",
  "calibration.iteration": "Itération",
  "calibration.break": "Pause",
  "calibration.nextAction": "Suivant : {action}",
  "calibration.secondsRemaining": "secondes restantes",
  "calibration.ready": "Prêt",
  "calibration.lastCalibrated": "Dernière calibration",
  "calibration.lastAtAgo": "Dernière : {date} ({ago})",
  "calibration.noPrevious": "Aucune calibration précédente enregistrée",
  "calibration.footer": "Échap pour fermer · Événements diffusés via WebSocket",
  "calibration.presets": "Préréglages rapides",
  "calibration.presetsDesc": "Sélectionnez une configuration de calibration selon votre objectif, âge et cas d'usage.",
  "calibration.applyPreset": "Appliquer",
  "calibration.orCustom": "Ou configurer manuellement :",
  "calibration.preset.baseline": "Yeux ouverts / fermés",
  "calibration.preset.baselineDesc": "Référence classique : repos yeux ouverts vs fermés. Recommandé pour débutants.",
  "calibration.preset.focus": "Concentration / Détente",
  "calibration.preset.focusDesc": "Neurofeedback : calcul mental vs respiration calme. Usage général.",
  "calibration.preset.meditation": "Méditation",
  "calibration.preset.meditationDesc": "Pensée active vs méditation de pleine conscience. Pour méditants.",
  "calibration.preset.sleep": "Pré-sommeil / Somnolence",
  "calibration.preset.sleepDesc": "Éveil alerte vs somnolence. Pour la recherche sur le sommeil.",
  "calibration.preset.gaming": "Jeu / Performance",
  "calibration.preset.gamingDesc":
    "Tâche très exigeante vs repos passif. Pour l'e-sport et le biofeedback haute performance.",
  "calibration.preset.children": "Enfants / Courte attention",
  "calibration.preset.childrenDesc":
    "Phases plus courtes (10 s) pour enfants ou utilisateurs à durée d'attention limitée.",
  "calibration.preset.clinical": "Clinique / Recherche",
  "calibration.preset.clinicalDesc":
    "Protocole étendu à 5 itérations avec longues phases d'action pour la recherche ou la baseline clinique.",
  "calibration.preset.stress": "Stress / Anxiété",
  "calibration.preset.stressDesc":
    "Calme au repos vs facteur de stress cognitif léger. Pour le suivi du stress et de l'anxiété.",
  "calibration.moveUp": "Monter",
  "calibration.moveDown": "Descendre",
  "calibration.removeAction": "Supprimer l'action",
};

export default calibration;
