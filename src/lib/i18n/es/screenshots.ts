// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Spanish "screenshots" namespace — reference translation. */
const screenshots: Record<string, string> = {
  "screenshots.title": "Captura de pantalla",
  "screenshots.enableToggle": "Habilitar captura de pantalla",
  "screenshots.enableDesc":
    "Capture la ventana activa periódicamente e incrústela con un modelo de visión para la búsqueda de similitud visual.",
  "screenshots.sessionOnlyToggle": "Solo sesión",
  "screenshots.sessionOnlyDesc": "Capture únicamente durante las sesiones de grabación de EEG activas.",
  "screenshots.interval": "Intervalo de captura",
  "screenshots.intervalDesc":
    "Alineado con épocas de inclusión de EEG (5 s cada una). 1× = cada época, 2× = cada dos épocas, hasta 12× (60 s).",
  "screenshots.intervalUnit": "s",
  "screenshots.intervalEpoch": "época",
  "screenshots.imageSize": "Tamaño de imagen",
  "screenshots.imageSizeDesc":
    "Resolución intermedia (px). La ventana capturada cambia de tamaño para ajustarse a este cuadrado antes de guardarla e incrustarla.",
  "screenshots.imageSizeUnit": "píxeles",
  "screenshots.imageSizeRecommended": "Recomendado para el modelo actual:",
  "screenshots.quality": "Calidad WebP",
  "screenshots.qualityDesc": "Calidad de compresión WebP (0–100). Inferior = archivos más pequeños.",
  "screenshots.embeddingModel": "Modelo de incrustación",
  "screenshots.embeddingModelDesc":
    "Modelo de visión utilizado para generar incrustaciones de imágenes para búsqueda de similitudes.",
  "screenshots.backendFastembed": "fastembed (ONNX local)",
  "screenshots.backendMmproj": "mmproj (proyector de visión LLM)",
  "screenshots.backendLlmVlm": "LLM VLM (incrustar + OCR mediante modelo de visión)",
  "screenshots.modelClip": "CLIP ViT-B/32 — 512d (rápido, predeterminado)",
  "screenshots.modelNomic": "Visión integrada Nomic v1.5 — 768d",
  "screenshots.reembed": "Volver a insertar capturas de pantalla",
  "screenshots.reembedDesc":
    "Vuelva a calcular las incrustaciones de todas las capturas de pantalla existentes utilizando el modelo actual.",
  "screenshots.reembedBtn": "Volver a incrustar y reindexar",
  "screenshots.reembedNowBtn": "Volver a insertar ahora",
  "screenshots.reembedding": "Incrustando…",
  "screenshots.stale": "duro",
  "screenshots.unembedded": "desintegrado",
  "screenshots.estimate": "Tiempo estimado:",
  "screenshots.modelChanged": "Modelo de incrustación cambiado",
  "screenshots.modelChangedDesc":
    "Las capturas de pantalla se incorporaron con un modelo diferente. Vuelva a insertarlo para obtener resultados de búsqueda consistentes.",
  "screenshots.privacyNote":
    "Todas las capturas de pantalla se almacenan únicamente localmente y nunca se transmiten. Opción de participación, sesión cerrada de forma predeterminada.",
  "screenshots.storagePath": "Almacenamiento: ~/.skill/screenshots/",
  "screenshots.permissionRequired": "Se requiere permiso de grabación de pantalla",
  "screenshots.permissionDesc":
    "macOS requiere permiso de Grabación de audio de pantalla y sistema para capturar otras ventanas de aplicaciones. Sin él, las capturas de pantalla pueden estar en blanco o mostrar solo su propia aplicación.",
  "screenshots.permissionGranted": "Permiso de grabación de pantalla concedido.",
  "screenshots.openPermissionSettings": "Abrir configuración de grabación de pantalla",
  "screenshots.ocrToggle": "Extracción de texto OCR",
  "screenshots.ocrToggleDesc":
    "Extraiga texto de capturas de pantalla para búsquedas basadas en texto. Se ejecuta en imágenes de resolución completa antes de reducir el tamaño.",
  "screenshots.gpuToggle": "Aceleración de GPU",
  "screenshots.gpuToggleDesc":
    "Utilice GPU para incrustaciones de imágenes y OCR. Desactívelo para forzar la inferencia de la CPU (libera la GPU para LLM/EEG).",
  "screenshots.ocrEngineSelect": "Motor OCR",
  "screenshots.ocrEngineAppleVision": "Apple Vision: GPU/motor neuronal (recomendado en macOS)",
  "screenshots.ocrEngineOcrs": "ocrs: CPU local basada en rten (multiplataforma)",
  "screenshots.ocrAppleVisionHint": "⚡ Apple Vision se ejecuta en GPU/ANE y es ~10 veces más rápido que ocrs en macOS",
  "screenshots.ocrActiveModels": "Modelos activos",
  "screenshots.ocrInference": "Inferencia",
  "screenshots.ocrTitle": "Extracción de texto OCR",
  "screenshots.ocrEngine": "OCR en el dispositivo",
  "screenshots.ocrDesc":
    "El texto se extrae de cada captura de pantalla en resolución completa antes de reducirlo utilizando el motor ocrs. El texto extraído está incrustado con BGE-Small-EN-v1.5 y indexado en un índice HNSW separado para búsqueda de texto semántico. Los modelos OCR (~10 MB cada uno) se descargan automáticamente la primera vez que se utilizan.",
  "screenshots.ocrDetModel": "Modelo de detección",
  "screenshots.ocrRecModel": "Modelo de reconocimiento",
  "screenshots.ocrTextEmbed": "Incrustación de texto",
  "screenshots.ocrIndex": "Índice de texto",
  "screenshots.ocrSearchHint":
    "Utilice la ventana de búsqueda → pestaña Imágenes para buscar el texto de la captura de pantalla.",
  "screenshots.ocrSearchTitle": "Buscar por texto en pantalla",
  "screenshots.ocrSearchPlaceholder": "Buscar texto visible en capturas de pantalla...",
  "screenshots.ocrSearchBtn": "Buscar",
  "screenshots.ocrModeSubstring": "Coincidencia de texto",
  "screenshots.ocrModeSemantic": "Semántico",
  "screenshots.ocrNoResults": "No se encontraron capturas de pantalla coincidentes.",
  "screenshots.perfTitle": "Rendimiento del oleoducto",
  "screenshots.perfCapture": "Capturar hilo",
  "screenshots.perfEmbed": "Insertar hilo",
  "screenshots.perfTotal": "total",
  "screenshots.perfWindowCapture": "captura de ventana",
  "screenshots.perfOcr": "extracción de OCR",
  "screenshots.perfResize": "Cambiar tamaño + pad",
  "screenshots.perfSave": "Guardar + SQLite",
  "screenshots.perfIterTotal": "Total de iteraciones",
  "screenshots.perfVisionEmbed": "Incorporación de visión",
  "screenshots.perfTextEmbed": "Incrustación de texto",
  "screenshots.perfQueue": "Profundidad de la cola",
  "screenshots.perfDrops": "Abandonó",
  "screenshots.perfBackoff": "Retroceder",
  "screenshots.perfDropsHint":
    "el hilo de inserción es demasiado lento: el intervalo aumenta automáticamente, se recuperará cuando la cola se agote",
  "screenshots.perfErrors": "errores",
  "screenshots.stats": "Estadística",
  "screenshots.totalCount": "Total de capturas de pantalla",
  "screenshots.embeddedCount": "Incorporado",
  "screenshots.unembeddedCount": "Aún no incorporado",
  "screenshots.staleCount": "obsoleto (modelo diferente)",
};

export default screenshots;
