## ¿Dónde se almacenan mis datos?
Todo se almacena localmente en {dataDir}/: grabaciones CSV sin procesar, índices vectoriales HNSW, bases de datos SQLite integradas, etiquetas, registros y configuraciones. No se envía nada a la nube.

## ¿Qué hace el codificador ZUNA?
ZUNA es uno de varios backends de incrustación de EEG disponibles en {app}. Es un codificador transformer acelerado por GPU que convierte épocas de EEG de 5 segundos en vectores de incrustación compactos. Estos vectores capturan la firma neuronal de cada momento y alimentan la función de búsqueda por similitud. Otros backends incluyen LUNA y NeuroRVQ.

## ¿Por qué la calibración requiere un dispositivo conectado?
La calibración ejecuta una tarea cronometrada (por ejemplo, ojos abiertos/ojos cerrados) y registra datos de EEG etiquetados. Sin datos de transmisión en vivo, la calibración no tendría ninguna señal neuronal para asociar con cada fase.

## ¿Cómo me conecto desde Python/Node.js?
Descubra el puerto WebSocket a través de mDNS (dns-sd -B _skill._tcp en macOS) y luego abra una conexión WebSocket estándar. Envíe comandos JSON y reciba transmisiones de eventos en vivo. Consulte la pestaña API para obtener detalles sobre el formato de cable.

## ¿Qué significan los indicadores de calidad de la señal?
Cada punto representa un electrodo EEG. Verde = buen contacto con la piel, poco ruido. Amarillo = algún artefacto de movimiento o ajuste holgado. Rojo = mucho ruido, mal contacto. Gris = no se detecta señal.

## ¿Puedo cambiar la frecuencia del filtro de muesca?
Sí, vaya a Configuración → Procesamiento de señal y elija 50 Hz (Europa, la mayor parte de Asia) o 60 Hz (América, Japón). Esto elimina la interferencia de la línea eléctrica de la pantalla y del cálculo de potencia de banda.

## ¿Cómo reinicio un dispositivo emparejado?
Abra Configuración → Dispositivos emparejados, luego haga clic en el botón × junto al dispositivo que desea olvidar. Luego podrás buscarlo nuevamente.

## ¿Por qué el ícono de la bandeja se vuelve rojo?
Bluetooth está desactivado en su sistema. Abra Configuración del sistema → Bluetooth y habilítelo. {app} se volverá a conectar automáticamente en aproximadamente 1 segundo.

## La aplicación sigue girando pero nunca se conecta. ¿Qué debo hacer?
1. Asegúrese de que el dispositivo esté encendido (Muse: mantenga presionado hasta que sienta una vibración; Ganglion/Cyton: verifique el LED azul). 2. Manténgalo a menos de 5 m. 3. Si aún falla, reinicie el dispositivo.

## ¿Por qué mi dispositivo se desconectó automáticamente?
Si no llegan datos durante 30 segundos después de haber recibido al menos una trama de EEG, {app} considera que el dispositivo se ha desconectado silenciosamente (por ejemplo, salió del rango BLE o se apagó sin una desconexión limpia). El ícono de la bandeja vuelve a gris y el escaneo se reanuda automáticamente.

## ¿Cómo otorgo permiso a Bluetooth?
macOS mostrará un cuadro de diálogo de permiso la primera vez que {app} intente conectarse. Si lo descartó, vaya a Configuración del sistema → Privacidad y seguridad → Bluetooth y habilite {app}.

## ¿Qué métricas se almacenan en la base de datos?
Cada época de 2,5 s almacena: el vector de incrustación de EEG, potencias de banda relativas (delta, theta, alfa, beta, gamma, gamma alta) promediadas entre canales, potencias de banda por canal como JSON, puntuaciones derivadas (relajación, compromiso), FAA, relaciones entre bandas (TAR, BAR, DTR), forma espectral (PSE, APF, BPS, SNR), coherencia, supresión Mu, índice de ánimo y promedios de PPG si están disponibles.

