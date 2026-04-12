# Guía de voz (TTS) en el dispositivo

## Guía de voz (TTS) en el dispositivo
NeuroSkill™ incluye un motor de conversión de texto a voz en inglés completamente integrado en el dispositivo. Anuncia las fases de calibración en voz alta (etiquetas de acción, pausas, finalización) y se puede activar de forma remota desde cualquier script a través de WebSocket o HTTP API. Toda la síntesis se ejecuta localmente: no se necesita Internet después de descargar una vez el modelo de ~30 MB.

## Cómo funciona
Preprocesamiento de texto → fragmentación de oraciones (≤400 caracteres) → fonemización a través de libespeak-ng (biblioteca C, en proceso, voz en-us) → tokenización (IPA → ID de enteros) → inferencia ONNX (modelo KittenTTS: input_ids + estilo + velocidad → forma de onda f32) → 1 s de silencio → rodio se reproduce en la salida de audio predeterminada del sistema.

## Modelo
KittenML/kitten-tts-mini-0.8 de HuggingFace Hub. Voz: Jasper (inglés en-us). Frecuencia de muestreo: 24 000 Hz mono float32. INT8 ONNX cuantificado: solo CPU, no se requiere GPU. Almacenado en caché en ~/.cache/huggingface/hub/ después de la primera descarga.

## Requisitos
espeak-ng debe estar instalado y en PATH: proporciona fonemización IPA en proceso (vinculada como una biblioteca C, no generada como un subproceso). macOS: instalación de cerveza espeak-ng. Ubuntu/Debian: apto para instalar libespeak-ng-dev. Alpine: apk agrega espeak-ng-dev. Fedora: dnf instala espeak-ng-devel.

## Integración de calibración
Cuando comienza una sesión de calibración, el motor se precalienta en segundo plano (descargando el modelo si es necesario). En cada fase, la ventana de calibración llama a tts_speak con la etiqueta de acción, anuncio de pausa, mensaje de finalización o aviso de cancelación. El habla nunca bloquea la calibración: todas las llamadas TTS son de tipo "dispara y olvida".

## API: diga el comando
Active la voz desde cualquier script externo, herramienta de automatización o agente LLM. El comando regresa inmediatamente mientras se reproduce el audio. WebSocket: {"command":"say","text":"your message"}. HTTP: POST /say con cuerpo {"text":"your message"}. CLI (curl): curl -X POST http://localhost:<port>/say -d \\'{"text":"hello"}\\' -H \\'Tipo de contenido: aplicación/json\\'.

## Registro de depuración
Habilite el registro de síntesis TTS en Configuración → Voz para escribir eventos (texto hablado, recuento de muestras, latencia de inferencia) en el archivo de registro de NeuroSkill™. Útil para medir la latencia y diagnosticar problemas.

## Pruébelo aquí
Utilice el siguiente widget para probar el motor TTS directamente desde esta ventana de ayuda.
