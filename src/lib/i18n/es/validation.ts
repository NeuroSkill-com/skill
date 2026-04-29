// SPDX-License-Identifier: GPL-3.0-only
/** ES — "validation" namespace. */
const validation: Record<string, string> = {
  "settingsTabs.validation": "Validación",
  "validation.title": "Validación e Investigación",
  "validation.intro":
    "Instrumentos de investigación opcionales que calibran el Coach de Pausas y la Puntuación de Foco contra medidas externas. Ninguno es necesario para usar NeuroSkill.",
  "validation.disclaimer":
    "Solo herramienta de investigación — no es un dispositivo médico. No aprobado por la FDA, CE ni ningún organismo regulador. No para uso clínico.",

  "validation.master.title": "Controles globales",
  "validation.master.respectFlow": "Respetar estado de flow",
  "validation.master.respectFlowDesc":
    "Cuando entres en flow, todos los avisos quedan suprimidos. Activado por defecto — déjalo activado.",
  "validation.master.quietBefore": "Inicio horas tranquilas",
  "validation.master.quietAfter": "Fin horas tranquilas",
  "validation.master.quietDesc":
    "Hora local. No se disparan avisos fuera de esta ventana. inicio = fin desactiva las horas tranquilas.",

  "validation.kss.title": "Escala de Somnolencia de Karolinska (KSS)",
  "validation.kss.desc":
    "Auto-informe de 5 segundos (1–9) de somnolencia momentánea. Calibra el Coach de Pausas contra el estado subjetivo.",
  "validation.kss.enabled": "Activar avisos KSS",
  "validation.kss.maxPerDay": "Máx. avisos por día",
  "validation.kss.minInterval": "Mín. minutos entre avisos",
  "validation.kss.triggerBreakCoach": "Disparar cuando el Coach de Pausas detecte fatiga",
  "validation.kss.triggerRandom": "Disparar muestras de control aleatorias ocasionales",
  "validation.kss.triggerRandomDesc":
    "Necesario para calcular ROC/AUC — sin negativos solo vemos casos positivos de fatiga.",
  "validation.kss.randomWeight": "Peso de muestra aleatoria (0–1)",

  "validation.tlx.title": "NASA-TLX (carga de trabajo, 6 escalas)",
  "validation.tlx.desc":
    "Auto-informe de 60 segundos con 6 subescalas tras una unidad de trabajo. Mide carga — complementario a la somnolencia KSS.",
  "validation.tlx.enabled": "Activar avisos NASA-TLX",
  "validation.tlx.maxPerDay": "Máx. avisos por día",
  "validation.tlx.minTaskMin": "Duración mínima de tarea (min) para preguntar",
  "validation.tlx.endOfDay": "Resumen de carga al final del día",

  "validation.tlx.form.title": "Evalúa la tarea que acabas de terminar",
  "validation.tlx.mental": "Demanda Mental",
  "validation.tlx.physical": "Demanda Física",
  "validation.tlx.temporal": "Demanda Temporal",
  "validation.tlx.performance": "Rendimiento",
  "validation.tlx.effort": "Esfuerzo",
  "validation.tlx.frustration": "Frustración",

  "validation.pvt.title": "Tarea de Vigilancia Psicomotora (PVT)",
  "validation.pvt.desc":
    "Tarea de tiempo de reacción de 3 minutos. La medida objetiva de vigilancia — lenta de recopilar pero la señal más fuerte en la literatura.",
  "validation.pvt.enabled": "Activar recordatorios semanales de PVT",
  "validation.pvt.weeklyReminder": "Mostrar recordatorio cuando no haya PVT esta semana",
  "validation.pvt.runNow": "Ejecutar PVT ahora (3 min)",
  "validation.pvt.task.start": "Iniciar",
  "validation.pvt.task.cancel": "Cancelar",
  "validation.pvt.task.close": "Cerrar",

  "validation.eeg.title": "Índice de fatiga EEG (Jap et al. 2009)",
  "validation.eeg.desc":
    "Calculado continuamente a partir del flujo de potencia de banda cuando hay un casco NeuroSkill conectado. Fórmula: (α + θ) / β. Pasivo — sin coste.",
  "validation.eeg.enabled": "Calcular índice de fatiga EEG",
  "validation.eeg.windowSecs": "Ventana móvil (segundos)",
  "validation.eeg.current": "Valor actual",
  "validation.eeg.noHeadset": "Sin casco EEG transmitiendo",

  "validation.calibrationWeek.title": "Semana de Calibración",
  "validation.calibrationWeek.desc":
    "Ráfaga opcional de 7 días con muestreo más frecuente. Aumenta KSS a 8/día, dispara TLX tras cada bloque de flow ≥ 20 min, pide un PVT a mitad de semana. Vuelve a tu configuración normal el día 8.",
  "validation.calibrationWeek.start": "Iniciar Semana de Calibración",

  "validation.results.title": "Resultados recientes",
  "validation.save.saved": "Guardado",
};
export default validation;
