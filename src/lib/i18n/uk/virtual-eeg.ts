// SPDX-License-Identifier: GPL-3.0-only
/** UK "virtual-eeg" namespace — Симулятор віртуального пристрою EEG. */
const virtualEeg: Record<string, string> = {
  "settingsTabs.virtualEeg": "Віртуальний EEG",

  "veeg.title": "Віртуальний пристрій EEG",
  "veeg.desc":
    "Симулюйте гарнітуру EEG для тестування, демонстрацій та розробки. Генерує синтетичні дані, що проходять через увесь конвеєр обробки сигналу.",

  "veeg.status": "Стан",
  "veeg.running": "Працює",
  "veeg.stopped": "Зупинено",
  "veeg.start": "Запустити",
  "veeg.stop": "Зупинити",

  "veeg.channels": "Канали",
  "veeg.channelsDesc": "Кількість електродів EEG для симуляції.",
  "veeg.sampleRate": "Частота дискретизації (Hz)",
  "veeg.sampleRateDesc": "Відліків на секунду на канал.",

  "veeg.template": "Шаблон сигналу",
  "veeg.templateDesc": "Оберіть тип синтетичного сигналу для генерації.",
  "veeg.templateSine": "Синусоїди",
  "veeg.templateSineDesc": "Чисті синусоїди в стандартних частотних смугах (дельта, тета, альфа, бета, гамма).",
  "veeg.templateGoodQuality": "EEG хорошої якості",
  "veeg.templateGoodQualityDesc": "Реалістичний EEG у стані спокою з домінантним альфа-ритмом та рожевим шумом на тлі.",
  "veeg.templateBadQuality": "EEG поганої якості",
  "veeg.templateBadQualityDesc":
    "Зашумлений сигнал із м'язовими артефактами, мережевою наводкою 50/60 Hz та стрибками електродів.",
  "veeg.templateInterruptions": "Переривчасте з'єднання",
  "veeg.templateInterruptionsDesc":
    "Хороший сигнал із періодичними випаданнями, що імітують нещільні електроди або бездротові завади.",
  "veeg.templateFile": "З файлу",
  "veeg.templateFileDesc": "Відтворення відліків із файлу CSV або EDF.",

  "veeg.quality": "Якість сигналу",
  "veeg.qualityDesc": "Налаштуйте відношення сигнал/шум. Вище = чистіший сигнал.",
  "veeg.qualityPoor": "Погана",
  "veeg.qualityFair": "Задовільна",
  "veeg.qualityGood": "Хороша",
  "veeg.qualityExcellent": "Відмінна",

  "veeg.chooseFile": "Обрати файл",
  "veeg.noFile": "Файл не обрано",
  "veeg.fileLoaded": "{name} ({channels} каналів, {samples} відліків)",

  "veeg.advanced": "Додатково",
  "veeg.amplitudeUv": "Амплітуда (µV)",
  "veeg.amplitudeDesc": "Розмах амплітуди згенерованих сигналів.",
  "veeg.noiseUv": "Рівень шуму (µV)",
  "veeg.noiseDesc": "Середньоквадратична амплітуда адитивного гаусівського шуму.",
  "veeg.lineNoise": "Мережева наводка",
  "veeg.lineNoiseDesc": "Додати мережеву наводку 50 Hz або 60 Hz.",
  "veeg.lineNoise50": "50 Hz",
  "veeg.lineNoise60": "60 Hz",
  "veeg.lineNoiseNone": "Немає",
  "veeg.dropoutProb": "Ймовірність випадання",
  "veeg.dropoutDesc": "Ймовірність втрати сигналу на секунду (0 = немає, 1 = постійно).",

  "veeg.preview": "Попередній перегляд сигналу",
  "veeg.previewDesc": "Перегляд у реальному часі перших 4 каналів.",

  // ── Вікно віртуальних пристроїв ───────────────────────────────────────────────
  "window.title.virtualDevices": "{app} – Віртуальні пристрої",

  "vdev.title": "Віртуальні пристрої",
  "vdev.desc":
    "Тестуйте NeuroSkill без фізичного обладнання EEG. Оберіть шаблон, що відповідає реальному пристрою, або налаштуйте власне синтетичне джерело сигналу.",

  "vdev.presets": "Шаблони пристроїв",
  "vdev.statusRunning": "Віртуальний пристрій передає дані",
  "vdev.statusStopped": "Жодного віртуального пристрою не запущено",
  "vdev.selected": "Готово",
  "vdev.configure": "Налаштувати",
  "vdev.customConfig": "Власна конфігурація",

  "vdev.presetMuse": "Muse S",
  "vdev.presetMuseDesc": "Розташування пов'язки на 4 канали — TP9, AF7, AF8, TP10.",
  "vdev.presetCyton": "OpenBCI Cyton",
  "vdev.presetCytonDesc": "Дослідницький сигнал на 8 каналів, повний фронтальний/центральний монтаж.",
  "vdev.presetCap32": "Шапочка EEG на 32 канали",
  "vdev.presetCap32Desc": "Повна міжнародна система 10-20, 32 електроди.",
  "vdev.presetAlpha": "Виражений альфа",
  "vdev.presetAlphaDesc": "Помітний альфа-ритм 10 Hz — розслаблений базовий рівень із заплющеними очима.",
  "vdev.presetArtifact": "Тест артефактів",
  "vdev.presetArtifactDesc": "Зашумлений сигнал із м'язовими артефактами та мережевою наводкою 50 Hz.",
  "vdev.presetDropout": "Тест випадань",
  "vdev.presetDropoutDesc": "Періодична втрата сигналу, що імітує нещільні електроди.",
  "vdev.presetMinimal": "Мінімальний (1 канал)",
  "vdev.presetMinimalDesc": "Одноканальна синусоїда — найменше можливе навантаження.",
  "vdev.presetCustom": "Власний",
  "vdev.presetCustomDesc": "Визначте власну кількість каналів, частоту, шаблон та рівень шуму.",

  "vdev.lslSourceTitle": "Віртуальне джерело LSL",
  "vdev.lslRunning": "Передавання синтетичного EEG через LSL",
  "vdev.lslStopped": "Віртуальне джерело LSL зупинено",
  "vdev.lslDesc": "Запускає локальне джерело Lab Streaming Layer для тестування виявлення та підключення потоків LSL.",
  "vdev.lslHint":
    "Відкрийте Налаштування → вкладку LSL і натисніть «Сканувати мережу», щоб побачити SkillVirtualEEG у списку потоків, а потім підключіться до нього.",
  "vdev.lslStarted": "Віртуальне джерело LSL тепер передає дані в локальній мережі.",

  // Панель стану
  "vdev.statusSource": "Джерело LSL",
  "vdev.statusSession": "Сеанс",
  "vdev.sessionConnected": "Підключено",
  "vdev.sessionConnecting": "Підключення…",
  "vdev.sessionDisconnected": "Відключено",
  "vdev.startBtn": "Запустити віртуальний пристрій",
  "vdev.stopBtn": "Зупинити віртуальний пристрій",
  "vdev.autoConnect": "Автоматичне підключення до панелі",
  "vdev.autoConnectDesc": "Підключити панель до цього джерела одразу після запуску.",

  // Попередній перегляд
  "vdev.previewOffline": "Попередній перегляд сигналу (офлайн)",
  "vdev.previewOfflineDesc":
    "Попередній перегляд форми хвилі на стороні клієнта — показує форму сигналу до підключення. Дані ще не передаються.",

  // Власний шаблон — канали / частота
  "vdev.cfgChannels": "Канали",
  "vdev.cfgChannelsDesc": "Кількість електродів EEG для симуляції.",
  "vdev.cfgRate": "Частота дискретизації",
  "vdev.cfgRateDesc": "Відліків на секунду на канал.",

  // Власний шаблон — якість сигналу
  "vdev.cfgQuality": "Якість сигналу",
  "vdev.cfgQualityDesc": "Відношення сигнал/шум. Вище = чистіший сигнал.",

  // Власний шаблон — шаблон
  "vdev.cfgTemplate": "Шаблон сигналу",
  "vdev.cfgTemplateSine": "Синусоїди",
  "vdev.cfgTemplateSineDesc": "Чисті синусоїди на частотах дельта, тета, альфа, бета та гамма.",
  "vdev.cfgTemplateGood": "EEG хорошої якості",
  "vdev.cfgTemplateGoodDesc": "Реалістичний стан спокою з домінантним альфа та рожевим шумом на тлі.",
  "vdev.cfgTemplateBad": "EEG поганої якості",
  "vdev.cfgTemplateBadDesc": "Зашумлений сигнал із м'язовими артефактами, мережевою наводкою та стрибками електродів.",
  "vdev.cfgTemplateInterruptions": "Переривчасте з'єднання",
  "vdev.cfgTemplateInterruptionsDesc": "Хороший сигнал із періодичними випаданнями, що імітують нещільні електроди.",

  // Власний шаблон — додатково
  "vdev.cfgAdvanced": "Додатково",
  "vdev.cfgAmplitude": "Амплітуда (µV)",
  "vdev.cfgAmplitudeDesc": "Розмах амплітуди імітованого сигналу.",
  "vdev.cfgNoise": "Рівень шуму (µV)",
  "vdev.cfgNoiseDesc": "Середньоквадратична амплітуда адитивного гаусівського фонового шуму.",
  "vdev.cfgLineNoise": "Мережева наводка",
  "vdev.cfgLineNoiseDesc": "Додати мережеву наводку 50 Hz або 60 Hz.",
  "vdev.cfgLineNoiseNone": "Немає",
  "vdev.cfgLineNoise50": "50 Hz",
  "vdev.cfgLineNoise60": "60 Hz",
  "vdev.cfgDropout": "Ймовірність випадання",
  "vdev.cfgDropoutDesc": "Ймовірність втрати сигналу на секунду (0 = ніколи, 1 = постійно).",
};

export default virtualEeg;
