// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Spanish "calibration" namespace — reference translation. */
const calibration: Record<string, string> = {
  "calibration.title": "Calibración",
  "calibration.profiles": "Perfiles de calibración",
  "calibration.newProfile": "Nuevo perfil",
  "calibration.editProfile": "Editar perfil",
  "calibration.profileName": "Nombre del perfil",
  "calibration.profileNamePlaceholder": "p.ej. Ojos abiertos / cerrados",
  "calibration.addAction": "Agregar acción",
  "calibration.actionLabel": "Etiqueta de acción...",
  "calibration.breakLabel": "romper",
  "calibration.selectProfile": "Perfil",
  "calibration.moveUp": "Subir",
  "calibration.moveDown": "Bajar",
  "calibration.removeAction": "Eliminar acción",
  "calibration.descriptionN": "Este protocolo ejecuta {actions}, repetido <strong>{count}</strong> veces.",
  "calibration.timingDescN": "{loops} bucles · {actions} acciones · {breakSecs}s se rompen entre cada uno",
  "calibration.notifActionBody": "Bucle {loop} de {total}",
  "calibration.notifBreakBody": "Siguiente: {next}",
  "calibration.notifDoneBody": "Todos los bucles {n} completados.",
  "calibration.recording": "● Grabación",
  "calibration.neverCalibrated": "Nunca calibrado",
  "calibration.lastAgo": "Último: {ago}",
  "calibration.eegCalibration": "Calibración EEG",
  "calibration.description":
    'Esta tarea alterna entre <strong class="text-blue-600 dark:text-blue-400">{action1}</strong> y <strong class="text-violet-600 dark:text-violet-400">{action2}</strong> con descansos intermedios, repetidos <strong>{count}</strong> veces.',
  "calibration.timingDesc":
    "Cada acción dura {actionSecs}s con un descanso de {breakSecs}s. Las etiquetas se guardan automáticamente.",
  "calibration.startCalibration": "Iniciar calibración",
  "calibration.complete": "Calibración completa",
  "calibration.completeDesc":
    "Todas las iteraciones {n} se completaron con éxito. Se han guardado etiquetas para cada fase de acción.",
  "calibration.runAgain": "correr de nuevo",
  "calibration.iteration": "Iteración",
  "calibration.break": "Romper",
  "calibration.nextAction": "Siguiente: {action}",
  "calibration.secondsRemaining": "segundos restantes",
  "calibration.ready": "Listo",
  "calibration.lastCalibrated": "Última calibración",
  "calibration.lastAtAgo": "Último: {date} ({ago})",
  "calibration.noPrevious": "No se registró ninguna calibración previa",
  "calibration.footer": "Esc para cerrar · Eventos retransmitidos vía WebSocket",
  "calibration.presets": "Preajustes rápidos",
  "calibration.presetsDesc":
    "Seleccione una configuración de calibración según su objetivo, edad y caso de uso. La configuración aún se puede ajustar a continuación.",
  "calibration.applyPreset": "Aplicar",
  "calibration.orCustom": "O configurar manualmente:",
  "calibration.preset.baseline": "Ojos abiertos / cerrados",
  "calibration.preset.baselineDesc":
    "Classic baseline: resting eyes-open vs eyes-closed. Best for beginners & first calibration.",
  "calibration.preset.focus": "Enfocarse / Relajarse",
  "calibration.preset.focusDesc": "Neurofeedback: aritmética mental versus respiración tranquila. Uso generalizado.",
  "calibration.preset.meditation": "Meditación",
  "calibration.preset.meditationDesc":
    "Pensamiento activo versus meditación de atención plena. Para meditadores y practicantes.",
  "calibration.preset.sleep": "Antes de dormir / Somnolencia",
  "calibration.preset.sleepDesc":
    "Vigilia alerta versus somnolencia. Para investigación del sueño y seguimiento de la relajación.",
  "calibration.preset.gaming": "Juegos/Rendimiento",
  "calibration.preset.gamingDesc":
    "Tarea de alta exigencia versus descanso pasivo. Para deportes electrónicos y biorretroalimentación de máximo rendimiento.",
  "calibration.preset.children": "Niños / Atención Corta",
  "calibration.preset.childrenDesc":
    "Fases más cortas (10 s) para niños o usuarios con resistencia de concentración limitada.",
  "calibration.preset.clinical": "Clínica / Investigación",
  "calibration.preset.clinicalDesc":
    "Extended 5-iteration protocol with long action phases for research or clinical baseline.",
  "calibration.preset.stress": "Estrés / Ansiedad",
  "calibration.preset.stressDesc":
    "Resting calm vs. mild cognitive stressor. For anxiety and stress-response tracking.",
};

export default calibration;