## ¿Qué es la comparación de sesiones?
Comparar (⌘⇧M) le permite elegir dos rangos de tiempo y compararlos uno al lado del otro: barras de potencia de banda relativa con deltas, todas las puntuaciones y proporciones derivadas, asimetría alfa frontal, hipnogramas de estadificación del sueño y Brain Nebula™, una proyección de incorporación UMAP 3D.

## ¿Qué es Brain Nebula™?
Brain Nebula™ (técnicamente: UMAP Embedding Distribution) proyecta incrustaciones de EEG de alta dimensión en un espacio 3D para que estados cerebrales similares aparezcan como puntos cercanos. El rango A (azul) y el rango B (ámbar) forman grupos distintos cuando las sesiones difieren. Puede orbitar, hacer zoom y hacer clic en puntos etiquetados para rastrear conexiones temporales. Se pueden resaltar varias etiquetas simultáneamente en diferentes colores.

## ¿Por qué Brain Nebula™ muestra una nube aleatoria al principio?
La proyección UMAP es computacionalmente costosa y se ejecuta en una cola de trabajos en segundo plano para que la interfaz de usuario siga respondiendo. Mientras se realiza la computación, se muestra una nube de marcador de posición aleatoria. Una vez que la proyección está lista, los puntos se animan suavemente hasta sus posiciones finales.

## ¿Qué son las etiquetas y cómo se utilizan?
Las etiquetas son etiquetas definidas por el usuario (por ejemplo, 'meditación', 'lectura') adjuntas a un momento durante la grabación. Se almacenan junto con las incrustaciones de EEG. En el visor UMAP, los puntos etiquetados aparecen más grandes con anillos de colores; haga clic en uno para rastrear esa etiqueta a lo largo del tiempo en ambas sesiones.

## ¿Qué es la asimetría alfa frontal (FAA)?
FAA es ln(AF8 α) − ln(AF7 α). Los valores positivos sugieren motivación de aproximación (compromiso, curiosidad). Los valores negativos sugieren retraimiento (evitación, ansiedad).

## ¿Cómo funciona la puesta en escena del sueño?
Cada época del EEG se clasifica como Wake, N1, N2, N3 o REM según la potencia relativa delta, theta, alfa y beta. La vista de comparación muestra un hipnograma para cada sesión con desgloses de etapas y porcentajes de tiempo.

## ¿Cuáles son los atajos de teclado?
⌘⇧O: abre la ventana {app}. ⌘⇧M — Comparación de sesiones abiertas. Personalice los atajos en Configuración → Atajos.

## ¿Qué es la API WebSocket?
{app} expone una API JSON WebSocket en la red local (mDNS: _skill._tcp). Comandos: estado, etiqueta, buscar, comparar (métricas + suspensión + ticket UMAP), sesiones, suspensión, umap (poner en cola proyección 3D), umap_poll (recuperar resultado). Ejecute 'node test.js' para realizar una prueba de humo.

## ¿Qué son las puntuaciones de relajación y compromiso?
Relajación = α/(β+θ), que mide la vigilia tranquila. Compromiso = β/(α+θ), que mide la implicación mental sostenida. Ambos se asignan de 0 a 100 mediante un sigmoide.

## ¿Qué son TAR, BAR y DTR?
TAR (Theta/Alpha): mayor = más somnoliento o más meditativo. BAR (Beta/Alfa): mayor = más estresado o concentrado. DTR (Delta/Theta): mayor = sueño o relajación más profundos. Todo promediado entre canales.

## ¿Qué son PSE, APF, BPS y SNR?
PSE (Entropía espectral de potencia, 0–1): complejidad espectral. APF (Frecuencia pico alfa, Hz): frecuencia de potencia alfa máxima. BPS (pendiente de potencia de banda): exponente aperiódico 1/f. SNR (relación señal-ruido, dB): banda ancha frente a ruido de línea.

