// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Spanish "ui" namespace — reference translation. */
const ui: Record<string, string> = {
  "tip.delta":
    "Banda delta (1–4 Hz). Dominante en sueño profundo. Los valores elevados al despertar sugieren somnolencia.",
  "tip.theta": "Banda theta (4-8 Hz). Asociado con la somnolencia, la creatividad y la meditación.",
  "tip.alpha": "Banda alfa (8–12 Hz). Prominente durante la vigilia relajada con los ojos cerrados.",
  "tip.beta": "Banda beta (12-30 Hz). Asociado con el pensamiento activo y la atención enfocada.",
  "tip.gamma": "Banda gamma (30–50 Hz). Vinculado a un mayor procesamiento cognitivo y percepción.",
  "tip.relaxation": "Relación de potencia alfa a beta+theta. Más alto = estado más tranquilo y relajado.",
  "tip.engagement": "Actividad beta relativa a alfa+theta. Refleja implicación mental y estado de alerta.",
  "tip.faa":
    "Asimetría alfa frontal: diferencia en potencia alfa entre los electrodos frontales derecho e izquierdo. Positivo = motivación de aproximación.",
  "tip.tar": "Relación theta/alfa. Los valores elevados sugieren somnolencia o atención interior.",
  "tip.bar": "Relación Beta/Alfa. Los valores altos indican cognición alerta y activa.",
  "tip.dtr": "Relación Delta/Theta. Elevado en sueño profundo o enlentecimiento patológico.",
  "tip.pse":
    "Entropía del espectro de potencia: qué tan uniformemente se distribuye la potencia entre las frecuencias. 1,0 = ruido blanco.",
  "tip.apf":
    "Frecuencia máxima alfa: la frecuencia dominante dentro de la banda alfa (8–12 Hz). Normalmente ~10 Hz en adultos sanos.",
  "tip.mood": "Índice de valencia derivado de la asimetría frontal. 0 = negativo, 100 = positivo.",
  "tip.bps":
    "Pendiente espectral 1/f. Una pendiente más pronunciada (más negativa) sugiere una actividad neuronal más estructurada.",
  "tip.snr": "Relación señal-ruido en dB. Cuanto más alto es la señal más limpia; por debajo de 3 dB es ruidoso.",
  "tip.coherence":
    "Coherencia de fase entre electrodos frontales izquierdo y derecho. Más alto = hemisferios más sincronizados.",
  "tip.muSuppression":
    "Supresión del ritmo Mu (8-13 Hz) en sitios centrales. Los valores <0,8 sugieren imágenes motoras u observación del movimiento.",
  "tip.tbr": "Relación theta/beta. Un TBR elevado se asocia con un control ejecutivo reducido.",
  "tip.sef95":
    "Frecuencia de borde espectral 95 %: frecuencia por debajo de la cual se encuentra el 95 % de la potencia total. Más bajo = EEG más lento.",
  "tip.spectralCentroid":
    "Centro de masa del espectro de potencia. Sube con el estado de alerta y baja con la somnolencia.",
  "tip.hjorthActivity": "Variación de la señal: representa la potencia del EEG de superficie.",
  "tip.hjorthMobility":
    "Relación de desviaciones estándar de la primera derivada a la señal. Frecuencia media aproximada.",
  "tip.hjorthComplexity":
    "Relación entre movilidad de la primera derivada y movilidad de la señal. Mide el ancho de banda.",
  "tip.permEntropy": "Entropía de permutación: complejidad de patrones ordinales. Alto = más irregular/complejo.",
  "tip.higuchiFd":
    "Dimensión fractal de Higuchi: medida de complejidad en el dominio del tiempo. Mayor = forma de onda más compleja.",
  "tip.dfaExponent":
    "Exponente del análisis de fluctuación sin tendencia. ~0,5 = no correlacionado, ~1,0 = correlacionado a largo plazo.",
  "tip.sampleEntropy": "Entropía de muestra: irregularidad de la señal. Mayor = menos predecible.",
  "tip.pacThetaGamma":
    "Acoplamiento fase-amplitud entre fase theta y amplitud gamma. Vinculado a la codificación de la memoria.",
  "tip.lateralityIndex": "Asimetría de poder izquierda-derecha en todas las bandas. Positivo = derecha dominante.",
  "tip.hr": "Frecuencia cardíaca derivada de los intervalos entre latidos PPG.",
  "tip.rmssd":
    "Media cuadrática de diferencias sucesivas entre latidos del corazón. Métrica clave de VFC parasimpática.",
  "tip.sdnn": "Desviación estándar de los intervalos latido a latido. Refleja la VFC general.",
  "tip.pnn50": "Porcentaje de intervalos sucesivos que difieren en > 50 ms. Mayor = mayor tono vagal.",
  "tip.lfHfRatio": "Relación HRV de baja frecuencia a alta frecuencia. Alto = dominio simpático; bajo = parasimpático.",
  "tip.respiratoryRate": "Frecuencia respiratoria estimada a partir de la arritmia sinusal respiratoria PPG.",
  "tip.spo2": "Saturación estimada de oxígeno en sangre a partir de la relación PPG rojo/IR. Normal: 95-100 %.",
  "tip.perfusionIndex":
    "Flujo sanguíneo pulsátil como % de la señal estática. Mayor = pulso más fuerte en el sitio del sensor.",
  "tip.stressIndex": "Índice de estrés de Baevsky derivado del histograma HRV. Mayor = mayor estrés simpático.",
  "tip.meditation": "Puntuación compuesta que combina dominancia alfa, quietud y coherencia de la VFC.",
  "tip.cognitiveLoad": "Relación theta frontal/alfa parietal. Mayor = mayor carga de trabajo mental.",
  "tip.drowsiness": "Combinación de relación theta/alfa y aumento de potencia alfa. Más alto = más somnoliento.",
  "tip.blinks": "Parpadeos detectados mediante picos de amplitud del electrodo frontal (AF7/AF8).",
  "tip.blinkRate": "Parpadeos por minuto. Frecuencia espontánea normal: 15-20/min.",
  "tip.pitch": "Inclinación de la cabeza hacia adelante/atrás en grados desde la fusión del acelerómetro + giroscopio.",
  "tip.roll":
    "Inclinación de la cabeza hacia la izquierda/derecha en grados desde la fusión del acelerómetro + giroscopio.",
  "tip.stillness": "Puntuación de quietud del movimiento de la cabeza (0-100). 100 = perfectamente quieto.",
  "tip.nods": "Se detectaron movimientos verticales de cabeza (oscilaciones de tono).",
  "tip.shakes": "Se detectaron sacudidas horizontales de la cabeza (oscilaciones de balanceo).",
  "tip.headache":
    "Correlación del dolor de cabeza: hiperexcitabilidad cortical: beta elevada + alfa suprimida + BAR alto. Cambios en la excitabilidad cortical observados en estados de dolor de cabeza. Solo indicador de investigación.",
  "tip.migraine":
    "Correlación de migraña: proxy de depresión cortical extendida: delta elevado + supresión alfa + lateralización hemisférica. Delta aumenta y alfa se suprime durante la migraña. Solo indicador de investigación.",
  "tip.consciousness.lzc":
    "Proxy de complejidad de Lempel-Ziv: riqueza de información de la señal aproximada mediante entropía de permutación (complejidad ordinal) y dimensión fractal de Higuchi (complejidad fractal). Más alto = EEG más complejo y rico en información, como se ve en estados conscientes versus inconscientes.",
  "tip.consciousness.wakefulness":
    "Nivel de vigilia: somnolencia inversa modulada por BAR y TAR. Los valores altos indican un estado cerebral activo y alerta. Los valores bajos sugieren somnolencia o inicio del sueño. Basado en índices de excitación alfa/theta.",
  "tip.consciousness.integration":
    "Proxy de integración de información (teoría del espacio de trabajo global): compuesto de coherencia entre canales × PAC theta-gamma × entropía espectral. Mayor = actividad cerebral más integrada, una firma de estados conscientes en IIT y marcos de espacios de trabajo globales.",

  "goals.title": "Meta de grabación diaria",
  "goals.subtitle": "Establezca un objetivo de cuánto tiempo desea grabar cada día.",
  "goals.targetMinutes": "Minutos objetivo por día",
  "goals.presets": "Preajustes rápidos",
  "goals.howItWorks": "como funciona",
  "goals.info1":
    "Su barra de progreso aparece en el panel mientras graba, mostrando el tiempo de hoy en comparación con su objetivo.",
  "goals.info2": "Se envía una notificación cuando alcanza su objetivo diario.",
  "goals.info3": "La grabación diaria genera una racha, que se muestra en la página Historial.",
  "goals.chartTitle": "Últimos 30 días",
  "goals.today": "Hoy",
  "goals.noData": "Aún no hay sesiones de grabación. Conecte unos auriculares para iniciar el seguimiento.",
  "goals.legendGoalMet": "Meta cumplida",
  "goals.legendHalfway": "≥50 % del objetivo",
  "goals.legendSomeProgress": "Algunos avances",

  "updates.title": "Actualizaciones de software",
  "updates.currentVersion": "Versión actual: v{version}",
  "updates.upToDate": "Estás al día",
  "updates.checking": "Comprobando…",
  "updates.checkNow": "Buscar actualizaciones",
  "updates.lastChecked": "Última comprobación",
  "updates.available": "disponible",
  "updates.installed": "v{version} instalado",
  "updates.readyToRestart": "Listo para reiniciar",
  "updates.restartToApply": "Reinicie la aplicación para aplicar la actualización.",
  "updates.restartNow": "Reiniciar ahora",
  "updates.restartWhenReady": "Actualización lista: reinicie cuando esté listo.",
  "updates.sessionLiveBlocked":
    "Se está grabando una sesión de EEG. Detenga la sesión antes de reiniciarla o ciérrela y reiníciela más tarde.",
  "updates.restartingIn": "Reiniciando en {secs}s…",
  "updates.downloadFailed": "Error en la descarga: haga clic en Reintentar para volver a intentarlo.",
  "updates.autoUpdateFailedOnline": "La actualización automática falló. Descargue la última versión en línea.",
  "updates.openDownloadPageFailed": "No se pudo abrir la página de descarga automáticamente: {error}",
  "updates.retry": "Reintentar",
  "updates.downloading": "Descargando v{version}…",
  "updates.autoCheck": "Comprobar automáticamente",
  "updates.autoCheckDesc": "Busque actualizaciones una vez al día cuando se inicie la aplicación.",
  "updates.downloadNow": "Descargar",
  "updates.checkInterval": "Verificar frecuencia",
  "updates.checkIntervalDesc":
    "Con qué frecuencia la aplicación busca actualizaciones automáticamente en segundo plano.",
  "updates.interval15m": "15 minutos",
  "updates.interval30m": "30 minutos",
  "updates.interval1h": "1 hora",
  "updates.interval4h": "4 horas",
  "updates.interval24h": "24 horas",
  "updates.intervalOff": "Apagado",
  "updates.intervalOffWarning":
    "Las comprobaciones de actualizaciones automáticas están deshabilitadas. Utilice el botón de arriba para comprobarlo manualmente.",
  "updates.autostart": "Iniciar sesión",
  "updates.autostartDesc": "Se inicia automáticamente cuando inicia sesión en su computadora.",
  "updates.footer":
    "Las actualizaciones se descargan automáticamente. Reinicie cuando esté listo para presentar la solicitud.",

  "whatsNew.title": "Qué hay de nuevo",
  "whatsNew.version": "Versión {version}",
  "whatsNew.gotIt": "Entendido",
  "whatsNew.older": "Anterior",
  "whatsNew.newer": "Más nuevo",
  "whatsNew.unreleased": "Inédito",

  "svg.layerQuery": "Consulta",
  "svg.layerTextMatches": "Coincidencias de texto",
  "svg.layerEegNeighbors": "Vecinos EEG",
  "svg.layerFoundLabels": "Etiquetas encontradas",
  "svg.legendQuery": "Consulta",
  "svg.legendText": "Texto",
  "svg.legendEeg": "EEG",
  "svg.legendFound": "Encontrado",
  "svg.generatedBy": "Generado por {app}",

  "disclaimer.title": "Sólo para uso en investigación",
  "disclaimer.body":
    "{app} es una herramienta de investigación de código abierto para el análisis exploratorio de EEG. NO es un dispositivo médico y NO ha sido autorizado ni aprobado por la FDA, la CE ni ningún organismo regulador. No debe utilizarse para diagnóstico clínico, decisiones de tratamiento ni ningún propósito médico. Todas las métricas son resultados de investigaciones experimentales, no mediciones clínicas validadas. No confíe en ningún resultado de este software para tomar decisiones relacionadas con la salud. Consulte a un profesional de la salud calificado si tiene alguna inquietud médica. No es un sistema médico. Consulte siempre las referencias para comprender cómo se calculan y utilizan las métricas. Recuerde que la posición de los electrodos, su ajuste, su calidad e incluso su ingesta actual de café influyen en la calidad de la señal.",
  "disclaimer.short": "Sólo para fines de investigación. No es un dispositivo médico. No para uso clínico.",
  "disclaimer.nonCommercial":
    "Este software se proporciona únicamente para uso educativo y de investigación no comercial.",
  "disclaimer.exgPlacement":
    "Las señales EXG (como EEG, EMG, ECG y EOG) dependen en gran medida de dónde y cómo se colocan los electrodos en el cuerpo. Incluso pequeños cambios en la posición, el espaciado o la ubicación de referencia de los electrodos pueden afectar el tamaño, la forma y los valores de las métricas calculadas de la señal. Las diferencias en el contacto con la piel, la impedancia y la calidad de la conexión también pueden afectar la calidad de la señal. Debido a esto, los datos EXG y las mediciones derivadas pueden no ser directamente comparables si cambia la ubicación de los electrodos o la configuración de registro.",
  "disclaimer.footer":
    "⚠️ Solo una herramienta de investigación, no un sistema médico. Consulte las referencias para obtener detalles métricos. El ajuste, la colocación y la ingesta de café de los electrodos afectan la calidad de la señal.",
  "disclaimer.copyright": "© {year} {app}. Reservados todos los derechos.",

  "cmdK.title": "Paleta de comandos",
  "cmdK.placeholder": "Escribe un comando...",
  "cmdK.noResults": "No hay comandos coincidentes",
  "cmdK.navigate": "navegar por",
  "cmdK.run": "ejecutar",
  "cmdK.footerHint": "⌘K para alternar",
  "cmdK.sectionNavigation": "Navegación",
  "cmdK.sectionDevice": "Dispositivo",
  "cmdK.sectionCalibration": "Calibración",
  "cmdK.sectionUtilities": "Utilidades",
  "cmdK.openSettings": "Abrir configuración",
  "cmdK.openHelp": "Abrir ayuda",
  "cmdK.openHistory": "Abrir historial",
  "cmdK.openSearch": "Buscar",
  "cmdK.openLabel": "Agregar etiqueta",
  "cmdK.retryConnect": "Reintentar conexión",
  "cmdK.openBtSettings": "Abra la configuración de Bluetooth",
  "cmdK.openCalibration": "Iniciar calibración",
  "cmdK.calibrationError": "Calibración no disponible",
  "cmdK.showShortcuts": "Mostrar atajos de teclado",
  "cmdK.highContrastOn": "Habilitar alto contraste",
  "cmdK.highContrastOff": "Desactivar alto contraste",
  "cmdK.checkUpdates": "Buscar actualizaciones",
  "cmdK.openApi": "Estado de la API",
  "cmdK.openCompare": "Comparar",
  "cmdK.openOnboarding": "Asistente de configuración",
  "cmdK.openElectrodes": "Guía de colocación de electrodos",
  "cmdK.kw.settings":
    "preferencias configuración filtro de dispositivo muesca procesamiento de señal idioma fuente apariencia tema atajos teclas de acceso rápido registro actualizaciones directorio de datos dispositivos emparejados",
  "cmdK.kw.help":
    "documentación preguntas frecuentes guía de referencia solución de problemas electrodos api privacidad",
  "cmdK.kw.history": "sesiones grabaciones datos pasados ​​revisar reproducción dormir hipnograma eliminar exportar",
  "cmdK.kw.compare":
    "sesiones en paralelo puntuaciones de potencia de banda de diferencias métricas de estadificación del sueño umap",
  "cmdK.kw.search": "incrustaciones similitud vecino más cercano rango de tiempo de consulta HNSW UMAP encontrar",
  "cmdK.kw.label": "etiqueta anotar marcar marca de tiempo nota del evento",
  "cmdK.kw.retryConnect":
    "reconectar musa openbci neurable mw75 emotiv idun ganglio guardián cyton auriculares escanear par ble",
  "cmdK.kw.btSettings":
    "par bluetooth muse openbci neurable mw75 emotiv idun ganglio guardián cyton auriculares inalámbricos adaptador ble",
  "cmdK.kw.calibration": "ojos abiertos cerrados línea de base tren tarea guiada acción etiqueta cronometrado",
  "cmdK.kw.api": "Clientes del servidor websocket puerto mdns bonjour streaming comandos json",
  "cmdK.kw.onboarding":
    "Asistente de configuración Primera ejecución Bienvenido Bluetooth Ajuste Calibración Comenzando",
  "cmdK.kw.electrodes": "Ubicación del sensor TP9 AF7 AF8 TP10 Diagrama de ajuste del cabezal de calidad del contacto",
  "cmdK.kw.shortcuts": "teclas de acceso rápido del teclado combinaciones de teclas teclas comandos",
  "cmdK.kw.highContrast": "accesibilidad visibilidad contraste bordes texto",
  "cmdK.kw.updates": "versión actualización descargar instalar parche",

  "toast.connected": "Conectado",
  "toast.connectedMsg": "{name} ahora está transmitiendo datos de EEG.",
  "toast.connectionLost": "Conexión perdida",
  "toast.connectionLostMsg": "{name} desconectado.",
  "toast.bluetoothOff": "Bluetooth desactivado",
  "toast.bluetoothOffMsg": "Bluetooth no está disponible: actívelo para conectarse.",
  "toast.bluetoothRestored": "Bluetooth restaurado",
  "toast.bluetoothRestoredMsg": "Bluetooth ha vuelto: reconectándose...",
  "toast.lowBattery": "Batería baja",
  "toast.lowBatteryMsg": "Batería al {pct}%: considere cargarla.",
  "toast.criticalBattery": "Batería crítica",
  "toast.criticalBatteryMsg": "Batería al {pct}%: cárguela pronto.",
  "toast.calibrationComplete": "Calibración completa",
  "toast.calibrationCompleteMsg": "Todas las iteraciones de calibración finalizaron exitosamente.",

  "apiStatus.title": "Estado de la API de WebSocket",
  "apiStatus.refresh": "Refrescar",
  "apiStatus.serverRunning": "Servidor en ejecución",
  "apiStatus.port": "Puerto",
  "apiStatus.protocol": "Protocolo",
  "apiStatus.discovery": "Descubrimiento",
  "apiStatus.connectedClients": "Clientes conectados",
  "apiStatus.noClients": "No hay clientes conectados",
  "apiStatus.noClientsHint": "Conecte un cliente WebSocket a ws://localhost:{port} o descubra a través de mDNS.",
  "apiStatus.connectedSince": "desde",
  "apiStatus.requestLog": "Registro de solicitudes",
  "apiStatus.entries": "entradas",
  "apiStatus.noRequests": "Aún no hay solicitudes",
  "apiStatus.time": "Tiempo",
  "apiStatus.client": "Cliente",
  "apiStatus.command": "Comando",
  "apiStatus.status": "Estado",
  "apiStatus.quickConnect": "Conexión rápida",
  "apiStatus.clickToCopy": "Haga clic para copiar",

  "quitDialog.title": "Salir de NeuroSkill™",
  "quitDialog.description": "¿Seguro que quieres salir de NeuroSkill™?",

  "window.title.main": "{app}",
  "window.title.settings": "{app} – Configuración",
  "window.title.help": "{app} – Ayuda",
  "window.title.history": "{app} – Historia",
  "window.title.compare": "{app} – Comparar",
  "window.title.session": "{app} – Detalle de la sesión",
  "window.title.search": "{app} – Búsqueda de EEG",
  "window.title.calibration": "{app} – Calibración",
  "window.title.focusTimer": "Temporizador de enfoque",
  "window.title.labels": "Todas las etiquetas",
  "window.title.label": "Agregar etiqueta",
  "window.title.onboarding": "{app} – Bienvenido",
  "window.title.api": "{app} – Estado de la API",
  "window.title.about": "Acerca de {app}",

  "about.title": "Acerca de NeuroSkill™",
  "about.links": "Enlaces",
  "about.sourceCode": "Código fuente",
  "about.authors": "Autores",
  "about.license": "Licencia",
  "about.acknowledgements": "Agradecimientos",
  "about.discord": "Discord",

  "downloads.windowTitle": "Descargas",
  "downloads.subtitle": "Gestione las descargas de modelos activos y completados.",
  "downloads.loading": "Cargando descargas…",
  "downloads.empty": "Aún no hay descargas.",
  "downloads.initiatedAt": "Iniciado",
  "downloads.initiatedUnknown": "Desconocido",
  "downloads.pause": "Pausa",
  "downloads.resume": "Reanudar",
  "downloads.cancel": "Cancelar",
  "downloads.delete": "Borrar",
  "downloads.status.notDownloaded": "No descargado",
  "downloads.status.downloading": "Descargando…",
  "downloads.status.paused": "En pausa",
  "downloads.status.downloaded": "Descargado",
  "downloads.status.failed": "Fallido",
  "downloads.status.cancelled": "Cancelado",

  "devices.title": "Dispositivos",
  "devices.subtitle":
    "Administre dispositivos BCI emparejados y descubiertos, procesamiento de señales e integración de EEG.",
  "devices.pairedCount": "{n} emparejado",
  "devices.pairedDevices": "Dispositivos emparejados",
  "devices.discoveredDevices": "Dispositivos descubiertos",
  "devices.noPaired": "No hay dispositivos emparejados",
  "devices.noPairedHint":
    "Encienda sus auriculares BCI y aparecerán aquí una vez descubiertos. Emparéjelo para habilitar conexiones automáticas.",
  "devices.noDiscovered": "No hay dispositivos cercanos",
  "devices.noDiscoveredHint":
    "Encienda sus auriculares BCI y colóquelos dentro del alcance de Bluetooth. Los dispositivos descubiertos aparecerán aquí automáticamente.",
  "devices.deviceSingular": "dispositivo",
  "devices.devicePlural": "dispositivos",
};

export default ui;
