// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Spanish "perm" namespace — reference translation. */
const perm: Record<string, string> = {
  "perm.intro":
    "{app} utiliza una pequeña cantidad de permisos opcionales del sistema operativo para habilitar funciones como notificaciones y marcas de tiempo de actividad del teclado/ratón. Todos los datos permanecen en su dispositivo.",
  "perm.granted": "Otorgada",
  "perm.denied": "No concedido",
  "perm.unknown": "Desconocido",
  "perm.notRequired": "No requerido",
  "perm.systemManaged": "Gestionado por el sistema operativo",
  "perm.accessibility": "Accesibilidad",
  "perm.accessibilityDesc":
    "El seguimiento de la actividad del teclado y el mouse utiliza un CGEventTap (macOS) para registrar marcas de tiempo de la última pulsación de tecla y evento del mouse. No se almacenan pulsaciones de teclas ni posiciones del cursor, solo marcas de tiempo de segundos Unix. Esto requiere permiso de Accesibilidad en macOS.",
  "perm.accessibilityOk":
    "Permiso concedido. Se están registrando marcas de tiempo de actividad del teclado y el mouse.",
  "perm.accessibilityPending": "Comprobando el estado del permiso...",
  "perm.howToGrant": "Cómo otorgar este permiso:",
  "perm.accessStep1": 'Haga clic en "Abrir configuración de accesibilidad" a continuación.',
  "perm.accessStep2": "Busque {app} en la lista (o haga clic en el botón + para agregarlo).",
  "perm.accessStep3": "Actívalo.",
  "perm.accessStep4": "Regrese aquí: el estado se actualizará automáticamente.",
  "perm.openAccessibilitySettings": "Abrir configuración de accesibilidad",
  "perm.bluetooth": "bluetooth",
  "perm.bluetoothDesc":
    "Bluetooth se usa para conectar tus auriculares BCI (Muse, MW75 Neuro, OpenBCI Ganglion, IDUN Guardian, etc.). En macOS, el sistema solicitará acceso a Bluetooth la primera vez que la app escanee. En Linux y Windows no se necesita un permiso aparte.",
  "perm.openBluetoothSettings": "Abra la configuración de Bluetooth",
  "perm.notifications": "Notificaciones",
  "perm.notificationsDesc":
    "Las notificaciones se usan para avisarte cuando alcanzas tu objetivo diario de grabación y cuando hay una actualización de software disponible. En macOS y Windows, el sistema solicitará permiso la primera vez que se envíe una notificación.",
  "perm.openNotificationsSettings": "Abrir configuración de notificaciones",
  "perm.matrix": "Resumen de permisos",
  "perm.feature": "Característica",
  "perm.matrixBluetooth": "Bluetooth (dispositivo BCI)",
  "perm.matrixKeyboardMouse": "Marcas de tiempo del teclado y el mouse",
  "perm.matrixActiveWindow": "Seguimiento de ventana activa",
  "perm.matrixNotifications": "Notificaciones",
  "perm.matrixNone": "No se necesita permiso",
  "perm.matrixAccessibility": "Accesibilidad requerida",
  "perm.matrixOsPrompt": "Avisos del sistema operativo en el primer uso",
  "perm.legendNone": "No se necesita permiso",
  "perm.legendRequired": "Se requiere permiso del sistema operativo: se degrada silenciosamente si no está presente",
  "perm.legendPrompt": "Avisos del sistema operativo en el primer uso",
  "perm.why": "¿Por qué {app} los necesita?",
  "perm.whyBluetooth": "bluetooth",
  "perm.whyBluetoothDesc": "Para descubrir y transmitir datos desde sus auriculares BCI a través de BLE.",
  "perm.whyAccessibility": "Accesibilidad",
  "perm.whyAccessibilityDesc":
    "Para marcar la hora de los eventos del teclado y el mouse para el contexto de la actividad. Sólo se almacena la hora del evento, nunca lo que se escribió ni dónde estaba el cursor.",
  "perm.whyNotifications": "Notificaciones",
  "perm.whyNotificationsDesc":
    "Para notificarle cuando alcance su objetivo de grabación diario y cuando las actualizaciones estén listas.",
  "perm.privacyNote":
    "Todos los datos se almacenan localmente en su dispositivo y nunca se transmiten a ningún servidor. Puede desactivar cualquier función en Configuración → Seguimiento de actividad.",
  "perm.screenRecording": "Grabación de pantalla",
  "perm.screenRecordingDesc":
    "Necesario para capturar otras ventanas de aplicaciones para el sistema de incrustación de capturas de pantalla. macOS redacta el contenido de la ventana sin este permiso.",
  "perm.screenRecordingOk":
    "Se concede permiso de grabación de pantalla. La captura de pantalla funcionará correctamente.",
  "perm.screenRecordingStep1":
    "Abra Configuración del sistema → Privacidad y seguridad → Grabación de audio de pantalla y sistema",
  "perm.screenRecordingStep2": "Busque NeuroSkill™ en la lista y habilítelo",
  "perm.screenRecordingStep3":
    "Es posible que tengas que cerrar y reiniciar la aplicación para que el cambio surta efecto.",
  "perm.openScreenRecordingSettings": "Abrir configuración de grabación de pantalla",
  "perm.whyScreenRecording": "Grabación de pantalla",
  "perm.whyScreenRecordingDesc":
    "Capturar la ventana activa para la búsqueda de similitud visual y la correlación EEG intermodal. Solo se almacenan capturas de pantalla voluntarias, nunca grabaciones continuas.",
  "perm.matrixScreenRecording": "Captura de pantalla",
  "perm.matrixScreenRecordingReq": "Se requiere grabación de pantalla",
};

export default perm;