## ¿Qué es la relación Theta/Beta (TBR)?
TBR es la relación entre el poder theta absoluto y el poder beta absoluto. Los valores más altos indican una activación cortical reducida: la TBR elevada se asocia con somnolencia y desregulación de la atención. Referencia: Angelidis et al. (2016).

## ¿Qué son los parámetros de Hjorth?
Tres características en el dominio del tiempo de Hjorth (1970): Actividad (varianza de la señal/potencia total), Movilidad (estimación de la frecuencia media) y Complejidad (ancho de banda/desviación de un seno puro). Son computacionalmente baratos y ampliamente utilizados en canalizaciones de EEG ML.

## ¿Qué medidas de complejidad no lineales se calculan?
Cuatro medidas: entropía de permutación (complejidad del patrón ordinal, Bandt y Pompe 2002), dimensión fractal de Higuchi (estructura fractal de la señal, Higuchi 1988), exponente DFA (correlaciones temporales de largo alcance, Peng et al. 1994) y entropía de muestra (regularidad de la señal, Richman y Moorman 2000). Todos se promedian en los 4 canales de EEG.

## ¿Qué son SEF95, centroide espectral, PAC y índice de lateralidad?
SEF95 (Frecuencia de borde espectral) es la frecuencia por debajo de la cual se encuentra el 95 % de la potencia total y se utiliza en la monitorización de la anestesia. El centroide espectral es la frecuencia media ponderada en potencia (indicador de excitación). PAC (acoplamiento de amplitud de fase) mide la interacción de frecuencia cruzada theta-gamma asociada con la codificación de la memoria. El índice de lateralidad es la asimetría de poder generalizada izquierda/derecha en todas las bandas.

## ¿Qué métricas de PPG se calculan?
En Muse 2/S (con sensor PPG): frecuencia cardíaca (lpm) a partir de la detección de pico IR, RMSSD/SDNN/pNN50 (variabilidad de la frecuencia cardíaca: tono parasimpático), relación LF/HF (equilibrio simpatovagal), frecuencia respiratoria (respiraciones/min de la envoltura PPG), estimación de SpO₂ (oxígeno en sangre no calibrado a partir de la relación rojo/IR), índice de perfusión (flujo sanguíneo periférico) e índice de estrés de Baevsky (estrés autónomo). Estos aparecen en la sección PPG Vitals cuando se conecta una diadema equipada con PPG.

## ¿Cómo uso el temporizador de enfoque?
Abra el Temporizador de enfoque a través del menú de la bandeja, la Paleta de comandos (⌘K → "Temporizador de enfoque") o el acceso directo global (⌘⇧P de forma predeterminada). Elija un ajuste preestablecido: Pomodoro (25/5), Trabajo profundo (50/10) o Enfoque corto (15/5), o establezca duraciones personalizadas. Habilite "Etiquetado automático de EEG" para que NeuroSkill™ etiquete automáticamente las grabaciones de EEG al inicio y al final de cada fase de enfoque. Los puntos de sesión rastrean tus rondas completadas. Sus configuraciones preestablecidas y personalizadas se guardan automáticamente y se restauran la próxima vez que abra el temporizador.

## ¿Cómo administro o edito mis anotaciones?
Abra la ventana Etiquetas a través de la Paleta de comandos (⌘K → "Todas las etiquetas"). Muestra todas las anotaciones con edición de texto en línea (haga clic en una etiqueta, presione ⌘↵ para guardar o Esc para cancelar), eliminar (con confirmación) y metadatos que muestran el rango de tiempo del EEG. Utilice el cuadro de búsqueda para filtrar por texto. Las etiquetas están paginadas a razón de 50 por página para archivos grandes.

## ¿Cómo comparo dos sesiones específicas una al lado de la otra?
Desde la página Historial, haga clic en "Comparación rápida" para ingresar al modo de comparación. Aparecen casillas de verificación en cada fila de sesión: seleccione exactamente dos, luego haga clic en "Comparar seleccionados" para abrir la ventana Comparar precargada con ambas sesiones. Alternativamente, abra Comparar desde la bandeja o la Paleta de comandos y use los menús desplegables de la sesión manualmente.

