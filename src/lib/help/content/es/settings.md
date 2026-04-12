# Pestaña de configuración
Configure las preferencias del dispositivo, el procesamiento de señales, la incorporación de parámetros, la calibración, los accesos directos y el registro.

## Dispositivos emparejados
Enumera todos los dispositivos BCI que ha visto la aplicación. Puede configurar un dispositivo preferido (objetivo de conexión automática), olvidar dispositivos o buscar otros nuevos. La intensidad de la señal RSSI se muestra para los dispositivos vistos recientemente.

## Procesamiento de señales
Configure la cadena de filtros EEG en tiempo real: corte de paso bajo (elimina el ruido de alta frecuencia), corte de paso alto (elimina la deriva de CC) y filtro de muesca de línea eléctrica (elimina el zumbido y los armónicos de la red eléctrica de 50 o 60 Hz). Los cambios se aplican inmediatamente a la visualización de la forma de onda y a las potencias de las bandas.

## Incrustación de EEG
Ajuste la superposición entre épocas de incrustación consecutivas de 5 segundos. Una mayor superposición significa más incorporaciones por minuto (resolución temporal más fina en la búsqueda) a costa de más almacenamiento y computación.

## Calibración
Configura la tarea de calibración: etiquetas de acción (p. ej., "ojos abiertos", "ojos cerrados"), duración de fases, número de repeticiones y si la calibración debe iniciarse automáticamente al abrir la aplicación.

## Guía de voz de calibración (TTS)
Durante la calibración, la app anuncia cada fase por nombre usando síntesis de voz local en inglés. El motor usa KittenTTS (tract-onnx, ~30 MB) con fonemización de espeak-ng. El modelo se descarga desde HuggingFace Hub en el primer inicio y luego queda en caché local: después de eso no sale ningún dato de tu dispositivo. La voz se activa al inicio de sesión, en cada fase de acción, en cada descanso ("Break. Next: …") y al completar la sesión. Requiere espeak-ng en PATH (brew / apt / apk install espeak-ng). Solo inglés.

## Atajos globales
Configure atajos de teclado en todo el sistema para abrir las ventanas Etiqueta, Búsqueda, Configuración y Calibración desde cualquier aplicación. Utiliza el formato de acelerador estándar (por ejemplo, CmdOrCtrl+Shift+L).

## Registro de depuración
Cambie el registro por subsistema al archivo de registro diario en {dataDir}/logs/. Los subsistemas incluyen incrustador, dispositivos, websocket, csv, filtro y bandas.

## Actualizaciones
Busque e instale actualizaciones de aplicaciones. Utiliza el actualizador integrado de Tauri con verificación de firma Ed25519.

## Apariencia
Elija un modo de color (Sistema / Claro / Oscuro), habilite Contraste alto para bordes y texto más fuertes y elija una combinación de colores de gráfico para formas de onda de EEG y visualizaciones de potencia de banda. Hay disponibles paletas aptas para daltónicos. El idioma también se cambia aquí mediante el selector de configuración regional.

## Objetivos
Establezca un objetivo de grabación diario en minutos. Aparece una barra de progreso en el panel durante la transmisión y se activa una notificación cuando alcanzas tu objetivo. El gráfico de los últimos 30 días muestra qué días llegó (verde), llegó a la mitad (ámbar), realizó algún progreso (oscuro) o se perdió (ninguno).

## Incrustaciones de texto
Seleccione el modelo de transformador de oración utilizado para incrustar el texto de su etiqueta para la búsqueda semántica. Los modelos más pequeños (≤384-dim, por ejemplo, All-MiniLM-L6-v2) son rápidos y suficientes para la búsqueda personal. Los modelos más grandes producen representaciones más ricas a costa del tamaño de descarga y el tiempo de inferencia. Los pesos se descargan una vez desde HuggingFace y se almacenan en caché localmente. Después de cambiar de modelo, ejecute Volver a incrustar todas las etiquetas para volver a indexar.

## Atajos
Configure atajos de teclado globales (teclas de acceso rápido para todo el sistema) para abrir las ventanas Etiqueta, Búsqueda, Configuración y Calibración. También muestra todos los atajos de la aplicación (⌘K para la paleta de comandos, ? para la superposición de atajos, ⌘↵ para enviar una etiqueta). Los atajos utilizan el formato de acelerador estándar, p. CmdOCtrl+Mayús+L.

# Seguimiento de actividad
NeuroSkill puede, opcionalmente, registrar qué aplicación está en primer plano y cuándo se utilizaron por última vez el teclado y el mouse. Ambas funciones están desactivadas de forma predeterminada, son totalmente locales y se pueden configurar de forma independiente en Configuración → Seguimiento de actividad.

