// SPDX-License-Identifier: GPL-3.0-only
/** FR — "validation" namespace. */
const validation: Record<string, string> = {
  "settingsTabs.validation": "Validation",
  "validation.title": "Validation et Recherche",
  "validation.intro":
    "Instruments de recherche optionnels qui calibrent le Coach de Pause et le Score de Focus avec des mesures externes. Aucun n'est requis pour utiliser NeuroSkill.",
  "validation.disclaimer":
    "Outil de recherche uniquement — pas un dispositif médical. Non approuvé par la FDA, le CE ou tout organisme de réglementation. Pas pour usage clinique.",

  "validation.master.title": "Contrôles globaux",
  "validation.master.respectFlow": "Respecter l'état de flow",
  "validation.master.respectFlowDesc":
    "Lorsque vous entrez en flow, toutes les invites sont supprimées. Activé par défaut — laissez-le activé.",
  "validation.master.quietBefore": "Début des heures calmes",
  "validation.master.quietAfter": "Fin des heures calmes",
  "validation.master.quietDesc":
    "Heure locale. Aucune invite hors de cette fenêtre. début = fin désactive les heures calmes.",

  "validation.kss.title": "Échelle de Somnolence de Karolinska (KSS)",
  "validation.kss.desc":
    "Auto-évaluation de 5 secondes (1–9) de la somnolence momentanée. Calibre le Coach de Pause par rapport à l'état subjectif.",
  "validation.kss.enabled": "Activer les invites KSS",
  "validation.kss.maxPerDay": "Max d'invites par jour",
  "validation.kss.minInterval": "Min minutes entre invites",
  "validation.kss.triggerBreakCoach": "Déclencher quand le Coach de Pause détecte la fatigue",
  "validation.kss.triggerRandom": "Déclencher des échantillons de contrôle aléatoires",
  "validation.kss.triggerRandomDesc":
    "Nécessaire pour calculer ROC/AUC — sans négatifs, on ne voit que les cas positifs.",
  "validation.kss.randomWeight": "Poids des échantillons aléatoires (0–1)",

  "validation.tlx.title": "NASA-TLX (charge de travail, 6 échelles)",
  "validation.tlx.desc":
    "Auto-évaluation de 60 secondes avec 6 sous-échelles après une unité de travail. Mesure la charge — complémentaire à la somnolence KSS.",
  "validation.tlx.enabled": "Activer les invites NASA-TLX",
  "validation.tlx.maxPerDay": "Max d'invites par jour",
  "validation.tlx.minTaskMin": "Durée minimale de la tâche (min) pour demander",
  "validation.tlx.endOfDay": "Résumé de charge en fin de journée",

  "validation.tlx.form.title": "Évaluez la tâche que vous venez de terminer",
  "validation.tlx.mental": "Demande Mentale",
  "validation.tlx.physical": "Demande Physique",
  "validation.tlx.temporal": "Demande Temporelle",
  "validation.tlx.performance": "Performance",
  "validation.tlx.effort": "Effort",
  "validation.tlx.frustration": "Frustration",

  "validation.pvt.title": "Tâche de Vigilance Psychomotrice (PVT)",
  "validation.pvt.desc":
    "Tâche de temps de réaction de 3 minutes. La mesure objective de vigilance — lente à collecter mais le signal le plus fort dans la littérature.",
  "validation.pvt.enabled": "Activer les rappels hebdomadaires PVT",
  "validation.pvt.weeklyReminder": "Afficher un rappel quand pas de PVT cette semaine",
  "validation.pvt.runNow": "Lancer PVT (3 min)",
  "validation.pvt.task.start": "Démarrer",
  "validation.pvt.task.cancel": "Annuler",
  "validation.pvt.task.close": "Fermer",

  "validation.eeg.title": "Indice de fatigue EEG (Jap et al. 2009)",
  "validation.eeg.desc":
    "Calculé en continu à partir du flux de puissance de bande quand un casque NeuroSkill est connecté. Formule : (α + θ) / β. Passif — sans coût.",
  "validation.eeg.enabled": "Calculer l'indice de fatigue EEG",
  "validation.eeg.windowSecs": "Fenêtre glissante (secondes)",
  "validation.eeg.current": "Valeur actuelle",
  "validation.eeg.noHeadset": "Aucun casque EEG en streaming",

  "validation.calibrationWeek.title": "Semaine de Calibration",
  "validation.calibrationWeek.desc":
    "Salve opt-in de 7 jours avec un échantillonnage plus fréquent. Augmente KSS à 8/jour, déclenche TLX après chaque bloc de flow ≥ 20 min, demande un PVT en milieu de semaine. Reprend automatiquement vos réglages le jour 8.",
  "validation.calibrationWeek.start": "Démarrer une Semaine de Calibration",

  "validation.results.title": "Résultats récents",
  "validation.save.saved": "Enregistré",
};
export default validation;
