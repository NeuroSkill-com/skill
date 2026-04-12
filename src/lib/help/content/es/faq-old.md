## ¿Cómo se dispara un gancho?
El trabajador compara cada nueva incorporación de EEG con ejemplos de etiquetas recientes seleccionados por palabra clave + similitud de texto. Si la mejor distancia del coseno está por debajo de su umbral, el gancho se dispara.

## ¿Por qué el ícono de la bandeja se vuelve rojo?
Bluetooth está desactivado en tu Mac. Abra Configuración del sistema → Bluetooth y habilítelo. {app} se volverá a conectar automáticamente en aproximadamente 1 segundo.

## La aplicación sigue girando pero nunca se conecta. ¿Qué debo hacer?
1. Asegúrese de que el dispositivo BCI esté encendido (Muse: mantenga presionado hasta que sienta una vibración; Ganglion/Cyton: verifique el LED azul). 2. Manténgalo a menos de 5 m. 3. Si aún falla, reinicie el dispositivo.

## ¿Cómo otorgo permiso a Bluetooth?
macOS mostrará un cuadro de diálogo de permiso la primera vez que {app} intente conectarse. Si lo descartó, vaya a Configuración del sistema → Privacidad y seguridad → Bluetooth y habilite {app}.

## ¿Puedo recibir datos de EEG en otra aplicación en la misma red?
Sí. Conecte un cliente WebSocket a la dirección que se muestra en el resultado de descubrimiento de Bonjour (consulte la sección Transmisión de red local más arriba). Recibirá métricas derivadas (~4 Hz eventos de bandas eeg con más de 60 puntuaciones) y el estado del dispositivo (~1 Hz). Nota: los flujos de muestra de EEG/PPG/IMU sin procesar no están disponibles a través de la API WebSocket; solo puntuaciones procesadas y potencias de banda.

## ¿Dónde se guardan mis grabaciones de EEG?
Las muestras sin procesar (sin filtrar) se escriben en un archivo CSV en la carpeta de datos de su aplicación ({dataDir}/ en macOS/Linux). Se crea un archivo por sesión.

## ¿Qué significan los puntos de calidad de la señal?
Cada punto representa un canal EEG (TP9, AF7, AF8, TP10). Verde = Bueno (bajo nivel de ruido, buen contacto con la piel). Amarillo = Aceptable (algún artefacto de movimiento o electrodo suelto). Rojo = Deficiente (alto ruido, contacto muy flojo o electrodo fuera de la piel). Gris = Sin señal.

## ¿Para qué sirve el filtro de muesca powerline?
La red eléctrica induce ruidos de 50 o 60 Hz en las grabaciones de EEG. El filtro de muesca elimina esa frecuencia (y sus armónicos) de la visualización de forma de onda. Seleccione 60 Hz (EE. UU./Japón) o 50 Hz (UE/Reino Unido) para que coincida con su red eléctrica local.

## ¿Qué métricas se almacenan en la base de datos?
Cada época de 2,5 segundos almacena: el vector de incrustación ZUNA (32-D), potencias de banda relativas (delta, theta, alfa, beta, gamma, gamma alta) promediadas entre canales, potencias de banda por canal como un blob JSON, puntuaciones derivadas (relajación, compromiso), asimetría alfa frontal (FAA), relaciones de banda cruzada (TAR, BAR, DTR, TBR), forma espectral (PSE, APF, SEF95, centroide espectral, BPS, SNR), coherencia, supresión de Mu, composición del estado de ánimo, parámetros de Hjorth (actividad, movilidad, complejidad), complejidad no lineal (entropía de permutación, FD de Higuchi, DFA, entropía de muestra), PAC (θ–γ), índice de lateralidad, promedios de PPG y métricas derivadas de PPG (HR, RMSSD, SDNN, pNN50, LF/HF, frecuencia respiratoria, SpO₂, índice de perfusión, índice de estrés) si hay un Muse 2/S conectado.