## Seguimiento de ventana activa
Un hilo en segundo plano se activa cada segundo y pregunta al sistema operativo qué aplicación se encuentra actualmente en primer plano. Cuando el nombre de la aplicación o el título de la ventana cambia, se inserta una fila en Activity.sqlite: el nombre para mostrar de la aplicación (por ejemplo, "Safari"), la ruta completa al paquete de la aplicación o al ejecutable, el título de la ventana principal (por ejemplo, el nombre del documento o la página web actual) y una grabación de marca de tiempo de un segundo de Unix cuando esa ventana se activó. Si permanece en la misma ventana, no se escribe ninguna fila nueva: el tiempo de inactividad en una sola aplicación no produce actividad en la base de datos. En macOS el rastreador llama a osascript; No se necesita ningún permiso de accesibilidad para el nombre y la ruta de la aplicación, pero el título de la ventana puede estar vacío para las aplicaciones en espacio aislado. En Linux usa xdotool y xprop (requiere una sesión X11). En Windows utiliza una llamada GetForegroundWindow de PowerShell.

## Seguimiento de actividad del teclado y el mouse
Un enlace de entrada global (rdev) escucha cada pulsación de tecla y evento del mouse o trackpad en todo el sistema. No registra lo que escribió, qué teclas presionó ni dónde se movió el cursor; solo actualiza dos marcas de tiempo de Unix en segundos en la memoria: una para el evento de teclado más reciente y otra para el evento de mouse/trackpad más reciente. Estos se descargan en Activity.sqlite cada 60 segundos, pero solo cuando al menos un valor ha cambiado desde la última descarga, por lo que los períodos inactivos no dejan rastro. El panel de Configuración recibe un evento de actualización en vivo (regulado a una vez por segundo como máximo) para que los campos "Último teclado" y "Último mouse" reflejen la actividad casi en tiempo real.

## Dónde se almacenan los datos
Todos los datos de actividad se encuentran en un único archivo SQLite: ~/.skill/activity.sqlite. Nunca se transmite, sincroniza ni incluye en ningún análisis. Se mantienen dos tablas: active_windows (una fila por cambio de enfoque de ventana, con el nombre de la aplicación, ruta, título y marca de tiempo) y input_activity (una fila por cada 60 segundos de descarga cuando se detectó actividad, con marcas de tiempo del último teclado y del último mouse). Ambas tablas tienen un índice descendente en la columna de marca de tiempo. El modo de diario WAL está habilitado para que las escrituras en segundo plano nunca bloqueen las lecturas. Puede abrir, inspeccionar, exportar o eliminar el archivo en cualquier momento con cualquier navegador SQLite.

## Permisos requeridos del sistema operativo
macOS: el seguimiento de ventanas activas (nombre y ruta de la aplicación) no requiere permisos especiales. El seguimiento del teclado y el mouse utiliza un CGEventTap que requiere acceso de Accesibilidad: abra Configuración del sistema → Privacidad y seguridad → Accesibilidad, busque NeuroSkill en la lista y actívelo. Sin este permiso, el enlace de entrada falla silenciosamente: las marcas de tiempo permanecen en cero y el resto de la aplicación no se ve afectado en absoluto. Puede desactivar la opción en Configuración → Seguimiento de actividad para evitar que se solicite permiso por completo. Linux: ambas funciones requieren una sesión X11. El seguimiento de ventanas activas utiliza xdotool y xprop, que están preinstalados en la mayoría de las distribuciones de escritorio. El seguimiento de entrada utiliza la extensión XRecord de libxtst. Si falta alguna de las herramientas, esa función registra una advertencia y se desactiva. Windows: no se requieren permisos especiales. El seguimiento de ventanas activas utiliza GetForegroundWindow a través de PowerShell; el seguimiento de entrada utiliza SetWindowsHookEx.

## Deshabilitar y borrar datos
Ambos cambios en Configuración → Seguimiento de actividad entran en vigor de inmediato; no es necesario reiniciar. Deshabilitar el seguimiento de ventanas activas impide que se inserten nuevas filas en active_windows y borra el estado de la ventana actual en la memoria. Deshabilitar el seguimiento de entrada evita que la devolución de llamada de rdev actualice las marcas de tiempo y evita futuros vaciados en input_activity; Las filas existentes no se eliminan automáticamente. Para eliminar todo el historial recopilado: salga de la aplicación, elimine ~/.skill/activity.sqlite y luego reiníciela. Se creará automáticamente una base de datos vacía en el próximo inicio.

# UMAP