## ¿Cómo funciona la búsqueda con incrustación de texto?
Su consulta se convierte en un vector mediante el mismo modelo de transformador de oraciones que indexa sus etiquetas. Luego, ese vector se busca en el índice de etiquetas HNSW utilizando una búsqueda aproximada del vecino más cercano. Los resultados son sus propias anotaciones clasificadas por similitud semántica, por lo que al buscar "tranquilo y concentrado" aparecerán etiquetas como "lectura profunda" o "meditación" incluso si esas palabras exactas nunca aparecieron en su consulta. Requiere descargar el modelo de incrustación y crear el índice de etiquetas (Configuración → Incrustaciones).

## ¿Cómo funciona la búsqueda intermodal interactiva?
La búsqueda interactiva une texto, EEG y tiempo en una sola consulta. Paso 1: su consulta de texto está incrustada. Paso 2: se encuentran las etiquetas semánticamente similares text-k superiores. Paso 3: para cada etiqueta, {app} calcula la incrustación media del EEG en su ventana de grabación y recupera las épocas de EEG más cercanas del eeg-k superior de todos los índices diarios, cruzando desde el lenguaje al espacio del estado cerebral. Paso 4: para cada momento de EEG encontrado, cualquier anotación dentro de ± minutos de alcance se recopila como "etiquetas encontradas". Las cuatro capas de nodos (Consulta → Coincidencias de texto → Vecinos EEG → Etiquetas encontradas) se representan como un gráfico dirigido de 4 capas. Exporte como SVG para una imagen estática o como fuente DOT para su posterior procesamiento en Graphviz.

## ¿Cómo activo el discurso TTS desde un script o una herramienta de automatización?
Utilice WebSocket o API HTTP. WebSocket: envía {"command":"say","text":"your message"}. HTTP (curl): curl -X POST http://localhost:<port>/say -H \\'Tipo de contenido: aplicación/json\\' -d \\'{"text":"your message"}\\'. El comando decir es disparar y olvidar: responde inmediatamente mientras el audio se reproduce en segundo plano.

## ¿Por qué no hay sonido de TTS?
Verifique que espeak-ng esté instalado en PATH (brew install espeak-ng en macOS, apt install espeak-ng en Ubuntu). Verifique que la salida de audio de su sistema no esté silenciada ni enrutada a un dispositivo diferente. En la primera ejecución, el modelo (~30 MB) debe terminar de descargarse antes de que se escuche algún sonido. Habilite el registro de depuración TTS en Configuración → Voz para ver los eventos de síntesis en el archivo de registro.

## ¿Puedo cambiar la voz o el idioma de TTS?
La versión actual utiliza la voz Jasper English (en-us) del modelo KittenML/kitten-tts-mini-0.8. Sólo el texto en inglés está fonemizado correctamente. Se planean voces adicionales y soporte de idiomas para futuras versiones.

## ¿TTS requiere una conexión a Internet?
Solo una vez, para la descarga inicial del modelo de ~30 MB desde HuggingFace Hub. Después de eso, toda la síntesis se ejecuta completamente fuera de línea. El modelo se almacena en caché en ~/.cache/huggingface/hub/ y se reutiliza en cada lanzamiento posterior.

## ¿Qué placas OpenBCI admite NeuroSkill™?
NeuroSkill™ es compatible con todas las placas del ecosistema OpenBCI a través del crate openbci publicado (crates.io/crates/openbci): Ganglion (4 canales, BLE), Ganglion + WiFi Shield (4 canales, 1 kHz), Cyton (8 canales, dongle USB), Cyton + WiFi Shield (8 canales, 1 kHz), Cyton+Daisy (16 canales, dongle USB), Cyton+Daisy + WiFi Shield (16 canales, 1 kHz) y Galea (24 canales, UDP). Cualquier placa se puede utilizar junto con otro dispositivo BCI. Selecciona la placa en Configuración → OpenBCI y luego haz clic en Conectar.

