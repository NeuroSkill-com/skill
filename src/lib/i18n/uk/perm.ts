// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** UK "perm" namespace translations. */
const perm: Record<string, string> = {
  "perm.intro":
    "{app} використовує кілька необов'язкових системних дозволів для таких функцій, як часові мітки активності клавіатури/миші та сповіщення. Усі дані залишаються на вашому пристрої.",
  "perm.granted": "Надано",
  "perm.denied": "Не надано",
  "perm.unknown": "Невідомо",
  "perm.notRequired": "Не потрібно",
  "perm.systemManaged": "Керується ОС",
  "perm.accessibility": "Спеціальні можливості",
  "perm.accessibilityDesc":
    "Відстеження активності клавіатури та миші використовує CGEventTap (macOS) для запису часових міток останнього натискання клавіші та руху миші. Жодних натискань клавіш чи положень курсору не зберігається — лише Unix-секундні мітки. На macOS для цього потрібен дозвіл «Спеціальні можливості».",
  "perm.accessibilityOk": "Дозвіл надано. Часові мітки активності клавіатури та миші записуються.",
  "perm.accessibilityPending": "Перевірка статусу дозволу…",
  "perm.howToGrant": "Як надати цей дозвіл:",
  "perm.accessStep1": "Натисніть «Відкрити налаштування доступності» нижче.",
  "perm.accessStep2": "Знайдіть {app} у списку (або натисніть +, щоб додати).",
  "perm.accessStep3": "Увімкніть перемикач.",
  "perm.accessStep4": "Поверніться сюди — статус оновиться автоматично.",
  "perm.openAccessibilitySettings": "Відкрити налаштування доступності",
  "perm.bluetooth": "Bluetooth",
  "perm.bluetoothDesc":
    "Bluetooth використовується для підключення до BCI-гарнітури (Muse, MW75 Neuro, OpenBCI Ganglion, IDUN Guardian тощо). На macOS система показує одноразовий запит дозволу під час першого сканування. На Linux і Windows спеціальних дозволів не потрібно.",
  "perm.openBluetoothSettings": "Відкрити налаштування Bluetooth",
  "perm.notifications": "Сповіщення",
  "perm.notificationsDesc":
    "Сповіщення надсилаються, коли ви досягаєте щоденної мети запису і коли доступне оновлення програмного забезпечення. На macOS і Windows ОС запитує дозвіл під час першого надсилання сповіщення.",
  "perm.openNotificationsSettings": "Відкрити налаштування сповіщень",
  "perm.matrix": "Зведення дозволів",
  "perm.feature": "Функція",
  "perm.matrixBluetooth": "Bluetooth (пристрій BCI)",
  "perm.matrixKeyboardMouse": "Часові мітки клавіатури та миші",
  "perm.matrixActiveWindow": "Відстеження активного вікна",
  "perm.matrixNotifications": "Сповіщення",
  "perm.matrixNone": "Дозвіл не потрібен",
  "perm.matrixAccessibility": "Потрібні спеціальні можливості",
  "perm.matrixOsPrompt": "ОС запитає при першому використанні",
  "perm.legendNone": "Дозвіл не потрібен",
  "perm.legendRequired": "Потрібен дозвіл ОС — без нього функція вимикається мовчки",
  "perm.legendPrompt": "ОС запитає при першому виклику",
  "perm.why": "Навіщо {app} потрібні ці дозволи?",
  "perm.whyBluetooth": "Bluetooth",
  "perm.whyBluetoothDesc": "Для виявлення BCI-гарнітури та потокової передачі даних через BLE.",
  "perm.whyAccessibility": "Спеціальні можливості",
  "perm.whyAccessibilityDesc":
    "Для запису часових міток подій клавіатури та миші з метою контекстуалізації активності. Зберігається лише час події — ніколи те, що було введено чи де був курсор.",
  "perm.whyNotifications": "Сповіщення",
  "perm.whyNotificationsDesc": "Щоб сповіщати про досягнення щоденної мети запису та доступність оновлень.",
  "perm.privacyNote":
    "Усі дані зберігаються локально на вашому пристрої і ніколи не передаються на жодний сервер. Ви можете вимкнути будь-яку функцію в Налаштуваннях → Відстеження активності.",
  "perm.screenRecording": "Запис екрана",
  "perm.screenRecordingDesc":
    "Потрібно для захоплення вікон інших програм для системи вбудовування знімків екрана. macOS приховує вміст вікон без цього дозволу.",
  "perm.screenRecordingOk": "Дозвіл на запис екрана надано. Захоплення знімків екрана працюватиме коректно.",
  "perm.screenRecordingStep1":
    "Відкрийте Системні налаштування → Конфіденційність і безпека → Запис екрана та системного аудіо",
  "perm.screenRecordingStep2": "Знайдіть NeuroSkill™ у списку та увімкніть",
  "perm.screenRecordingStep3": "Можливо, потрібно буде вийти і перезапустити програму, щоб зміни набули чинності",
  "perm.openScreenRecordingSettings": "Відкрити налаштування запису екрана",
  "perm.whyScreenRecording": "Запис екрана",
  "perm.whyScreenRecordingDesc":
    "Для захоплення активного вікна для пошуку візуальної подібності та крос-модальної кореляції EEG. Зберігаються лише обрані знімки — ніколи безперервний запис.",
  "perm.matrixScreenRecording": "Захоплення знімка екрана",
  "perm.matrixScreenRecordingReq": "Потрібен запис екрана",
  "perm.calendar": "Календар",
  "perm.calendarDesc":
    "Інструменти календаря можуть читати події для надання контексту планування. Дозвіл запитується macOS за потреби.",
  "perm.requestCalendarPermission": "Запросити дозвіл на календар",
  "perm.openCalendarSettings": "Відкрити налаштування конфіденційності календаря",
  "perm.location": "Служби місцезнаходження",
  "perm.locationDesc":
    "На macOS служби місцезнаходження використовують CoreLocation (GPS/Wi-Fi/стільниковий) для точного позиціонування. На Linux та Windows застосунок використовує IP-геолокацію без потреби дозволу. Якщо служби місцезнаходження відхилено, застосунок автоматично переходить на IP-геолокацію.",
  "perm.locationOk": "Дозвіл на місцезнаходження надано. CoreLocation буде використовуватися для точного місцезнаходження.",
  "perm.locationFallback": "Місцезнаходження не авторизовано — застосунок використовуватиме IP-геолокацію (точність на рівні міста).",
  "perm.locationStep1": "Відкрийте Налаштування системи → Конфіденційність та безпека → Служби місцезнаходження",
  "perm.locationStep2": "Знайдіть {app} у списку та увімкніть",
  "perm.locationStep3": "Поверніться сюди — статус оновиться автоматично",
  "perm.requestLocationPermission": "Запитати дозвіл на місцезнаходження",
  "perm.openLocationSettings": "Відкрити налаштування місцезнаходження",
  "perm.whyLocation": "Місцезнаходження",
  "perm.whyLocationDesc":
    "Для надання точного контексту місцезнаходження для LLM та зберігання даних GPS поряд з даними здоров’я. Переходить на IP-геолокацію у разі відмови.",
  "perm.matrixLocation": "Місцезнаходження (GPS / IP)",
  "perm.matrixLocationReq": "Служби місцезнаходження (опціонально — резерв IP)",
  "perm.openInputMonitoringSettings": "Відкрити налаштування моніторингу введення",
  "perm.openFocusSettings": "Відкрити налаштування фокусування",
  "perm.whyCalendar": "Календар",
  "perm.whyCalendarDesc":
    "Щоб надати контекст планування інструментам ШІ, щоб асистент міг посилатися на ваші майбутні події.",
  "perm.matrixCalendar": "Календарні події",
  "perm.matrixCalendarReq": "Потрібен доступ до календаря",
};

export default perm;
