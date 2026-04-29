// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** UK "onboarding" namespace translations. */
const onboarding: Record<string, string> = {
  "onboarding.title": "Ласкаво просимо до {app}",
  "onboarding.step.welcome": "Ласкаво просимо",
  "onboarding.step.bluetooth": "Bluetooth",
  "onboarding.step.fit": "Перевірка посадки",
  "onboarding.step.calibration": "Калібрування",
  "onboarding.step.models": "Моделі",
  "onboarding.step.tray": "Трей",
  "onboarding.step.permissions": "Дозволи",
  "onboarding.step.extensions": "Розширення",
  "onboarding.step.enable_bluetooth": "Увімкнути Bluetooth",
  "onboarding.step.done": "Готово",
  "onboarding.newBadge": "Нове",
  "onboarding.fontSizeLabel": "Розмір тексту",
  "onboarding.fontSizeDecrease": "Зменшити розмір тексту",
  "onboarding.fontSizeIncrease": "Збільшити розмір тексту",
  "onboarding.welcomeBackTitle": "З поверненням до {app}",
  "onboarding.whatsNewTitle": "Що нового з часу останнього налаштування",
  "onboarding.whatsNewBody":
    "Ми додали кілька нових кроків з моменту, коли ви востаннє запускали цей майстер. Ваше існуюче налаштування (Bluetooth, калібрування, моделі) залишилося незмінним — можете швидко переглянути. Нові кроки позначені тут і відмічені тегом «Нове» на смузі прогресу:",
  "onboarding.trayHint": "Знайдіть значок програми у рядку меню / треї",
  "onboarding.permissionsHint": "Необов'язково: дозволити захоплення активної програми, файлів, буфера обміну",
  "onboarding.extensionsHint": "Необов'язково: встановити помічники для VS Code, браузера та shell",
  "onboarding.welcomeTitle": "Ласкаво просимо до {app}",
  "onboarding.welcomeBody":
    "{app} записує, аналізує та індексує дані ЕЕГ з будь-якого підтримуваного BCI-пристрою. Давайте налаштуємо все за кілька кроків.",
  "onboarding.bluetoothHint": "Підключити пристрій BCI",
  "onboarding.fitHint": "Перевірити якість контакту датчиків",
  "onboarding.calibrationHint": "Провести швидке калібрування",
  "onboarding.modelsHint": "Завантажити рекомендовані локальні моделі ШІ",
  "onboarding.bluetoothTitle": "Підключіть ваш пристрій BCI",
  "onboarding.bluetoothBody":
    "Увімкніть пристрій BCI та надіньте його. {app} автоматично знайде пристрої поблизу та підключиться.",
  "onboarding.btConnected": "Підключено до {name}",
  "onboarding.btScanning": "Пошук…",
  "onboarding.btReady": "Готовий до пошуку",
  "onboarding.btScan": "Сканувати",
  "onboarding.btInstructions": "Як підключитися",
  "onboarding.btStep1":
    "Увімкніть пристрій BCI (утримуйте кнопку, переведіть перемикач або натисніть кнопку залежно від гарнітури).",
  "onboarding.btStep2": "Надіньте пристрій — датчики мають знаходитися за вухами та на лобі.",
  "onboarding.btStep3": "Натисніть Сканувати вище. {app} знайде та підключиться до найближчого пристрою автоматично.",
  "onboarding.btSuccess": "Пристрій підключено! Можна продовжувати.",
  "onboarding.fitTitle": "Перевірка посадки пристрою",
  "onboarding.fitBody":
    "Хороший контакт датчиків необхідний для чистих даних ЕЕГ. Усі чотири датчики мають показувати зелений або жовтий.",
  "onboarding.sensorQuality": "Якість датчиків у реальному часі",
  "onboarding.quality.good": "Добре",
  "onboarding.quality.fair": "Середнє",
  "onboarding.quality.poor": "Погано",
  "onboarding.quality.no_signal": "Немає сигналу",
  "onboarding.fitNeedsBt": "Спочатку підключіть пристрій, щоб бачити дані датчиків у реальному часі.",
  "onboarding.fitTips": "Поради для кращого контакту",
  "onboarding.fitTip1": "Вушні датчики (TP9/TP10): розмістіть за вухами та трохи вище. Приберіть волосся з датчиків.",
  "onboarding.fitTip2":
    "Лобні датчики (AF7/AF8): мають лежати рівно на чистій шкірі — протріть сухою тканиною за потреби.",
  "onboarding.fitTip3": "Якщо контакт поганий, злегка зволожте датчики вологим пальцем. Це покращує провідність.",
  "onboarding.fitGood": "Чудова посадка! Усі датчики мають хороший контакт.",
  "onboarding.calibrationTitle": "Запуск калібрування",
  "onboarding.calibrationBody":
    "Калібрування записує маркований ЕЕГ, поки ви чергуєте між двома ментальними станами. Це допомагає {app} вивчити базові патерни вашого мозку.",
  "onboarding.openCalibration": "Відкрити калібрування",
  "onboarding.calibrationNeedsBt": "Спочатку підключіть пристрій для запуску калібрування.",
  "onboarding.calibrationSkip": "Можна пропустити і калібрувати пізніше з меню трею або налаштувань.",
  "onboarding.enableBluetoothTitle": "Увімкніть Bluetooth на вашому Mac",
  "onboarding.enableBluetoothBody":
    "{app} потребує, щоб адаптер Bluetooth вашого Mac був ввімкнений, щоб знайти й підключити ваш BCI-пристрій. Увімкніть Bluetooth у Налаштуваннях системи, якщо він вимкнений.",
  "onboarding.enableBluetoothStatus": "Адаптер Bluetooth",
  "onboarding.enableBluetoothHint":
    "Відкрийте Налаштування → Bluetooth та увімкніть Bluetooth. Якщо ви запускаєте у розробці через Terminal, переконайтеся, що системний адаптер увімкнений.",
  "onboarding.enableBluetoothOpen": "Відкрити налаштування Bluetooth",
  "onboarding.modelsTitle": "Завантаження рекомендованих моделей",
  "onboarding.modelsBody":
    "Для найкращого локального досвіду завантажте зараз ці стандартні моделі: Qwen3.5 4B (Q4_K_M), кодер ZUNA, NeuTTS та Kitten TTS.",
  "onboarding.models.downloadAll": "Завантажити рекомендований набір",
  "onboarding.models.download": "Завантажити",
  "onboarding.models.downloading": "Завантаження…",
  "onboarding.models.downloaded": "Завантажено",
  "onboarding.models.qwenTitle": "Qwen3.5 4B (Q4_K_M)",
  "onboarding.models.qwenDesc":
    "Рекомендована модель чату. Використовує Q4_K_M для найкращого балансу якості та швидкості на більшості ноутбуків.",
  "onboarding.models.zunaTitle": "Кодер EEG ZUNA",
  "onboarding.models.zunaDesc": "Потрібен для ембедингів EEG, семантичної історії та аналітики станів мозку.",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc":
    "Рекомендований багатомовний голосовий рушій з кращою якістю та підтримкою клонування.",
  "onboarding.models.kittenTitle": "Kitten TTS",
  "onboarding.models.kittenDesc":
    "Легкий швидкий голосовий бекенд, корисний як запасний варіант та для систем з обмеженими ресурсами.",
  "onboarding.trayTitle": "Знайдіть застосунок у треї",
  "onboarding.trayBody":
    "{app} працює у фоновому режимі. Після налаштування значок у рядку меню (macOS) або в треї системи (Windows/Linux) — ваша точка доступу до застосунку.",
  "onboarding.tray.states": "Значок змінює колір залежно від стану:",
  "onboarding.tray.grey": "Сірий — відключено",
  "onboarding.tray.amber": "Бурштиновий — пошук або підключення",
  "onboarding.tray.green": "Зелений — підключено та запис",
  "onboarding.tray.red": "Червоний — Bluetooth вимкнено",
  "onboarding.tray.open": "Натисніть на значок у треї будь-коли, щоб показати або приховати панель.",
  "onboarding.tray.menu":
    "Клік правою кнопкою (або лівою на Windows/Linux) відкриває швидке меню — підключити, позначити, калібрувати тощо.",
  "onboarding.extensionsTitle": "Супутні розширення",
  "onboarding.extensionsBody":
    "{app} може отримувати додатковий контекст з вашого редактора, браузера та терміналу. Кожна інтеграція є окремою частиною, яку можна встановити чи пропустити незалежно — жодна не потрібна для роботи функцій ЕЕГ.",
  "onboarding.extensionsPrivacy":
    "Та сама гарантія конфіденційності, що й для всього іншого: кожне розширення звітує локальному демону через порт localhost, а ці дані записуються в activity.sqlite на цьому комп'ютері. Нічого не надсилається до NeuroSkill або будь-кого іншого.",
  "onboarding.extensionsSkip":
    "Усе необов'язково. Ви можете встановити, оновити або видалити будь-який з них пізніше в Налаштування → Розширення та Налаштування → Термінал.",
  "onboarding.extensions.vscodeTitle": "Редактор на основі VS Code",
  "onboarding.extensions.vscodeDesc":
    "Додає відстеження редагування на рівні файлу, AI-пропозиції inline та інтеграцію з циклом розробки. Працює з VS Code, VSCodium, Cursor, Windsurf, Trae, Positron — встановлені форки автоматично виявляються.",
  "onboarding.extensions.browserTitle": "Розширення браузера",
  "onboarding.extensions.browserDesc":
    "Записує активну вкладку, час фокусу сторінки та шаблони читання з вашого браузера. Sideload підтримується для Chrome, Firefox, Edge та Safari (Safari потребує додаткового кроку підпису).",
  "onboarding.extensions.terminalTitle": "Хуки терміналу / shell",
  "onboarding.extensions.terminalDesc":
    "Додає невеликий хук preexec/precmd до вашого shell, щоб програма могла співвідносити час команд зі станом фокусу. Виберіть zsh, bash, fish або PowerShell — модифікує ваш rc-файл одним рядком source, повністю оборотно пізніше.",

  "onboarding.permissionsTitle": "Необов'язкове відстеження активності",
  "onboarding.permissionsBody":
    '{app} може записувати, над чим ви працювали, щоб співвіднести дані ЕЕГ/фокусу з реальним контекстом — "я втратив фокус, пишучи цей PR" замість просто "я втратив фокус о 15:00". Вимкнено за замовчуванням і повністю необов\'язково.',
  "onboarding.permissionsPrivacy":
    "Усе залишається на цьому комп'ютері. Записана активність зберігається в локальному файлі activity.sqlite і ніколи не надсилається на жоден сервер — ні до NeuroSkill, ні до когось іще. Кожну опцію можна вимкнути будь-коли; записані дані залишаються на диску, доки ви їх не видалите.",
  "onboarding.permissionsSkip":
    "Усе вимкнено за замовчуванням. Можна увімкнути будь-який пункт пізніше в Налаштування → Відстеження активності.",
  "onboarding.permissionsActiveWindowDesc":
    "Захоплює програму на передньому плані, заголовок вікна, активну вкладку браузера та шлях до відкритого файлу в редакторі. macOS запитає дозвіл Спеціальних можливостей / Автоматизації для кожного браузера й редактора.",
  "onboarding.permissionsInputDesc":
    "Записує лише позначки часу використання клавіатури/миші — ніколи не які клавіші, ніколи не положення, ніколи не вміст. Не потребує системних дозволів.",
  "onboarding.permissionsFileDesc":
    "Спостерігає за Documents, Desktop, Downloads та типовими теками розробки на події створення/зміни/видалення. Записує лише шляхи та часові позначки — вміст файлів ніколи не читається. macOS може запитати Повний доступ до диска.",
  "onboarding.permissionsScreenshotsDesc":
    "Знімає екран з певним інтервалом, виконує OCR над текстом і індексує і те й інше для візуального пошуку та запитів типу «що було на екрані о 15:00». macOS запитає Запис екрана. Інтервал, якість і OCR налаштовуються в Налаштування → Знімки екрана.",
  "onboarding.permissionsLocationDesc":
    "Записує місцезнаходження пристрою разом з блоками фокусу (дім vs офіс vs кафе), щоб зміни місць можна було співвідносити зі станом фокусу. macOS запитає Служби геолокації. Зберігається локально; ніколи не передається.",
  "onboarding.permissionsCalendarDesc":
    "Читає метадані подій календаря (заголовок, час, тривалість, кількість учасників), щоб співвідносити густоту зустрічей з падіннями концентрації. macOS запитає Доступ до календаря при першому використанні. Вміст подій ніколи не передається.",
  "onboarding.permissionsClipboardDesc":
    "Записує, коли змінюється буфер обміну (яка програма, тип вмісту, розмір). Сам вміст ніколи не читається. Лише macOS; запитає дозвіл Автоматизації.",
  "onboarding.downloadsComplete": "Усі завантаження завершені!",
  "onboarding.downloadsCompleteBody":
    "Рекомендовані моделі завантажені та готові до використання. Щоб завантажити більше моделей або перейти на інші, відкрийте",
  "onboarding.downloadMoreSettings": "параметри програми",
  "onboarding.doneTitle": "Все готово!",
  "onboarding.doneBody": "{app} працює у вашому меню. Ось кілька корисних порад:",
  "onboarding.doneTip.tray": "{app} знаходиться в треї меню. Натисніть на іконку, щоб показати/сховати панель.",
  "onboarding.doneTip.shortcuts": "Використовуйте ⌘K для палітри команд або ? для перегляду всіх гарячих клавіш.",
  "onboarding.doneTip.help": "Відкрийте довідку з меню трею для повного опису всіх функцій.",
  "onboarding.back": "Назад",
  "onboarding.next": "Далі",
  "onboarding.getStarted": "Почати",
  "onboarding.finish": "Завершити",
  "onboarding.models.ocrTitle": "OCR Models",
  "onboarding.models.ocrDesc":
    "Text detection + recognition models for extracting text from screenshots. Enables text search across captured screens (~10 MB each).",
  "onboarding.screenRecTitle": "Дозвіл на запис екрана",
  "onboarding.screenRecDesc":
    "Потрібен на macOS для захоплення вікон інших програм для системи знімків екрана. Без нього знімки можуть бути порожніми.",
  "onboarding.screenRecOpen": "Відкрити налаштування",
};

export default onboarding;