## ¿Cómo conecto el Ganglion a través de Bluetooth?
1. Enciende el Ganglion; el LED azul debería parpadear lentamente. 2. En Configuración → OpenBCI selecciona "Ganglion — 4ch · BLE". 3. Guarda la configuración y luego haz clic en Conectar. NeuroSkill™ escanea hasta el tiempo de espera configurado (predeterminado 10 s). Mantén la placa a una distancia de entre 3 y 5 m. En macOS, otorga permiso de Bluetooth cuando se solicite (o ve a Configuración del sistema → Privacidad y seguridad → Bluetooth).

## Mi Ganglion está encendido pero NeuroSkill™ no puede encontrarlo. ¿Qué debo intentar?
1. Confirme que el LED azul esté parpadeando (fijo o apagado significa que no hay publicidad; presione el botón para activarlo). 2. Aumente el tiempo de espera del escaneo BLE en Configuración → OpenBCI. 3. Mueva la tabla a una distancia máxima de 2 m. 4. Salga de NeuroSkill™ y vuelva a abrirlo para restablecer el adaptador BLE. 5. Desactive y vuelva a activar Bluetooth en Configuración del sistema. 6. Asegúrese de que no haya ninguna otra aplicación (GUI de OpenBCI, otra instancia de NeuroSkill™) conectada: BLE solo permite una central a la vez. 7. En macOS 14+, verifique que NeuroSkill™ tenga permiso de Bluetooth en Configuración del sistema → Privacidad y seguridad → Bluetooth.

## ¿Cómo conecto un Cyton a través de USB?
1. Conecte la llave de radio USB a su computadora (la llave es la radio; la placa Cyton en sí no tiene puerto USB). 2. Encienda el Cyton: deslice el interruptor de encendido a la PC. 3. En Configuración → OpenBCI seleccione "Cyton — 8ch · USB serial". 4. Haga clic en Actualizar para enumerar los puertos serie, luego seleccione el puerto (/dev/cu.usbserial-… en macOS, /dev/ttyUSB0 en Linux, COM3 en Windows) o déjelo en blanco para la detección automática. 5. Guarde la configuración y haga clic en Conectar.

## El puerto serie no aparece en la lista o aparece un error de permiso denegado. ¿Cómo lo soluciono?
macOS: el dongle aparece como /dev/cu.usbserial-*. Si no está presente, instale el controlador CP210x o FTDI VCP desde el sitio del fabricante del chip. Linux: ejecute sudo usermod -aG dialout $USER, luego cierre sesión y vuelva a iniciarla. Verifique que el dispositivo aparezca en /dev/ttyUSB0 o /dev/ttyACM0 después de conectarlo. Windows: instale el controlador CP2104 USB a UART; el puerto COM aparecerá en Administrador de dispositivos → Puertos (COM y LPT).

## ¿Cómo me conecto a través de OpenBCI WiFi Shield?
1. Apile el WiFi Shield encima del Cyton o Ganglion y encienda la placa. 2. En su computadora, conéctese a la red WiFi que transmite el escudo (SSID: OpenBCI-XXXX, generalmente sin contraseña). 3. En Configuración → OpenBCI seleccione la variante de placa WiFi correspondiente. 4. Ingrese IP 192.168.4.1 (escudo predeterminado) o déjelo en blanco para el descubrimiento automático. 5. Haga clic en Conectar. WiFi Shield transmite a 1000 Hz: configure el filtro de paso bajo en ≤ 500 Hz en Procesamiento de señal para evitar alias.

## ¿Qué es el tablero Galea y cómo lo configuro?
Galea de OpenBCI es un auricular de investigación de bioseñales de 24 canales que combina sensores EEG, EMG y AUX, que se transmiten a través de UDP. Para conectarse: 1. Encienda Galea y conéctelo a su red local. 2. En Configuración → OpenBCI seleccione "Galea — 24ch · UDP". 3. Ingrese la dirección IP de Galea (o déjela en blanco para aceptarla de cualquier remitente). 4. Haga clic en Conectar. Los canales 1 a 8 son EEG (impulsa el análisis en tiempo real); 9 a 16 son EMG; 17–24 son auxiliares. Los 24 se guardan en CSV.

