# Descripción general
NeuroSkill incluye un servidor LLM local opcional que le brinda un asistente de inteligencia artificial privado compatible con OpenAI sin enviar ningún dato a la nube.

## ¿Qué es la función LLM?
La función LLM incorpora un servidor de inferencia respaldado por llama.cpp directamente dentro de la aplicación. Cuando está habilitado, sirve puntos finales compatibles con OpenAI (/v1/chat/completions, /v1/completions, /v1/embeddings, /v1/models, /health) en el mismo puerto local que la API WebSocket. Puede apuntar a cualquier cliente compatible con OpenAI (Chatbot UI, Continuar, Open Interpreter o sus propios scripts).

## Privacidad y uso sin conexión
Toda la inferencia se ejecuta en su máquina. Ningún token, aviso o finalización sale nunca de localhost. La única actividad de la red es la descarga inicial del modelo desde HuggingFace Hub. Una vez que un modelo se almacena en caché localmente, puedes desconectarte de Internet por completo.

## API compatible con OpenAI
El servidor habla el mismo protocolo que la API OpenAI. Cualquier biblioteca que acepte un parámetro base_url (openai-python, openai-node, LangChain, LlamaIndex, etc.) funciona de inmediato. Establezca base_url en http://localhost:<port>/v1 y deje la clave API vacía a menos que haya configurado una en Configuración de inferencia.

# Gestión de modelos
Explore, descargue y active modelos de lenguaje cuantificados por GGUF desde el catálogo integrado.

## Catálogo de modelos
El catálogo enumera familias de modelos seleccionados (por ejemplo, Qwen, Llama, Gemma, Phi) con múltiples variantes de cuantificación por familia. Utilice el menú desplegable de familias para buscar y luego elija una cantidad específica para descargar. Los modelos marcados con ★ son los predeterminados recomendados para esa familia.

## Niveles de cuantificación
Cada modelo está disponible en varios niveles de cuantificación GGUF (Q4_K_M, Q5_K_M, Q6_K, Q8_0, etc.). Los cuantos más bajos son más pequeños y más rápidos, pero sacrifican algo de calidad. Q4_K_M suele ser la mejor compensación. Q8_0 casi no tiene pérdidas, pero requiere aproximadamente el doble de memoria. BF16/F16/F32 son pesos de referencia no cuantificados.

## Insignias de ajuste de hardware
Cada fila cuantitativa muestra una insignia codificada por colores que estima qué tan bien se adapta a su hardware: 🟢 Funciona excelente: cabe completamente en GPU VRAM con espacio libre. 🟡 Funciona bien: cabe en VRAM con un margen reducido. 🟠 Ajuste perfecto: es posible que necesite una descarga parcial de la CPU o un tamaño de contexto reducido. 🔴 No cabe: es demasiado grande para la memoria disponible. La estimación considera la VRAM de GPU, la RAM del sistema, el tamaño del modelo y la sobrecarga de contexto.

## Visión / Modelos Multimodales
Las familias etiquetadas como Vision o Multimodal incluyen un archivo de proyector multimodal opcional (mmproj). Descargue tanto el modelo de texto como su proyector para habilitar la entrada de imágenes en la ventana de chat. El proyector amplía el modelo de texto; no es un modelo independiente.

## Descargar y eliminar
Haga clic en 'Descargar' para buscar un modelo de HuggingFace Hub. Una barra de progreso muestra el estado de descarga en tiempo real. Puedes cancelar en cualquier momento. Los modelos descargados se almacenan localmente y se pueden eliminar para liberar espacio en el disco. Utilice el botón 'Actualizar caché' para volver a escanear el catálogo si modifica manualmente el directorio del modelo.

# Configuración de inferencia
Ajuste cómo el servidor carga y ejecuta modelos.

## Capas de GPU
Controla cuántas capas de transformador se descargan a la GPU. Establezca en 'Todos' para obtener la velocidad máxima si el modelo cabe en VRAM. Establezca en 0 para inferencia solo de CPU. Los valores intermedios dividen el modelo entre GPU y CPU, lo que resulta útil cuando el modelo apenas supera la capacidad de VRAM.