## UMAP
Parámetros de control para la proyección UMAP 3D utilizada en Comparación de sesiones: número de vecinos (controla la estructura local frente a la global), distancia mínima (con qué precisión se agrupan los puntos) y la métrica (coseno o euclidiana). Un mayor número de vecinos preserva una topología más global; los recuentos más bajos revelan grupos locales detallados. Las proyecciones se ejecutan en un trabajo en segundo plano y los resultados se almacenan en caché.

# Pestaña Modelo EEG
Supervise el codificador ZUNA y el estado del índice del vector HNSW.

## Estado del codificador
Muestra si el codificador ZUNA wgpu está cargado, el resumen de la arquitectura (dimensión, capas, cabezales) y la ruta al archivo de peso .safetensors. El codificador se ejecuta completamente en el dispositivo utilizando su GPU.

## Incrustaciones hoy
Un contador en vivo de cuántas épocas de EEG de 5 segundos se han incluido en el índice HNSW de hoy. Cada incorporación es un vector compacto que captura la firma neuronal de ese momento.

## Parámetros HNSW
M (conexiones por nodo) y ef_construction (ancho de búsqueda durante la construcción) controlan la relación calidad/velocidad del índice del vecino más cercano. Los valores más altos dan una mejor recuperación pero usan más memoria. Los valores predeterminados (M=16, ef=200) son un buen equilibrio.

## Normalización de datos
El factor de escala data_norm aplicado al EEG sin procesar antes de la codificación. El valor predeterminado (10) está configurado para los auriculares Muse 2/Muse S.

# Tableros OpenBCI
Conecte y configure cualquier placa OpenBCI (Ganglion, Cyton, Cyton+Daisy, variantes WiFi Shield o Galea) de forma independiente o junto con otro dispositivo BCI.

## Selección de placa
Elija qué placa OpenBCI utilizar. Ganglion (4 canales, BLE) es la opción más portátil. Cyton (8 canales, serie USB) agrega un mayor número de canales. Cyton+Daisy duplica esto a 16 canales. Las variantes de WiFi Shield reemplazan el enlace USB/BLE con una transmisión Wi-Fi de 1 kHz. Galea (24 canales, UDP) es una placa de investigación de alta densidad. Todas las variantes pueden funcionar de forma independiente o junto con otro dispositivo BCI.

## Ganglion BLE
Ganglion se conecta por Bluetooth Low Energy. Pulsa Conectar y NeuroSkill™ buscará el Ganglion anunciándose más cercano durante el tiempo de escaneo configurado. Mantén la placa a 3–5 m y encendida (LED azul parpadeando). Solo puede haber un Ganglion activo por adaptador Bluetooth. Amplía el tiempo de escaneo BLE en Configuración si la placa tarda en anunciarse.

## Puerto serie (Cyton / Cyton+Daisy)
Las placas Cyton se comunican a través de una llave de radio USB. Deje el campo del puerto serie en blanco para detectar automáticamente el primer puerto disponible o ingréselo explícitamente (/dev/cu.usbserial-… en macOS, /dev/ttyUSB0 en Linux, COM3 en Windows). Conecte el dongle antes de hacer clic en Conectar y asegúrese de tener permisos de puerto serie; en Linux, agregue su usuario al grupo de acceso telefónico.

## Escudo WiFi
OpenBCI WiFi Shield crea su propio punto de acceso de 2,4 GHz (SSID: OpenBCI-XXXX). Conecta tu ordenador a esa red y configura la IP en 192.168.4.1 (puerta de enlace predeterminada del shield). Alternativamente, el shield puede unirse a tu red local: usa la IP asignada en ese caso. Deja el campo IP vacío para intentar autodetección vía mDNS. WiFi Shield transmite a 1 kHz: establece el corte del filtro paso bajo en ≤ 500 Hz en Configuración de procesamiento de señal.

## Galea
Galea es un auricular de bioseñales de grado de investigación de 24 canales (EEG + EMG + AUX) que transmite a través de UDP. Ingrese la dirección IP del dispositivo Galea o déjela en blanco para aceptar paquetes de cualquier remitente en la red local. Los canales 1 a 8 son EEG y generan análisis en tiempo real; los canales 9 a 16 son EMG; 17–24 son auxiliares. Los 24 canales se guardan en CSV.

## Etiquetas de canales y ajustes preestablecidos
Asigne entre 10 y 20 nombres de electrodos estándar a cada canal físico para que las métricas de potencia de banda, la asimetría alfa frontal y las visualizaciones de electrodos tengan en cuenta los electrodos. Utilice un ajuste preestablecido (Frontal, Motor, Occipital, Completo 10-20) para completar las etiquetas automáticamente o escriba nombres personalizados. Los canales más allá de los primeros 4 se registran únicamente en CSV y no impulsan el proceso de análisis en tiempo real.