## ¿Puedo utilizar dos dispositivos BCI al mismo tiempo?
Sí — NeuroSkill™ puede transmitir desde ambos simultáneamente. El dispositivo que se conecte primero controla el panel en vivo, la visualización de potencia de banda y la canalización de incrustación de EEG. Los datos del segundo dispositivo se graban en CSV para análisis sin conexión. El análisis simultáneo de múltiples dispositivos en la canalización en tiempo real está planificado para una versión futura.

## Sólo 4 de los 8 canales de mi Cyton se utilizan para análisis en vivo, ¿por qué?
La canalización de análisis en tiempo real (filtros, potencias de banda, incrustaciones de EEG, puntos de calidad de señal) está diseñada actualmente para entradas de 4 canales para coincidir con el formato de los auriculares Muse. Para Cyton (8 canales) y Cyton+Daisy (16 canales), los canales 1 a 4 alimentan la canalización en vivo; todos los canales se escriben en CSV para trabajo sin conexión. El soporte completo de canalización multicanal está en la hoja de ruta.

## ¿Cómo mejoro la calidad de la señal en una placa OpenBCI?
1. Aplique gel o pasta conductora en cada sitio del electrodo y separe el cabello para hacer contacto directo con el cuero cabelludo. 2. Verifique la impedancia con la verificación de impedancia de la GUI de OpenBCI antes de grabar; apunte a < 20 kΩ. 3. Conecte el electrodo de polarización SRB a la mastoides (detrás de la oreja) para obtener una referencia sólida. 4. Mantenga los cables de los electrodos cortos y alejados de las fuentes de alimentación. 5. Utilice el filtro de muesca en Configuración → Procesamiento de señal (50 Hz para Europa, 60 Hz para América). 6. Para Ganglion BLE: aleje la placa de los puertos USB 3.0, que emiten interferencias de 2,4 GHz.

## ¿{app} es compatible con la diadema AWEAR?
Sí. AWEAR es un dispositivo EEG BLE de un solo canal que muestrea a 256 Hz. La conexión funciona igual que con otros dispositivos BLE: encienda la diadema, otorgue permiso de Bluetooth si se solicita, y {app} la descubrirá y conectará automáticamente. El canal EEG único alimenta la canalización de análisis en tiempo real.

## Mi conexión OpenBCI se cae repetidamente: ¿cómo la estabilizo?
Ganglion BLE: mantén la placa a menos de 2 m; conecta el adaptador BLE del equipo host a un puerto USB 2.0 (USB 3.0 emite ruido de 2,4 GHz que puede degradar BLE). Cyton USB: usa un cable USB corto y de alta calidad, conectado directamente al ordenador en lugar de un hub. WiFi Shield: asegúrate de que el canal de 2,4 GHz del shield no se superponga con tu router; acerca la placa. En general, evita ejecutar otras aplicaciones con alto uso inalámbrico (videollamadas, sincronización de archivos) durante las grabaciones.

## ¿Qué registra exactamente el seguimiento de actividad?
El seguimiento de ventanas activas escribe una fila en Activity.sqlite cada vez que cambia el título de la ventana o aplicación frontal. Cada fila contiene: el nombre para mostrar de la aplicación (por ejemplo, "Safari", "Código VS"), la ruta completa al binario o al paquete de la aplicación, el título de la ventana (por ejemplo, el nombre del documento o el título de la página web; puede estar vacío para aplicaciones en espacio aislado) y una marca de tiempo de un segundo de Unix de cuándo se activó. El seguimiento del teclado y el mouse escribe una muestra periódica cada 60 segundos, pero solo cuando ha habido actividad desde la última descarga. Cada muestra almacena dos marcas de tiempo de Unix en segundos: el último evento del teclado y el último evento del mouse/trackpad. No registra qué teclas presionó, qué texto escribió, dónde estaba el cursor o en qué botones hizo clic. Ambas funciones están habilitadas de forma predeterminada y se pueden desactivar de forma independiente en Configuración → Seguimiento de actividad.