## Tamaño del contexto
El tamaño de la caché KV en tokens. 'Auto' elige el contexto más grande que se ajuste a su GPU/RAM según el tamaño y la cuantificación del modelo. Los contextos más grandes permiten que el modelo recuerde más historial de conversaciones pero consume más memoria. Las opciones están limitadas al máximo entrenado del modelo. Si se encuentra con errores de falta de memoria, intente reducir el tamaño del contexto.

## Solicitudes paralelas
Número máximo de bucles de decodificación simultáneos. Los valores más altos permiten que varios clientes compartan el servidor pero aumentan el uso máximo de memoria. Para la mayoría de las configuraciones de un solo usuario, 1 está bien.

## Clave API
Se requiere un token de portador opcional en cada solicitud /v1/*. Déjelo vacío para acceso abierto en localhost. Establezca una clave si expone el puerto en una red local y desea restringir el acceso.

# Herramientas integradas
El chat de LLM puede llamar a herramientas locales para recopilar información o tomar acciones en su nombre.

## Cómo funcionan las herramientas
Cuando el uso de herramientas está habilitado, el modelo puede solicitar llamar a una o más herramientas durante una conversación. La aplicación ejecuta la herramienta localmente y envía el resultado al modelo para que pueda incorporar información del mundo real en su respuesta. Las herramientas solo se invocan cuando el modelo las solicita explícitamente; nunca se ejecutan en segundo plano.

## Herramientas seguras
Fecha, Ubicación, Búsqueda web, Búsqueda web y Leer archivo son herramientas de solo lectura que no pueden modificar su sistema. Fecha devuelve la fecha y hora locales actuales. La ubicación proporciona una geolocalización aproximada basada en IP. Web Search ejecuta una consulta de respuesta instantánea DuckDuckGo. Web Fetch recupera el cuerpo del texto de una URL pública. Leer archivo lee archivos locales con paginación opcional.

## Herramientas privilegiadas (⚠️)
Bash, Write File y Edit File pueden modificar su sistema. Bash ejecuta comandos de shell con los mismos permisos que la aplicación. Write File crea o sobrescribe archivos en el disco. Editar archivo realiza ediciones de búsqueda y reemplazo. Están deshabilitados de forma predeterminada y muestran una insignia de advertencia. Habilítelos sólo si comprende los riesgos.

## Modo de ejecución y límites
El modo paralelo permite que el modelo llame a varias herramientas a la vez (más rápido). El modo secuencial los ejecuta uno a la vez (más seguro para herramientas con efectos secundarios). 'Rondas máximas' limita la cantidad de viajes de ida y vuelta de llamada de herramienta/resultado de herramienta que se permiten por mensaje. 'Máximo de llamadas por ronda' limita el número de invocaciones simultáneas de herramientas.

# Chat y registros
Interactuar con el modelo y monitorear la actividad del servidor.

## Ventana de conversación
Abra la ventana de chat desde la tarjeta del servidor LLM o el menú de la bandeja. Proporciona una interfaz de chat familiar con representación de rebajas, resaltado de código y visualización de llamadas de herramientas. Las conversaciones son efímeras: no se guardan en el disco. Los modelos con capacidad de visión aceptan archivos adjuntos de imágenes mediante arrastrar y soltar o con el botón de archivos adjuntos.

## Usando clientes externos
Debido a que el servidor es compatible con OpenAI, puede utilizar cualquier interfaz de chat externa. Apunte a http://localhost:<port>/v1, establezca una clave API si configuró una y seleccione cualquier nombre de modelo de /v1/models. Las opciones populares incluyen Open WebUI, Chatbot UI, Continuar (VS Code) y curl/httpie para secuencias de comandos.

## Registros del servidor
El visor de registros en la parte inferior del panel de configuración de LLM transmite la salida del servidor en tiempo real. Muestra el progreso de carga del modelo, la velocidad de generación de tokens y cualquier error. Habilite el modo 'Detallado' en la sección avanzada para obtener resultados de diagnóstico detallados de llama.cpp. Registra el desplazamiento automático, pero puedes pausarlo desplazándote hacia arriba manualmente.
