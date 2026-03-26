// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Spanish "onboarding" namespace — reference translation. */
const onboarding: Record<string, string> = {
  "onboarding.title": "Bienvenido a {app}",
  "onboarding.step.welcome": "Bienvenido",
  "onboarding.step.bluetooth": "bluetooth",
  "onboarding.step.fit": "Comprobación de ajuste",
  "onboarding.step.calibration": "Calibración",
  "onboarding.step.models": "Modelos",
  "onboarding.step.tray": "Bandeja",
  "onboarding.step.done": "Hecho",
  "onboarding.welcomeTitle": "Bienvenido a {app}",
  "onboarding.welcomeBody":
    "{app} registra, analiza e indexa sus datos de EEG desde cualquier dispositivo BCI compatible. Vamos a configurarlo en unos pocos pasos rápidos.",
  "onboarding.bluetoothHint": "Conecte su dispositivo BCI",
  "onboarding.fitHint": "Comprobar la calidad del contacto del sensor",
  "onboarding.calibrationHint": "Ejecute una sesión de calibración rápida",
  "onboarding.modelsHint": "Descargue los modelos de IA locales recomendados",
  "onboarding.bluetoothTitle": "Conecte su dispositivo BCI",
  "onboarding.bluetoothBody":
    "Encienda su dispositivo BCI y úselo. {app} buscará dispositivos cercanos y se conectará automáticamente.",
  "onboarding.btConnected": "Conectado a {name}",
  "onboarding.btScanning": "Exploración…",
  "onboarding.btReady": "Listo para escanear",
  "onboarding.btScan": "Escanear",
  "onboarding.btInstructions": "Cómo conectarse",
  "onboarding.btStep1":
    "Encienda su dispositivo BCI (mantenga presionado el botón de encendido, active el interruptor o presione el botón según su auricular).",
  "onboarding.btStep2":
    "Colóquese los auriculares en la cabeza; los sensores deben descansar detrás de las orejas y en la frente.",
  "onboarding.btStep3":
    "Haga clic en Escanear arriba. {app} buscará y se conectará automáticamente al dispositivo BCI más cercano.",
  "onboarding.btSuccess": "¡Auriculares conectados! Puedes continuar.",
  "onboarding.fitTitle": "Comprobar el ajuste de los auriculares",
  "onboarding.fitBody":
    "Un buen contacto del sensor es esencial para obtener datos EEG limpios. Los cuatro sensores deberían mostrarse de color verde o amarillo.",
  "onboarding.sensorQuality": "Calidad del sensor en vivo",
  "onboarding.quality.good": "Bien",
  "onboarding.quality.fair": "Justo",
  "onboarding.quality.poor": "Pobre",
  "onboarding.quality.no_signal": "Sin señal",
  "onboarding.fitNeedsBt": "Primero conecte sus auriculares para ver los datos del sensor en vivo.",
  "onboarding.fitTips": "Consejos para un mejor contacto",
  "onboarding.fitTip1":
    "Sensores de oído (TP9/TP10): colóquelos detrás y ligeramente por encima de las orejas. Quite el pelo que cubra los sensores.",
  "onboarding.fitTip2":
    "Sensores de frente (AF7/AF8): deben quedar planos sobre la piel limpia; límpielos con un paño seco si es necesario.",
  "onboarding.fitTip3":
    "Si el contacto es deficiente, humedezca ligeramente los sensores con un dedo húmedo. Esto mejora la conductividad.",
  "onboarding.fitGood": "¡Gran ajuste! Todos los sensores tienen buen contacto.",
  "onboarding.calibrationTitle": "Ejecutar calibración",
  "onboarding.calibrationBody":
    "Registros de calibración etiquetados como EEG mientras alterna entre dos estados mentales. Esto ayuda a {app} a aprender los patrones básicos de su cerebro.",
  "onboarding.openCalibration": "Abrir calibración",
  "onboarding.calibrationNeedsBt": "Conecte sus auriculares primero para ejecutar la calibración.",
  "onboarding.calibrationSkip":
    "Puede omitir esto y calibrar más tarde desde el menú de la bandeja o la configuración.",
  "onboarding.modelsTitle": "Descargar Modelos Recomendados",
  "onboarding.modelsBody":
    "Para obtener la mejor experiencia local, descargue estos valores predeterminados ahora: Qwen3.5 4B (Q4_K_M), codificador ZUNA, NeuTTS y Kitten TTS.",
  "onboarding.models.downloadAll": "Descargar conjunto recomendado",
  "onboarding.models.download": "Descargar",
  "onboarding.models.downloading": "Descargando…",
  "onboarding.models.downloaded": "Descargado",
  "onboarding.models.qwenTitle": "Qwen3.5 4B (Q4_K_M)",
  "onboarding.models.qwenDesc":
    "Recommended chat model. Uses Q4_K_M for the best quality/speed balance on most laptops.",
  "onboarding.models.zunaTitle": "Codificador ZUNA EEG",
  "onboarding.models.zunaDesc":
    "Necesario para incrustaciones de EEG, historial semántico y análisis posteriores del estado cerebral.",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc": "Motor de voz multilingüe recomendado con mejor calidad y soporte de clonación.",
  "onboarding.models.kittenTitle": "Gatito TTS",
  "onboarding.models.kittenDesc":
    "Lightweight fast voice backend, useful as a quick fallback and for low-resource systems.",
  "onboarding.models.ocrTitle": "Modelos de OCR",
  "onboarding.models.ocrDesc":
    "Text detection + recognition models for extracting text from screenshots. Enables text search across captured screens (~10 MB each).",
  "onboarding.screenRecTitle": "Permiso de grabación de pantalla",
  "onboarding.screenRecDesc":
    "Requerido en macOS para capturar ventanas de otras aplicaciones para el sistema de captura de pantalla. Sin él, las capturas de pantalla pueden aparecer en blanco.",
  "onboarding.screenRecOpen": "Abrir configuración",
  "onboarding.trayTitle": "Encuentra la aplicación en tu bandeja",
  "onboarding.trayBody":
    "{app} se ejecuta silenciosamente en segundo plano. Después de la configuración, el ícono en la barra de menú (macOS) o en la bandeja del sistema (Windows/Linux) es su punto de entrada a la aplicación.",
  "onboarding.tray.states": "El icono cambia de color para mostrar el estado:",
  "onboarding.tray.grey": "Gris: desconectado",
  "onboarding.tray.amber": "Ámbar: escaneando o conectando",
  "onboarding.tray.green": "Verde: conectado y grabando",
  "onboarding.tray.red": "Rojo: Bluetooth está desactivado",
  "onboarding.tray.open":
    "Haga clic en el icono de la bandeja en cualquier momento para mostrar u ocultar el panel principal.",
  "onboarding.tray.menu":
    "Haga clic derecho en el icono (o haga clic izquierdo en Windows/Linux) para realizar acciones rápidas: conectar, etiquetar, calibrar y más.",
  "onboarding.downloadsComplete": "¡Todas las descargas completas!",
  "onboarding.downloadsCompleteBody":
    "Los modelos recomendados están descargados y listos para usar. Para descargar más modelos o cambiar a otros diferentes, abra",
  "onboarding.downloadMoreSettings": "configuración de la aplicación",
  "onboarding.doneTitle": "¡Ya estás listo!",
  "onboarding.doneBody": "{app} se está ejecutando en su barra de menú. Aquí hay algunas cosas que debe saber:",
  "onboarding.doneTip.tray":
    "{app} vive en la bandeja de la barra de menú. Haga clic en el icono para mostrar/ocultar el panel.",
  "onboarding.doneTip.shortcuts":
    "Utilice ⌘K para abrir la paleta de comandos o ? para ver todos los atajos de teclado.",
  "onboarding.doneTip.help":
    "Abra la Ayuda en el menú de la bandeja para obtener una referencia completa de cada función.",
  "onboarding.back": "Atrás",
  "onboarding.next": "Próximo",
  "onboarding.getStarted": "Empezar",
  "onboarding.finish": "Finalizar",
};

export default onboarding;
