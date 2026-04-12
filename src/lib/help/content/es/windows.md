# ventanas
{app} usa ventanas separadas para tareas específicas. Cada uno se puede abrir desde el menú contextual de la bandeja o mediante un atajo de teclado global.

## 🏷 Ventana de etiqueta
Se abre a través del menú de la bandeja, el acceso directo global o el botón de etiqueta en la ventana principal. Escriba una etiqueta de texto libre para anotar el momento actual del EEG (por ejemplo, "meditación", "lectura enfocada"). La etiqueta se guarda en {dataDir}/labels.sqlite con el rango de marca de tiempo exacto. Envíe con Ctrl/⌘+Entrar o haga clic en Enviar. Presione Escape para cancelar.

## 🔍 Ventana de búsqueda
La ventana de búsqueda tiene tres modos: similitud de EEG, texto e interactivo, cada uno de los cuales consulta los datos registrados de una manera diferente.

## Búsqueda de similitud de EEG
Elija un rango de fecha y hora de inicio/finalización y ejecute una búsqueda aproximada del vecino más cercano en todas las incrustaciones de ZUNA registradas en esa ventana. El índice HNSW devuelve las k épocas de EEG de 5 segundos más similares de todo su historial, clasificadas por distancia de coseno. Menor distancia = estado cerebral más similar. Cualquier etiqueta que se superponga a una marca de tiempo de resultado se muestra en línea. Útil para encontrar momentos pasados ​​que "parecieron" similares a un período de referencia.

## Búsqueda de incrustación de texto
Escriba cualquier concepto, actividad o estado mental en lenguaje sencillo (por ejemplo, "enfoque profundo", "ansioso", "meditación con los ojos cerrados"). Su consulta está integrada en el mismo modelo de transformador de oraciones que se utiliza para la indexación de etiquetas y se compara con cada anotación que haya escrito mediante similitud de coseno sobre el índice de etiquetas HNSW. Los resultados son sus propias etiquetas clasificadas por cercanía semántica, no por concordancia de palabras clave. Puede filtrar la lista y reordenarla por fecha o similitud. Un gráfico kNN 3D visualiza la estructura de vecindad: el nodo de consulta se encuentra en el centro, las etiquetas de resultados se irradian hacia afuera según la distancia.

## Búsqueda intermodal interactiva
Ingrese un concepto de texto libre y {app} ejecutará una canalización intermodal de cuatro pasos: (1) la consulta se incrusta en un vector de texto; (2) se recuperan las k etiquetas semánticamente más similares (texto-k); (3) para cada etiqueta coincidente, se calcula su incrustación EEG media y se utiliza para buscar en los índices EEG HNSW diarios los k momentos EEG más similares (eeg-k); (4) para cada vecino EEG, se recopilan etiquetas cercanas dentro de ± minutos de alcance (etiqueta-k). El resultado es un gráfico dirigido con cuatro capas de nodos (Consulta → Coincidencias de texto → Vecinos EEG → Etiquetas encontradas) representado como una visualización 3D interactiva y exportable como SVG o Graphviz DOT. Utilice los controles deslizantes text-k / eeg-k / label-k para controlar la densidad del gráfico y ±reach para ampliar o reducir la ventana de búsqueda temporal.

## 🎯 Ventana de calibración
Ejecuta una tarea de calibración guiada: alternando fases de acción (por ejemplo, "ojos abiertos" → descanso → "ojos cerrados" → descanso) para un número configurable de bucles. Requiere un dispositivo BCI de transmisión conectado. Los eventos de calibración se emiten a través del bus de eventos Tauri y WebSocket para que las herramientas externas puedan sincronizarse. La marca de tiempo de la última calibración completada se guarda en la configuración.

## ⚙ Ventana de configuración
Cuatro pestañas: Configuración, Atajos (teclas de acceso rápido globales, paleta de comandos, teclas en la aplicación), Modelo EEG (codificador y estado HNSW). Ábralo desde el menú de la bandeja o el botón de engranaje en la ventana principal.

## ?  Ventana de ayuda
Esta ventana. Una referencia completa para cada parte de la interfaz {app}: el panel principal, cada pestaña de configuración, cada ventana emergente, el ícono de la bandeja y la API WebSocket. Abrir desde el menú de la bandeja.

## 🧭 Asistente de configuración
Un asistente de primera ejecución de cinco pasos que lo guía a través del emparejamiento de Bluetooth, el ajuste de los auriculares y la primera calibración. Se abre automáticamente en el primer lanzamiento; se puede volver a abrir en cualquier momento desde la paleta de comandos (⌘K → Asistente de configuración).

## 🌐 Ventana de estado de API
Un panel en vivo que muestra todos los clientes WebSocket actualmente conectados y un registro de solicitudes desplazable. Muestra el puerto del servidor, el protocolo y la información de descubrimiento de mDNS. Incluye fragmentos de conexión rápida para ws:// y dns-sd. Se actualiza automáticamente cada 2 segundos. Ábralo desde el menú de la bandeja o la paleta de comandos.

## 🌙 Puesta en escena del sueño
Para sesiones que duran 30 minutos o más, la vista Historial muestra un hipnograma generado automáticamente: un gráfico en escalera de las etapas del sueño (Wake / N1 / N2 / N3 / REM) clasificadas según proporciones de potencia de las bandas delta, theta, alfa y beta. Amplíe cualquier sesión larga en Historial para ver el hipnograma con un desglose por etapa que muestra el porcentaje y la duración. Nota: los auriculares BCI de consumo, como Muse, utilizan 4 electrodos secos, por lo que la estadificación es aproximada; no es un polisomnógrafo clínico.

## ⚖ Ventana de comparación
Elija dos rangos de tiempo cualesquiera en la línea de tiempo y compare sus distribuciones promedio de potencia de banda, puntajes de relajación/compromiso y asimetría alfa frontal uno al lado del otro. Incluye estadificación del sueño, métricas avanzadas y Brain Nebula™, una proyección UMAP en 3D que muestra cuán similares son los dos períodos en el espacio EEG de alta dimensión. Abra desde el menú de la bandeja o la paleta de comandos (⌘K → Comparar).

# Superposiciones y paleta de comandos
Superposiciones de acceso rápido disponibles en cada ventana mediante atajos de teclado.

## ⌨ Paleta de comandos (⌘K / Ctrl+K)
Un menú desplegable de acceso rápido que enumera todas las acciones ejecutables en la aplicación. Comience a escribir comandos de filtro difuso, use ↑↓ para navegar y presione Entrar para ejecutar. Disponible en todas las ventanas. Los comandos incluyen abrir ventanas (Configuración, Ayuda, Búsqueda, Etiqueta, Historial, Calibración), acciones del dispositivo (volver a intentar conectar, abrir la configuración de Bluetooth) y utilidades (mostrar superposiciones de accesos directos, buscar actualizaciones).

## ?  Superposición de atajos de teclado
Prensa ? en cualquier ventana (fuera de las entradas de texto) para alternar una superposición flotante que enumera todos los atajos de teclado: atajos globales configurados en Configuración → Atajos, además de teclas en la aplicación como ⌘K para la paleta de comandos y ⌘Enter para enviar etiquetas. Prensa ? nuevamente o Esc para descartar.
