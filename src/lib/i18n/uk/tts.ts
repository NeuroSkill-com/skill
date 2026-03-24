// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** UK "tts" namespace translations. */
const tts: Record<string, string> = {
  "ttsTab.backendSection": "Голосовий рушій",
  "ttsTab.backendKitten": "KittenTTS",
  "ttsTab.backendKittenTag": "ONNX · Англійська · ~30 МБ",
  "ttsTab.backendKittenDesc": "Компактна ONNX-модель, швидка на будь-якому CPU, лише англійська.",
  "ttsTab.backendNeutts": "NeuTTS",
  "ttsTab.backendNeuttsTag": "GGUF · Клонування голосу · Багатомовний",
  "ttsTab.backendNeuttsDesc":
    "GGUF LLM-бекбон з декодером NeuCodec. Клонує будь-який голос; підтримує англійську, німецьку, французьку, іспанську.",
  "ttsTab.statusSection": "Стан рушія",
  "ttsTab.statusReady": "Готово",
  "ttsTab.statusLoading": "Завантаження…",
  "ttsTab.statusIdle": "Не активний",
  "ttsTab.statusUnloaded": "Не завантажено",
  "ttsTab.statusError": "Помилка",
  "ttsTab.preloadButton": "Попереднє завантаження",
  "ttsTab.retryButton": "Повторити",
  "ttsTab.preloadOnStartup": "Попереднє завантаження рушія при запуску",
  "ttsTab.preloadOnStartupDesc": "Прогріває активний рушій у фоні при запуску програми",
  "ttsTab.unloadButton": "Вивантажити",
  "ttsTab.errorTitle": "Помилка завантаження",
  "ttsTab.requirements": "Потрібний espeak-ng у PATH",
  "ttsTab.requirementsDesc": "macOS: brew install espeak-ng · Ubuntu: apt install espeak-ng",
  "ttsTab.kittenConfigSection": "Налаштування KittenTTS",
  "ttsTab.kittenVoiceLabel": "Голос",
  "ttsTab.kittenModelInfo": "KittenML/kitten-tts-mini-0.8 · 24 кГц · ~30 МБ",
  "ttsTab.neuttsConfigSection": "Налаштування NeuTTS",
  "ttsTab.neuttsModelLabel": "Базова модель",
  "ttsTab.neuttsModelDesc": "Менший GGUF = швидше; більший = природніше. Q4 рекомендовано для більшості систем.",
  "ttsTab.neuttsVoiceSection": "Референсний голос",
  "ttsTab.neuttsVoiceDesc": "Оберіть попередньо налаштований голос або надайте власний WAV-кліп для клонування голосу.",
  "ttsTab.neuttsPresetLabel": "Попередні голоси",
  "ttsTab.neuttsCustomOption": "Власний WAV…",
  "ttsTab.neuttsRefWavLabel": "Референсний WAV",
  "ttsTab.neuttsRefWavNone": "Файл не вибрано",
  "ttsTab.neuttsRefWavBrowse": "Огляд…",
  "ttsTab.neuttsRefTextLabel": "Транскрипт",
  "ttsTab.neuttsRefTextPlaceholder": "Введіть точно те, що сказано у WAV-кліпі",
  "ttsTab.neuttsSaveButton": "Зберегти",
  "ttsTab.neuttsSaved": "Збережено",
  "ttsTab.voiceJo": "Jo",
  "ttsTab.voiceDave": "Dave",
  "ttsTab.voiceGreta": "Greta",
  "ttsTab.voiceJuliette": "Juliette",
  "ttsTab.voiceMateo": "Mateo",
  "ttsTab.voiceCustom": "Власний…",
  "ttsTab.testSection": "Тест голосу",
  "ttsTab.testDesc": "Введіть будь-який текст і натисніть Говорити, щоб почути активний рушій.",
  "ttsTab.startupSection": "Запуск",
  "ttsTab.loggingSection": "Журналювання для налагодження",
  "ttsTab.loggingLabel": "Журналювання синтезу TTS",
  "ttsTab.loggingDesc": "Записує події синтезу (текст, кількість семплів, затримка) у файл журналу.",
  "ttsTab.apiSection": "API",
  "ttsTab.apiDesc": "Запускайте мовлення з будь-якого скрипту через WebSocket або HTTP API:",
  "ttsTab.apiExampleWs": 'WebSocket:  {"command":"say","text":"Очі заплющено."}',
  "ttsTab.apiExampleHttp": 'HTTP (curl): POST /say  body: {"text":"Очі заплющено."}',

  "helpTts.overviewTitle": "Голосовий супровід на пристрої (TTS)",
  "helpTts.overviewBody":
    "NeuroSkill™ включає повністю локальний рушій синтезу мовлення. Він оголошує фази калібрування вголос та запускається через WebSocket або HTTP API. Вся синтеза локальна — інтернет не потрібен після завантаження моделі (~30 МБ).",
  "helpTts.howItWorksTitle": "Як це працює",
  "helpTts.howItWorksBody":
    "Обробка тексту → розбиття на речення (≤400 символів) → фонемізація через libespeak-ng (C-бібліотека, в процесі, голос en-us) → токенізація (IPA → IDs) → ONNX-інференція (KittenTTS) → 1 с тиші → відтворення через rodio.",
  "helpTts.modelTitle": "Model",
  "helpTts.modelBody":
    "KittenML/kitten-tts-mini-0.8 з HuggingFace Hub. Голос: Jasper (en-us). 24 000 Гц моно float32. Квантований INT8 ONNX — CPU. Кешується після першого завантаження.",
  "helpTts.requirementsTitle": "Вимоги",
  "helpTts.requirementsBody":
    "espeak-ng повинен бути встановлений та в PATH. macOS: brew install espeak-ng. Ubuntu/Debian: apt install libespeak-ng-dev. Alpine: apk add espeak-ng-dev. Fedora: dnf install espeak-ng-devel.",
  "helpTts.calibrationTitle": "Інтеграція з калібруванням",
  "helpTts.calibrationBody":
    "При початку калібрування рушій прогрівається у фоні (завантаження моделі за потреби). На кожному етапі викликається tts_speak. Мовлення не блокує калібрування — всі виклики fire-and-forget.",
  "helpTts.apiTitle": "API — команда say",
  "helpTts.apiBody":
    'Запускайте мовлення з будь-якого скрипту або LLM-агента. Команда повертається негайно. WebSocket: {"command":"say","text":"ваше повідомлення"}. HTTP: POST /say з body {"text":"ваше повідомлення"}. CLI: curl -X POST http://localhost:<port>/say -d \'{"text":"привіт"}\' -H \'Content-Type: application/json\'.',
  "helpTts.loggingTitle": "Журналювання для налагодження",
  "helpTts.loggingBody":
    "Увімкніть журналювання TTS у Налаштуваннях → Голос для запису подій синтезу (текст, семпли, затримка) у файл журналу NeuroSkill™. Корисно для діагностики.",
  "helpTts.testTitle": "Протестуйте тут",
  "helpTts.testBody": "Скористайтеся віджетом нижче для тестування рушія TTS з цього вікна довідки.",
};

export default tts;
