// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Spanish "tts" namespace — reference translation. */
const tts: Record<string, string> = {
  "ttsTab.backendSection": "Motor de voz",
  "ttsTab.backendKitten": "gatitotts",
  "ttsTab.backendKittenTag": "ONNX · Inglés · ~30 MB",
  "ttsTab.backendKittenDesc": "Modelo ONNX compacto, rápido en cualquier CPU, solo en inglés.",
  "ttsTab.backendNeutts": "NeuTTS",
  "ttsTab.backendNeuttsTag": "GGUF · Clonación de voz · Multilingüe",
  "ttsTab.backendNeuttsDesc":
    "GGUF LLM backbone with NeuCodec decoder. Clone any voice; supports English, German, French, Spanish.",
  "ttsTab.statusSection": "Estado del motor",
  "ttsTab.statusReady": "Listo",
  "ttsTab.statusLoading": "Cargando…",
  "ttsTab.statusIdle": "Inactivo",
  "ttsTab.statusUnloaded": "descargado",
  "ttsTab.statusError": "Fallido",
  "ttsTab.preloadButton": "Precarga",
  "ttsTab.retryButton": "Reintentar",
  "ttsTab.unloadButton": "Descargar",
  "ttsTab.errorTitle": "error de carga",
  "ttsTab.preloadOnStartup": "Precargar el motor al arrancar",
  "ttsTab.preloadOnStartupDesc": "Calienta el motor activo en segundo plano cuando se inicia la aplicación",
  "ttsTab.requirements": "Requiere hablar en PATH",
  "ttsTab.requirementsDesc": "macOS: brew install espeak-ng · Ubuntu: apt install espeak-ng",
  "ttsTab.kittenConfigSection": "Configuración de KittyTTS",
  "ttsTab.kittenVoiceLabel": "Voz",
  "ttsTab.kittenModelInfo": "KittenML/kitten-tts-mini-0.8 · 24 kHz · ~30 MB",
  "ttsTab.neuttsConfigSection": "Configuración de NeuTTS",
  "ttsTab.neuttsModelLabel": "modelo de columna vertebral",
  "ttsTab.neuttsModelDesc":
    "GGUF más pequeño = más rápido; más grande = más natural. Se recomienda Q4 para la mayoría de los sistemas.",
  "ttsTab.neuttsVoiceSection": "Voz de referencia",
  "ttsTab.neuttsVoiceDesc": "Elija una voz preestablecida o proporcione su propio clip WAV para clonar voz.",
  "ttsTab.neuttsPresetLabel": "Voces preestablecidas",
  "ttsTab.neuttsCustomOption": "WAV personalizado...",
  "ttsTab.neuttsRefWavLabel": "Referencia WAV",
  "ttsTab.neuttsRefWavNone": "Ningún archivo seleccionado",
  "ttsTab.neuttsRefWavBrowse": "Navegar…",
  "ttsTab.neuttsRefTextLabel": "Transcripción",
  "ttsTab.neuttsRefTextPlaceholder": "Escribe exactamente lo que se dice en el clip WAV.",
  "ttsTab.neuttsSaveButton": "Guardar",
  "ttsTab.neuttsSaved": "Guardado",
  "ttsTab.voiceJo": "Jo",
  "ttsTab.voiceDave": "dave",
  "ttsTab.voiceGreta": "greta",
  "ttsTab.voiceJuliette": "julieta",
  "ttsTab.voiceMateo": "mateo",
  "ttsTab.voiceCustom": "Costumbre…",
  "ttsTab.testSection": "Prueba de voz",
  "ttsTab.testDesc": "Escriba cualquier texto y presione Hablar para escuchar el motor activo.",
  "ttsTab.startupSection": "Puesta en marcha",
  "ttsTab.loggingSection": "Registro de depuración",
  "ttsTab.loggingLabel": "Registro de síntesis TTS",
  "ttsTab.loggingDesc":
    "Escriba eventos de síntesis (texto, recuento de muestras, latencia) en el archivo de registro.",
  "ttsTab.apiSection": "API",
  "ttsTab.apiDesc": "Active la voz desde cualquier script o herramienta a través de WebSocket o HTTP API:",
  "ttsTab.apiExampleWs": 'WebSocket: {"command":"say","text":"Eyes closed."}',
  "ttsTab.apiExampleHttp": 'HTTP (curl): POST /decir cuerpo: {"text":"Eyes closed."}',

  "helpTts.overviewTitle": "Guía de voz (TTS) en el dispositivo",
  "helpTts.overviewBody":
    "NeuroSkill™ incluye un motor de conversión de texto a voz en inglés completamente integrado en el dispositivo. Anuncia las fases de calibración en voz alta (etiquetas de acción, pausas, finalización) y se puede activar de forma remota desde cualquier script a través de WebSocket o HTTP API. Toda la síntesis se ejecuta localmente: no se necesita Internet después de descargar una vez el modelo de ~30 MB.",
  "helpTts.howItWorksTitle": "Cómo funciona",
  "helpTts.howItWorksBody":
    "Preprocesamiento de texto → fragmentación de oraciones (≤400 caracteres) → fonemización a través de libespeak-ng (biblioteca C, en proceso, voz en-us) → tokenización (IPA → ID de enteros) → inferencia ONNX (modelo KittenTTS: input_ids + estilo + velocidad → forma de onda f32) → 1 s de silencio → rodio se reproduce en la salida de audio predeterminada del sistema.",
  "helpTts.modelTitle": "Modelo",
  "helpTts.modelBody":
    "KittenML/kitten-tts-mini-0.8 de HuggingFace Hub. Voz: Jasper (inglés en-us). Frecuencia de muestreo: 24 000 Hz mono float32. INT8 ONNX cuantificado: solo CPU, no se requiere GPU. Almacenado en caché en ~/.cache/huggingface/hub/ después de la primera descarga.",
  "helpTts.requirementsTitle": "Requisitos",
  "helpTts.requirementsBody":
    "espeak-ng debe estar instalado y en PATH: proporciona fonemización IPA en proceso (vinculada como una biblioteca C, no generada como un subproceso). macOS: instalación de cerveza espeak-ng. Ubuntu/Debian: apto para instalar libespeak-ng-dev. Alpine: apk agrega espeak-ng-dev. Fedora: dnf instala espeak-ng-devel.",
  "helpTts.calibrationTitle": "Integración de calibración",
  "helpTts.calibrationBody":
    'Cuando comienza una sesión de calibración, el motor se precalienta en segundo plano (descargando el modelo si es necesario). En cada fase, la ventana de calibración llama a tts_speak con la etiqueta de acción, anuncio de pausa, mensaje de finalización o aviso de cancelación. El habla nunca bloquea la calibración: todas las llamadas TTS son de tipo "dispara y olvida".',
  "helpTts.apiTitle": "API: diga el comando",
  "helpTts.apiBody":
    'Active la voz desde cualquier script externo, herramienta de automatización o agente LLM. El comando regresa inmediatamente mientras se reproduce el audio. WebSocket: {"command":"say","text":"your message"}. HTTP: POST /say con cuerpo {"text":"your message"}. CLI (curl): curl -X POST http://localhost:<port>/say -d \\\'{"text":"hello"}\\\' -H \\\'Tipo de contenido: aplicación/json\\\'.',
  "helpTts.loggingTitle": "Registro de depuración",
  "helpTts.loggingBody":
    "Habilite el registro de síntesis TTS en Configuración → Voz para escribir eventos (texto hablado, recuento de muestras, latencia de inferencia) en el archivo de registro de NeuroSkill™. Útil para medir la latencia y diagnosticar problemas.",
  "helpTts.testTitle": "Pruébelo aquí",
  "helpTts.testBody": "Utilice el siguiente widget para probar el motor TTS directamente desde esta ventana de ayuda.",
};

export default tts;
