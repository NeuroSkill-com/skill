# Descripción general

## Transmisión en vivo
{app} transmite métricas de EEG derivadas y el estado del dispositivo a través de un servidor WebSocket local. Los eventos de transmisión incluyen: bandas eeg (~4 Hz: más de 60 puntuaciones), estado del dispositivo (~1 Hz: batería, estado de conexión) y etiqueta creada. Las muestras sin procesar de EEG/PPG/IMU no están disponibles a través de la API WebSocket. El servicio se anuncia a través de Bonjour/mDNS como _skill._tcp para que los clientes puedan descubrirlo automáticamente.

## Comandos
Los clientes pueden enviar comandos JSON a través de WebSocket: estado (instantánea completa del sistema), calibrar (calibración abierta), etiqueta (enviar una anotación), buscar (consulta del vecino más cercano), sesiones (listar grabaciones), comparar (métricas A/B + suspensión + UMAP), suspensión (puesta en escena del sueño), umap/umap_poll (proyección de incrustación 3D). Las respuestas llegan a la misma conexión que JSON con un valor booleano "ok".

# Referencia de comando

## estado
_(ninguno)_

Returns device state, session info, embedding counts (today & all-time), label count, last calibration timestamp, and per-channel signal quality.

## calibrar
_(ninguno)_

Abre la ventana de calibración. Requiere un dispositivo de transmisión conectado.

## etiqueta
texto (cadena, requerido); label_start_utc (u64, opcional; el valor predeterminado es ahora)

Inserta una etiqueta con marca de tiempo en la base de datos de etiquetas. Devuelve el nuevo label_id.

## buscar
start_utc, end_utc (u64, obligatorio); k, ef (u64, opcional)

Searches the HNSW embedding index for the k nearest neighbours within the given time range.

## comparar
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, obligatorio)

Compares two time ranges by returning aggregated band-power metrics (relative powers, relaxation/engagement scores, and FAA) for each. Returns { a: SessionMetrics, b: SessionMetrics }.

## sesiones
_(ninguno)_

Lists all embedding sessions discovered from the daily eeg.sqlite databases. Sessions are contiguous recording ranges (gap > 2 min = new session). Returns newest first.

## dormir
start_utc, end_utc (u64, obligatorio)

Classifies each embedding epoch in the time range into a sleep stage (Wake/N1/N2/N3/REM) using band-power ratios and returns a hypnogram with per-stage summary.

## mapa
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, obligatorio)

Enqueues a 3D UMAP projection of embeddings from two sessions. Returns a job_id for polling. Non-blocking.

## umap_poll
job_id (cadena, requerida)

Polls for the result of a previously enqueued UMAP job. Returns { status: 'pending' | 'done', points?: [...] }.

## decir
texto: cadena (obligatorio)

Speak text via on-device TTS. Fire-and-forget — returns immediately while audio plays in the background. Initialises the TTS engine on first call.
