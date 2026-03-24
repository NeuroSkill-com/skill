// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** HE "tts" namespace translations. */
const tts: Record<string, string> = {
  "ttsTab.backendSection": "מנוע קול",
  "ttsTab.backendKitten": "KittenTTS",
  "ttsTab.backendKittenTag": "ONNX · אנגלית · ~30 MB",
  "ttsTab.backendKittenDesc": "מודל ONNX קומפקטי, מהיר בכל CPU, אנגלית בלבד.",
  "ttsTab.backendNeutts": "NeuTTS",
  "ttsTab.backendNeuttsTag": "GGUF · שכפול קול · רב-לשוני",
  "ttsTab.backendNeuttsDesc":
    "GGUF LLM backbone עם מפענח NeuCodec. משכפל כל קול; תומך באנגלית, גרמנית, צרפתית, ספרדית.",
  "ttsTab.statusSection": "מצב מנוע",
  "ttsTab.statusReady": "מוכן",
  "ttsTab.statusLoading": "טוען…",
  "ttsTab.statusIdle": "לא פעיל",
  "ttsTab.statusUnloaded": "לא טעון",
  "ttsTab.statusError": "שגיאה",
  "ttsTab.preloadButton": "טעינה מוקדמת",
  "ttsTab.retryButton": "נסה שוב",
  "ttsTab.preloadOnStartup": "טעינה מוקדמת של המנוע בהפעלה",
  "ttsTab.preloadOnStartupDesc": "מחמם את המנוע הפעיל ברקע בעת הפעלת האפליקציה",
  "ttsTab.unloadButton": "פריקה",
  "ttsTab.errorTitle": "שגיאת טעינה",
  "ttsTab.requirements": "דורש espeak-ng ב-PATH",
  "ttsTab.requirementsDesc": "macOS: brew install espeak-ng · Ubuntu: apt install espeak-ng",
  "ttsTab.kittenConfigSection": "הגדרות KittenTTS",
  "ttsTab.kittenVoiceLabel": "קול",
  "ttsTab.kittenModelInfo": "KittenML/kitten-tts-mini-0.8 · 24 kHz · ~30 MB",
  "ttsTab.neuttsConfigSection": "הגדרות NeuTTS",
  "ttsTab.neuttsModelLabel": "מודל Backbone",
  "ttsTab.neuttsModelDesc": "GGUF קטן יותר = מהיר יותר; גדול יותר = טבעי יותר. Q4 מומלץ לרוב המערכות.",
  "ttsTab.neuttsVoiceSection": "קול ייחוס",
  "ttsTab.neuttsVoiceDesc": "בחר קול מוגדר מראש או ספק קליפ WAV משלך לשכפול קול.",
  "ttsTab.neuttsPresetLabel": "קולות מוגדרים מראש",
  "ttsTab.neuttsCustomOption": "WAV מותאם…",
  "ttsTab.neuttsRefWavLabel": "WAV ייחוס",
  "ttsTab.neuttsRefWavNone": "לא נבחר קובץ",
  "ttsTab.neuttsRefWavBrowse": "עיון…",
  "ttsTab.neuttsRefTextLabel": "תמליל",
  "ttsTab.neuttsRefTextPlaceholder": "הקלד בדיוק מה שנאמר בקליפ ה-WAV",
  "ttsTab.neuttsSaveButton": "שמור",
  "ttsTab.neuttsSaved": "נשמר",
  "ttsTab.voiceJo": "Jo",
  "ttsTab.voiceDave": "Dave",
  "ttsTab.voiceGreta": "Greta",
  "ttsTab.voiceJuliette": "Juliette",
  "ttsTab.voiceMateo": "Mateo",
  "ttsTab.voiceCustom": "מותאם…",
  "ttsTab.testSection": "בדיקת קול",
  "ttsTab.testDesc": "הקלד כל טקסט ולחץ דבר כדי לשמוע את המנוע הפעיל.",
  "ttsTab.startupSection": "הפעלה",
  "ttsTab.loggingSection": "רישום לניפוי באגים",
  "ttsTab.loggingLabel": "רישום סינתזת TTS",
  "ttsTab.loggingDesc": "כותב אירועי סינתזה (טקסט, מספר דגימות, זמן תגובה) לקובץ הרישום.",
  "ttsTab.apiSection": "API",
  "ttsTab.apiDesc": "הפעל דיבור מכל סקריפט או כלי דרך WebSocket או HTTP API:",
  "ttsTab.apiExampleWs": 'WebSocket:  {"command":"say","text":"עיניים עצומות."}',
  "ttsTab.apiExampleHttp": 'HTTP (curl): POST /say  body: {"text":"עיניים עצומות."}',

  "helpTts.overviewTitle": "הנחיה קולית מקומית (TTS)",
  "helpTts.overviewBody":
    "NeuroSkill™ כולל מנוע דיבור מקומי לחלוטין. הוא מכריז על שלבי כיול בקול (תוויות, הפסקות, סיום) ומופעל דרך WebSocket או HTTP API. הסינתזה מקומית — אין צורך באינטרנט לאחר הורדת המודל (~30 MB).",
  "helpTts.howItWorksTitle": "איך זה עובד",
  "helpTts.howItWorksBody":
    "עיבוד טקסט → חלוקה למשפטים (≤400 תווים) → פונמיזציה באמצעות libespeak-ng (ספריית C, בתוך התהליך, קול en-us) → טוקניזציה (IPA → IDs) → הסקת ONNX (KittenTTS) → 1 שנייה שקט → השמעה דרך rodio.",
  "helpTts.modelTitle": "Model",
  "helpTts.modelBody":
    "KittenML/kitten-tts-mini-0.8 מ-HuggingFace Hub. קול: Jasper (en-us). 24,000 Hz מונו float32. ONNX מכומת INT8 — CPU בלבד. נשמר במטמון לאחר הורדה ראשונה.",
  "helpTts.requirementsTitle": "דרישות",
  "helpTts.requirementsBody":
    "espeak-ng חייב להיות מותקן ובנתיב PATH. macOS: brew install espeak-ng. Ubuntu/Debian: apt install libespeak-ng-dev. Alpine: apk add espeak-ng-dev. Fedora: dnf install espeak-ng-devel.",
  "helpTts.calibrationTitle": "שילוב בכיול",
  "helpTts.calibrationBody":
    "כשמתחיל כיול, המנוע מתחמם ברקע (מוריד את המודל במידת הצורך). בכל שלב נקרא tts_speak. הדיבור לעולם אינו חוסם את הכיול — כל הקריאות הן fire-and-forget.",
  "helpTts.apiTitle": "API — פקודת say",
  "helpTts.apiBody":
    'הפעילו דיבור מכל סקריפט או סוכן LLM. הפקודה חוזרת מיד. WebSocket: {"command":"say","text":"ההודעה שלך"}. HTTP: POST /say עם body {"text":"ההודעה שלך"}. CLI: curl -X POST http://localhost:<port>/say -d \'{"text":"שלום"}\' -H \'Content-Type: application/json\'.',
  "helpTts.loggingTitle": "רישום לניפוי באגים",
  "helpTts.loggingBody":
    "הפעילו רישום TTS בהגדרות → קול כדי לכתוב אירועי סינתזה (טקסט, דגימות, השהיה) לקובץ הלוג של NeuroSkill™. שימושי למדידת השהיה ואבחון בעיות.",
  "helpTts.testTitle": "בדקו כאן",
  "helpTts.testBody": "השתמשו בווידג'ט למטה כדי לבדוק את מנוע ה-TTS מחלון עזרה זה.",
};

export default tts;
