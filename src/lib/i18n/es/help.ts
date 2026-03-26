// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Spanish "help" namespace — reference translation. */
const help: Record<string, string> = {
  "helpTabs.dashboard": "Panel",
  "helpTabs.electrodes": "Electrodos",
  "helpTabs.settings": "Ajustes",
  "helpTabs.windows": "ventanas",
  "helpTabs.api": "API",
  "helpTabs.privacy": "Privacidad",
  "helpTabs.references": "Referencias",
  "helpTabs.faq": "Preguntas frecuentes",

  "helpDash.mainWindow": "Ventana principal",
  "helpDash.mainWindowDesc":
    "La ventana principal es el panel principal. Muestra datos de EEG en tiempo real, el estado del dispositivo y la calidad de la señal. Siempre está visible en la barra de menú.",
  "helpDash.statusHero": "Héroe de estado",
  "helpDash.statusHeroBody":
    "La tarjeta superior muestra el estado de conexión en vivo de su dispositivo BCI. Un anillo de color y una insignia indican si el dispositivo está desconectado, escaneando, conectado o si Bluetooth está apagado. Cuando está conectado, se muestran el nombre del dispositivo, el número de serie y la dirección MAC (haga clic para revelar/ocultar).",
  "helpDash.battery": "Batería",
  "helpDash.batteryBody":
    "Una barra de progreso que muestra la carga actual de la batería del auricular BCI conectado. El color cambia de verde (alto), pasando por ámbar y rojo (bajo), a medida que cae la carga.",
  "helpDash.signalQuality": "Calidad de la señal",
  "helpDash.signalQualityBody":
    "Cuatro puntos codificados por colores: uno por electrodo EEG (TP9, AF7, AF8, TP10). Verde = buen contacto con la piel y poco ruido. Amarillo = regular (algún artefacto). Rojo = deficiente (alto ruido/electrodo suelto). Gris = sin señal. La calidad se calcula a partir de una ventana RMS móvil sobre los datos sin procesar del EEG.",
  "helpDash.eegChannelGrid": "Cuadrícula de canales EEG",
  "helpDash.eegChannelGridBody":
    "Cuatro tarjetas que muestran el último valor de muestra (en µV) para cada canal, codificadas por colores para que coincidan con el gráfico de formas de onda a continuación.",
  "helpDash.uptimeSamples": "Tiempo de actividad y muestras",
  "helpDash.uptimeSamplesBody":
    "El tiempo de actividad cuenta los segundos desde que comenzó la sesión actual. Muestras es el número total de muestras de EEG sin procesar recibidas del auricular en esta sesión.",
  "helpDash.csvRecording": "Grabación CSV",
  "helpDash.csvRecordingBody":
    "Cuando está conectado, un indicador REC muestra el nombre del archivo CSV que se está escribiendo en {dataDir}/. Las muestras de EEG sin procesar (sin filtrar) se guardan continuamente: un archivo por sesión.",
  "helpDash.bandPowers": "Poderes de banda",
  "helpDash.bandPowersBody":
    "Un gráfico de barras en vivo que muestra la potencia relativa en cada banda de frecuencia de EEG estándar: Delta (1 a 4 Hz), Theta (4 a 8 Hz), Alfa (8 a 13 Hz), Beta (13 a 30 Hz) y Gamma (30 a 50 Hz). Actualizado a ~4 Hz desde una FFT con ventana Hann de 512 muestras. Cada canal se muestra por separado.",
  "helpDash.faa": "Asimetría Alfa Frontal (FAA)",
  "helpDash.faaBody":
    "Un medidor anclado en el centro que muestra el índice de asimetría frontal alfa en tiempo real: ln(AF8 α) − ln(AF7 α). Los valores positivos indican un mayor poder alfa frontal derecho, que se asocia con la motivación de aproximación del hemisferio izquierdo. Los valores negativos indican tendencia a la retirada. El valor se suaviza con una media móvil exponencial y normalmente oscila entre −1 y +1. FAA se almacena junto con cada época de incrustación de 5 segundos en eeg.sqlite.",
  "helpDash.eegWaveforms": "Formas de onda EEG",
  "helpDash.eegWaveformsBody":
    "Un gráfico de desplazamiento en el dominio del tiempo de la señal EEG filtrada para todos los canales. Debajo de cada forma de onda hay una cinta de espectrograma que muestra el contenido de frecuencia a lo largo del tiempo. El gráfico muestra los ~4 segundos de datos más recientes.",
  "helpDash.gpuUtilisation": "Utilización de GPU",
  "helpDash.gpuUtilisationBody":
    "Un pequeño gráfico en la parte superior de la ventana principal que muestra la utilización del codificador y decodificador de GPU. Visible solo mientras el codificador de incorporación ZUNA está activo. Ayuda a verificar que la canalización wgpu se esté ejecutando.",
  "helpDash.trayIconStates": "Estados del icono de la bandeja",
  "helpDash.trayGrey": "Gris: desconectado",
  "helpDash.trayGreyDesc": "Bluetooth está activado; no hay ningún dispositivo BCI conectado.",
  "helpDash.trayAmber": "Ámbar: escaneado",
  "helpDash.trayAmberDesc": "Buscando un dispositivo BCI o intentando conectarse.",
  "helpDash.trayGreen": "Verde: conectado",
  "helpDash.trayGreenDesc": "Transmisión de datos de EEG en vivo desde su dispositivo BCI.",
  "helpDash.trayRed": "Rojo: Bluetooth desactivado",
  "helpDash.trayRedDesc": "La radio Bluetooth está apagada. No es posible escanear ni conectar.",
  "helpDash.community": "Comunidad",
  "helpDash.communityDesc":
    "Join the NeuroSkill Discord community to ask questions, share feedback, and connect with other users and developers.",
  "helpDash.discordLink": "Únete a nuestra discordia",

  "helpSettings.settingsTab": "Pestaña de configuración",
  "helpSettings.settingsTabDesc":
    "Configure las preferencias del dispositivo, el procesamiento de señales, la incorporación de parámetros, la calibración, los accesos directos y el registro.",
  "helpSettings.pairedDevices": "Dispositivos emparejados",
  "helpSettings.pairedDevicesBody":
    "Enumera todos los dispositivos BCI que ha visto la aplicación. Puede configurar un dispositivo preferido (objetivo de conexión automática), olvidar dispositivos o buscar otros nuevos. La intensidad de la señal RSSI se muestra para los dispositivos vistos recientemente.",
  "helpSettings.signalProcessing": "Procesamiento de señales",
  "helpSettings.signalProcessingBody":
    "Configure la cadena de filtros EEG en tiempo real: corte de paso bajo (elimina el ruido de alta frecuencia), corte de paso alto (elimina la deriva de CC) y filtro de muesca de línea eléctrica (elimina el zumbido y los armónicos de la red eléctrica de 50 o 60 Hz). Los cambios se aplican inmediatamente a la visualización de la forma de onda y a las potencias de las bandas.",
  "helpSettings.eegEmbedding": "Incrustación de EEG",
  "helpSettings.eegEmbeddingBody":
    "Ajuste la superposición entre épocas de incrustación consecutivas de 5 segundos. Una mayor superposición significa más incorporaciones por minuto (resolución temporal más fina en la búsqueda) a costa de más almacenamiento y computación.",
  "helpSettings.calibration": "Calibración",
  "helpSettings.calibrationBody":
    'Configura la tarea de calibración: etiquetas de acción (p. ej., "ojos abiertos", "ojos cerrados"), duración de fases, número de repeticiones y si la calibración debe iniciarse automáticamente al abrir la aplicación.',
  "helpSettings.calibrationTts": "Guía de voz de calibración (TTS)",
  "helpSettings.calibrationTtsBody":
    'Durante la calibración, la app anuncia cada fase por nombre usando síntesis de voz local en inglés. El motor usa KittenTTS (tract-onnx, ~30 MB) con fonemización de espeak-ng. El modelo se descarga desde HuggingFace Hub en el primer inicio y luego queda en caché local: después de eso no sale ningún dato de tu dispositivo. La voz se activa al inicio de sesión, en cada fase de acción, en cada descanso ("Break. Next: …") y al completar la sesión. Requiere espeak-ng en PATH (brew / apt / apk install espeak-ng). Solo inglés.',
  "helpSettings.globalShortcuts": "Atajos globales",
  "helpSettings.globalShortcutsBody":
    "Configure atajos de teclado en todo el sistema para abrir las ventanas Etiqueta, Búsqueda, Configuración y Calibración desde cualquier aplicación. Utiliza el formato de acelerador estándar (por ejemplo, CmdOrCtrl+Shift+L).",
  "helpSettings.debugLogging": "Registro de depuración",
  "helpSettings.debugLoggingBody":
    "Cambie el registro por subsistema al archivo de registro diario en {dataDir}/logs/. Los subsistemas incluyen incrustador, dispositivos, websocket, csv, filtro y bandas.",
  "helpSettings.updates": "Actualizaciones",
  "helpSettings.updatesBody":
    "Busque e instale actualizaciones de aplicaciones. Utiliza el actualizador integrado de Tauri con verificación de firma Ed25519.",
  "helpSettings.appearanceTab": "Apariencia",
  "helpSettings.appearanceTabBody":
    "Elija un modo de color (Sistema / Claro / Oscuro), habilite Contraste alto para bordes y texto más fuertes y elija una combinación de colores de gráfico para formas de onda de EEG y visualizaciones de potencia de banda. Hay disponibles paletas aptas para daltónicos. El idioma también se cambia aquí mediante el selector de configuración regional.",
  "helpSettings.goalsTab": "Objetivos",
  "helpSettings.goalsTabBody":
    "Establezca un objetivo de grabación diario en minutos. Aparece una barra de progreso en el panel durante la transmisión y se activa una notificación cuando alcanzas tu objetivo. El gráfico de los últimos 30 días muestra qué días llegó (verde), llegó a la mitad (ámbar), realizó algún progreso (oscuro) o se perdió (ninguno).",
  "helpSettings.embeddingsTab": "Incrustaciones de texto",
  "helpSettings.embeddingsTabBody":
    "Seleccione el modelo de transformador de oración utilizado para incrustar el texto de su etiqueta para la búsqueda semántica. Los modelos más pequeños (≤384-dim, por ejemplo, All-MiniLM-L6-v2) son rápidos y suficientes para la búsqueda personal. Los modelos más grandes producen representaciones más ricas a costa del tamaño de descarga y el tiempo de inferencia. Los pesos se descargan una vez desde HuggingFace y se almacenan en caché localmente. Después de cambiar de modelo, ejecute Volver a incrustar todas las etiquetas para volver a indexar.",
  "helpSettings.shortcutsTab": "Atajos",
  "helpSettings.shortcutsTabBody":
    "Configure atajos de teclado globales (teclas de acceso rápido para todo el sistema) para abrir las ventanas Etiqueta, Búsqueda, Configuración y Calibración. También muestra todos los atajos de la aplicación (⌘K para la paleta de comandos, ? para la superposición de atajos, ⌘↵ para enviar una etiqueta). Los atajos utilizan el formato de acelerador estándar, p. CmdOCtrl+Mayús+L.",
  "helpSettings.activitySection": "Seguimiento de actividad",
  "helpSettings.activitySectionDesc":
    "NeuroSkill puede, opcionalmente, registrar qué aplicación está en primer plano y cuándo se utilizaron por última vez el teclado y el mouse. Ambas funciones están desactivadas de forma predeterminada, son totalmente locales y se pueden configurar de forma independiente en Configuración → Seguimiento de actividad.",
  "helpSettings.activeWindowHelp": "Seguimiento de ventana activa",
  "helpSettings.activeWindowHelpBody":
    'Un hilo en segundo plano se activa cada segundo y pregunta al sistema operativo qué aplicación se encuentra actualmente en primer plano. Cuando el nombre de la aplicación o el título de la ventana cambia, se inserta una fila en Activity.sqlite: el nombre para mostrar de la aplicación (por ejemplo, "Safari"), la ruta completa al paquete de la aplicación o al ejecutable, el título de la ventana principal (por ejemplo, el nombre del documento o la página web actual) y una grabación de marca de tiempo de un segundo de Unix cuando esa ventana se activó. Si permanece en la misma ventana, no se escribe ninguna fila nueva: el tiempo de inactividad en una sola aplicación no produce actividad en la base de datos. En macOS el rastreador llama a osascript; No se necesita ningún permiso de accesibilidad para el nombre y la ruta de la aplicación, pero el título de la ventana puede estar vacío para las aplicaciones en espacio aislado. En Linux usa xdotool y xprop (requiere una sesión X11). En Windows utiliza una llamada GetForegroundWindow de PowerShell.',
  "helpSettings.inputActivityHelp": "Seguimiento de actividad del teclado y el mouse",
  "helpSettings.inputActivityHelpBody":
    'Un enlace de entrada global (rdev) escucha cada pulsación de tecla y evento del mouse o trackpad en todo el sistema. No registra lo que escribió, qué teclas presionó ni dónde se movió el cursor; solo actualiza dos marcas de tiempo de Unix en segundos en la memoria: una para el evento de teclado más reciente y otra para el evento de mouse/trackpad más reciente. Estos se descargan en Activity.sqlite cada 60 segundos, pero solo cuando al menos un valor ha cambiado desde la última descarga, por lo que los períodos inactivos no dejan rastro. El panel de Configuración recibe un evento de actualización en vivo (regulado a una vez por segundo como máximo) para que los campos "Último teclado" y "Último mouse" reflejen la actividad casi en tiempo real.',
  "helpSettings.activityStorageHelp": "Dónde se almacenan los datos",
  "helpSettings.activityStorageHelpBody":
    "Todos los datos de actividad se encuentran en un único archivo SQLite: ~/.skill/activity.sqlite. Nunca se transmite, sincroniza ni incluye en ningún análisis. Se mantienen dos tablas: active_windows (una fila por cambio de enfoque de ventana, con el nombre de la aplicación, ruta, título y marca de tiempo) y input_activity (una fila por cada 60 segundos de descarga cuando se detectó actividad, con marcas de tiempo del último teclado y del último mouse). Ambas tablas tienen un índice descendente en la columna de marca de tiempo. El modo de diario WAL está habilitado para que las escrituras en segundo plano nunca bloqueen las lecturas. Puede abrir, inspeccionar, exportar o eliminar el archivo en cualquier momento con cualquier navegador SQLite.",
  "helpSettings.activityPermissionsHelp": "Permisos requeridos del sistema operativo",
  "helpSettings.activityPermissionsHelpBody":
    "macOS: el seguimiento de ventanas activas (nombre y ruta de la aplicación) no requiere permisos especiales. El seguimiento del teclado y el mouse utiliza un CGEventTap que requiere acceso de Accesibilidad: abra Configuración del sistema → Privacidad y seguridad → Accesibilidad, busque NeuroSkill en la lista y actívelo. Sin este permiso, el enlace de entrada falla silenciosamente: las marcas de tiempo permanecen en cero y el resto de la aplicación no se ve afectado en absoluto. Puede desactivar la opción en Configuración → Seguimiento de actividad para evitar que se solicite permiso por completo. Linux: ambas funciones requieren una sesión X11. El seguimiento de ventanas activas utiliza xdotool y xprop, que están preinstalados en la mayoría de las distribuciones de escritorio. El seguimiento de entrada utiliza la extensión XRecord de libxtst. Si falta alguna de las herramientas, esa función registra una advertencia y se desactiva. Windows: no se requieren permisos especiales. El seguimiento de ventanas activas utiliza GetForegroundWindow a través de PowerShell; el seguimiento de entrada utiliza SetWindowsHookEx.",
  "helpSettings.activityDisablingHelp": "Deshabilitar y borrar datos",
  "helpSettings.activityDisablingHelpBody":
    "Ambos cambios en Configuración → Seguimiento de actividad entran en vigor de inmediato; no es necesario reiniciar. Deshabilitar el seguimiento de ventanas activas impide que se inserten nuevas filas en active_windows y borra el estado de la ventana actual en la memoria. Deshabilitar el seguimiento de entrada evita que la devolución de llamada de rdev actualice las marcas de tiempo y evita futuros vaciados en input_activity; Las filas existentes no se eliminan automáticamente. Para eliminar todo el historial recopilado: salga de la aplicación, elimine ~/.skill/activity.sqlite y luego reiníciela. Se creará automáticamente una base de datos vacía en el próximo inicio.",
  "helpSettings.umapTab": "UMAP",
  "helpSettings.umapTabBody":
    "Parámetros de control para la proyección UMAP 3D utilizada en Comparación de sesiones: número de vecinos (controla la estructura local frente a la global), distancia mínima (con qué precisión se agrupan los puntos) y la métrica (coseno o euclidiana). Un mayor número de vecinos preserva una topología más global; los recuentos más bajos revelan grupos locales detallados. Las proyecciones se ejecutan en un trabajo en segundo plano y los resultados se almacenan en caché.",
  "helpSettings.eegModelTab": "Pestaña Modelo EEG",
  "helpSettings.eegModelTabDesc": "Supervise el codificador ZUNA y el estado del índice del vector HNSW.",
  "helpSettings.encoderStatus": "Estado del codificador",
  "helpSettings.encoderStatusBody":
    "Muestra si el codificador ZUNA wgpu está cargado, el resumen de la arquitectura (dimensión, capas, cabezales) y la ruta al archivo de peso .safetensors. El codificador se ejecuta completamente en el dispositivo utilizando su GPU.",
  "helpSettings.embeddingsToday": "Incrustaciones hoy",
  "helpSettings.embeddingsTodayBody":
    "Un contador en vivo de cuántas épocas de EEG de 5 segundos se han incluido en el índice HNSW de hoy. Cada incorporación es un vector compacto que captura la firma neuronal de ese momento.",
  "helpSettings.hnswParams": "Parámetros HNSW",
  "helpSettings.hnswParamsBody":
    "M (conexiones por nodo) y ef_construction (ancho de búsqueda durante la construcción) controlan la relación calidad/velocidad del índice del vecino más cercano. Los valores más altos dan una mejor recuperación pero usan más memoria. Los valores predeterminados (M=16, ef=200) son un buen equilibrio.",
  "helpSettings.dataNorm": "Normalización de datos",
  "helpSettings.dataNormBody":
    "El factor de escala data_norm aplicado al EEG sin procesar antes de la codificación. El valor predeterminado (10) está configurado para los auriculares Muse 2/Muse S.",
  "helpSettings.openbciSection": "Tableros OpenBCI",
  "helpSettings.openbciSectionDesc":
    "Conecte y configure cualquier placa OpenBCI (Ganglion, Cyton, Cyton+Daisy, variantes WiFi Shield o Galea) de forma independiente o junto con otro dispositivo BCI.",
  "helpSettings.openbciBoard": "Selección de placa",
  "helpSettings.openbciBoardBody":
    "Elija qué placa OpenBCI utilizar. Ganglion (4 canales, BLE) es la opción más portátil. Cyton (8 canales, serie USB) agrega un mayor número de canales. Cyton+Daisy duplica esto a 16 canales. Las variantes de WiFi Shield reemplazan el enlace USB/BLE con una transmisión Wi-Fi de 1 kHz. Galea (24 canales, UDP) es una placa de investigación de alta densidad. Todas las variantes pueden funcionar de forma independiente o junto con otro dispositivo BCI.",
  "helpSettings.openbciGanglion": "Ganglion BLE",
  "helpSettings.openbciGanglionBody":
    "Ganglion se conecta por Bluetooth Low Energy. Pulsa Conectar y NeuroSkill™ buscará el Ganglion anunciándose más cercano durante el tiempo de escaneo configurado. Mantén la placa a 3–5 m y encendida (LED azul parpadeando). Solo puede haber un Ganglion activo por adaptador Bluetooth. Amplía el tiempo de escaneo BLE en Configuración si la placa tarda en anunciarse.",
  "helpSettings.openbciSerial": "Puerto serie (Cyton / Cyton+Daisy)",
  "helpSettings.openbciSerialBody":
    "Las placas Cyton se comunican a través de una llave de radio USB. Deje el campo del puerto serie en blanco para detectar automáticamente el primer puerto disponible o ingréselo explícitamente (/dev/cu.usbserial-… en macOS, /dev/ttyUSB0 en Linux, COM3 en Windows). Conecte el dongle antes de hacer clic en Conectar y asegúrese de tener permisos de puerto serie; en Linux, agregue su usuario al grupo de acceso telefónico.",
  "helpSettings.openbciWifi": "Escudo WiFi",
  "helpSettings.openbciWifiBody":
    "OpenBCI WiFi Shield crea su propio punto de acceso de 2,4 GHz (SSID: OpenBCI-XXXX). Conecta tu ordenador a esa red y configura la IP en 192.168.4.1 (puerta de enlace predeterminada del shield). Alternativamente, el shield puede unirse a tu red local: usa la IP asignada en ese caso. Deja el campo IP vacío para intentar autodetección vía mDNS. WiFi Shield transmite a 1 kHz: establece el corte del filtro paso bajo en ≤ 500 Hz en Configuración de procesamiento de señal.",
  "helpSettings.openbciGalea": "Galea",
  "helpSettings.openbciGaleaBody":
    "Galea es un auricular de bioseñales de grado de investigación de 24 canales (EEG + EMG + AUX) que transmite a través de UDP. Ingrese la dirección IP del dispositivo Galea o déjela en blanco para aceptar paquetes de cualquier remitente en la red local. Los canales 1 a 8 son EEG y generan análisis en tiempo real; los canales 9 a 16 son EMG; 17–24 son auxiliares. Los 24 canales se guardan en CSV.",
  "helpSettings.openbciChannels": "Etiquetas de canales y ajustes preestablecidos",
  "helpSettings.openbciChannelsBody":
    "Asigne entre 10 y 20 nombres de electrodos estándar a cada canal físico para que las métricas de potencia de banda, la asimetría alfa frontal y las visualizaciones de electrodos tengan en cuenta los electrodos. Utilice un ajuste preestablecido (Frontal, Motor, Occipital, Completo 10-20) para completar las etiquetas automáticamente o escriba nombres personalizados. Los canales más allá de los primeros 4 se registran únicamente en CSV y no impulsan el proceso de análisis en tiempo real.",

  "helpWindows.title": "ventanas",
  "helpWindows.desc":
    "{app} usa ventanas separadas para tareas específicas. Cada uno se puede abrir desde el menú contextual de la bandeja o mediante un atajo de teclado global.",
  "helpWindows.labelTitle": "🏷 Ventana de etiqueta",
  "helpWindows.labelBody":
    'Se abre a través del menú de la bandeja, el acceso directo global o el botón de etiqueta en la ventana principal. Escriba una etiqueta de texto libre para anotar el momento actual del EEG (por ejemplo, "meditación", "lectura enfocada"). La etiqueta se guarda en {dataDir}/labels.sqlite con el rango de marca de tiempo exacto. Envíe con Ctrl/⌘+Entrar o haga clic en Enviar. Presione Escape para cancelar.',
  "helpWindows.searchTitle": "🔍 Ventana de búsqueda",
  "helpWindows.searchBody":
    "La ventana de búsqueda tiene tres modos: similitud de EEG, texto e interactivo, cada uno de los cuales consulta los datos registrados de una manera diferente.",
  "helpWindows.searchEegTitle": "Búsqueda de similitud de EEG",
  "helpWindows.searchEegBody":
    'Elija un rango de fecha y hora de inicio/finalización y ejecute una búsqueda aproximada del vecino más cercano en todas las incrustaciones de ZUNA registradas en esa ventana. El índice HNSW devuelve las k épocas de EEG de 5 segundos más similares de todo su historial, clasificadas por distancia de coseno. Menor distancia = estado cerebral más similar. Cualquier etiqueta que se superponga a una marca de tiempo de resultado se muestra en línea. Útil para encontrar momentos pasados ​​que "parecieron" similares a un período de referencia.',
  "helpWindows.searchTextTitle": "Búsqueda de incrustación de texto",
  "helpWindows.searchTextBody":
    'Escriba cualquier concepto, actividad o estado mental en lenguaje sencillo (por ejemplo, "enfoque profundo", "ansioso", "meditación con los ojos cerrados"). Su consulta está integrada en el mismo modelo de transformador de oraciones que se utiliza para la indexación de etiquetas y se compara con cada anotación que haya escrito mediante similitud de coseno sobre el índice de etiquetas HNSW. Los resultados son sus propias etiquetas clasificadas por cercanía semántica, no por concordancia de palabras clave. Puede filtrar la lista y reordenarla por fecha o similitud. Un gráfico kNN 3D visualiza la estructura de vecindad: el nodo de consulta se encuentra en el centro, las etiquetas de resultados se irradian hacia afuera según la distancia.',
  "helpWindows.searchInteractiveTitle": "Búsqueda intermodal interactiva",
  "helpWindows.searchInteractiveBody":
    "Ingrese un concepto de texto libre y {app} ejecutará una canalización intermodal de cuatro pasos: (1) la consulta se incrusta en un vector de texto; (2) se recuperan las k etiquetas semánticamente más similares (texto-k); (3) para cada etiqueta coincidente, se calcula su incrustación EEG media y se utiliza para buscar en los índices EEG HNSW diarios los k momentos EEG más similares (eeg-k); (4) para cada vecino EEG, se recopilan etiquetas cercanas dentro de ± minutos de alcance (etiqueta-k). El resultado es un gráfico dirigido con cuatro capas de nodos (Consulta → Coincidencias de texto → Vecinos EEG → Etiquetas encontradas) representado como una visualización 3D interactiva y exportable como SVG o Graphviz DOT. Utilice los controles deslizantes text-k / eeg-k / label-k para controlar la densidad del gráfico y ±reach para ampliar o reducir la ventana de búsqueda temporal.",
  "helpWindows.calTitle": "🎯 Ventana de calibración",
  "helpWindows.calBody":
    'Ejecuta una tarea de calibración guiada: alternando fases de acción (por ejemplo, "ojos abiertos" → descanso → "ojos cerrados" → descanso) para un número configurable de bucles. Requiere un dispositivo BCI de transmisión conectado. Los eventos de calibración se emiten a través del bus de eventos Tauri y WebSocket para que las herramientas externas puedan sincronizarse. La marca de tiempo de la última calibración completada se guarda en la configuración.',
  "helpWindows.settingsTitle": "⚙ Ventana de configuración",
  "helpWindows.settingsBody":
    "Cuatro pestañas: Configuración, Atajos (teclas de acceso rápido globales, paleta de comandos, teclas en la aplicación), Modelo EEG (codificador y estado HNSW). Ábralo desde el menú de la bandeja o el botón de engranaje en la ventana principal.",
  "helpWindows.helpTitle": "?  Ventana de ayuda",
  "helpWindows.helpBody":
    "Esta ventana. Una referencia completa para cada parte de la interfaz {app}: el panel principal, cada pestaña de configuración, cada ventana emergente, el ícono de la bandeja y la API WebSocket. Abrir desde el menú de la bandeja.",
  "helpWindows.onboardingTitle": "🧭 Asistente de configuración",
  "helpWindows.onboardingBody":
    "Un asistente de primera ejecución de cinco pasos que lo guía a través del emparejamiento de Bluetooth, el ajuste de los auriculares y la primera calibración. Se abre automáticamente en el primer lanzamiento; se puede volver a abrir en cualquier momento desde la paleta de comandos (⌘K → Asistente de configuración).",
  "helpWindows.apiTitle": "🌐 Ventana de estado de API",
  "helpWindows.apiBody":
    "Un panel en vivo que muestra todos los clientes WebSocket actualmente conectados y un registro de solicitudes desplazable. Muestra el puerto del servidor, el protocolo y la información de descubrimiento de mDNS. Incluye fragmentos de conexión rápida para ws:// y dns-sd. Se actualiza automáticamente cada 2 segundos. Ábralo desde el menú de la bandeja o la paleta de comandos.",
  "helpWindows.sleepTitle": "🌙 Puesta en escena del sueño",
  "helpWindows.sleepBody":
    "Para sesiones que duran 30 minutos o más, la vista Historial muestra un hipnograma generado automáticamente: un gráfico en escalera de las etapas del sueño (Wake / N1 / N2 / N3 / REM) clasificadas según proporciones de potencia de las bandas delta, theta, alfa y beta. Amplíe cualquier sesión larga en Historial para ver el hipnograma con un desglose por etapa que muestra el porcentaje y la duración. Nota: los auriculares BCI de consumo, como Muse, utilizan 4 electrodos secos, por lo que la estadificación es aproximada; no es un polisomnógrafo clínico.",
  "helpWindows.compareTitle": "⚖ Ventana de comparación",
  "helpWindows.compareBody":
    "Elija dos rangos de tiempo cualesquiera en la línea de tiempo y compare sus distribuciones promedio de potencia de banda, puntajes de relajación/compromiso y asimetría alfa frontal uno al lado del otro. Incluye estadificación del sueño, métricas avanzadas y Brain Nebula™, una proyección UMAP en 3D que muestra cuán similares son los dos períodos en el espacio EEG de alta dimensión. Abra desde el menú de la bandeja o la paleta de comandos (⌘K → Comparar).",
  "helpWindows.overlaysTitle": "Superposiciones y paleta de comandos",
  "helpWindows.overlaysDesc":
    "Superposiciones de acceso rápido disponibles en cada ventana mediante atajos de teclado.",
  "helpWindows.cmdPaletteTitle": "⌨ Paleta de comandos (⌘K / Ctrl+K)",
  "helpWindows.cmdPaletteBody":
    "Un menú desplegable de acceso rápido que enumera todas las acciones ejecutables en la aplicación. Comience a escribir comandos de filtro difuso, use ↑↓ para navegar y presione Entrar para ejecutar. Disponible en todas las ventanas. Los comandos incluyen abrir ventanas (Configuración, Ayuda, Búsqueda, Etiqueta, Historial, Calibración), acciones del dispositivo (volver a intentar conectar, abrir la configuración de Bluetooth) y utilidades (mostrar superposiciones de accesos directos, buscar actualizaciones).",
  "helpWindows.shortcutsOverlayTitle": "?  Superposición de atajos de teclado",
  "helpWindows.shortcutsOverlayBody":
    "Prensa ? en cualquier ventana (fuera de las entradas de texto) para alternar una superposición flotante que enumera todos los atajos de teclado: atajos globales configurados en Configuración → Atajos, además de teclas en la aplicación como ⌘K para la paleta de comandos y ⌘Enter para enviar etiquetas. Prensa ? nuevamente o Esc para descartar.",

  "help.searchPlaceholder": "Buscar ayuda...",
  "help.searchNoResults": 'No hay resultados para "{query}"',

  "helpApi.overview": "Descripción general",
  "helpApi.liveStreaming": "Transmisión en vivo",
  "helpApi.liveStreamingBody":
    "{app} transmite métricas de EEG derivadas y el estado del dispositivo a través de un servidor WebSocket local. Los eventos de transmisión incluyen: bandas eeg (~4 Hz: más de 60 puntuaciones), estado del dispositivo (~1 Hz: batería, estado de conexión) y etiqueta creada. Las muestras sin procesar de EEG/PPG/IMU no están disponibles a través de la API WebSocket. El servicio se anuncia a través de Bonjour/mDNS como _skill._tcp para que los clientes puedan descubrirlo automáticamente.",
  "helpApi.commands": "Comandos",
  "helpApi.commandsBody":
    'Los clientes pueden enviar comandos JSON a través de WebSocket: estado (instantánea completa del sistema), calibrar (calibración abierta), etiqueta (enviar una anotación), buscar (consulta del vecino más cercano), sesiones (listar grabaciones), comparar (métricas A/B + suspensión + UMAP), suspensión (puesta en escena del sueño), umap/umap_poll (proyección de incrustación 3D). Las respuestas llegan a la misma conexión que JSON con un valor booleano "ok".',
  "helpApi.commandReference": "Referencia de comando",
  "helpApi.discoveryWireFormat": "Descubrimiento y formato de cable",
  "helpApi.discoverService": "Descubre el servicio",
  "helpApi.outboundEvents": "Eventos salientes (servidor → cliente)",
  "helpApi.inboundCommands": "Comandos entrantes (cliente → servidor)",
  "helpApi.response": "Respuesta",
  "helpApi.cmdStatus": "estado",
  "helpApi.cmdStatusParams": "_(ninguno)_",
  "helpApi.cmdStatusDesc":
    "Returns device state, session info, embedding counts (today & all-time), label count, last calibration timestamp, and per-channel signal quality.",
  "helpApi.cmdCalibrate": "calibrar",
  "helpApi.cmdCalibrateParams": "_(ninguno)_",
  "helpApi.cmdCalibrateDesc": "Abre la ventana de calibración. Requiere un dispositivo de transmisión conectado.",
  "helpApi.cmdLabel": "etiqueta",
  "helpApi.cmdLabelParams":
    "texto (cadena, requerido); label_start_utc (u64, opcional; el valor predeterminado es ahora)",
  "helpApi.cmdLabelDesc":
    "Inserta una etiqueta con marca de tiempo en la base de datos de etiquetas. Devuelve el nuevo label_id.",
  "helpApi.cmdSearch": "buscar",
  "helpApi.cmdSearchParams": "start_utc, end_utc (u64, obligatorio); k, ef (u64, opcional)",
  "helpApi.cmdSearchDesc":
    "Searches the HNSW embedding index for the k nearest neighbours within the given time range.",
  "helpApi.cmdCompare": "comparar",
  "helpApi.cmdCompareParams": "a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, obligatorio)",
  "helpApi.cmdCompareDesc":
    "Compares two time ranges by returning aggregated band-power metrics (relative powers, relaxation/engagement scores, and FAA) for each. Returns { a: SessionMetrics, b: SessionMetrics }.",
  "helpApi.cmdSessions": "sesiones",
  "helpApi.cmdSessionsParams": "_(ninguno)_",
  "helpApi.cmdSessionsDesc":
    "Lists all embedding sessions discovered from the daily eeg.sqlite databases. Sessions are contiguous recording ranges (gap > 2 min = new session). Returns newest first.",
  "helpApi.cmdSleep": "dormir",
  "helpApi.cmdSleepParams": "start_utc, end_utc (u64, obligatorio)",
  "helpApi.cmdSleepDesc":
    "Classifies each embedding epoch in the time range into a sleep stage (Wake/N1/N2/N3/REM) using band-power ratios and returns a hypnogram with per-stage summary.",
  "helpApi.cmdUmap": "mapa",
  "helpApi.cmdUmapParams": "a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, obligatorio)",
  "helpApi.cmdUmapDesc":
    "Enqueues a 3D UMAP projection of embeddings from two sessions. Returns a job_id for polling. Non-blocking.",
  "helpApi.cmdUmapPoll": "umap_poll",
  "helpApi.cmdUmapPollParams": "job_id (cadena, requerida)",
  "helpApi.cmdUmapPollDesc":
    "Polls for the result of a previously enqueued UMAP job. Returns { status: 'pending' | 'done', points?: [...] }.",

  "helpPrivacy.overview": "Descripción general de privacidad",
  "helpPrivacy.overviewDesc":
    "{app} está diseñado para ser completamente local primero. Sus datos, incrustaciones, etiquetas y configuraciones de EEG nunca salen de su máquina a menos que usted elija explícitamente compartirlos.",
  "helpPrivacy.dataStorage": "Almacenamiento de datos",
  "helpPrivacy.allLocal": "Todos los datos permanecen en su dispositivo",
  "helpPrivacy.allLocalBody":
    "Cada dato de los registros {app} (muestras de EEG sin procesar (CSV), incrustaciones de ZUNA (índice SQLite + HNSW), etiquetas de texto, marcas de tiempo de calibración, registros y configuraciones) se almacena localmente en {dataDir}/. No se cargan datos a ningún servicio en la nube, servidor o tercero.",
  "helpPrivacy.noAccounts": "Sin cuentas de usuario",
  "helpPrivacy.noAccountsBody":
    "{app} no requiere registro, inicio de sesión ni ninguna forma de creación de cuenta. No se almacenan ni transmiten identificadores de usuario, tokens ni credenciales de autenticación.",
  "helpPrivacy.dataLocation": "Ubicación de datos",
  "helpPrivacy.dataLocationBody":
    "Todos los archivos se almacenan en {dataDir}/ en macOS y Linux. Cada día de grabación tiene su propio subdirectorio AAAAMMDD que contiene la base de datos EEG SQLite y el índice vectorial HNSW. Las etiquetas están en {dataDir}/labels.sqlite. Los registros están en {dataDir}/logs/. Puede eliminar cualquiera de estos archivos en cualquier momento.",
  "helpPrivacy.network": "Actividad de red",
  "helpPrivacy.noTelemetry": "Sin telemetría ni análisis",
  "helpPrivacy.noTelemetryBody":
    "{app} no recopila análisis de uso, informes de fallos, telemetría ni ninguna forma de seguimiento del comportamiento. No hay SDK de análisis, píxeles de seguimiento ni balizas de teléfono residencial integrados en la aplicación.",
  "helpPrivacy.localWs": "Servidor WebSocket solo local",
  "helpPrivacy.localWsBody":
    "{app} ejecuta un servidor WebSocket vinculado a su interfaz de red local para la transmisión LAN a herramientas complementarias. Este servidor no está expuesto a Internet. Transmite métricas EEG derivadas (potencias de banda, puntuaciones, frecuencia cardíaca) y actualizaciones de estado a clientes en la misma red local. Los flujos de muestra sin procesar de EEG/PPG/IMU no se transmiten.",
  "helpPrivacy.mdns": "Servicio mDNS/Bonjour",
  "helpPrivacy.mdnsBody":
    "{app} registra un _skill._tcp.local. Servicio mDNS para que los clientes LAN puedan descubrir el puerto WebSocket automáticamente. Este anuncio es sólo local (DNS de multidifusión) y no es visible fuera de su red.",
  "helpPrivacy.updateChecks": "Comprobaciones de actualización",
  "helpPrivacy.updateChecksBody":
    "Cuando hace clic en 'Buscar actualizaciones' en Configuración, {app} se comunica con el punto final de actualización configurado para buscar una versión más nueva. Esta es la única solicitud de Internet saliente que realiza la aplicación y solo ocurre cuando la activa explícitamente. Los paquetes de actualización se verifican con una firma Ed25519 antes de la instalación.",
  "helpPrivacy.bluetooth": "Bluetooth y seguridad del dispositivo",
  "helpPrivacy.ble": "Bluetooth de bajo consumo (BLE)",
  "helpPrivacy.bleBody":
    "{app} se comunica con tu dispositivo BCI mediante Bluetooth Low Energy o serie USB. La conexión usa la pila estándar del sistema: CoreBluetooth (macOS) o BlueZ (Linux). No se instalan drivers Bluetooth personalizados ni módulos de kernel.",
  "helpPrivacy.osPermissions": "Permisos a nivel de sistema operativo",
  "helpPrivacy.osPermissionsBody":
    "El acceso a Bluetooth requiere un permiso explícito del sistema. En macOS, debe otorgar acceso a Bluetooth en Configuración del sistema → Privacidad y seguridad → Bluetooth. {app} no puede acceder a Bluetooth sin su consentimiento.",
  "helpPrivacy.deviceIds": "Identificadores de dispositivos",
  "helpPrivacy.deviceIdsBody":
    "El número de serie del dispositivo y la dirección MAC se reciben del auricular BCI y se muestran en la interfaz de usuario. Estos identificadores se almacenan únicamente en el archivo de configuración local y nunca se transmiten a través de la red.",
  "helpPrivacy.onDevice": "Procesamiento en el dispositivo",
  "helpPrivacy.gpuLocal": "La inferencia de GPU permanece local",
  "helpPrivacy.gpuLocalBody":
    "El codificador integrado ZUNA se ejecuta completamente en su GPU local a través de wgpu. Los pesos del modelo se cargan desde la caché local de Hugging Face (~/.cache/huggingface/). No se envían datos de EEG a ninguna API de inferencia externa ni GPU en la nube.",
  "helpPrivacy.filtering": "Filtrado y análisis",
  "helpPrivacy.filteringBody":
    "Todo el procesamiento de señales (filtrado para guardar superposiciones, cálculo de potencia de banda FFT, generación de espectrogramas y monitoreo de la calidad de la señal) se ejecuta localmente en su CPU/GPU. Ningún dato EEG sin procesar o procesado sale de su máquina.",
  "helpPrivacy.nnSearch": "Búsqueda de vecino más cercano",
  "helpPrivacy.nnSearchBody":
    "El índice de vectores HNSW utilizado para la búsqueda de similitudes se crea y consulta completamente en su dispositivo. Las consultas de búsqueda nunca salen de su máquina.",
  "helpPrivacy.yourData": "Tus datos, tu control",
  "helpPrivacy.access": "Acceso",
  "helpPrivacy.accessBody":
    "Todos sus datos están en {dataDir}/ en formatos estándar (CSV, SQLite, binario HNSW). Puedes leerlo, copiarlo o procesarlo con cualquier herramienta.",
  "helpPrivacy.delete": "Borrar",
  "helpPrivacy.deleteBody":
    "Elimine cualquier archivo o directorio en {dataDir}/ en cualquier momento. No hay que preocuparse por las copias de seguridad en la nube. La desinstalación de la aplicación elimina solo el binario de la aplicación: sus datos en {dataDir}/ no se modifican a menos que los elimine.",
  "helpPrivacy.export": "Exportar",
  "helpPrivacy.exportBody":
    "Las grabaciones CSV y las bases de datos SQLite son formatos estándar portátiles. Cópielos a cualquier máquina o impórtelos a Python, R, MATLAB o cualquier herramienta de análisis.",
  "helpPrivacy.encrypt": "cifrar",
  "helpPrivacy.encryptBody":
    "{app} no cifra datos en reposo. Si necesita cifrado a nivel de disco, utilice el cifrado de disco completo de su sistema operativo (FileVault en macOS, LUKS en Linux).",
  "helpPrivacy.activityTracking": "Seguimiento de actividad",
  "helpPrivacy.activityTrackingBody":
    "Cuando está habilitado, NeuroSkill registra qué aplicación está en primer plano y la última vez que se utilizaron el teclado y el mouse. Estos datos permanecen completamente en su dispositivo en ~/.skill/activity.sqlite; nunca se envían a ningún servidor, no se registran de forma remota ni se incluyen en ningún tipo de análisis. Capturas de seguimiento de ventanas activas: nombre de la aplicación, ruta ejecutable, título de la ventana y marca de tiempo de Unix en la que esa ventana se activó. El seguimiento del teclado y el mouse captura solo dos marcas de tiempo (último evento del teclado, último evento del mouse): nunca pulsaciones de teclas, texto escrito, coordenadas del cursor ni objetivos de clic. Ambas funciones se pueden desactivar de forma independiente en Configuración → Seguimiento de actividad; Al desactivar una función, se detiene inmediatamente la recopilación. Las filas existentes no se eliminan automáticamente, pero puedes eliminarlas en cualquier momento eliminando Activity.sqlite.",
  "helpPrivacy.activityPermission": "Permiso de accesibilidad (macOS)",
  "helpPrivacy.activityPermissionBody":
    "En macOS, el seguimiento del teclado y el mouse requiere el permiso de Accesibilidad porque instala un CGEventTap, un enlace a nivel del sistema que intercepta eventos de entrada. Apple exige este permiso para cualquier aplicación que lea entradas globales. El permiso se solicita solo cuando la función está habilitada. Si lo rechaza o lo revoca, el enlace falla silenciosamente: el resto de la aplicación continúa normalmente y solo las marcas de tiempo de actividad de entrada permanecen en cero. El seguimiento de ventana activa (nombre/ruta de la aplicación) no requiere Accesibilidad: utiliza AppleScript/osascript que funciona dentro de los derechos de aplicación normales.",
  "helpPrivacy.summaryTitle": "Resumen",
  "helpPrivacy.summaryNoCloud":
    "Ninguna nube. Todos los datos, incrustaciones, etiquetas y configuraciones de EEG se almacenan localmente en {dataDir}/.",
  "helpPrivacy.summaryNoTelemetry":
    "Sin telemetría. Sin análisis, informes de fallos ni seguimiento de uso de ningún tipo.",
  "helpPrivacy.summaryNoAccounts": "Sin cuentas. Sin registro, inicio de sesión ni identificadores de usuario.",
  "helpPrivacy.summaryOneReq":
    "Una solicitud de red opcional. Actualice las comprobaciones, solo cuando las active explícitamente.",
  "helpPrivacy.summaryOnDevice":
    "Totalmente en el dispositivo. La inferencia de GPU, el procesamiento de señales y la búsqueda se ejecutan localmente.",
  "helpPrivacy.summaryActivityLocal":
    "El seguimiento de la actividad es solo local. El foco de la ventana y las marcas de tiempo de entrada se escriben en Activity.sqlite en su dispositivo y nunca lo abandonan.",

  "helpFaq.title": "Preguntas frecuentes",
  "helpFaq.q1": "¿Dónde se almacenan mis datos?",
  "helpFaq.a1":
    "Todo se almacena localmente en {dataDir}/: grabaciones CSV sin procesar, índices vectoriales HNSW, bases de datos SQLite integradas, etiquetas, registros y configuraciones. No se envía nada a la nube.",
  "helpFaq.q2": "¿Qué hace el codificador ZUNA?",
  "helpFaq.a2":
    "ZUNA es un codificador transformador acelerado por GPU que convierte épocas de EEG de 5 segundos en vectores de incrustación compactos. Estos vectores capturan la firma neuronal de cada momento y potencian la función de búsqueda de similitudes.",
  "helpFaq.q3": "¿Por qué la calibración requiere un dispositivo conectado?",
  "helpFaq.a3":
    "La calibración ejecuta una tarea cronometrada (por ejemplo, ojos abiertos/ojos cerrados) y registra datos de EEG etiquetados. Sin datos de transmisión en vivo, la calibración no tendría ninguna señal neuronal para asociar con cada fase.",
  "helpFaq.q4": "¿Cómo me conecto desde Python/Node.js?",
  "helpFaq.a4":
    "Descubra el puerto WebSocket a través de mDNS (dns-sd -B _skill._tcp en macOS) y luego abra una conexión WebSocket estándar. Envíe comandos JSON y reciba transmisiones de eventos en vivo. Consulte la pestaña API para obtener detalles sobre el formato de cable.",
  "helpFaq.q5": "¿Qué significan los indicadores de calidad de la señal?",
  "helpFaq.a5":
    "Cada punto representa un electrodo EEG. Verde = buen contacto con la piel, poco ruido. Amarillo = algún artefacto de movimiento o ajuste holgado. Rojo = mucho ruido, mal contacto. Gris = no se detecta señal.",
  "helpFaq.q6": "¿Puedo cambiar la frecuencia del filtro de muesca?",
  "helpFaq.a6":
    "Sí, vaya a Configuración → Procesamiento de señal y elija 50 Hz (Europa, la mayor parte de Asia) o 60 Hz (América, Japón). Esto elimina la interferencia de la línea eléctrica de la pantalla y del cálculo de potencia de banda.",
  "helpFaq.q7": "¿Cómo reinicio un dispositivo emparejado?",
  "helpFaq.a7":
    "Abra Configuración → Dispositivos emparejados, luego haga clic en el botón × junto al dispositivo que desea olvidar. Luego podrás buscarlo nuevamente.",
  "helpFaq.q8": "¿Por qué el ícono de la bandeja se vuelve rojo?",
  "helpFaq.a8":
    "Bluetooth está desactivado en su sistema. Abra Configuración del sistema → Bluetooth y habilítelo. {app} se volverá a conectar automáticamente en aproximadamente 1 segundo.",
  "helpFaq.q9": "La aplicación sigue girando pero nunca se conecta. ¿Qué debo hacer?",
  "helpFaq.a9":
    "1. Asegúrese de que el dispositivo esté encendido (Muse: mantenga presionado hasta que sienta una vibración; Ganglion/Cyton: verifique el LED azul). 2. Manténgalo a menos de 5 m. 3. Si aún falla, reinicie el dispositivo.",
  "helpFaq.q10": "¿Cómo otorgo permiso a Bluetooth?",
  "helpFaq.a10":
    "macOS mostrará un cuadro de diálogo de permiso la primera vez que {app} intente conectarse. Si lo descartó, vaya a Configuración del sistema → Privacidad y seguridad → Bluetooth y habilite {app}.",
  "helpFaq.q11": "¿Qué métricas se almacenan en la base de datos?",
  "helpFaq.a11":
    "Cada época de 2,5 s almacena: el vector de incrustación ZUNA (32-D), potencias de banda relativas (delta, theta, alfa, beta, gamma, gamma alta) promediadas entre canales, potencias de banda por canal como JSON, puntuaciones derivadas (relajación, compromiso), FAA, relaciones de banda cruzada (TAR, BAR, DTR), forma espectral (PSE, APF, BPS, SNR), coherencia, supresión de Mu, estado de ánimo índice y promedios PPG si están disponibles.",
  "helpFaq.q12": "¿Qué es la comparación de sesiones?",
  "helpFaq.a12":
    "Comparar (⌘⇧M) le permite elegir dos rangos de tiempo y compararlos uno al lado del otro: barras de potencia de banda relativa con deltas, todas las puntuaciones y proporciones derivadas, asimetría alfa frontal, hipnogramas de estadificación del sueño y Brain Nebula™, una proyección de incorporación UMAP 3D.",
  "helpFaq.q13": "¿Qué es Brain Nebula™?",
  "helpFaq.a13":
    "Brain Nebula™ (técnicamente: UMAP Embedding Distribution) proyecta incrustaciones de EEG de alta dimensión en un espacio 3D para que estados cerebrales similares aparezcan como puntos cercanos. El rango A (azul) y el rango B (ámbar) forman grupos distintos cuando las sesiones difieren. Puede orbitar, hacer zoom y hacer clic en puntos etiquetados para rastrear conexiones temporales. Se pueden resaltar varias etiquetas simultáneamente en diferentes colores.",
  "helpFaq.q14": "¿Por qué Brain Nebula™ muestra una nube aleatoria al principio?",
  "helpFaq.a14":
    "La proyección UMAP es computacionalmente costosa y se ejecuta en una cola de trabajos en segundo plano para que la interfaz de usuario siga respondiendo. Mientras se realiza la computación, se muestra una nube de marcador de posición aleatoria. Una vez que la proyección está lista, los puntos se animan suavemente hasta sus posiciones finales.",
  "helpFaq.q15": "¿Qué son las etiquetas y cómo se utilizan?",
  "helpFaq.a15":
    "Las etiquetas son etiquetas definidas por el usuario (por ejemplo, 'meditación', 'lectura') adjuntas a un momento durante la grabación. Se almacenan junto con las incrustaciones de EEG. En el visor UMAP, los puntos etiquetados aparecen más grandes con anillos de colores; haga clic en uno para rastrear esa etiqueta a lo largo del tiempo en ambas sesiones.",
  "helpFaq.q16": "¿Qué es la asimetría alfa frontal (FAA)?",
  "helpFaq.a16":
    "FAA es ln(AF8 α) − ln(AF7 α). Los valores positivos sugieren motivación de aproximación (compromiso, curiosidad). Los valores negativos sugieren retraimiento (evitación, ansiedad).",
  "helpFaq.q17": "¿Cómo funciona la puesta en escena del sueño?",
  "helpFaq.a17":
    "Cada época del EEG se clasifica como Wake, N1, N2, N3 o REM según la potencia relativa delta, theta, alfa y beta. La vista de comparación muestra un hipnograma para cada sesión con desgloses de etapas y porcentajes de tiempo.",
  "helpFaq.q18": "¿Cuáles son los atajos de teclado?",
  "helpFaq.a18":
    "⌘⇧O: abre la ventana {app}. ⌘⇧M — Comparación de sesiones abiertas. Personalice los atajos en Configuración → Atajos.",
  "helpFaq.q19": "¿Qué es la API WebSocket?",
  "helpFaq.a19":
    "{app} expone una API JSON WebSocket en la red local (mDNS: _skill._tcp). Comandos: estado, etiqueta, buscar, comparar (métricas + suspensión + ticket UMAP), sesiones, suspensión, umap (poner en cola proyección 3D), umap_poll (recuperar resultado). Ejecute 'node test.js' para realizar una prueba de humo.",
  "helpFaq.q20": "¿Qué son las puntuaciones de relajación y compromiso?",
  "helpFaq.a20":
    "Relajación = α/(β+θ), que mide la vigilia tranquila. Compromiso = β/(α+θ), que mide la implicación mental sostenida. Ambos se asignan de 0 a 100 mediante un sigmoide.",
  "helpFaq.q21": "¿Qué son TAR, BAR y DTR?",
  "helpFaq.a21":
    "TAR (Theta/Alpha): mayor = más somnoliento o más meditativo. BAR (Beta/Alfa): mayor = más estresado o concentrado. DTR (Delta/Theta): mayor = sueño o relajación más profundos. Todo promediado entre canales.",
  "helpFaq.q22": "¿Qué son PSE, APF, BPS y SNR?",
  "helpFaq.a22":
    "PSE (Entropía espectral de potencia, 0–1): complejidad espectral. APF (Frecuencia pico alfa, Hz): frecuencia de potencia alfa máxima. BPS (pendiente de potencia de banda): exponente aperiódico 1/f. SNR (relación señal-ruido, dB): banda ancha frente a ruido de línea.",
  "helpFaq.q23": "¿Qué es la relación Theta/Beta (TBR)?",
  "helpFaq.a23":
    "TBR es la relación entre el poder theta absoluto y el poder beta absoluto. Los valores más altos indican una activación cortical reducida: la TBR elevada se asocia con somnolencia y desregulación de la atención. Referencia: Angelidis et al. (2016).",
  "helpFaq.q24": "¿Qué son los parámetros de Hjorth?",
  "helpFaq.a24":
    "Tres características en el dominio del tiempo de Hjorth (1970): Actividad (varianza de la señal/potencia total), Movilidad (estimación de la frecuencia media) y Complejidad (ancho de banda/desviación de un seno puro). Son computacionalmente baratos y ampliamente utilizados en canalizaciones de EEG ML.",
  "helpFaq.q25": "¿Qué medidas de complejidad no lineales se calculan?",
  "helpFaq.a25":
    "Cuatro medidas: entropía de permutación (complejidad del patrón ordinal, Bandt y Pompe 2002), dimensión fractal de Higuchi (estructura fractal de la señal, Higuchi 1988), exponente DFA (correlaciones temporales de largo alcance, Peng et al. 1994) y entropía de muestra (regularidad de la señal, Richman y Moorman 2000). Todos se promedian en los 4 canales de EEG.",
  "helpFaq.q26": "¿Qué son SEF95, centroide espectral, PAC y índice de lateralidad?",
  "helpFaq.a26":
    "SEF95 (Frecuencia de borde espectral) es la frecuencia por debajo de la cual se encuentra el 95 % de la potencia total y se utiliza en la monitorización de la anestesia. El centroide espectral es la frecuencia media ponderada en potencia (indicador de excitación). PAC (acoplamiento de amplitud de fase) mide la interacción de frecuencia cruzada theta-gamma asociada con la codificación de la memoria. El índice de lateralidad es la asimetría de poder generalizada izquierda/derecha en todas las bandas.",
  "helpFaq.q27": "¿Qué métricas de PPG se calculan?",
  "helpFaq.a27":
    "En Muse 2/S (con sensor PPG): frecuencia cardíaca (lpm) a partir de la detección de pico IR, RMSSD/SDNN/pNN50 (variabilidad de la frecuencia cardíaca: tono parasimpático), relación LF/HF (equilibrio simpatovagal), frecuencia respiratoria (respiraciones/min de la envoltura PPG), estimación de SpO₂ (oxígeno en sangre no calibrado a partir de la relación rojo/IR), índice de perfusión (flujo sanguíneo periférico) e índice de estrés de Baevsky (estrés autónomo). Estos aparecen en la sección PPG Vitals cuando se conecta una diadema equipada con PPG.",
  "helpFaq.q28": "¿Cómo uso el temporizador de enfoque?",
  "helpFaq.a28":
    'Abra el Temporizador de enfoque a través del menú de la bandeja, la Paleta de comandos (⌘K → "Temporizador de enfoque") o el acceso directo global (⌘⇧P de forma predeterminada). Elija un ajuste preestablecido: Pomodoro (25/5), Trabajo profundo (50/10) o Enfoque corto (15/5), o establezca duraciones personalizadas. Habilite "Etiquetado automático de EEG" para que NeuroSkill™ etiquete automáticamente las grabaciones de EEG al inicio y al final de cada fase de enfoque. Los puntos de sesión rastrean tus rondas completadas. Sus configuraciones preestablecidas y personalizadas se guardan automáticamente y se restauran la próxima vez que abra el temporizador.',
  "helpFaq.q29": "¿Cómo administro o edito mis anotaciones?",
  "helpFaq.a29":
    'Abra la ventana Etiquetas a través de la Paleta de comandos (⌘K → "Todas las etiquetas"). Muestra todas las anotaciones con edición de texto en línea (haga clic en una etiqueta, presione ⌘↵ para guardar o Esc para cancelar), eliminar (con confirmación) y metadatos que muestran el rango de tiempo del EEG. Utilice el cuadro de búsqueda para filtrar por texto. Las etiquetas están paginadas a razón de 50 por página para archivos grandes.',
  "helpFaq.q30": "¿Cómo comparo dos sesiones específicas una al lado de la otra?",
  "helpFaq.a30":
    'Desde la página Historial, haga clic en "Comparación rápida" para ingresar al modo de comparación. Aparecen casillas de verificación en cada fila de sesión: seleccione exactamente dos, luego haga clic en "Comparar seleccionados" para abrir la ventana Comparar precargada con ambas sesiones. Alternativamente, abra Comparar desde la bandeja o la Paleta de comandos y use los menús desplegables de la sesión manualmente.',
  "helpFaq.q31": "¿Cómo funciona la búsqueda con incrustación de texto?",
  "helpFaq.a31":
    'Su consulta se convierte en un vector mediante el mismo modelo de transformador de oraciones que indexa sus etiquetas. Luego, ese vector se busca en el índice de etiquetas HNSW utilizando una búsqueda aproximada del vecino más cercano. Los resultados son sus propias anotaciones clasificadas por similitud semántica, por lo que al buscar "tranquilo y concentrado" aparecerán etiquetas como "lectura profunda" o "meditación" incluso si esas palabras exactas nunca aparecieron en su consulta. Requiere descargar el modelo de incrustación y crear el índice de etiquetas (Configuración → Incrustaciones).',
  "helpFaq.q32": "¿Cómo funciona la búsqueda intermodal interactiva?",
  "helpFaq.a32":
    'La búsqueda interactiva une texto, EEG y tiempo en una sola consulta. Paso 1: su consulta de texto está incrustada. Paso 2: se encuentran las etiquetas semánticamente similares text-k superiores. Paso 3: para cada etiqueta, {app} calcula la incrustación media del EEG en su ventana de grabación y recupera las épocas de EEG más cercanas del eeg-k superior de todos los índices diarios, cruzando desde el lenguaje al espacio del estado cerebral. Paso 4: para cada momento de EEG encontrado, cualquier anotación dentro de ± minutos de alcance se recopila como "etiquetas encontradas". Las cuatro capas de nodos (Consulta → Coincidencias de texto → Vecinos EEG → Etiquetas encontradas) se representan como un gráfico dirigido de 4 capas. Exporte como SVG para una imagen estática o como fuente DOT para su posterior procesamiento en Graphviz.',

  "helpOld.hooksTitle": "Ganchos proactivos",
  "helpOld.hooksDesc":
    "Los ganchos escuchan en segundo plano: coincidencia difusa de palabras clave → expansión de texto-vecino → verificación de distancia EEG. Si coinciden, la aplicación transmite un evento de enlace y muestra una notificación.",
  "helpOld.hooksFlow": "lindo flujo",
  "helpOld.hooksFaqQ": "¿Cómo se dispara un gancho?",
  "helpOld.hooksFaqA":
    "El trabajador compara cada nueva incorporación de EEG con ejemplos de etiquetas recientes seleccionados por palabra clave + similitud de texto. Si la mejor distancia del coseno está por debajo de su umbral, el gancho se dispara.",
  "helpOld.trayIconStates": "Estados del icono de la bandeja",
  "helpOld.trayIconDesc":
    "El ícono de la barra de menú cambia de color y forma para reflejar el estado actual de la conexión de un vistazo.",
  "helpOld.greyDisconnected": "Gris: desconectado",
  "helpOld.greyDesc": "Bluetooth está activado; no hay ningún dispositivo BCI conectado.",
  "helpOld.spinningScanning": "Girar - Escanear",
  "helpOld.spinningDesc": "Buscando un dispositivo BCI o intentando conectarse.",
  "helpOld.greenConnected": "Verde: conectado",
  "helpOld.greenDesc": "Transmisión de datos de EEG en vivo desde su dispositivo BCI.",
  "helpOld.redBtOff": "Rojo: Bluetooth desactivado",
  "helpOld.redDesc": "La radio Bluetooth está apagada. No es posible escanear ni conectar.",
  "helpOld.btLifecycle": "Ciclo de vida de Bluetooth y reconexión automática",
  "helpOld.btLifecycleDesc":
    "{app} monitorea el estado de Bluetooth en tiempo real usando CoreBluetooth (macOS) o BlueZ (Linux). Sin demoras en las encuestas: los cambios de estado se reflejan en un segundo.",
  "helpOld.btStep1": "Bluetooth se apaga",
  "helpOld.btStep1Desc":
    "El icono de la bandeja se vuelve rojo al instante. La tarjeta Bluetooth-Off reemplaza la vista principal. No se realiza ningún escaneo ni intento de conexión.",
  "helpOld.btStep2": "Bluetooth se vuelve a encender",
  "helpOld.btStep2Desc":
    "En aproximadamente 1 segundo, {app} reanuda el escaneo automáticamente. Si un dispositivo preferido se emparejó previamente, se inicia un intento de conexión.",
  "helpOld.btStep3": "El dispositivo BCI está encendido",
  "helpOld.btStep3Desc":
    "El escáner en segundo plano lo detecta en 3 a 6 segundos y se conecta automáticamente; el icono se vuelve verde.",
  "helpOld.btStep4": "Dispositivo no encontrado inmediatamente",
  "helpOld.btStep4Desc":
    "{app} reintenta silenciosamente cada 3 segundos. La ruleta permanece visible hasta que se encuentra el dispositivo o lo cancelas manualmente.",
  "helpOld.btStep5": "Haces clic en Reintentar",
  "helpOld.btStep5Desc":
    "El mismo bucle de reintento automático que la reconexión automática. Vuelve a intentarlo cada 3 segundos hasta que se encuentre el dispositivo.",
  "helpOld.examples": "Ejemplos",
  "helpOld.example1Title": "Ejemplo 1: inicio normal",
  "helpOld.example2Title": "Ejemplo 2: Bluetooth desactivado y activado",
  "helpOld.example3Title": "Ejemplo 3: dispositivo BCI encendido después de restaurar BT",
  "helpOld.ex1Step1": "{app} se abre → buscando dispositivo BCI",
  "helpOld.ex1Step2": "Dispositivo encontrado en 5 s",
  "helpOld.ex1Step3": "Conectado: transmisión de EEG",
  "helpOld.ex2Step1": "Conectado → el usuario desactiva Bluetooth",
  "helpOld.ex2Step2": 'El icono se vuelve rojo; Se muestra la tarjeta "Bluetooth desactivado"',
  "helpOld.ex2Step3": "… el usuario vuelve a habilitar Bluetooth …",
  "helpOld.ex2Step4": "Se reanuda el escaneo automático (~1 s)",
  "helpOld.ex2Step5": "Reconectado: se reanudó la transmisión",
  "helpOld.ex3Step1": "BT activado, dispositivo aún apagado → reinténtalo cada 3 s",
  "helpOld.ex3Step2": "… el usuario enciende el dispositivo BCI…",
  "helpOld.ex3Step3": "Dispositivo descubierto en el siguiente ciclo de escaneo",
  "helpOld.ex3Step4": "Conectado automáticamente: no es necesario presionar ningún botón",
  "helpOld.broadcastEvents": "Eventos de transmisión (servidor → cliente)",
  "helpOld.commands": "Comandos (cliente → servidor)",
  "helpOld.wsTitle": "Transmisión en red local (WebSocket)",
  "helpOld.wsDesc":
    "{app} transmite métricas de EEG derivadas (potencias de banda de ~4 Hz, puntuaciones, frecuencia cardíaca) y el estado del dispositivo (~1 Hz) a través de un servidor WebSocket local. Las muestras sin procesar de EEG/PPG/IMU no se transmiten. El servicio se anuncia a través de Bonjour/mDNS para que los clientes puedan descubrirlo sin conocer la dirección IP.",
  "helpOld.discoverService": "Descubre el servicio",
  "helpOld.wireFormat": "Formato de cable (JSON)",
  "helpOld.faq": "Preguntas frecuentes",
  "helpOld.faqQ1": "¿Por qué el ícono de la bandeja se vuelve rojo?",
  "helpOld.faqA1":
    "Bluetooth está desactivado en tu Mac. Abra Configuración del sistema → Bluetooth y habilítelo. {app} se volverá a conectar automáticamente en aproximadamente 1 segundo.",
  "helpOld.faqQ2": "La aplicación sigue girando pero nunca se conecta. ¿Qué debo hacer?",
  "helpOld.faqA2":
    "1. Asegúrese de que el dispositivo BCI esté encendido (Muse: mantenga presionado hasta que sienta una vibración; Ganglion/Cyton: verifique el LED azul). 2. Manténgalo a menos de 5 m. 3. Si aún falla, reinicie el dispositivo.",
  "helpOld.faqQ3": "¿Cómo otorgo permiso a Bluetooth?",
  "helpOld.faqA3":
    "macOS mostrará un cuadro de diálogo de permiso la primera vez que {app} intente conectarse. Si lo descartó, vaya a Configuración del sistema → Privacidad y seguridad → Bluetooth y habilite {app}.",
  "helpOld.faqQ4": "¿Puedo recibir datos de EEG en otra aplicación en la misma red?",
  "helpOld.faqA4":
    "Sí. Conecte un cliente WebSocket a la dirección que se muestra en el resultado de descubrimiento de Bonjour (consulte la sección Transmisión de red local más arriba). Recibirá métricas derivadas (~4 Hz eventos de bandas eeg con más de 60 puntuaciones) y el estado del dispositivo (~1 Hz). Nota: los flujos de muestra de EEG/PPG/IMU sin procesar no están disponibles a través de la API WebSocket; solo puntuaciones procesadas y potencias de banda.",
  "helpOld.faqQ5": "¿Dónde se guardan mis grabaciones de EEG?",
  "helpOld.faqA5":
    "Las muestras sin procesar (sin filtrar) se escriben en un archivo CSV en la carpeta de datos de su aplicación ({dataDir}/ en macOS/Linux). Se crea un archivo por sesión.",
  "helpOld.faqQ6": "¿Qué significan los puntos de calidad de la señal?",
  "helpOld.faqA6":
    "Cada punto representa un canal EEG (TP9, AF7, AF8, TP10). Verde = Bueno (bajo nivel de ruido, buen contacto con la piel). Amarillo = Aceptable (algún artefacto de movimiento o electrodo suelto). Rojo = Deficiente (alto ruido, contacto muy flojo o electrodo fuera de la piel). Gris = Sin señal.",
  "helpOld.faqQ7": "¿Para qué sirve el filtro de muesca powerline?",
  "helpOld.faqA7":
    "La red eléctrica induce ruidos de 50 o 60 Hz en las grabaciones de EEG. El filtro de muesca elimina esa frecuencia (y sus armónicos) de la visualización de forma de onda. Seleccione 60 Hz (EE. UU./Japón) o 50 Hz (UE/Reino Unido) para que coincida con su red eléctrica local.",
  "helpOld.faqQ8": "¿Qué métricas se almacenan en la base de datos?",
  "helpOld.faqA8":
    "Cada época de 2,5 segundos almacena: el vector de incrustación ZUNA (32-D), potencias de banda relativas (delta, theta, alfa, beta, gamma, gamma alta) promediadas entre canales, potencias de banda por canal como un blob JSON, puntuaciones derivadas (relajación, compromiso), asimetría alfa frontal (FAA), relaciones de banda cruzada (TAR, BAR, DTR, TBR), forma espectral (PSE, APF, SEF95, centroide espectral, BPS, SNR), coherencia, supresión de Mu, composición del estado de ánimo, parámetros de Hjorth (actividad, movilidad, complejidad), complejidad no lineal (entropía de permutación, FD de Higuchi, DFA, entropía de muestra), PAC (θ–γ), índice de lateralidad, promedios de PPG y métricas derivadas de PPG (HR, RMSSD, SDNN, pNN50, LF/HF, frecuencia respiratoria, SpO₂, índice de perfusión, índice de estrés) si hay un Muse 2/S conectado.",
  "helpOld.faqQ9": "¿Qué es la función de comparación de sesiones?",
  "helpOld.faqA9":
    "Comparación de sesiones (⌘⇧M) le permite elegir dos sesiones de grabación y compararlas una al lado de la otra. Muestra: barras de potencia de banda relativa con deltas, todas las puntuaciones y proporciones derivadas, asimetría alfa frontal, hipnogramas de estadificación del sueño y una proyección de incorporación UMAP 3D que visualiza cuán similares son las dos sesiones en un espacio de características de alta dimensión.",
  "helpOld.faqQ10": "¿Qué es el visor UMAP 3D?",
  "helpOld.faqA10":
    "El visor UMAP proyecta incrustaciones de EEG de alta dimensión en un espacio 3D para que estados cerebrales similares aparezcan como puntos cercanos. La sesión A (azul) y la sesión B (ámbar) forman grupos distintos si las sesiones son diferentes. Puede orbitar, hacer zoom y hacer clic en puntos etiquetados para ver sus conexiones temporales.",
  "helpOld.faqQ11": "¿Por qué el visor UMAP muestra al principio una nube aleatoria?",
  "helpOld.faqA11":
    "UMAP es costoso desde el punto de vista computacional: se ejecuta en una cola de trabajos en segundo plano para que la interfaz de usuario siga respondiendo. Mientras se calcula, se muestra una nube de marcador de posición gaussiana aleatoria. Una vez que la proyección real está lista, los puntos se animan suavemente hasta sus posiciones finales.",
  "helpOld.faqQ12": "¿Qué son las etiquetas y cómo se utilizan?",
  "helpOld.faqA12":
    "Las etiquetas son etiquetas definidas por el usuario (por ejemplo, 'meditación', 'lectura', 'ansioso') que usted adjunta a un momento en el tiempo durante una grabación. Se almacenan junto con las incorporaciones de EEG en la base de datos. En el visor UMAP, los puntos etiquetados aparecen como puntos más grandes con anillos de colores.",
  "helpOld.faqQ13": "¿Qué es la asimetría alfa frontal (FAA)?",
  "helpOld.faqA13":
    "FAA es ln(AF8 α) − ln(AF7 α). Un valor positivo sugiere una mayor supresión alfa en el hemisferio izquierdo, asociada con la motivación de aproximación (compromiso, curiosidad). Un valor negativo sugiere retraimiento (evitación, ansiedad).",
  "helpOld.faqQ14": "¿Cómo funciona la puesta en escena del sueño?",
  "helpOld.faqA14":
    "{app} clasifica cada época de EEG en sueño Wake, N1 (ligero), N2, N3 (profundo) o REM según las relaciones de potencia relativas delta, theta, alfa y beta. La vista de comparación muestra un hipnograma para cada sesión con desgloses de etapas codificados por colores y porcentajes de tiempo.",
  "helpOld.faqQ15": "¿Cuáles son los atajos de teclado?",
  "helpOld.faqA15":
    "⌘⇧O: abre la ventana {app}. ⌘⇧M — Comparación de sesiones abiertas. Puede personalizar los atajos en Configuración → Atajos.",
  "helpOld.faqQ16": "¿Qué es la API WebSocket?",
  "helpOld.faqA16":
    "{app} expone una API WebSocket basada en JSON en la red local (mDNS: _skill._tcp). Los comandos incluyen: estado, etiqueta, búsqueda, comparación, sesiones, suspensión, umap y umap_poll. Ejecute 'node test.js' desde el directorio del proyecto para probar todos los comandos.",
  "helpOld.faqQ17": "¿Cuáles son las puntuaciones derivadas (Relajación, Compromiso)?",
  "helpOld.faqA17":
    "Relajación = α / (β + θ), que mide la vigilia tranquila. Compromiso = β / (α + θ), que mide la implicación mental sostenida. Ambos están mapeados en una escala de 0 a 100.",
  "helpOld.faqQ18": "¿Cuáles son las relaciones entre bandas?",
  "helpOld.faqA18":
    "TAR (Theta/Alpha): los valores más altos indican somnolencia o estados meditativos. BAR (Beta/Alfa): los valores más altos indican estrés o atención concentrada. DTR (Delta/Theta): los valores más altos indican sueño profundo o relajación profunda. Todos se promedian entre canales.",
  "helpOld.faqQ19": "¿Qué son PSE, APF, BPS y SNR?",
  "helpOld.faqA19":
    "PSE (Entropía espectral de potencia, 0–1) mide la complejidad espectral. APF (Alpha Peak Frequency, Hz) es la frecuencia de máxima potencia alfa. BPS (pendiente de banda-potencia) es el exponente aperiódico 1/f. SNR (relación señal-ruido, dB) compara la potencia de banda ancha con el ruido de línea de 50 a 60 Hz.",

  "helpTabs.tts": "Voz",

  "helpApi.cmdSay": "decir",
  "helpApi.cmdSayParams": "texto: cadena (obligatorio)",
  "helpApi.cmdSayDesc":
    "Speak text via on-device TTS. Fire-and-forget — returns immediately while audio plays in the background. Initialises the TTS engine on first call.",

  "helpFaq.q33": "¿Cómo activo el discurso TTS desde un script o una herramienta de automatización?",
  "helpFaq.a33":
    'Utilice WebSocket o API HTTP. WebSocket: envía {"command":"say","text":"your message"}. HTTP (curl): curl -X POST http://localhost:<port>/say -H \\\'Tipo de contenido: aplicación/json\\\' -d \\\'{"text":"your message"}\\\'. El comando decir es disparar y olvidar: responde inmediatamente mientras el audio se reproduce en segundo plano.',
  "helpFaq.q34": "¿Por qué no hay sonido de TTS?",
  "helpFaq.a34":
    "Verifique que espeak-ng esté instalado en PATH (brew install espeak-ng en macOS, apt install espeak-ng en Ubuntu). Verifique que la salida de audio de su sistema no esté silenciada ni enrutada a un dispositivo diferente. En la primera ejecución, el modelo (~30 MB) debe terminar de descargarse antes de que se escuche algún sonido. Habilite el registro de depuración TTS en Configuración → Voz para ver los eventos de síntesis en el archivo de registro.",
  "helpFaq.q35": "¿Puedo cambiar la voz o el idioma de TTS?",
  "helpFaq.a35":
    "La versión actual utiliza la voz Jasper English (en-us) del modelo KittenML/kitten-tts-mini-0.8. Sólo el texto en inglés está fonemizado correctamente. Se planean voces adicionales y soporte de idiomas para futuras versiones.",
  "helpFaq.q36": "¿TTS requiere una conexión a Internet?",
  "helpFaq.a36":
    "Solo una vez, para la descarga inicial del modelo de ~30 MB desde HuggingFace Hub. Después de eso, toda la síntesis se ejecuta completamente fuera de línea. El modelo se almacena en caché en ~/.cache/huggingface/hub/ y se reutiliza en cada lanzamiento posterior.",
  "helpFaq.q37": "¿Qué placas OpenBCI admite NeuroSkill™?",
  "helpFaq.a37":
    "NeuroSkill™ es compatible con todas las placas del ecosistema OpenBCI a través del crate openbci publicado (crates.io/crates/openbci): Ganglion (4 canales, BLE), Ganglion + WiFi Shield (4 canales, 1 kHz), Cyton (8 canales, dongle USB), Cyton + WiFi Shield (8 canales, 1 kHz), Cyton+Daisy (16 canales, dongle USB), Cyton+Daisy + WiFi Shield (16 canales, 1 kHz) y Galea (24 canales, UDP). Cualquier placa se puede utilizar junto con otro dispositivo BCI. Selecciona la placa en Configuración → OpenBCI y luego haz clic en Conectar.",
  "helpFaq.q38": "¿Cómo conecto el Ganglion a través de Bluetooth?",
  "helpFaq.a38":
    '1. Enciende el Ganglion; el LED azul debería parpadear lentamente. 2. En Configuración → OpenBCI selecciona "Ganglion — 4ch · BLE". 3. Guarda la configuración y luego haz clic en Conectar. NeuroSkill™ escanea hasta el tiempo de espera configurado (predeterminado 10 s). Mantén la placa a una distancia de entre 3 y 5 m. En macOS, otorga permiso de Bluetooth cuando se solicite (o ve a Configuración del sistema → Privacidad y seguridad → Bluetooth).',
  "helpFaq.q39": "Mi Ganglion está encendido pero NeuroSkill™ no puede encontrarlo. ¿Qué debo intentar?",
  "helpFaq.a39":
    "1. Confirme que el LED azul esté parpadeando (fijo o apagado significa que no hay publicidad; presione el botón para activarlo). 2. Aumente el tiempo de espera del escaneo BLE en Configuración → OpenBCI. 3. Mueva la tabla a una distancia máxima de 2 m. 4. Salga de NeuroSkill™ y vuelva a abrirlo para restablecer el adaptador BLE. 5. Desactive y vuelva a activar Bluetooth en Configuración del sistema. 6. Asegúrese de que no haya ninguna otra aplicación (GUI de OpenBCI, otra instancia de NeuroSkill™) conectada: BLE solo permite una central a la vez. 7. En macOS 14+, verifique que NeuroSkill™ tenga permiso de Bluetooth en Configuración del sistema → Privacidad y seguridad → Bluetooth.",
  "helpFaq.q40": "¿Cómo conecto un Cyton a través de USB?",
  "helpFaq.a40":
    '1. Conecte la llave de radio USB a su computadora (la llave es la radio; la placa Cyton en sí no tiene puerto USB). 2. Encienda el Cyton: deslice el interruptor de encendido a la PC. 3. En Configuración → OpenBCI seleccione "Cyton — 8ch · USB serial". 4. Haga clic en Actualizar para enumerar los puertos serie, luego seleccione el puerto (/dev/cu.usbserial-… en macOS, /dev/ttyUSB0 en Linux, COM3 en Windows) o déjelo en blanco para la detección automática. 5. Guarde la configuración y haga clic en Conectar.',
  "helpFaq.q41": "El puerto serie no aparece en la lista o aparece un error de permiso denegado. ¿Cómo lo soluciono?",
  "helpFaq.a41":
    "macOS: el dongle aparece como /dev/cu.usbserial-*. Si no está presente, instale el controlador CP210x o FTDI VCP desde el sitio del fabricante del chip. Linux: ejecute sudo usermod -aG dialout $USER, luego cierre sesión y vuelva a iniciarla. Verifique que el dispositivo aparezca en /dev/ttyUSB0 o /dev/ttyACM0 después de conectarlo. Windows: instale el controlador CP2104 USB a UART; el puerto COM aparecerá en Administrador de dispositivos → Puertos (COM y LPT).",
  "helpFaq.q42": "¿Cómo me conecto a través de OpenBCI WiFi Shield?",
  "helpFaq.a42":
    "1. Apile el WiFi Shield encima del Cyton o Ganglion y encienda la placa. 2. En su computadora, conéctese a la red WiFi que transmite el escudo (SSID: OpenBCI-XXXX, generalmente sin contraseña). 3. En Configuración → OpenBCI seleccione la variante de placa WiFi correspondiente. 4. Ingrese IP 192.168.4.1 (escudo predeterminado) o déjelo en blanco para el descubrimiento automático. 5. Haga clic en Conectar. WiFi Shield transmite a 1000 Hz: configure el filtro de paso bajo en ≤ 500 Hz en Procesamiento de señal para evitar alias.",
  "helpFaq.q43": "¿Qué es el tablero Galea y cómo lo configuro?",
  "helpFaq.a43":
    'Galea de OpenBCI es un auricular de investigación de bioseñales de 24 canales que combina sensores EEG, EMG y AUX, que se transmiten a través de UDP. Para conectarse: 1. Encienda Galea y conéctelo a su red local. 2. En Configuración → OpenBCI seleccione "Galea — 24ch · UDP". 3. Ingrese la dirección IP de Galea (o déjela en blanco para aceptarla de cualquier remitente). 4. Haga clic en Conectar. Los canales 1 a 8 son EEG (impulsa el análisis en tiempo real); 9 a 16 son EMG; 17–24 son auxiliares. Los 24 se guardan en CSV.',
  "helpFaq.q44": "¿Puedo utilizar dos dispositivos BCI al mismo tiempo?",
  "helpFaq.a44":
    "Sí, NeuroSkill™ puede transmitir desde ambos simultáneamente. Cualquiera que sea el dispositivo que se conecte primero, controlará el panel en vivo, la pantalla de potencia de banda y el canal de integración de ZUNA. Los datos del segundo dispositivo se registran en CSV para su análisis fuera de línea. Está previsto un análisis multidispositivo simultáneo en tiempo real para una versión futura.",
  "helpFaq.q45": "Sólo 4 de los 8 canales de mi Cyton se utilizan para análisis en vivo, ¿por qué?",
  "helpFaq.a45":
    "El proceso de análisis en tiempo real (filtros, potencias de banda, incrustaciones de ZUNA, puntos de calidad de señal) está diseñado actualmente para entradas de 4 canales para que coincidan con el formato de los auriculares Muse. Para Cyton (8 canales) y Cyton+Daisy (16 canales), los canales 1 a 4 alimentan la canalización en vivo; Todos los canales están escritos en CSV para trabajar sin conexión. El soporte total de canalización multicanal está en la hoja de ruta.",
  "helpFaq.q46": "¿Cómo mejoro la calidad de la señal en una placa OpenBCI?",
  "helpFaq.a46":
    "1. Aplique gel o pasta conductora en cada sitio del electrodo y separe el cabello para hacer contacto directo con el cuero cabelludo. 2. Verifique la impedancia con la verificación de impedancia de la GUI de OpenBCI antes de grabar; apunte a < 20 kΩ. 3. Conecte el electrodo de polarización SRB a la mastoides (detrás de la oreja) para obtener una referencia sólida. 4. Mantenga los cables de los electrodos cortos y alejados de las fuentes de alimentación. 5. Utilice el filtro de muesca en Configuración → Procesamiento de señal (50 Hz para Europa, 60 Hz para América). 6. Para Ganglion BLE: aleje la placa de los puertos USB 3.0, que emiten interferencias de 2,4 GHz.",
  "helpFaq.q47": "Mi conexión OpenBCI se cae repetidamente: ¿cómo la estabilizo?",
  "helpFaq.a47":
    "Ganglion BLE: mantén la placa a menos de 2 m; conecta el adaptador BLE del equipo host a un puerto USB 2.0 (USB 3.0 emite ruido de 2,4 GHz que puede degradar BLE). Cyton USB: usa un cable USB corto y de alta calidad, conectado directamente al ordenador en lugar de un hub. WiFi Shield: asegúrate de que el canal de 2,4 GHz del shield no se superponga con tu router; acerca la placa. En general, evita ejecutar otras aplicaciones con alto uso inalámbrico (videollamadas, sincronización de archivos) durante las grabaciones.",
  "helpFaq.q48": "¿Qué registra exactamente el seguimiento de actividad?",
  "helpFaq.a48":
    'El seguimiento de ventanas activas escribe una fila en Activity.sqlite cada vez que cambia el título de la ventana o aplicación frontal. Cada fila contiene: el nombre para mostrar de la aplicación (por ejemplo, "Safari", "Código VS"), la ruta completa al binario o al paquete de la aplicación, el título de la ventana (por ejemplo, el nombre del documento o el título de la página web; puede estar vacío para aplicaciones en espacio aislado) y una marca de tiempo de un segundo de Unix de cuándo se activó. El seguimiento del teclado y el mouse escribe una muestra periódica cada 60 segundos, pero solo cuando ha habido actividad desde la última descarga. Cada muestra almacena dos marcas de tiempo de Unix en segundos: el último evento del teclado y el último evento del mouse/trackpad. No registra qué teclas presionó, qué texto escribió, dónde estaba el cursor o en qué botones hizo clic. Ambas funciones están habilitadas de forma predeterminada y se pueden desactivar de forma independiente en Configuración → Seguimiento de actividad.',
  "helpFaq.q49": "¿Por qué macOS solicita acceso de Accesibilidad para el seguimiento de entradas?",
  "helpFaq.a49":
    'El seguimiento del teclado y el mouse utiliza CGEventTap, una API de macOS que intercepta eventos de entrada en todo el sistema antes de que lleguen a aplicaciones individuales. Apple requiere el permiso de Accesibilidad para cualquier aplicación que lea entradas globales, independientemente de lo que esa aplicación haga con ella. Sin acceso a Accesibilidad, el grifo falla silenciosamente: NeuroSkill continúa funcionando normalmente, pero las marcas de tiempo del último teclado y del último mouse permanecen en cero. Para otorgar acceso: Configuración del sistema → Privacidad y seguridad → Accesibilidad → buscar NeuroSkill → activar. Si prefiere no otorgarlo, desactive la opción "Seguimiento de actividad del teclado y el mouse" en Configuración; esto evita que el gancho se instale en primer lugar. El seguimiento de ventanas activas (nombre y ruta de la aplicación) utiliza AppleScript/osascript y no requiere permiso de Accesibilidad.',
  "helpFaq.q50": "¿Cómo borro o elimino los datos de seguimiento de actividad?",
  "helpFaq.a50":
    "Todos los datos de seguimiento de actividad se encuentran en un solo archivo: ~/.skill/activity.sqlite. Para eliminar todo: salga de NeuroSkill, elimine ese archivo y luego reinicie; se crea automáticamente una base de datos vacía en el siguiente inicio. Para detener la recopilación futura sin tocar los datos existentes, desactive ambas opciones en Configuración → Seguimiento de actividad; Los cambios entran en vigor inmediatamente sin necesidad de reiniciar. Para eliminar filas de forma selectiva, puede abrir el archivo en cualquier navegador SQLite (por ejemplo, DB Browser para SQLite) y ELIMINAR de active_windows o input_activity.",
  "helpFaq.q51": "¿Por qué {app} solicita permiso de Accesibilidad en macOS?",
  "helpFaq.a51":
    "{app} usa la API CGEventTap de macOS para registrar la última vez que se presionó una tecla o se movió el mouse. Esto se utiliza para calcular las marcas de tiempo de actividad del teclado y el mouse que se muestran en el panel Seguimiento de actividad. Sólo se almacena la marca de tiempo, sin pulsaciones de teclas ni posiciones del cursor. La función se degrada silenciosamente si no se concede el permiso.",
  "helpFaq.q52": "¿{app} necesita permiso de Bluetooth?",
  "helpFaq.a52":
    "Sí. {app} utiliza Bluetooth Low Energy (BLE) para conectarse a sus auriculares BCI. En macOS, el sistema mostrará un mensaje de permiso de Bluetooth por única vez cuando la aplicación intente escanear por primera vez. En Linux y Windows no se requiere ningún permiso explícito de Bluetooth.",
  "helpFaq.q53": "¿Cómo otorgo permiso de Accesibilidad en macOS?",
  "helpFaq.a53":
    'Abra Configuración del sistema → Privacidad y seguridad → Accesibilidad. Busque {app} en la lista y actívelo. También puedes hacer clic en "Abrir configuración de accesibilidad" en la pestaña Permisos dentro de la aplicación.',
  "helpFaq.q54": "¿Qué pasa si niego el permiso de Accesibilidad?",
  "helpFaq.a54":
    "Las marcas de tiempo de actividad del teclado y el mouse no se registrarán y permanecerán en cero. Todas las demás funciones (transmisión de EEG, potencia de banda, calibración, TTS, búsqueda) continúan funcionando normalmente. Puede desactivar la función por completo en Configuración → Seguimiento de actividad.",
  "helpFaq.q55": "¿Puedo revocar permisos después de otorgarlos?",
  "helpFaq.a55":
    "Sí. Abra Configuración del sistema → Privacidad y seguridad → Accesibilidad (o Notificaciones) y desactive {app}. La característica relevante dejará de funcionar inmediatamente sin necesidad de reiniciar.",

  "helpTabs.llm": "LLM",

  "helpLlm.overviewSection": "Descripción general",
  "helpLlm.overviewSectionDesc":
    "NeuroSkill incluye un servidor LLM local opcional que le brinda un asistente de inteligencia artificial privado compatible con OpenAI sin enviar ningún dato a la nube.",
  "helpLlm.whatIsTitle": "¿Qué es la función LLM?",
  "helpLlm.whatIsBody":
    "La función LLM incorpora un servidor de inferencia respaldado por llama.cpp directamente dentro de la aplicación. Cuando está habilitado, sirve puntos finales compatibles con OpenAI (/v1/chat/completions, /v1/completions, /v1/embeddings, /v1/models, /health) en el mismo puerto local que la API WebSocket. Puede apuntar a cualquier cliente compatible con OpenAI (Chatbot UI, Continuar, Open Interpreter o sus propios scripts).",
  "helpLlm.privacyTitle": "Privacidad y uso sin conexión",
  "helpLlm.privacyBody":
    "Toda la inferencia se ejecuta en su máquina. Ningún token, aviso o finalización sale nunca de localhost. La única actividad de la red es la descarga inicial del modelo desde HuggingFace Hub. Una vez que un modelo se almacena en caché localmente, puedes desconectarte de Internet por completo.",
  "helpLlm.compatTitle": "API compatible con OpenAI",
  "helpLlm.compatBody":
    "El servidor habla el mismo protocolo que la API OpenAI. Cualquier biblioteca que acepte un parámetro base_url (openai-python, openai-node, LangChain, LlamaIndex, etc.) funciona de inmediato. Establezca base_url en http://localhost:<port>/v1 y deje la clave API vacía a menos que haya configurado una en Configuración de inferencia.",
  "helpLlm.modelsSection": "Gestión de modelos",
  "helpLlm.modelsSectionDesc":
    "Explore, descargue y active modelos de lenguaje cuantificados por GGUF desde el catálogo integrado.",
  "helpLlm.catalogTitle": "Catálogo de modelos",
  "helpLlm.catalogBody":
    "El catálogo enumera familias de modelos seleccionados (por ejemplo, Qwen, Llama, Gemma, Phi) con múltiples variantes de cuantificación por familia. Utilice el menú desplegable de familias para buscar y luego elija una cantidad específica para descargar. Los modelos marcados con ★ son los predeterminados recomendados para esa familia.",
  "helpLlm.quantsTitle": "Niveles de cuantificación",
  "helpLlm.quantsBody":
    "Cada modelo está disponible en varios niveles de cuantificación GGUF (Q4_K_M, Q5_K_M, Q6_K, Q8_0, etc.). Los cuantos más bajos son más pequeños y más rápidos, pero sacrifican algo de calidad. Q4_K_M suele ser la mejor compensación. Q8_0 casi no tiene pérdidas, pero requiere aproximadamente el doble de memoria. BF16/F16/F32 son pesos de referencia no cuantificados.",
  "helpLlm.hardwareFitTitle": "Insignias de ajuste de hardware",
  "helpLlm.hardwareFitBody":
    "Cada fila cuantitativa muestra una insignia codificada por colores que estima qué tan bien se adapta a su hardware: 🟢 Funciona excelente: cabe completamente en GPU VRAM con espacio libre. 🟡 Funciona bien: cabe en VRAM con un margen reducido. 🟠 Ajuste perfecto: es posible que necesite una descarga parcial de la CPU o un tamaño de contexto reducido. 🔴 No cabe: es demasiado grande para la memoria disponible. La estimación considera la VRAM de GPU, la RAM del sistema, el tamaño del modelo y la sobrecarga de contexto.",
  "helpLlm.visionTitle": "Visión / Modelos Multimodales",
  "helpLlm.visionBody":
    "Las familias etiquetadas como Vision o Multimodal incluyen un archivo de proyector multimodal opcional (mmproj). Descargue tanto el modelo de texto como su proyector para habilitar la entrada de imágenes en la ventana de chat. El proyector amplía el modelo de texto; no es un modelo independiente.",
  "helpLlm.downloadTitle": "Descargar y eliminar",
  "helpLlm.downloadBody":
    "Haga clic en 'Descargar' para buscar un modelo de HuggingFace Hub. Una barra de progreso muestra el estado de descarga en tiempo real. Puedes cancelar en cualquier momento. Los modelos descargados se almacenan localmente y se pueden eliminar para liberar espacio en el disco. Utilice el botón 'Actualizar caché' para volver a escanear el catálogo si modifica manualmente el directorio del modelo.",
  "helpLlm.inferenceSection": "Configuración de inferencia",
  "helpLlm.inferenceSectionDesc": "Ajuste cómo el servidor carga y ejecuta modelos.",
  "helpLlm.gpuLayersTitle": "Capas de GPU",
  "helpLlm.gpuLayersBody":
    "Controla cuántas capas de transformador se descargan a la GPU. Establezca en 'Todos' para obtener la velocidad máxima si el modelo cabe en VRAM. Establezca en 0 para inferencia solo de CPU. Los valores intermedios dividen el modelo entre GPU y CPU, lo que resulta útil cuando el modelo apenas supera la capacidad de VRAM.",
  "helpLlm.ctxSizeTitle": "Tamaño del contexto",
  "helpLlm.ctxSizeBody":
    "El tamaño de la caché KV en tokens. 'Auto' elige el contexto más grande que se ajuste a su GPU/RAM según el tamaño y la cuantificación del modelo. Los contextos más grandes permiten que el modelo recuerde más historial de conversaciones pero consume más memoria. Las opciones están limitadas al máximo entrenado del modelo. Si se encuentra con errores de falta de memoria, intente reducir el tamaño del contexto.",
  "helpLlm.parallelTitle": "Solicitudes paralelas",
  "helpLlm.parallelBody":
    "Número máximo de bucles de decodificación simultáneos. Los valores más altos permiten que varios clientes compartan el servidor pero aumentan el uso máximo de memoria. Para la mayoría de las configuraciones de un solo usuario, 1 está bien.",
  "helpLlm.apiKeyTitle": "Clave API",
  "helpLlm.apiKeyBody":
    "Se requiere un token de portador opcional en cada solicitud /v1/*. Déjelo vacío para acceso abierto en localhost. Establezca una clave si expone el puerto en una red local y desea restringir el acceso.",
  "helpLlm.toolsSection": "Herramientas integradas",
  "helpLlm.toolsSectionDesc":
    "El chat de LLM puede llamar a herramientas locales para recopilar información o tomar acciones en su nombre.",
  "helpLlm.toolsOverviewTitle": "Cómo funcionan las herramientas",
  "helpLlm.toolsOverviewBody":
    "Cuando el uso de herramientas está habilitado, el modelo puede solicitar llamar a una o más herramientas durante una conversación. La aplicación ejecuta la herramienta localmente y envía el resultado al modelo para que pueda incorporar información del mundo real en su respuesta. Las herramientas solo se invocan cuando el modelo las solicita explícitamente; nunca se ejecutan en segundo plano.",
  "helpLlm.toolsSafeTitle": "Herramientas seguras",
  "helpLlm.toolsSafeBody":
    "Fecha, Ubicación, Búsqueda web, Búsqueda web y Leer archivo son herramientas de solo lectura que no pueden modificar su sistema. Fecha devuelve la fecha y hora locales actuales. La ubicación proporciona una geolocalización aproximada basada en IP. Web Search ejecuta una consulta de respuesta instantánea DuckDuckGo. Web Fetch recupera el cuerpo del texto de una URL pública. Leer archivo lee archivos locales con paginación opcional.",
  "helpLlm.toolsDangerTitle": "Herramientas privilegiadas (⚠️)",
  "helpLlm.toolsDangerBody":
    "Bash, Write File y Edit File pueden modificar su sistema. Bash ejecuta comandos de shell con los mismos permisos que la aplicación. Write File crea o sobrescribe archivos en el disco. Editar archivo realiza ediciones de búsqueda y reemplazo. Están deshabilitados de forma predeterminada y muestran una insignia de advertencia. Habilítelos sólo si comprende los riesgos.",
  "helpLlm.toolsExecModeTitle": "Modo de ejecución y límites",
  "helpLlm.toolsExecModeBody":
    "El modo paralelo permite que el modelo llame a varias herramientas a la vez (más rápido). El modo secuencial los ejecuta uno a la vez (más seguro para herramientas con efectos secundarios). 'Rondas máximas' limita la cantidad de viajes de ida y vuelta de llamada de herramienta/resultado de herramienta que se permiten por mensaje. 'Máximo de llamadas por ronda' limita el número de invocaciones simultáneas de herramientas.",
  "helpLlm.chatSection": "Chat y registros",
  "helpLlm.chatSectionDesc": "Interactuar con el modelo y monitorear la actividad del servidor.",
  "helpLlm.chatWindowTitle": "Ventana de conversación",
  "helpLlm.chatWindowBody":
    "Abra la ventana de chat desde la tarjeta del servidor LLM o el menú de la bandeja. Proporciona una interfaz de chat familiar con representación de rebajas, resaltado de código y visualización de llamadas de herramientas. Las conversaciones son efímeras: no se guardan en el disco. Los modelos con capacidad de visión aceptan archivos adjuntos de imágenes mediante arrastrar y soltar o con el botón de archivos adjuntos.",
  "helpLlm.chatApiTitle": "Usando clientes externos",
  "helpLlm.chatApiBody":
    "Debido a que el servidor es compatible con OpenAI, puede utilizar cualquier interfaz de chat externa. Apunte a http://localhost:<port>/v1, establezca una clave API si configuró una y seleccione cualquier nombre de modelo de /v1/models. Las opciones populares incluyen Open WebUI, Chatbot UI, Continuar (VS Code) y curl/httpie para secuencias de comandos.",
  "helpLlm.serverLogsTitle": "Registros del servidor",
  "helpLlm.serverLogsBody":
    "El visor de registros en la parte inferior del panel de configuración de LLM transmite la salida del servidor en tiempo real. Muestra el progreso de carga del modelo, la velocidad de generación de tokens y cualquier error. Habilite el modo 'Detallado' en la sección avanzada para obtener resultados de diagnóstico detallados de llama.cpp. Registra el desplazamiento automático, pero puedes pausarlo desplazándote hacia arriba manualmente.",

  "helpTabs.hooks": "Ganchos",

  "helpHooks.overviewSection": "Descripción general",
  "helpHooks.overviewSectionDesc":
    "Los ganchos proactivos permiten que la aplicación active acciones automáticamente cuando sus patrones de EEG recientes coincidan con palabras clave o estados cerebrales específicos.",
  "helpHooks.whatIsTitle": "¿Qué son los ganchos proactivos?",
  "helpHooks.whatIsBody":
    "Un gancho proactivo es una regla que monitorea las incrustaciones recientes de etiquetas de EEG en tiempo real. Cuando la distancia del coseno entre las incrustaciones recientes de su estado cerebral y las incrustaciones de palabras clave del gancho cae por debajo de un umbral configurado, el gancho se activa: envía un comando, muestra una notificación, activa TTS o transmite un evento WebSocket. Los ganchos le permiten crear automatizaciones de neurorretroalimentación de circuito cerrado sin escribir código.",
  "helpHooks.howItWorksTitle": "Cómo funciona",
  "helpHooks.howItWorksBody":
    "Cada pocos segundos, la aplicación calcula incorporaciones de EEG a partir de los datos cerebrales más recientes. Estos se comparan con las incrustaciones de palabras clave definidas en cada gancho activo utilizando similitud de coseno sobre el índice HNSW. Si se alcanza el umbral de distancia de cualquier gancho, el gancho se dispara. Un tiempo de reutilización evita que el mismo gancho se dispare repetidamente en rápida sucesión. La coincidencia es puramente local: ningún dato sale de su máquina.",
  "helpHooks.scenariosTitle": "Escenarios",
  "helpHooks.scenariosBody":
    "Cada gancho puede limitarse a un escenario: cognitivo, emocional, físico o cualquiera. Los ganchos cognitivos se dirigen a estados mentales como la concentración, la distracción o la fatiga mental. Los ganchos emocionales se dirigen a estados afectivos como el estrés, la calma o la frustración. Los ganchos físicos se dirigen a estados corporales como la somnolencia o la fatiga física. 'Cualquiera' coincide independientemente de la categoría del escenario inferido.",
  "helpHooks.configSection": "Configurar un gancho",
  "helpHooks.configSectionDesc": "Cada gancho tiene varios campos que controlan cuándo y cómo se dispara.",
  "helpHooks.nameTitle": "Nombre del gancho",
  "helpHooks.nameBody":
    "Un nombre descriptivo para el gancho (por ejemplo, 'Deep Work Guard', 'Calm Recovery'). El nombre se utiliza en el registro histórico y en los eventos de WebSocket. Debe ser único en todos los ganchos.",
  "helpHooks.keywordsTitle": "Palabras clave",
  "helpHooks.keywordsBody":
    'Una o más palabras clave o frases cortas que describan el estado cerebral que desea detectar (por ejemplo, "concentración", "trabajo profundo", "estrés", "cansado"). Estos se integran utilizando el mismo modelo de transformador de oraciones que las etiquetas de EEG. El gancho se activa cuando las incorporaciones recientes de EEG están cerca de estas incorporaciones de palabras clave en el espacio vectorial compartido.',
  "helpHooks.keywordSugTitle": "Sugerencias de palabras clave",
  "helpHooks.keywordSugBody":
    'A medida que escribe una palabra clave, la aplicación sugiere términos relacionados de su historial de etiquetas existente utilizando una coincidencia de cadenas difusa y una similitud de incrustación semántica. Las sugerencias muestran una insignia de fuente: "difusa" para coincidencias basadas en cadenas, "semántica" para coincidencias basadas en incrustaciones o "difusa+semántica" para ambas. Utilice las teclas de flecha ↑/↓ y Enter para aceptar rápidamente una sugerencia.',
  "helpHooks.distanceTitle": "Umbral de distancia",
  "helpHooks.distanceBody":
    "La distancia máxima de coseno (0–1) entre las incrustaciones de EEG recientes y las incrustaciones de palabras clave del gancho para que se dispare el gancho. Los valores más bajos requieren una coincidencia más cercana (más estricta), los valores más altos se activan con más frecuencia (más indulgentes). Los valores típicos oscilan entre 0,08 (muy estricto) y 0,25 (laxo). Comience alrededor de 0,12–0,16 y ajuste según la herramienta de sugerencias.",
  "helpHooks.distanceSugTitle": "Herramienta de sugerencia de distancia",
  "helpHooks.distanceSugBody":
    "Haga clic en 'Sugerir umbral' para analizar los datos de EEG registrados con las palabras clave del gancho. La herramienta calcula la distribución de la distancia (min, p25, p50, p75, max) y recomienda un umbral que equilibra la sensibilidad y la especificidad. Una barra de percentiles visual muestra dónde se encuentran los umbrales actuales y sugeridos en la distribución. Haga clic en 'Aplicar' para utilizar el valor sugerido.",
  "helpHooks.recentLimitTitle": "Referencias recientes",
  "helpHooks.recentLimitBody":
    "El número de muestras de inclusión de EEG más recientes que se compararán con las palabras clave del gancho (predeterminado: 12). Los valores más altos suavizan los picos transitorios pero aumentan la latencia de detección. Los valores más bajos reaccionan más rápido pero pueden dispararse con artefactos breves. Rango válido: 10–20.",
  "helpHooks.commandTitle": "Comando",
  "helpHooks.commandBody":
    "Una cadena de comando opcional que se transmite en el evento WebSocket cuando se activa el gancho (por ejemplo, 'focus_reset', 'calm_breath'). Las herramientas de automatización externas que escuchan en WebSocket pueden reaccionar a este comando para activar acciones, notificaciones o scripts específicos de la aplicación.",
  "helpHooks.textTitle": "Texto de carga útil",
  "helpHooks.textBody":
    'Un mensaje opcional legible por humanos incluido en el evento de activación del gancho (por ejemplo, "Tómate un descanso de 2 minutos"). Este texto se muestra en las notificaciones y se puede pronunciar en voz alta a través de TTS si la guía de voz está habilitada.',
  "helpHooks.advancedSection": "Avanzado",
  "helpHooks.advancedSectionDesc": "Consejos, historia e integración con herramientas externas.",
  "helpHooks.examplesTitle": "Ejemplos rápidos",
  "helpHooks.examplesBody":
    "El panel 'Ejemplos rápidos' proporciona plantillas de ganchos listas para usar para casos de uso comunes: Deep Work Guard (restablecimiento del enfoque cognitivo), Calm Recovery (alivio del estrés emocional) y Body Break (fatiga física). Haga clic en cualquier ejemplo para agregarlo como un nuevo enlace con palabras clave, escenario, umbral y carga útil precargados. Ajuste los valores para que coincidan con sus patrones EEG personales.",
  "helpHooks.historyTitle": "Historia del incendio del gancho",
  "helpHooks.historyBody":
    "El registro histórico plegable en la parte inferior del panel Hooks registra cada evento de disparo de gancho con marca de tiempo, etiqueta coincidente, distancia de coseno, comando y palabras clave en el momento del disparo. Úselo para auditar el comportamiento de los enlaces, verificar umbrales y depurar falsos positivos. Expanda cualquier fila para ver todos los detalles. Los controles de paginación le permiten explorar eventos más antiguos.",
  "helpHooks.wsEventsTitle": "Eventos WebSocket",
  "helpHooks.wsEventsBody":
    "Cuando se activa un enlace, la aplicación transmite un evento JSON a través de la API WebSocket que contiene el nombre del enlace, el comando, el texto, la etiqueta coincidente, la distancia y la marca de tiempo. Los clientes externos pueden escuchar estos eventos para crear automatizaciones personalizadas, por ejemplo, atenuar las luces, pausar la música, enviar un mensaje de Slack o iniciar sesión en un panel personal.",
  "helpHooks.tipsTitle": "Consejos de sintonización",
  "helpHooks.tipsBody":
    'Comience con un gancho y algunas palabras clave que coincidan con las etiquetas que ya registró. Utilice la herramienta de sugerencia de distancia para establecer un umbral inicial. Supervise el registro del historial durante un día y ajústelo: reduzca el umbral si ve falsos positivos, súbalo si el anzuelo nunca se dispara. Agregar palabras clave más específicas (por ejemplo, "lectura profunda" versus "enfoque") generalmente mejora la precisión. Evite palabras clave muy cortas o genéricas de una sola palabra, a menos que desee una concordancia amplia.',
};

export default help;
