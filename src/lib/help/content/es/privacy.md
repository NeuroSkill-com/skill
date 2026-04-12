# Descripción general de privacidad
{app} está diseñado para ser completamente local primero. Sus datos, incrustaciones, etiquetas y configuraciones de EEG nunca salen de su máquina a menos que usted elija explícitamente compartirlos.

# Almacenamiento de datos

## Todos los datos permanecen en su dispositivo
Cada dato que {app} registra — muestras de EEG sin procesar (CSV), incrustaciones de EEG (SQLite + índice HNSW), etiquetas de texto, marcas de tiempo de calibración, registros y configuraciones — se almacena localmente en {dataDir}/. No se sube ningún dato a ningún servicio en la nube, servidor o tercero.

## Sin cuentas de usuario
{app} no requiere registro, inicio de sesión ni ninguna forma de creación de cuenta. No se almacenan ni transmiten identificadores de usuario, tokens ni credenciales de autenticación.

## Ubicación de datos
Todos los archivos se almacenan en {dataDir}/ en macOS y Linux. Cada día de grabación tiene su propio subdirectorio AAAAMMDD que contiene la base de datos EEG SQLite y el índice vectorial HNSW. Las etiquetas están en {dataDir}/labels.sqlite. Los registros están en {dataDir}/logs/. Puede eliminar cualquiera de estos archivos en cualquier momento.

# Actividad de red

## Sin telemetría ni análisis
{app} no recopila análisis de uso, informes de fallos, telemetría ni ninguna forma de seguimiento del comportamiento. No hay SDK de análisis, píxeles de seguimiento ni balizas de teléfono residencial integrados en la aplicación.

## Servidor WebSocket solo local
{app} ejecuta un servidor WebSocket vinculado a su interfaz de red local para la transmisión LAN a herramientas complementarias. Este servidor no está expuesto a Internet. Transmite métricas EEG derivadas (potencias de banda, puntuaciones, frecuencia cardíaca) y actualizaciones de estado a clientes en la misma red local. Los flujos de muestra sin procesar de EEG/PPG/IMU no se transmiten.

## Servicio mDNS/Bonjour
{app} registra un _skill._tcp.local. Servicio mDNS para que los clientes LAN puedan descubrir el puerto WebSocket automáticamente. Este anuncio es sólo local (DNS de multidifusión) y no es visible fuera de su red.

## Comprobaciones de actualización
Cuando hace clic en 'Buscar actualizaciones' en Configuración, {app} se comunica con el punto final de actualización configurado para buscar una versión más nueva. Esta es la única solicitud de Internet saliente que realiza la aplicación y solo ocurre cuando la activa explícitamente. Los paquetes de actualización se verifican con una firma Ed25519 antes de la instalación.

# Bluetooth y seguridad del dispositivo

## Bluetooth de bajo consumo (BLE)
{app} se comunica con tu dispositivo BCI mediante Bluetooth Low Energy o serie USB. La conexión usa la pila estándar del sistema: CoreBluetooth (macOS) o BlueZ (Linux). No se instalan drivers Bluetooth personalizados ni módulos de kernel.

## Permisos a nivel de sistema operativo
El acceso a Bluetooth requiere un permiso explícito del sistema. En macOS, debe otorgar acceso a Bluetooth en Configuración del sistema → Privacidad y seguridad → Bluetooth. {app} no puede acceder a Bluetooth sin su consentimiento.

## Identificadores de dispositivos
El número de serie del dispositivo y la dirección MAC se reciben del auricular BCI y se muestran en la interfaz de usuario. Estos identificadores se almacenan únicamente en el archivo de configuración local y nunca se transmiten a través de la red.

# Procesamiento en el dispositivo

## La inferencia de GPU permanece local
El codificador de incrustación de EEG se ejecuta completamente en su GPU local a través de wgpu. Los pesos del modelo se cargan desde la caché local de Hugging Face (~/.cache/huggingface/). No se envían datos de EEG a ninguna API de inferencia externa ni GPU en la nube. Las incrustaciones de texto para la búsqueda de etiquetas utilizan nomic-embed-text-v1.5, también almacenado en caché localmente.