## ¿Qué es la función de comparación de sesiones?
Comparación de sesiones (⌘⇧M) le permite elegir dos sesiones de grabación y compararlas una al lado de la otra. Muestra: barras de potencia de banda relativa con deltas, todas las puntuaciones y proporciones derivadas, asimetría alfa frontal, hipnogramas de estadificación del sueño y una proyección de incorporación UMAP 3D que visualiza cuán similares son las dos sesiones en un espacio de características de alta dimensión.

## ¿Qué es el visor UMAP 3D?
El visor UMAP proyecta incrustaciones de EEG de alta dimensión en un espacio 3D para que estados cerebrales similares aparezcan como puntos cercanos. La sesión A (azul) y la sesión B (ámbar) forman grupos distintos si las sesiones son diferentes. Puede orbitar, hacer zoom y hacer clic en puntos etiquetados para ver sus conexiones temporales.

## ¿Por qué el visor UMAP muestra al principio una nube aleatoria?
UMAP es costoso desde el punto de vista computacional: se ejecuta en una cola de trabajos en segundo plano para que la interfaz de usuario siga respondiendo. Mientras se calcula, se muestra una nube de marcador de posición gaussiana aleatoria. Una vez que la proyección real está lista, los puntos se animan suavemente hasta sus posiciones finales.

## ¿Qué son las etiquetas y cómo se utilizan?
Las etiquetas son etiquetas definidas por el usuario (por ejemplo, 'meditación', 'lectura', 'ansioso') que usted adjunta a un momento en el tiempo durante una grabación. Se almacenan junto con las incorporaciones de EEG en la base de datos. En el visor UMAP, los puntos etiquetados aparecen como puntos más grandes con anillos de colores.

## ¿Qué es la asimetría alfa frontal (FAA)?
FAA es ln(AF8 α) − ln(AF7 α). Un valor positivo sugiere una mayor supresión alfa en el hemisferio izquierdo, asociada con la motivación de aproximación (compromiso, curiosidad). Un valor negativo sugiere retraimiento (evitación, ansiedad).

## ¿Cómo funciona la puesta en escena del sueño?
{app} clasifica cada época de EEG en sueño Wake, N1 (ligero), N2, N3 (profundo) o REM según las relaciones de potencia relativas delta, theta, alfa y beta. La vista de comparación muestra un hipnograma para cada sesión con desgloses de etapas codificados por colores y porcentajes de tiempo.

## ¿Cuáles son los atajos de teclado?
⌘⇧O: abre la ventana {app}. ⌘⇧M — Comparación de sesiones abiertas. Puede personalizar los atajos en Configuración → Atajos.

## ¿Qué es la API WebSocket?
{app} expone una API WebSocket basada en JSON en la red local (mDNS: _skill._tcp). Los comandos incluyen: estado, etiqueta, búsqueda, comparación, sesiones, suspensión, umap y umap_poll. Ejecute 'node test.js' desde el directorio del proyecto para probar todos los comandos.

## ¿Cuáles son las puntuaciones derivadas (Relajación, Compromiso)?
Relajación = α / (β + θ), que mide la vigilia tranquila. Compromiso = β / (α + θ), que mide la implicación mental sostenida. Ambos están mapeados en una escala de 0 a 100.

## ¿Cuáles son las relaciones entre bandas?
TAR (Theta/Alpha): los valores más altos indican somnolencia o estados meditativos. BAR (Beta/Alfa): los valores más altos indican estrés o atención concentrada. DTR (Delta/Theta): los valores más altos indican sueño profundo o relajación profunda. Todos se promedian entre canales.

## ¿Qué son PSE, APF, BPS y SNR?
PSE (Entropía espectral de potencia, 0–1) mide la complejidad espectral. APF (Alpha Peak Frequency, Hz) es la frecuencia de máxima potencia alfa. BPS (pendiente de banda-potencia) es el exponente aperiódico 1/f. SNR (relación señal-ruido, dB) compara la potencia de banda ancha con el ruido de línea de 50 a 60 Hz.