## ¿Por qué macOS solicita acceso de Accesibilidad para el seguimiento de entradas?
El seguimiento del teclado y el mouse utiliza CGEventTap, una API de macOS que intercepta eventos de entrada en todo el sistema antes de que lleguen a aplicaciones individuales. Apple requiere el permiso de Accesibilidad para cualquier aplicación que lea entradas globales, independientemente de lo que esa aplicación haga con ella. Sin acceso a Accesibilidad, el grifo falla silenciosamente: NeuroSkill continúa funcionando normalmente, pero las marcas de tiempo del último teclado y del último mouse permanecen en cero. Para otorgar acceso: Configuración del sistema → Privacidad y seguridad → Accesibilidad → buscar NeuroSkill → activar. Si prefiere no otorgarlo, desactive la opción "Seguimiento de actividad del teclado y el mouse" en Configuración; esto evita que el gancho se instale en primer lugar. El seguimiento de ventanas activas (nombre y ruta de la aplicación) utiliza AppleScript/osascript y no requiere permiso de Accesibilidad.

## ¿Cómo borro o elimino los datos de seguimiento de actividad?
Todos los datos de seguimiento de actividad se encuentran en un solo archivo: ~/.skill/activity.sqlite. Para eliminar todo: salga de NeuroSkill, elimine ese archivo y luego reinicie; se crea automáticamente una base de datos vacía en el siguiente inicio. Para detener la recopilación futura sin tocar los datos existentes, desactive ambas opciones en Configuración → Seguimiento de actividad; Los cambios entran en vigor inmediatamente sin necesidad de reiniciar. Para eliminar filas de forma selectiva, puede abrir el archivo en cualquier navegador SQLite (por ejemplo, DB Browser para SQLite) y ELIMINAR de active_windows o input_activity.

## ¿Por qué {app} solicita permiso de Accesibilidad en macOS?
{app} usa la API CGEventTap de macOS para registrar la última vez que se presionó una tecla o se movió el mouse. Esto se utiliza para calcular las marcas de tiempo de actividad del teclado y el mouse que se muestran en el panel Seguimiento de actividad. Sólo se almacena la marca de tiempo, sin pulsaciones de teclas ni posiciones del cursor. La función se degrada silenciosamente si no se concede el permiso.

## ¿{app} necesita permiso de Bluetooth?
Sí. {app} utiliza Bluetooth Low Energy (BLE) para conectarse a sus auriculares BCI. En macOS, el sistema mostrará un mensaje de permiso de Bluetooth por única vez cuando la aplicación intente escanear por primera vez. En Linux y Windows no se requiere ningún permiso explícito de Bluetooth.

## ¿Cómo otorgo permiso de Accesibilidad en macOS?
Abra Configuración del sistema → Privacidad y seguridad → Accesibilidad. Busque {app} en la lista y actívelo. También puedes hacer clic en "Abrir configuración de accesibilidad" en la pestaña Permisos dentro de la aplicación.

## ¿Qué pasa si niego el permiso de Accesibilidad?
Las marcas de tiempo de actividad del teclado y el mouse no se registrarán y permanecerán en cero. Todas las demás funciones (transmisión de EEG, potencia de banda, calibración, TTS, búsqueda) continúan funcionando normalmente. Puede desactivar la función por completo en Configuración → Seguimiento de actividad.

## ¿Puedo revocar permisos después de otorgarlos?
Sí. Abra Configuración del sistema → Privacidad y seguridad → Accesibilidad (o Notificaciones) y desactive {app}. La característica relevante dejará de funcionar inmediatamente sin necesidad de reiniciar.
