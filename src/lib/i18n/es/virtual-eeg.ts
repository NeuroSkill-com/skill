// SPDX-License-Identifier: GPL-3.0-only
/** ES "virtual-eeg" namespace — Simulador de dispositivo EEG virtual. */
const virtualEeg: Record<string, string> = {
  "settingsTabs.virtualEeg": "EEG Virtual",

  "veeg.title": "Dispositivo EEG Virtual",
  "veeg.desc":
    "Simule un auricular EEG para pruebas, demostraciones y desarrollo. Genera datos sintéticos que recorren toda la cadena de procesamiento de señal.",

  "veeg.status": "Estado",
  "veeg.running": "En ejecución",
  "veeg.stopped": "Detenido",
  "veeg.start": "Iniciar",
  "veeg.stop": "Detener",

  "veeg.channels": "Canales",
  "veeg.channelsDesc": "Número de electrodos EEG a simular.",
  "veeg.sampleRate": "Frecuencia de muestreo (Hz)",
  "veeg.sampleRateDesc": "Muestras por segundo por canal.",

  "veeg.template": "Plantilla de señal",
  "veeg.templateDesc": "Elija el tipo de señal sintética a generar.",
  "veeg.templateSine": "Ondas sinusoidales",
  "veeg.templateSineDesc":
    "Ondas sinusoidales limpias en bandas de frecuencia estándar (delta, theta, alfa, beta, gamma).",
  "veeg.templateGoodQuality": "EEG de buena calidad",
  "veeg.templateGoodQualityDesc": "EEG realista en reposo con ritmo alfa dominante y ruido rosa de fondo.",
  "veeg.templateBadQuality": "EEG de mala calidad",
  "veeg.templateBadQualityDesc":
    "Señal ruidosa con artefactos musculares, ruido de línea de 50/60 Hz y saltos de electrodo.",
  "veeg.templateInterruptions": "Conexión intermitente",
  "veeg.templateInterruptionsDesc":
    "Buena señal con interrupciones periódicas que simulan electrodos sueltos o interferencia inalámbrica.",
  "veeg.templateFile": "Desde archivo",
  "veeg.templateFileDesc": "Reproducir muestras de un archivo CSV o EDF.",

  "veeg.quality": "Calidad de señal",
  "veeg.qualityDesc": "Ajuste la relación señal-ruido. Mayor = señal más limpia.",
  "veeg.qualityPoor": "Deficiente",
  "veeg.qualityFair": "Aceptable",
  "veeg.qualityGood": "Buena",
  "veeg.qualityExcellent": "Excelente",

  "veeg.chooseFile": "Seleccionar archivo",
  "veeg.noFile": "Ningún archivo seleccionado",
  "veeg.fileLoaded": "{name} ({channels} canales, {samples} muestras)",

  "veeg.advanced": "Avanzado",
  "veeg.amplitudeUv": "Amplitud (µV)",
  "veeg.amplitudeDesc": "Amplitud pico a pico de las señales generadas.",
  "veeg.noiseUv": "Piso de ruido (µV)",
  "veeg.noiseDesc": "Amplitud RMS del ruido gaussiano aditivo.",
  "veeg.lineNoise": "Ruido de línea",
  "veeg.lineNoiseDesc": "Agregar interferencia de red eléctrica de 50 Hz o 60 Hz.",
  "veeg.lineNoise50": "50 Hz",
  "veeg.lineNoise60": "60 Hz",
  "veeg.lineNoiseNone": "Ninguno",
  "veeg.dropoutProb": "Probabilidad de interrupción",
  "veeg.dropoutDesc": "Probabilidad de pérdida de señal por segundo (0 = ninguna, 1 = constante).",

  "veeg.preview": "Vista previa de señal",
  "veeg.previewDesc": "Vista previa en tiempo real de los primeros 4 canales.",

  // ── Ventana de dispositivos virtuales ─────────────────────────────────────────
  "window.title.virtualDevices": "{app} – Dispositivos Virtuales",

  "vdev.title": "Dispositivos Virtuales",
  "vdev.desc":
    "Pruebe NeuroSkill sin hardware EEG físico. Elija una plantilla que corresponda a un dispositivo real o configure su propia fuente de señal sintética.",

  "vdev.presets": "Plantillas de dispositivo",
  "vdev.statusRunning": "Dispositivo virtual transmitiendo",
  "vdev.statusStopped": "Ningún dispositivo virtual activo",
  "vdev.selected": "Listo",
  "vdev.configure": "Configurar",
  "vdev.customConfig": "Configuración personalizada",

  "vdev.presetMuse": "Muse S",
  "vdev.presetMuseDesc": "Disposición de diadema de 4 canales — TP9, AF7, AF8, TP10.",
  "vdev.presetCyton": "OpenBCI Cyton",
  "vdev.presetCytonDesc": "Señal de investigación de 8 canales, montaje frontal/central completo.",
  "vdev.presetCap32": "Gorro EEG de 32 canales",
  "vdev.presetCap32Desc": "Sistema internacional 10-20 completo, 32 electrodos.",
  "vdev.presetAlpha": "Alfa intenso",
  "vdev.presetAlphaDesc": "Ritmo alfa prominente de 10 Hz — línea base relajada con ojos cerrados.",
  "vdev.presetArtifact": "Prueba de artefactos",
  "vdev.presetArtifactDesc": "Señal ruidosa con artefactos musculares y ruido de línea de 50 Hz.",
  "vdev.presetDropout": "Prueba de interrupciones",
  "vdev.presetDropoutDesc": "Pérdida periódica de señal que simula electrodos sueltos.",
  "vdev.presetMinimal": "Mínimo (1 canal)",
  "vdev.presetMinimalDesc": "Onda sinusoidal de un solo canal — la carga más ligera posible.",
  "vdev.presetCustom": "Personalizado",
  "vdev.presetCustomDesc": "Defina su propio número de canales, frecuencia, plantilla y nivel de ruido.",

  "vdev.lslSourceTitle": "Fuente LSL virtual",
  "vdev.lslRunning": "Transmitiendo EEG sintético vía LSL",
  "vdev.lslStopped": "Fuente LSL virtual detenida",
  "vdev.lslDesc": "Inicia una fuente local de Lab Streaming Layer para probar la detección y conexión de flujos LSL.",
  "vdev.lslHint":
    'Abra Ajustes → pestaña LSL y haga clic en "Escanear red" para ver SkillVirtualEEG en la lista de flujos, luego conéctese.',
  "vdev.lslStarted": "La fuente LSL virtual está transmitiendo en la red local.",

  // Panel de estado
  "vdev.statusSource": "Fuente LSL",
  "vdev.statusSession": "Sesión",
  "vdev.sessionConnected": "Conectado",
  "vdev.sessionConnecting": "Conectando…",
  "vdev.sessionDisconnected": "Desconectado",
  "vdev.startBtn": "Iniciar dispositivo virtual",
  "vdev.stopBtn": "Detener dispositivo virtual",
  "vdev.autoConnect": "Conectar automáticamente al panel",
  "vdev.autoConnectDesc": "Conectar el panel a esta fuente inmediatamente después de iniciar.",

  // Vista previa
  "vdev.previewOffline": "Vista previa de señal (sin conexión)",
  "vdev.previewOfflineDesc":
    "Vista previa de forma de onda del lado del cliente — muestra la forma de la señal antes de conectar. Aún no se transmiten datos.",

  // Plantilla personalizada — canales / frecuencia
  "vdev.cfgChannels": "Canales",
  "vdev.cfgChannelsDesc": "Número de electrodos EEG a simular.",
  "vdev.cfgRate": "Frecuencia de muestreo",
  "vdev.cfgRateDesc": "Muestras por segundo por canal.",

  // Plantilla personalizada — calidad de señal
  "vdev.cfgQuality": "Calidad de señal",
  "vdev.cfgQualityDesc": "Relación señal-ruido. Mayor = señal más limpia.",

  // Plantilla personalizada — plantilla
  "vdev.cfgTemplate": "Plantilla de señal",
  "vdev.cfgTemplateSine": "Ondas sinusoidales",
  "vdev.cfgTemplateSineDesc": "Ondas sinusoidales puras en frecuencias delta, theta, alfa, beta y gamma.",
  "vdev.cfgTemplateGood": "EEG de buena calidad",
  "vdev.cfgTemplateGoodDesc": "Estado de reposo realista con alfa dominante y ruido rosa de fondo.",
  "vdev.cfgTemplateBad": "EEG de mala calidad",
  "vdev.cfgTemplateBadDesc": "Señal ruidosa con artefactos musculares, ruido de línea y saltos de electrodo.",
  "vdev.cfgTemplateInterruptions": "Conexión intermitente",
  "vdev.cfgTemplateInterruptionsDesc": "Buena señal con interrupciones periódicas que simulan electrodos sueltos.",

  // Plantilla personalizada — avanzado
  "vdev.cfgAdvanced": "Avanzado",
  "vdev.cfgAmplitude": "Amplitud (µV)",
  "vdev.cfgAmplitudeDesc": "Amplitud pico a pico de la señal simulada.",
  "vdev.cfgNoise": "Piso de ruido (µV)",
  "vdev.cfgNoiseDesc": "Amplitud RMS del ruido gaussiano aditivo de fondo.",
  "vdev.cfgLineNoise": "Ruido de línea",
  "vdev.cfgLineNoiseDesc": "Inyectar interferencia de red eléctrica de 50 Hz o 60 Hz.",
  "vdev.cfgLineNoiseNone": "Ninguno",
  "vdev.cfgLineNoise50": "50 Hz",
  "vdev.cfgLineNoise60": "60 Hz",
  "vdev.cfgDropout": "Probabilidad de interrupción",
  "vdev.cfgDropoutDesc": "Probabilidad de pérdida de señal por segundo (0 = nunca, 1 = constante).",
};

export default virtualEeg;
