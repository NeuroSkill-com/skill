// SPDX-License-Identifier: GPL-3.0-only
/** UK — "validation" namespace. */
const validation: Record<string, string> = {
  "settingsTabs.validation": "Перевірка",
  "validation.title": "Перевірка та дослідження",
  "validation.intro":
    "Опціональні дослідницькі інструменти, що калібрують Тренера перерв і Оцінку фокусу проти зовнішніх вимірювань. Не обов'язкові для використання NeuroSkill.",
  "validation.disclaimer":
    "Лише дослідницький інструмент — не медичний пристрій. Не схвалено FDA, CE чи будь-яким регулятором. Не для клінічного використання.",

  "validation.master.title": "Глобальні запобіжники",
  "validation.master.respectFlow": "Поважати стан потоку",
  "validation.master.respectFlowDesc":
    "Коли ви входите в потік, усі підказки нижче пригнічуються. Увімкнено за замовчуванням — залиште так.",
  "validation.master.quietBefore": "Початок тихих годин",
  "validation.master.quietAfter": "Кінець тихих годин",
  "validation.master.quietDesc":
    "Локальний час. Поза цим вікном жодних підказок. початок = кінець повністю вимикає тихі години.",

  "validation.kss.title": "Каролінська шкала сонливості (KSS)",
  "validation.kss.desc":
    "5-секундний самозвіт (1-9) про моментальну сонливість. Калібрує Тренера перерв проти суб'єктивного стану.",
  "validation.kss.enabled": "Увімкнути запити KSS",
  "validation.kss.maxPerDay": "Макс. запитів на день",
  "validation.kss.minInterval": "Мін. хвилин між запитами",
  "validation.kss.triggerBreakCoach": "Запускати, коли Тренер перерв виявляє втому",
  "validation.kss.triggerRandom": "Запускати періодичні випадкові контрольні зразки",
  "validation.kss.triggerRandomDesc":
    "Потрібно для обчислення ROC/AUC — без негативних випадків бачимо лише позитивні.",
  "validation.kss.randomWeight": "Вага випадкових зразків (0-1)",

  "validation.tlx.title": "NASA-TLX (навантаження, 6 сирих шкал)",
  "validation.tlx.desc":
    "60-секундний самозвіт з 6 підшкалами після одиниці роботи. Вимірює навантаження — комплементарне до сонливості KSS.",
  "validation.tlx.enabled": "Увімкнути запити NASA-TLX",
  "validation.tlx.maxPerDay": "Макс. запитів на день",
  "validation.tlx.minTaskMin": "Мін. тривалість завдання (хв) для запиту",
  "validation.tlx.endOfDay": "Запуск також підсумку навантаження в кінці дня",

  "validation.tlx.form.title": "Оцініть щойно завершене завдання",
  "validation.tlx.mental": "Розумова вимога",
  "validation.tlx.physical": "Фізична вимога",
  "validation.tlx.temporal": "Часова вимога",
  "validation.tlx.performance": "Виконання",
  "validation.tlx.effort": "Зусилля",
  "validation.tlx.frustration": "Розчарування",

  "validation.pvt.title": "Завдання психомоторної пильності (PVT)",
  "validation.pvt.desc":
    "3-хвилинне завдання реакції. Об'єктивна міра пильності — повільне у зборі, але найсильніший сигнал у літературі.",
  "validation.pvt.enabled": "Увімкнути щотижневі нагадування PVT",
  "validation.pvt.weeklyReminder": "Показувати нагадування, коли цього тижня не було PVT",
  "validation.pvt.runNow": "Запустити PVT зараз (3 хв)",
  "validation.pvt.task.start": "Почати",
  "validation.pvt.task.cancel": "Скасувати",
  "validation.pvt.task.close": "Закрити",

  "validation.eeg.title": "Індекс втоми EEG (Jap et al. 2009)",
  "validation.eeg.desc":
    "Обчислюється безперервно з потоку потужності смуги, коли підключено гарнітуру NeuroSkill. Формула: (α + θ) / β. Пасивно — без витрат.",
  "validation.eeg.enabled": "Обчислювати індекс втоми EEG",
  "validation.eeg.windowSecs": "Ковзне вікно (секунди)",
  "validation.eeg.current": "Поточне значення",
  "validation.eeg.noHeadset": "Жодна EEG-гарнітура не транслює",

  "validation.calibrationWeek.title": "Тиждень калібрування",
  "validation.calibrationWeek.desc":
    "Опціональний 7-денний бурст з вищою частотою вибірки. Збільшує KSS до 8/день, запускає TLX після кожного блоку потоку ≥ 20 хв, просить один PVT в середині тижня. Автоматично повертає звичайні налаштування на 8-й день.",
  "validation.calibrationWeek.start": "Запустити тиждень калібрування",

  "validation.results.title": "Останні результати",
  "validation.save.saved": "Збережено",
};
export default validation;