## Filtrado y análisis
Todo el procesamiento de señales (filtrado para guardar superposiciones, cálculo de potencia de banda FFT, generación de espectrogramas y monitoreo de la calidad de la señal) se ejecuta localmente en su CPU/GPU. Ningún dato EEG sin procesar o procesado sale de su máquina.

## Búsqueda de vecino más cercano
El índice de vectores HNSW utilizado para la búsqueda de similitudes se crea y consulta completamente en su dispositivo. Las consultas de búsqueda nunca salen de su máquina.

# Tus datos, tu control

## Acceso
Todos sus datos están en {dataDir}/ en formatos estándar (CSV, SQLite, binario HNSW). Puedes leerlo, copiarlo o procesarlo con cualquier herramienta.

## Borrar
Elimine cualquier archivo o directorio en {dataDir}/ en cualquier momento. No hay que preocuparse por las copias de seguridad en la nube. La desinstalación de la aplicación elimina solo el binario de la aplicación: sus datos en {dataDir}/ no se modifican a menos que los elimine.

## Exportar
Las grabaciones CSV y las bases de datos SQLite son formatos estándar portátiles. Cópielos a cualquier máquina o impórtelos a Python, R, MATLAB o cualquier herramienta de análisis.

## cifrar
{app} no cifra datos en reposo. Si necesita cifrado a nivel de disco, utilice el cifrado de disco completo de su sistema operativo (FileVault en macOS, LUKS en Linux).

# Seguimiento de actividad

## Seguimiento de actividad
Cuando está habilitado, NeuroSkill registra qué aplicación está en primer plano y la última vez que se utilizaron el teclado y el mouse. Estos datos permanecen completamente en su dispositivo en ~/.skill/activity.sqlite; nunca se envían a ningún servidor, no se registran de forma remota ni se incluyen en ningún tipo de análisis. Capturas de seguimiento de ventanas activas: nombre de la aplicación, ruta ejecutable, título de la ventana y marca de tiempo de Unix en la que esa ventana se activó. El seguimiento del teclado y el mouse captura solo dos marcas de tiempo (último evento del teclado, último evento del mouse): nunca pulsaciones de teclas, texto escrito, coordenadas del cursor ni objetivos de clic. Ambas funciones se pueden desactivar de forma independiente en Configuración → Seguimiento de actividad; Al desactivar una función, se detiene inmediatamente la recopilación. Las filas existentes no se eliminan automáticamente, pero puedes eliminarlas en cualquier momento eliminando Activity.sqlite.

## Permiso de accesibilidad (macOS)
En macOS, el seguimiento del teclado y el mouse requiere el permiso de Accesibilidad porque instala un CGEventTap, un enlace a nivel del sistema que intercepta eventos de entrada. Apple exige este permiso para cualquier aplicación que lea entradas globales. El permiso se solicita solo cuando la función está habilitada. Si lo rechaza o lo revoca, el enlace falla silenciosamente: el resto de la aplicación continúa normalmente y solo las marcas de tiempo de actividad de entrada permanecen en cero. El seguimiento de ventana activa (nombre/ruta de la aplicación) no requiere Accesibilidad: utiliza AppleScript/osascript que funciona dentro de los derechos de aplicación normales.

# Resumen

## No cloud
Ninguna nube. Todos los datos, incrustaciones, etiquetas y configuraciones de EEG se almacenan localmente en {dataDir}/.

## No telemetry
Sin telemetría. Sin análisis, informes de fallos ni seguimiento de uso de ningún tipo.

## No accounts
Sin cuentas. Sin registro, inicio de sesión ni identificadores de usuario.

## One optional network request
Una solicitud de red opcional. Actualice las comprobaciones, solo cuando las active explícitamente.

## Fully on-device
Totalmente en el dispositivo. La inferencia de GPU, el procesamiento de señales y la búsqueda se ejecutan localmente.

## Activity tracking is local-only
El seguimiento de la actividad es solo local. El foco de la ventana y las marcas de tiempo de entrada se escriben en Activity.sqlite en su dispositivo y nunca lo abandonan.
