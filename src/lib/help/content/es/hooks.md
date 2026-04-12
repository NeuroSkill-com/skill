# Descripción general
Los ganchos proactivos permiten que la aplicación active acciones automáticamente cuando sus patrones de EEG recientes coincidan con palabras clave o estados cerebrales específicos.

## ¿Qué son los ganchos proactivos?
Un gancho proactivo es una regla que monitorea las incrustaciones recientes de etiquetas de EEG en tiempo real. Cuando la distancia del coseno entre las incrustaciones recientes de su estado cerebral y las incrustaciones de palabras clave del gancho cae por debajo de un umbral configurado, el gancho se activa: envía un comando, muestra una notificación, activa TTS o transmite un evento WebSocket. Los ganchos le permiten crear automatizaciones de neurorretroalimentación de circuito cerrado sin escribir código.

## Cómo funciona
Cada pocos segundos, la aplicación calcula incorporaciones de EEG a partir de los datos cerebrales más recientes. Estos se comparan con las incrustaciones de palabras clave definidas en cada gancho activo utilizando similitud de coseno sobre el índice HNSW. Si se alcanza el umbral de distancia de cualquier gancho, el gancho se dispara. Un tiempo de reutilización evita que el mismo gancho se dispare repetidamente en rápida sucesión. La coincidencia es puramente local: ningún dato sale de su máquina.

## Escenarios
Cada gancho puede limitarse a un escenario: cognitivo, emocional, físico o cualquiera. Los ganchos cognitivos se dirigen a estados mentales como la concentración, la distracción o la fatiga mental. Los ganchos emocionales se dirigen a estados afectivos como el estrés, la calma o la frustración. Los ganchos físicos se dirigen a estados corporales como la somnolencia o la fatiga física. 'Cualquiera' coincide independientemente de la categoría del escenario inferido.

# Configurar un gancho
Cada gancho tiene varios campos que controlan cuándo y cómo se dispara.

## Nombre del gancho
Un nombre descriptivo para el gancho (por ejemplo, 'Deep Work Guard', 'Calm Recovery'). El nombre se utiliza en el registro histórico y en los eventos de WebSocket. Debe ser único en todos los ganchos.

## Palabras clave
Una o más palabras clave o frases cortas que describan el estado cerebral que desea detectar (por ejemplo, "concentración", "trabajo profundo", "estrés", "cansado"). Estos se integran utilizando el mismo modelo de transformador de oraciones que las etiquetas de EEG. El gancho se activa cuando las incorporaciones recientes de EEG están cerca de estas incorporaciones de palabras clave en el espacio vectorial compartido.

## Sugerencias de palabras clave
A medida que escribe una palabra clave, la aplicación sugiere términos relacionados de su historial de etiquetas existente utilizando una coincidencia de cadenas difusa y una similitud de incrustación semántica. Las sugerencias muestran una insignia de fuente: "difusa" para coincidencias basadas en cadenas, "semántica" para coincidencias basadas en incrustaciones o "difusa+semántica" para ambas. Utilice las teclas de flecha ↑/↓ y Enter para aceptar rápidamente una sugerencia.

## Umbral de distancia
La distancia máxima de coseno (0–1) entre las incrustaciones de EEG recientes y las incrustaciones de palabras clave del gancho para que se dispare el gancho. Los valores más bajos requieren una coincidencia más cercana (más estricta), los valores más altos se activan con más frecuencia (más indulgentes). Los valores típicos oscilan entre 0,08 (muy estricto) y 0,25 (laxo). Comience alrededor de 0,12–0,16 y ajuste según la herramienta de sugerencias.

## Herramienta de sugerencia de distancia
Haga clic en 'Sugerir umbral' para analizar los datos de EEG registrados con las palabras clave del gancho. La herramienta calcula la distribución de la distancia (min, p25, p50, p75, max) y recomienda un umbral que equilibra la sensibilidad y la especificidad. Una barra de percentiles visual muestra dónde se encuentran los umbrales actuales y sugeridos en la distribución. Haga clic en 'Aplicar' para utilizar el valor sugerido.

## Referencias recientes
El número de muestras de inclusión de EEG más recientes que se compararán con las palabras clave del gancho (predeterminado: 12). Los valores más altos suavizan los picos transitorios pero aumentan la latencia de detección. Los valores más bajos reaccionan más rápido pero pueden dispararse con artefactos breves. Rango válido: 10–20.

## Comando
Una cadena de comando opcional que se transmite en el evento WebSocket cuando se activa el gancho (por ejemplo, 'focus_reset', 'calm_breath'). Las herramientas de automatización externas que escuchan en WebSocket pueden reaccionar a este comando para activar acciones, notificaciones o scripts específicos de la aplicación.

## Texto de carga útil
Un mensaje opcional legible por humanos incluido en el evento de activación del gancho (por ejemplo, "Tómate un descanso de 2 minutos"). Este texto se muestra en las notificaciones y se puede pronunciar en voz alta a través de TTS si la guía de voz está habilitada.

# Avanzado
Consejos, historia e integración con herramientas externas.

## Ejemplos rápidos
El panel 'Ejemplos rápidos' proporciona plantillas de ganchos listas para usar para casos de uso comunes: Deep Work Guard (restablecimiento del enfoque cognitivo), Calm Recovery (alivio del estrés emocional) y Body Break (fatiga física). Haga clic en cualquier ejemplo para agregarlo como un nuevo enlace con palabras clave, escenario, umbral y carga útil precargados. Ajuste los valores para que coincidan con sus patrones EEG personales.

## Historia del incendio del gancho
El registro histórico plegable en la parte inferior del panel Hooks registra cada evento de disparo de gancho con marca de tiempo, etiqueta coincidente, distancia de coseno, comando y palabras clave en el momento del disparo. Úselo para auditar el comportamiento de los enlaces, verificar umbrales y depurar falsos positivos. Expanda cualquier fila para ver todos los detalles. Los controles de paginación le permiten explorar eventos más antiguos.

## Eventos WebSocket
Cuando se activa un enlace, la aplicación transmite un evento JSON a través de la API WebSocket que contiene el nombre del enlace, el comando, el texto, la etiqueta coincidente, la distancia y la marca de tiempo. Los clientes externos pueden escuchar estos eventos para crear automatizaciones personalizadas, por ejemplo, atenuar las luces, pausar la música, enviar un mensaje de Slack o iniciar sesión en un panel personal.

## Consejos de sintonización
Comience con un gancho y algunas palabras clave que coincidan con las etiquetas que ya registró. Utilice la herramienta de sugerencia de distancia para establecer un umbral inicial. Supervise el registro del historial durante un día y ajústelo: reduzca el umbral si ve falsos positivos, súbalo si el anzuelo nunca se dispara. Agregar palabras clave más específicas (por ejemplo, "lectura profunda" versus "enfoque") generalmente mejora la precisión. Evite palabras clave muy cortas o genéricas de una sola palabra, a menos que desee una concordancia amplia.
