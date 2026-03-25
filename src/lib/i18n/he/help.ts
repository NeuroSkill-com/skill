// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** HE "help" namespace translations. */
const help: Record<string, string> = {
  "helpOld.hooksTitle": "הוקים פרואקטיביים",
  "helpOld.hooksDesc":
    "Hooks פועלים ברקע: התאמת מילות מפתח מטושטשת → הרחבת שכני טקסט → בדיקת מרחק EEG. כשיש התאמה, נשלח אירוע hook ומוצגת התראה.",
  "helpOld.hooksFlow": "זרימה חמודה",
  "helpOld.hooksFaqQ": "איך hook מופעל?",
  "helpOld.hooksFaqA":
    "תהליך הרקע משווה כל embedding EEG חדש לדוגמאות עדכניות של תוויות שנבחרו לפי מילות מפתח ודמיון טקסטואלי. אם מרחק הקוסינוס הטוב ביותר נמוך מהסף שלך, ה-hook מופעל.",

  "helpSettings.calibrationTts": "הנחיה קולית לכיול (TTS)",
  "helpSettings.calibrationTtsBody":
    'במהלך הכיול האפליקציה מכריזה על כל שלב בשמו באמצעות טקסט-לדיבור באנגלית על גבי המכשיר. המנוע מופעל על ידי KittenTTS (tract-onnx, ~30 MB) עם פונמיזציה של espeak-ng. המודל יורד מ-HuggingFace Hub בהפעלה הראשונה ונשמר מקומית — לאחר מכן לא יוצא מידע מהמכשיר שלך. הדיבור מופעל עבור: תחילת סשן, כל שלב פעולה, כל הפסקה ("הפסקה. הבא: …"), וסיום סשן. דורש espeak-ng ב-PATH (brew / apt / apk install espeak-ng). אנגלית בלבד.',
  "helpSettings.eegModelTabDesc": "עקוב אחר מקודד ZUNA ומצב אינדקס HNSW.",
  "helpSettings.encoderStatus": "מצב מקודד",
  "helpSettings.encoderStatusBody": "מציג אם מקודד ZUNA wgpu טעון.",
  "helpSettings.embeddingsToday": "הטמעות היום",
  "helpSettings.embeddingsTodayBody": "מונה חי של הטמעות שנוספו לאינדקס היומי.",
  "helpSettings.hnswParams": "פרמטרי HNSW",
  "helpSettings.hnswParamsBody": "M ו-ef_construction שולטים באיזון איכות/מהירות.",
  "helpSettings.dataNorm": "נרמול נתונים",
  "helpSettings.dataNormBody": "גורם data_norm המיושם על EEG גולמי. ברירת מחדל (10) מכויל ל-Muse 2/S.",
  "helpSettings.openbciSection": "מכשירי OpenBCI",
  "helpSettings.openbciSectionDesc": "חבר והגדר כל לוח OpenBCI — עצמאי או לצד מכשיר BCI אחר.",
  "helpSettings.openbciBoard": "בחירת לוח",
  "helpSettings.openbciBoardBody":
    "בחר לוח OpenBCI. Ganglion (4 ערוצים, BLE) הוא הנייד ביותר. Cyton (8 ערוצים, USB) מוסיף ערוצים. Cyton+Daisy מכפיל ל-16 ערוצים. גרסאות WiFi Shield מחליפות USB/BLE בזרם Wi-Fi של 1 kHz. Galea (24 ערוצים, UDP) הוא לוח מחקר בצפיפות גבוהה. כולם יכולים לפעול עצמאית או לצד מכשיר BCI אחר.",
  "helpSettings.openbciGanglion": "Ganglion BLE",
  "helpSettings.openbciGanglionBody":
    "ה-Ganglion מתחבר דרך Bluetooth Low Energy. לחץ חבר — NeuroSkill™ יסרוק אחר ה-Ganglion הקרוב ביותר עד לזמן הקצוב שהוגדר. שמור את הלוח בטווח של 3–5 מ' ומופעל (נורית כחולה מהבהבת). רק Ganglion אחד יכול להיות פעיל לכל מתאם Bluetooth.",
  "helpSettings.openbciSerial": "יציאה סדרתית (Cyton / Cyton+Daisy)",
  "helpSettings.openbciSerialBody":
    "לוחות Cyton מתקשרים דרך USB dongle רדיו. השאר את שדה היציאה ריק לזיהוי אוטומטי, או הזן אותה במפורש (/dev/cu.usbserial-… ב-macOS, /dev/ttyUSB0 ב-Linux, COM3 ב-Windows). חבר את ה-dongle לפני לחיצה על חבר. ב-Linux הוסף את המשתמש לקבוצת dialout.",
  "helpSettings.openbciWifi": "WiFi Shield",
  "helpSettings.openbciWifiBody":
    "ה-WiFi Shield של OpenBCI יוצר רשת 2.4 GHz משלו (SSID: OpenBCI-XXXX). חבר את המחשב לרשת זו והזן IP 192.168.4.1. לחלופין, ניתן לחבר את ה-Shield לרשת הביתית — הזן את ה-IP שהוקצה. השאר ריק לגילוי אוטומטי דרך mDNS. ה-WiFi Shield משדר ב-1 kHz — הגדר מסנן עובר-נמוך ל-≤ 500 Hz.",
  "helpSettings.openbciGalea": "Galea",
  "helpSettings.openbciGaleaBody":
    "Galea הוא אוזניות מחקר 24-ערוצים (EEG + EMG + AUX) המשדרות דרך UDP. הזן את כתובת ה-IP של מכשיר ה-Galea או השאר ריק לקבלה מכל שולח. ערוצים 1–8 הם EEG (ניתוח בזמן אמת); 9–16 EMG; 17–24 AUX. כל 24 הערוצים נשמרים ב-CSV.",
  "helpSettings.openbciChannels": "תוויות ערוצים וקביעות מוגדרות מראש",
  "helpSettings.openbciChannelsBody":
    "הקצה שמות אלקטרודות סטנדרטיים 10-20 לכל ערוץ פיזי. השתמש בקביעה מוגדרת מראש (קדמי, מוטורי, עורפי, מלא 10-20) או הזן שמות מותאמים. ערוצים מעבר ל-4 הראשונים נשמרים ב-CSV בלבד ואינם מניעים את צינור הניתוח בזמן אמת.",

  "helpWindows.title": "חלונות",
  "helpWindows.desc": "{app} משתמש בחלונות נפרדים למשימות ספציפיות.",
  "helpWindows.labelTitle": "🏷  חלון תווית",
  "helpWindows.labelBody": "נפתח דרך תפריט, קיצור או כפתור. הקלד תווית חופשית לסימון רגע EEG.",
  "helpWindows.searchTitle": "🔍  חלון חיפוש",
  "helpWindows.searchBody":
    "חלון החיפוש מציע שלושה מצבים — דמיון EEG, טקסט ואינטראקטיבי — שכל אחד מהם מבצע שאילתות על הנתונים שלך בדרך שונה.",
  "helpWindows.searchEegTitle": "חיפוש דמיון EEG",
  "helpWindows.searchEegBody":
    "בחר טווח תאריכים/שעות והפעל חיפוש שכנים קרובים על כל ה-embeddings של ZUNA שנרשמו בחלון זה. אינדקס HNSW מחזיר את k תקופות ה-EEG של 5 שניות הדומות ביותר מכל ההיסטוריה שלך, מדורגות לפי מרחק קוסינוס. מרחק קטן יותר = מצב מוח דומה יותר. תוויות החופפות לחותמת זמן של תוצאה מוצגות בשורה.",
  "helpWindows.searchTextTitle": "חיפוש embedding טקסטואלי",
  "helpWindows.searchTextBody":
    'הקלד כל מושג, פעילות או מצב מנטלי בשפה טבעית (למשל "ריכוז עמוק", "חרדה", "מדיטציה עם עיניים עצומות"). השאילתה מוטמעת על ידי אותו מודל sentence-transformer המשמש לאינדוקס תוויות, ומוצלבת עם כל ההערות שלך לפי דמיון קוסינוס באינדקס HNSW. התוצאות הן התוויות שלך עצמך, מדורגות לפי קרבה סמנטית — לא התאמת מילות מפתח. גרף kNN תלת-ממדי ממחיש את מבנה השכנות.',
  "helpWindows.searchInteractiveTitle": "חיפוש אינטראקטיבי רב-מודלי",
  "helpWindows.searchInteractiveBody":
    'הזן מושג בטקסט חופשי ו-{app} מריץ צינור עיבוד רב-מודלי בארבעה שלבים: (1) השאילתה מוטמעת; (2) text-k התוויות הדומות ביותר מבחינה סמנטית נאספות; (3) עבור כל תווית, {app} מחשב את ה-embedding הממוצע של ה-EEG לחלון ההקלטה שלה ומחפש את eeg-k תקופות ה-EEG הדומות ביותר; (4) עבור כל שכן EEG, הערות בטווח של ±reach דקות נאספות כ"תוויות שנמצאו". התוצאה היא גרף מכוון בארבע שכבות — שאילתה → התאמות טקסט → שכני EEG → תוויות שנמצאו — לתצוגה אינטראקטיבית תלת-ממדית, ניתן לייצוא כ-SVG או Graphviz DOT.',
  "helpWindows.calTitle": "🎯  חלון כיול",
  "helpWindows.calBody": "מריץ משימת כיול מודרכת. דורש מכשיר BCI מחובר ומשדר.",
  "helpWindows.settingsTitle": "⚙  חלון הגדרות",
  "helpWindows.settingsBody":
    "ארבע לשוניות: הגדרות, קיצורים (מקשי קיצור גלובליים, פלטת פקודות, מקשים באפליקציה), מודל EEG (מקודד ומצב HNSW). פתח מתפריט המגש או כפתור גלגל השיניים.",
  "helpWindows.helpTitle": "?  חלון עזרה",
  "helpWindows.helpBody": "חלון זה. התייחסות מלאה לכל חלק בממשק {app}.",
  "helpWindows.onboardingTitle": "🧭  אשף הגדרה",
  "helpWindows.onboardingBody":
    "אשף בן חמישה שלבים להפעלה ראשונה: חיבור בלוטות', התאמת מכשיר וכיול ראשון. נפתח אוטומטית בהפעלה הראשונה; ניתן לפתוח מחדש בכל עת מפלטת הפקודות (⌘K → אשף הגדרה).",
  "helpWindows.apiTitle": "🌐  חלון סטטוס API",
  "helpWindows.apiBody":
    "לוח מחוונים חי המציג את כל לקוחות WebSocket המחוברים כעת ויומן בקשות נגלל. מציג פורט שרת, פרוטוקול וגילוי mDNS. כולל קטעי חיבור מהיר ל-ws:// ו-dns-sd. מתרענן אוטומטית כל 2 שניות. פתח מתפריט המגש או פלטת הפקודות.",
  "helpWindows.overlaysTitle": "שכבות-על ופלטת פקודות",
  "helpWindows.overlaysDesc": "שכבות-על לגישה מהירה זמינות בכל חלון באמצעות קיצורי מקלדת.",
  "helpWindows.cmdPaletteTitle": "⌨  פלטת פקודות (⌘K / Ctrl+K)",
  "helpWindows.cmdPaletteBody":
    "תפריט נפתח מהיר המפרט כל פעולה ניתנת להרצה באפליקציה. הקלד לסינון, ↑↓ לניווט, Enter להפעלה. זמין בכל חלון. הפקודות כוללות פתיחת חלונות (הגדרות, עזרה, חיפוש, תווית, היסטוריה, כיול), פעולות מכשיר (ניסיון חיבור מחדש, הגדרות Bluetooth) וכלים (הצג קיצורים, בדוק עדכונים).",
  "helpWindows.shortcutsOverlayTitle": "?  שכבת קיצורי מקלדת",
  "helpWindows.shortcutsOverlayBody":
    "לחץ ? בכל חלון (מחוץ לשדות טקסט) להצגת שכבה צפה עם כל קיצורי המקלדת — קיצורים גלובליים שהוגדרו בהגדרות → קיצורים, בתוספת מקשים באפליקציה כמו ⌘K לפלטת הפקודות ו-⌘Enter לשמירת תוויות. לחץ שוב ? או Esc לסגירה.",

  "help.searchPlaceholder": "חפש בעזרה…",
  "help.searchNoResults": 'אין תוצאות עבור "{query}"',

  "helpWindows.sleepTitle": "🌙 שלבי שינה",
  "helpWindows.sleepBody":
    "עבור הפעלות של 30 דקות ומעלה מוצגת היפנוגרמה אוטומטית. הערה: אוזניות BCI צרכניות כמו Muse משתמשות ב-4 אלקטרודות יבשות — שלבי השינה הם קירוב, לא פוליסומנוגרפיה קלינית.",
  "helpWindows.compareTitle": "⚖  השווה",
  "helpWindows.compareBody":
    "בחר שני טווחי זמן בציר הזמן והשווה התפלגויות עוצמת פס, ציוני הרפיה/מעורבות ו-FAA זה לצד זה. כולל שלבי שינה, מדדים מתקדמים ו-Brain Nebula™ — הקרנת UMAP תלת-ממדית המציגה את הדמיון בין שני התקופות במרחב ה-EEG הרב-ממדי. פתח מתפריט ה-Tray או פלטת הפקודות (⌘K → השווה).",

  "helpApi.overview": "סקירה",
  "helpApi.liveStreaming": "שידור חי",
  "helpApi.liveStreamingBody":
    "{app} משדר מדדי EEG מעובדים וסטטוס מכשיר דרך שרת WebSocket מקומי. אירועים: eeg-bands (~4 Hz), device-status (~1 Hz), label-created.",
  "helpApi.commands": "פקודות",
  "helpApi.commandsBody":
    'לקוחות יכולים לשלוח פקודות JSON דרך WebSocket: status, calibrate, label, search, sessions, compare, sleep, umap/umap_poll. תגובות ב-JSON עם שדה "ok" בוליאני.',
  "helpApi.commandReference": "מדריך פקודות",
  "helpApi.discoveryWireFormat": "גילוי ופורמט",
  "helpApi.discoverService": "גלה את השירות",
  "helpApi.outboundEvents": "אירועים יוצאים (שרת → לקוח)",
  "helpApi.inboundCommands": "פקודות נכנסות (לקוח → שרת)",
  "helpApi.response": "תגובה",
  "helpApi.cmdStatus": "status",
  "helpApi.cmdStatusParams": "_(ללא)_",
  "helpApi.cmdStatusDesc": "מחזיר מצב מכשיר, מידע סשן, מוני הטמעות ואיכות אות.",
  "helpApi.cmdCalibrate": "calibrate",
  "helpApi.cmdCalibrateParams": "_(ללא)_",
  "helpApi.cmdCalibrateDesc": "פותח חלון כיול. דורש מכשיר מחובר.",
  "helpApi.cmdLabel": "label",
  "helpApi.cmdLabelParams": "text (מחרוזת, חובה); label_start_utc (u64, אופציונלי)",
  "helpApi.cmdLabelDesc": "מכניס תווית מתויגת בזמן למסד הנתונים.",
  "helpApi.cmdSearch": "search",
  "helpApi.cmdSearchParams": "start_utc, end_utc (u64, חובה); k, ef (u64, אופציונלי)",
  "helpApi.cmdSearchDesc": "מחפש k שכנים קרובים באינדקס HNSW.",
  "helpApi.cmdCompare": "compare",
  "helpApi.cmdCompareParams": "a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, נדרש)",
  "helpApi.cmdCompareDesc":
    "משווה שני טווחי זמן על ידי החזרת מדדי עוצמת פסים מצטברים (עוצמות יחסיות, ציוני מיקוד/הרפיה/מעורבות ו-FAA) עבור כל אחד. מחזיר { a: SessionMetrics, b: SessionMetrics }.",
  "helpApi.cmdSessions": "sessions",
  "helpApi.cmdSessionsParams": "_(ללא)_",
  "helpApi.cmdSessionsDesc":
    "מציג את כל הפגישות של embeddings ממסדי הנתונים היומיים. טווחי הקלטה רציפים (פער > 2 דק' = פגישה חדשה). החדשות ביותר ראשונות.",
  "helpApi.cmdSleep": "sleep",
  "helpApi.cmdSleepParams": "start_utc, end_utc (u64, נדרש)",
  "helpApi.cmdSleepDesc": "מסווג כל אפוק לשלב שינה (ער/N1/N2/N3/REM). מחזיר היפנוגרמה + סיכום.",
  "helpApi.cmdUmap": "umap",
  "helpApi.cmdUmapParams": "a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, נדרש)",
  "helpApi.cmdUmapDesc": "מכניס לתור משימת הטלת UMAP תלת-ממדית. מחזיר job_id לבדיקה. לא חוסם.",
  "helpApi.cmdUmapPoll": "umap_poll",
  "helpApi.cmdUmapPollParams": "job_id (מחרוזת, נדרש)",
  "helpApi.cmdUmapPollDesc": "בודק תוצאת משימת UMAP. מחזיר { status: pending | done, points?: [...] }.",

  "helpOld.trayIconStates": "מצבי אייקון המגש",
  "helpOld.trayIconDesc": "אייקון שורת התפריטים משנה צבע וצורה כדי לשקף את מצב החיבור הנוכחי במבט.",
  "helpOld.greyDisconnected": "אפור — מנותק",
  "helpOld.greyDesc": "Bluetooth פועל; אין מכשיר BCI מחובר.",
  "helpOld.spinningScanning": "מסתובב — סורק",
  "helpOld.spinningDesc": "מחפש מכשיר BCI או מנסה להתחבר.",
  "helpOld.greenConnected": "ירוק — מחובר",
  "helpOld.greenDesc": "שידור נתוני EEG בזמן אמת ממכשיר ה-BCI.",
  "helpOld.redBtOff": "אדום — Bluetooth כבוי",
  "helpOld.redDesc": "מודול ה-Bluetooth כבוי. לא ניתן לסרוק או להתחבר.",
  "helpOld.btLifecycle": "מחזור חיי Bluetooth וחיבור מחדש אוטומטי",
  "helpOld.btLifecycleDesc":
    "{app} עוקב אחר מצב Bluetooth בזמן אמת דרך CoreBluetooth (macOS) או BlueZ (Linux). ללא עיכוב.",
  "helpOld.btStep1": "Bluetooth נכבה",
  "helpOld.btStep1Desc": "אייקון המגש הופך לאדום מיד. כרטיס Bluetooth-Off מחליף את התצוגה הראשית.",
  "helpOld.btStep2": "Bluetooth מופעל מחדש",
  "helpOld.btStep2Desc": "תוך ~1 שנייה, {app} מחדש את הסריקה אוטומטית.",
  "helpOld.btStep3": "מכשיר ה-BCI מופעל",
  "helpOld.btStep3Desc": "הסורק מגלה אותו תוך 3–6 שניות ומתחבר אוטומטית.",
  "helpOld.btStep4": "מכשיר לא נמצא מיד",
  "helpOld.btStep4Desc": "{app} מנסה שוב בשקט כל 3 שניות.",
  "helpOld.btStep5": "אתה לוחץ על נסה שוב",
  "helpOld.btStep5Desc": "אותה לולאת ניסיונות חוזרים כמו חיבור מחדש אוטומטי. מנסה כל 3 שניות עד שהמכשיר נמצא.",
  "helpOld.examples": "דוגמאות",
  "helpOld.example1Title": "דוגמה 1 — הפעלה רגילה",
  "helpOld.example2Title": "דוגמה 2 — Bluetooth כבוי ואז מופעל",
  "helpOld.example3Title": "דוגמה 3 — מכשיר BCI מופעל לאחר שחזור BT",
  "helpOld.ex1Step1": "{app} נפתח → סורק מכשיר BCI",
  "helpOld.ex1Step2": "מכשיר נמצא תוך 5 שניות",
  "helpOld.ex1Step3": "מחובר — שידור EEG",
  "helpOld.ex2Step1": "מחובר → המשתמש מכבה Bluetooth",
  "helpOld.ex2Step2": 'האייקון הופך לאדום; כרטיס "Bluetooth כבוי" מוצג',
  "helpOld.ex2Step3": "… המשתמש מפעיל Bluetooth מחדש …",
  "helpOld.ex2Step4": "סריקה אוטומטית מתחדשת (~1 שנ')",
  "helpOld.ex2Step5": "מחובר מחדש — שידור חודש",
  "helpOld.ex3Step1": "BT פועל, מכשיר עדיין כבוי → retry כל 3 שנ'",
  "helpOld.ex3Step2": "… המשתמש מפעיל את מכשיר ה-BCI …",
  "helpOld.ex3Step3": "מכשיר התגלה במחזור הסריקה הבא",
  "helpOld.ex3Step4": "מחובר אוטומטית — ללא לחיצת כפתור",
  "helpOld.broadcastEvents": "אירועי שידור (שרת → לקוח)",
  "helpOld.commands": "פקודות (לקוח → שרת)",
  "helpOld.wsTitle": "שידור ברשת מקומית (WebSocket)",
  "helpOld.wsDesc":
    "{app} משדר מדדי EEG מעובדים (~4 Hz) וסטטוס מכשיר (~1 Hz) דרך שרת WebSocket מקומי. דגימות גולמיות אינן משודרות.",
  "helpOld.discoverService": "גלה את השירות",
  "helpOld.wireFormat": "פורמט חוט (JSON)",
  "helpOld.faq": "שאלות נפוצות",
  "helpOld.faqQ1": "למה אייקון המגש הופך לאדום?",
  "helpOld.faqA1": "Bluetooth כבוי. הפעל אותו בהגדרות מערכת → Bluetooth.",
  "helpOld.faqQ2": "האפליקציה מסתובבת אבל לא מתחברת?",
  "helpOld.faqA2":
    "1. וודא שמכשיר ה-BCI דלוק (Muse: החזק עד לרטט; Ganglion/Cyton: נורית כחולה). 2. היה בטווח 5 מ׳. 3. אם לא עובד, כבה והדלק מחדש.",
  "helpOld.faqQ3": "איך מעניקים הרשאת Bluetooth?",
  "helpOld.faqA3": "הגדרות מערכת → פרטיות ואבטחה → Bluetooth → הפעל {app}.",
  "helpOld.faqQ4": "האם אפשר לקבל נתוני EEG ברשת?",
  "helpOld.faqA4": "כן — מדדים מעובדים (~4 Hz) וסטטוס (~1 Hz) דרך WebSocket. דגימות גולמיות אינן משודרות.",
  "helpOld.faqQ5": "איפה ההקלטות שלי נשמרות?",
  "helpOld.faqA5": "ב-{dataDir}/ — קבצי CSV, SQLite, HNSW מסודרים לפי תאריך.",
  "helpOld.faqQ6": "מה משמעות נקודות איכות האות?",
  "helpOld.faqA6": "ירוק = מגע טוב. צהוב = בינוני. אדום = גרוע. אפור = אין אות.",
  "helpOld.faqQ7": "מהו פילטר ה-notch?",
  "helpOld.faqA7": "מסיר רעש 50/60 Hz של רשת החשמל מהתצוגה.",
  "helpOld.faqQ8": "אילו מדדים נשמרים?",
  "helpOld.faqA8":
    "עוצמות פסים, ציונים מעובדים, FAA, יחסים, צורה ספקטרלית, קוהרנטיות, Hjorth, מורכבות, מדדי PPG — לכל אפוק 2.5 שנ'.",
  "helpOld.faqQ9": "מהו השוואת פגישות?",
  "helpOld.faqA9": "משווה שתי פגישות זו לזו: עוצמות פסים, ציונים, FAA, שינה, UMAP.",
  "helpOld.faqQ10": "מהו מציג UMAP תלת-ממדי?",
  "helpOld.faqA10": "מטיל embeddings של EEG לתלת-ממד כך שמצבי מוח דומים מתקבצים.",
  "helpOld.faqQ11": "למה UMAP מראה ענן אקראי?",
  "helpOld.faqA11": "UMAP רץ ברקע. מוצג placeholder עד שההטלה מוכנה.",
  "helpOld.faqQ12": "מהן תוויות?",
  "helpOld.faqA12": "תגיות שהמשתמש מגדיר ומצמיד לרגעים במהלך הקלטה.",
  "helpOld.faqQ13": "מהי FAA?",
  "helpOld.faqA13": "ln(AF8 α) − ln(AF7 α). חיובי = מוטיבציית גישה, שלילי = נסיגה.",
  "helpOld.faqQ14": "איך עובד staging שינה?",
  "helpOld.faqA14": "מסווג אפוקות כ-ער/N1/N2/N3/REM לפי יחסי עוצמת פסים.",
  "helpOld.faqQ15": "קיצורי מקלדת?",
  "helpOld.faqA15": "⌘⇧O — פתח {app}. ⌘⇧M — השוואת פגישות. ניתן להתאמה בהגדרות → קיצורים.",
  "helpOld.faqQ16": "מהי ה-WebSocket API?",
  "helpOld.faqA16":
    "API של JSON ברשת מקומית (mDNS: _skill._tcp). פקודות: status, label, search, compare, sessions, sleep, umap, umap_poll.",
  "helpOld.faqQ17": "מהם Focus / Relaxation / Engagement?",
  "helpOld.faqA17": "ציונים מעובדים מיחסי עוצמת פסים, ממופים ל-0–100 דרך sigmoid.",
  "helpOld.faqQ18": "מהם TAR, BAR, DTR?",
  "helpOld.faqA18": "יחסים בין-פסיים: Theta/Alpha, Beta/Alpha, Delta/Theta.",
  "helpOld.faqQ19": "מהם PSE, APF, BPS, SNR?",
  "helpOld.faqA19": "תכונות צורה ספקטרלית: אנטרופיה, תדר שיא אלפא, שיפוע, יחס אות-רעש.",

  "helpPrivacy.overview": "סקירת פרטיות",
  "helpPrivacy.overviewDesc": "{app} מתוכנן להיות מקומי לחלוטין. הנתונים שלך לא עוזבים את המחשב.",
  "helpPrivacy.dataStorage": "אחסון נתונים",
  "helpPrivacy.allLocal": "כל הנתונים נשארים במכשיר שלך",
  "helpPrivacy.allLocalBody": "כל הנתונים נשמרים מקומית ב-{dataDir}/. שום דבר לא נשלח לענן.",
  "helpPrivacy.noAccounts": "ללא חשבונות",
  "helpPrivacy.noAccountsBody": "{app} לא דורש הרשמה, התחברות או חשבון.",
  "helpPrivacy.dataLocation": "מיקום נתונים",
  "helpPrivacy.dataLocationBody": "כל הקבצים תחת {dataDir}/ ב-macOS ו-Linux.",
  "helpPrivacy.network": "פעילות רשת",
  "helpPrivacy.noTelemetry": "ללא טלמטריה או אנליטיקה",
  "helpPrivacy.noTelemetryBody": "{app} לא אוסף נתוני שימוש, דוחות קריסה או מעקב.",
  "helpPrivacy.localWs": "שרת WebSocket מקומי בלבד",
  "helpPrivacy.localWsBody": "{app} מפעיל שרת WebSocket ברשת המקומית לשידור LAN.",
  "helpPrivacy.mdns": "שירות mDNS / Bonjour",
  "helpPrivacy.mdnsBody": "{app} רושם שירות mDNS לגילוי אוטומטי ברשת המקומית.",
  "helpPrivacy.updateChecks": "בדיקות עדכון",
  "helpPrivacy.updateChecksBody": "בלחיצה על 'בדוק עדכונים', {app} פונה לנקודת הקצה המוגדרת. זו הבקשה היחידה לאינטרנט.",
  "helpPrivacy.bluetooth": "Bluetooth ואבטחה",
  "helpPrivacy.ble": "Bluetooth Low Energy (BLE)",
  "helpPrivacy.bleBody": "{app} מתקשר עם מכשיר ה-BCI דרך BLE או USB סידורי באמצעות ערימת המערכת הסטנדרטית.",
  "helpPrivacy.osPermissions": "הרשאות מערכת",
  "helpPrivacy.osPermissionsBody": "גישת Bluetooth דורשת הרשאת מערכת מפורשת.",
  "helpPrivacy.deviceIds": "מזהי מכשיר",
  "helpPrivacy.deviceIdsBody": "מספר הסידורי וכתובת ה-MAC של אוזניות ה-BCI מאוחסנים רק באופן מקומי.",
  "helpPrivacy.onDevice": "עיבוד על המכשיר",
  "helpPrivacy.gpuLocal": "הסקת GPU נשארת מקומית",
  "helpPrivacy.gpuLocalBody": "מקודד ZUNA רץ לחלוטין על ה-GPU המקומי שלך.",
  "helpPrivacy.filtering": "סינון וניתוח",
  "helpPrivacy.filteringBody": "כל עיבוד האותות רץ מקומית על ה-CPU/GPU.",
  "helpPrivacy.nnSearch": "חיפוש שכנים קרובים",
  "helpPrivacy.nnSearchBody": "אינדקס HNSW נבנה ונשאל לחלוטין על המכשיר שלך.",
  "helpPrivacy.yourData": "הנתונים שלך, השליטה שלך",
  "helpPrivacy.access": "גישה",
  "helpPrivacy.accessBody": "כל הנתונים ב-{dataDir}/ בפורמטים סטנדרטיים (CSV, SQLite, HNSW בינארי).",
  "helpPrivacy.delete": "מחיקה",
  "helpPrivacy.deleteBody": "מחק כל קובץ תחת {dataDir}/ בכל עת. אין גיבויי ענן.",
  "helpPrivacy.export": "ייצוא",
  "helpPrivacy.exportBody": "הקלטות CSV ומסדי SQLite הם פורמטים ניידים סטנדרטיים.",
  "helpPrivacy.encrypt": "הצפנה",
  "helpPrivacy.encryptBody": "{app} לא מצפין נתונים במנוחה. השתמש בהצפנת דיסק של מערכת ההפעלה.",
  "helpPrivacy.activityTracking": "מעקב פעילות",
  "helpPrivacy.activityTrackingBody":
    "כשמופעל, NeuroSkill מתעד איזו אפליקציה בחזית ומתי המקלדת והעכבר שומשו לאחרונה. הנתונים נשארים לחלוטין במכשיר שלך ב-~/.skill/activity.sqlite — הם לעולם אינם נשלחים לשרת, מתועדים מרחוק או כלולים בניתוחים. מעקב חלון פעיל מלכד: שם אפליקציה, נתיב הפעלה, כותרת חלון וחותמת זמן. מעקב מקלדת/עכבר מלכד רק שתי חותמות זמן — לעולם לא הקשות, טקסט שהוקלד, קואורדינטות סמן או יעדי לחיצה. שתי הפונקציות ניתנות להשבתה בנפרד בהגדרות ← מעקב פעילות.",
  "helpPrivacy.activityPermission": "הרשאת נגישות (macOS)",
  "helpPrivacy.activityPermissionBody":
    "ב-macOS, מעקב מקלדת/עכבר דורש הרשאת נגישות כי הוא מתקין CGEventTap. אפל מחייבת הרשאה זו לכל אפליקציה שקוראת קלט גלובלי. ללא ההרשאה ה-hook נכשל בשקט: האפליקציה ממשיכה לעבוד נורמלית, רק חותמות הזמן נשארות באפס. מעקב חלון פעיל משתמש ב-osascript ואינו דורש הרשאת נגישות.",
  "helpPrivacy.summaryTitle": "סיכום",
  "helpPrivacy.summaryNoCloud": "ללא ענן. כל הנתונים נשמרים מקומית ב-{dataDir}/.",
  "helpPrivacy.summaryNoTelemetry": "ללא טלמטריה. ללא אנליטיקה או מעקב.",
  "helpPrivacy.summaryNoAccounts": "ללא חשבונות. ללא הרשמה או מזהים.",
  "helpPrivacy.summaryOneReq": "בקשת רשת אחת אופציונלית. בדיקות עדכון בלבד.",
  "helpPrivacy.summaryOnDevice": "לחלוטין על המכשיר. הסקת GPU, עיבוד אותות וחיפוש מקומי.",
  "helpPrivacy.summaryActivityLocal":
    "מעקב פעילות מקומי בלבד. מוקד החלונות וחותמות זמן הקלט נכתבים ל-activity.sqlite במכשיר שלך ואינם יוצאים ממנו לעולם.",

  "helpFaq.title": "שאלות נפוצות",
  "helpFaq.q1": "איפה הנתונים שלי מאוחסנים?",
  "helpFaq.a1": "הכל מאוחסן מקומית ב-{dataDir}/ — הקלטות CSV, אינדקסי HNSW, מסדי SQLite, תוויות, יומנים והגדרות.",
  "helpFaq.q2": "מה עושה מקודד ZUNA?",
  "helpFaq.a2": "ZUNA הוא מקודד transformer מואץ GPU שממיר תקופות EEG של 5 שניות לוקטורי הטמעה קומפקטיים.",
  "helpFaq.q3": "למה כיול דורש מכשיר מחובר?",
  "helpFaq.a3": "כיול מקליט נתוני EEG מתויגים. בלי שידור חי, אין אות עצבי לשיוך.",
  "helpFaq.q4": "איך אני מתחבר מ-Python / Node.js?",
  "helpFaq.a4": "גלה את פורט ה-WebSocket דרך mDNS, ואז פתח חיבור WebSocket רגיל. ראה לשונית API.",
  "helpFaq.q5": "מה מציינים מחווני איכות האות?",
  "helpFaq.a5": "כל נקודה מייצגת אלקטרודה. ירוק = קשר טוב. צהוב = ארטיפקט. אדום = קשר חלש. אפור = אין אות.",
  "helpFaq.q6": "אפשר לשנות את תדר מסנן הרשת?",
  "helpFaq.a6": "כן — הגדרות → עיבוד אותות: 50 Hz (אירופה) או 60 Hz (אמריקה, יפן).",
  "helpFaq.q7": "איך לאפס מכשיר מותאם?",
  "helpFaq.a7": "הגדרות → מכשירים מותאמים → × לשכוח.",
  "helpFaq.q8": "למה סמל המגש הופך לאדום?",
  "helpFaq.a8": "ה-Bluetooth כבוי. הפעל אותו בהגדרות המערכת.",
  "helpFaq.q9": "היישום מסתובב בלי להתחבר — מה לעשות?",
  "helpFaq.a9":
    "1. וודא שהמכשיר דלוק (Muse: החזק עד לרטט; Ganglion/Cyton: נורית כחולה). 2. היה בטווח 5 מ׳. 3. אם לא עובד, כבה והדלק מחדש.",
  "helpFaq.q10": "איך לתת הרשאת Bluetooth?",
  "helpFaq.a10": "macOS יציג דיאלוג הרשאה. אם דחית, לך להגדרות → פרטיות → Bluetooth.",
  "helpFaq.q11": "אילו מדדים נשמרים במסד הנתונים?",
  "helpFaq.a11":
    "כל 2.5 שניות נשמרים: וקטור ZUNA (32-D), עוצמות פס יחסיות (דלתא, תטא, אלפא, בטא, גמא, גמא-גבוהה) ממוצעות, עוצמות לפי ערוץ ב-JSON, ציוני נגזרים (מיקוד, הרפיה, מעורבות), FAA, יחסי בין-פסים (TAR, BAR, DTR), צורה ספקטרלית (PSE, APF, BPS, SNR), קוהרנטיות, דיכוי Mu, מדד מצב רוח וממוצעי PPG.",
  "helpFaq.q12": "מהי השוואת הפעלות?",
  "helpFaq.a12":
    "השווה (⌘⇧M) משווה שני טווחי זמן זה לצד זה: עמודות עוצמת פס עם הפרשים, כל הציונים והיחסים, FAA, היפנוגרמות שינה ו-Brain Nebula™ — הקרנת UMAP תלת-ממדית.",
  "helpFaq.q13": "מהו Brain Nebula™?",
  "helpFaq.a13":
    "Brain Nebula™ (טכנית: UMAP Embedding Distribution) מקרין הטמעות EEG רב-ממדיות לתלת-ממד כך שמצבי מוח דומים מופיעים כנקודות קרובות. טווח A (כחול) ו-B (ענבר) יוצרים אשכולות שונים. ניתן לסובב, לזום ולהקליק על נקודות מתויגות לצפייה בקשרים זמניים.",
  "helpFaq.q14": "למה Brain Nebula™ מציג ענן אקראי בהתחלה?",
  "helpFaq.a14":
    "הקרנת UMAP דורשת חישוב כבד ורצה בתור עבודות ברקע. בזמן החישוב מוצג ענן מציין מקום אקראי. כשההקרנה מוכנה, הנקודות מתנועעות בצורה חלקה למיקומן הסופי.",
  "helpFaq.q15": "מהן תוויות ואיך משתמשים בהן?",
  "helpFaq.a15":
    "תוויות הן תגיות שמשתמש מגדיר (למשל ״מדיטציה״, ״קריאה״) המשויכות לרגע מסוים. במציג UMAP, נקודות מתויגות מופיעות גדולות יותר עם טבעות צבעוניות — הקלק לראות חיבורים זמניים.",
  "helpFaq.q16": "מהי אסימטריית אלפא פרונטלית (FAA)?",
  "helpFaq.a16": "FAA = ln(AF8 α) − ln(AF7 α). ערכים חיוביים מצביעים על מוטיבציית גישה, שליליים על הימנעות.",
  "helpFaq.q17": "איך עובדת זיהוי שלבי שינה?",
  "helpFaq.a17":
    "כל תקופת EEG מסווגת כערות, N1, N2, N3 או REM לפי עוצמות יחסיות של דלתא, תטא, אלפא ובטא. תצוגת ההשוואה מציגה היפנוגרמה לכל הפעלה.",
  "helpFaq.q18": "מהם קיצורי המקלדת?",
  "helpFaq.a18": "⌘⇧O — פתיחת חלון {app}. ⌘⇧M — השוואת הפעלות. ניתן להתאמה בהגדרות → קיצורים.",
  "helpFaq.q19": "מהו ממשק WebSocket?",
  "helpFaq.a19":
    "{app} חושף ממשק WebSocket JSON ברשת המקומית (mDNS: _skill._tcp). פקודות: status, label, search, compare, sessions, sleep, umap (הכנסה לתור), umap_poll (שליפת תוצאה).",
  "helpFaq.q20": "מהם ציוני מיקוד, הרפיה ומעורבות?",
  "helpFaq.a20":
    "מיקוד = β/(α+θ), הרפיה = α/(β+θ), מעורבות = β/(α+θ) עם עקומה רכה יותר. כולם ממופים ל-0–100 דרך סיגמואיד.",
  "helpFaq.q21": "מהם TAR, BAR ו-DTR?",
  "helpFaq.a21":
    "TAR (תטא/אלפא) — גבוה יותר = נמנם יותר. BAR (בטא/אלפא) — גבוה יותר = מלחיץ/ממוקד יותר. DTR (דלתא/תטא) — גבוה יותר = שינה עמוקה יותר.",
  "helpFaq.q22": "מהם PSE, APF, BPS ו-SNR?",
  "helpFaq.a22":
    "PSE (אנטרופיה ספקטרלית, 0–1) — מורכבות ספקטרלית. APF (תדר שיא אלפא, Hz). BPS (שיפוע עוצמת פס) — מעריך 1/f. SNR (יחס אות לרעש, dB).",
  "helpFaq.q23": "מהו יחס תטא/בטא (TBR)?",
  "helpFaq.a23":
    "TBR הוא היחס בין עוצמת תטא מוחלטת לבטא מוחלטת. ערכים גבוהים יותר מצביעים על עוררות קורטיקלית מופחתת — TBR מוגבה קשור לנמנום ולדיסרגולציה קשבית. מקור: Angelidis et al. (2016).",
  "helpFaq.q24": "מהם פרמטרי Hjorth?",
  "helpFaq.a24":
    "שלוש תכונות בתחום הזמן מ-Hjorth (1970): פעילות (שונות האות / הספק כולל), ניידות (הערכת תדר ממוצע) ומורכבות (רוחב פס / סטייה מגל סינוס טהור). הם זולים חישובית ונפוצים בצנרת למידת מכונה של EEG.",
  "helpFaq.q25": "אילו מדדי מורכבות לא-לינאריים מחושבים?",
  "helpFaq.a25":
    "ארבעה מדדים: אנטרופיית תמורה (מורכבות דפוסים סידוריים, Bandt & Pompe 2002), ממד פרקטלי של Higuchi (מבנה פרקטלי של האות, Higuchi 1988), מעריך DFA (מתאמים זמניים ארוכי טווח, Peng et al. 1994) ואנטרופיית דגימה (סדירות אות, Richman & Moorman 2000). כולם ממוצעים על פני 4 ערוצי ה-EEG.",
  "helpFaq.q26": "מהם SEF95, מרכז כובד ספקטרלי, PAC ומדד לטרליות?",
  "helpFaq.a26":
    "SEF95 (תדר קצה ספקטרלי) הוא התדר שמתחתיו 95% מההספק הכולל — משמש בניטור הרדמה. מרכז הכובד הספקטרלי הוא התדר הממוצע המשוקלל בהספק (מדד עוררות). PAC (צימוד פאזה-אמפליטודה) מודד אינטראקציה צולבת בין תטא-גאמא הקשורה לקידוד זיכרון. מדד הלטרליות הוא אסימטריית הספק כללית שמאל/ימין על כל הפסים.",
  "helpFaq.q27": "אילו מדדי PPG מחושבים?",
  "helpFaq.a27":
    "ב-Muse 2/S (עם חיישן PPG): דופק (פעימות/דקה), RMSSD/SDNN/pNN50 (שונות דופק — טונוס פאראסימפתטי), יחס LF/HF (איזון סימפתטו-וגאלי), קצב נשימה (ממעטפת PPG), הערכת SpO₂ (לא מכויל), מדד זילוח (זרימת דם היקפית) ומדד הלחץ של Baevsky (לחץ אוטונומי).",
  "helpFaq.q28": "כיצד משתמשים בטיימר המיקוד?",
  "helpFaq.a28":
    'פתח את טיימר המיקוד דרך תפריט מגש המערכת, לוח הפקודות (⌘K ← "טיימר מיקוד") או קיצור המקשים הגלובלי (ברירת מחדל: ⌘⇧P). בחר פריסה — פומודורו (25/5), עבודה עמוקה (50/10) או מיקוד קצר (15/5) — או הגדר משכים מותאמים אישית. הפעל "תיוג EEG אוטומטי" כדי שהמערכת תסמן הקלטות EEG בתחילת וסוף כל שלב. ההגדרות נשמרות אוטומטית.',
  "helpFaq.q29": "כיצד מנהלים ועורכים הערות?",
  "helpFaq.a29":
    'פתח את חלון התוויות דרך לוח הפקודות (⌘K ← "כל התוויות"). הוא מציג את כל ההערות עם עריכת טקסט מוטמעת (לחץ על תווית, ⌘↵ לשמירה או Esc לביטול), מחיקה (עם אישור) ומטא-נתונים עם טווח הזמן של ה-EEG. השתמש בשורת החיפוש לסינון. התוויות מחולקות לדפים של 50 עבור ארכיונים גדולים.',
  "helpFaq.q30": "כיצד משווים שתי הפעלות זו לצד זו?",
  "helpFaq.a30":
    'בדף ההיסטוריה, לחץ על "השוואה מהירה" כדי להפעיל מצב השוואה. תיבות סימון יופיעו בכל שורת הפעלה — בחר בדיוק שתיים ולחץ על "השווה שנבחרו". לחלופין, פתח השוואה מהמגש או מלוח הפקודות ובחר הפעלות ידנית.',
  "helpFaq.q31": "כיצד פועל חיפוש ה-embedding הטקסטואלי?",
  "helpFaq.a31":
    'השאילתה שלך מומרת לוקטור על ידי אותו מודל sentence-transformer המאנדקס את התוויות שלך. הוקטור נחפש לאחר מכן באינדקס HNSW של התוויות בחיפוש שכנים קרובים מקורב. התוצאות הן ההערות שלך עצמך, מדורגות לפי דמיון סמנטי — חיפוש "רגוע וממוקד" יעלה תוויות כמו "קריאה עמוקה" או "מדיטציה" גם אם המילים הללו לא הופיעו בשאילתה שלך. דורש הורדת מודל ה-embedding ובנייה של אינדקס התוויות (הגדרות → Embeddings).',
  "helpFaq.q32": "כיצד פועל החיפוש האינטראקטיבי הרב-מודלי?",
  "helpFaq.a32":
    'החיפוש האינטראקטיבי מחבר טקסט, EEG וזמן בשאילתה אחת. שלב 1: שאילתת הטקסט מוטמעת. שלב 2: text-k התוויות הדומות ביותר מבחינה סמנטית נמצאות. שלב 3: עבור כל תווית, {app} מחשב את ה-embedding הממוצע של EEG לחלון ההקלטה שלה ומאחזר את eeg-k תקופות ה-EEG הקרובות ביותר מכל האינדקסים היומיים — מעבר משפה למרחב מצב-מוח. שלב 4: עבור כל רגע EEG שנמצא, הערות בטווח של ±reach דקות נאספות כ"תוויות שנמצאו". ארבע שכבות הצמתים (שאילתה → התאמות טקסט → שכני EEG → תוויות שנמצאו) מוצגות כגרף מכוון. ניתן לייצוא כ-SVG או כ-DOT.',
  "helpFaq.q37": "אילו לוחות OpenBCI NeuroSkill™ תומך בהם?",
  "helpFaq.a37":
    "NeuroSkill™ תומך בכל הלוחות במערכת האקולוגית של OpenBCI: Ganglion (4 ערוצים, BLE), Ganglion + WiFi Shield (4 ערוצים, 1 kHz), Cyton (8 ערוצים, USB dongle), Cyton + WiFi Shield (8 ערוצים, 1 kHz), Cyton+Daisy (16 ערוצים, USB), Cyton+Daisy + WiFi Shield (16 ערוצים, 1 kHz) ו-Galea (24 ערוצים, UDP). כל לוח יכול לפעול לצד מכשיר BCI אחר. בחר לוח בהגדרות → OpenBCI ולחץ חבר.",
  "helpFaq.q38": "כיצד מחברים את ה-Ganglion דרך Bluetooth?",
  "helpFaq.a38":
    "1. הפעל את ה-Ganglion — הנורית הכחולה צריכה להבהב לאט. 2. בהגדרות → OpenBCI בחר «Ganglion — 4 ערוצים · BLE». 3. שמור הגדרות ולחץ חבר. NeuroSkill™ יסרוק עד לזמן הקצוב שהוגדר (10 שניות כברירת מחדל). שמור את הלוח בטווח של 3–5 מ'. ב-macOS הענק הרשאת Bluetooth בשימוש הראשון.",
  "helpFaq.q39": "ה-Ganglion שלי מופעל אך NeuroSkill™ לא מוצא אותו — מה לעשות?",
  "helpFaq.a39":
    "1. בדוק שהנורית הכחולה מהבהבת (אם קבועה או כבויה — לחץ על הכפתור להתעוררות). 2. הגדל את זמן הקצוב לסריקת BLE בהגדרות. 3. קרב את הלוח לפחות מ-2 מ'. 4. סגור ופתח מחדש את NeuroSkill™ לאיפוס מתאם ה-BLE. 5. כבה והפעל מחדש את Bluetooth. 6. ודא שאין אפליקציה אחרת (OpenBCI GUI) כבר מחוברת — BLE מאפשר חיבור מרכזי אחד בלבד. 7. macOS 14+: בדוק הרשאת Bluetooth בהגדרות מערכת → פרטיות → Bluetooth.",
  "helpFaq.q40": "כיצד מחברים Cyton דרך USB?",
  "helpFaq.a40":
    "1. חבר את ה-USB radio dongle למחשב. 2. הפעל את ה-Cyton (הסט את המתג ל-PC). 3. בהגדרות → OpenBCI בחר «Cyton — 8 ערוצים · USB סדרתי». 4. לחץ רענן לרשימת יציאות ובחר את היציאה הנכונה (/dev/cu.usbserial-… ב-macOS, /dev/ttyUSB0 ב-Linux, COM3 ב-Windows) או השאר ריק לזיהוי אוטומטי. 5. שמור ולחץ חבר.",
  "helpFaq.q41": "היציאה הסדרתית אינה מופיעה או שאני מקבל שגיאת הרשאה — כיצד לתקן?",
  "helpFaq.a41":
    "macOS: ה-dongle מופיע כ-/dev/cu.usbserial-*. אם נעדר, התקן את מנהל ההתקן CP210x או FTDI VCP. Linux: הרץ sudo usermod -aG dialout $USER והתנתק/התחבר מחדש. בדוק שהמכשיר מופיע ב-/dev/ttyUSB0 לאחר החיבור. Windows: התקן מנהל התקן CP2104 USB-UART; יציאת COM תופיע במנהל ההתקנים תחת יציאות.",
  "helpFaq.q42": "כיצד משתמשים ב-WiFi Shield של OpenBCI?",
  "helpFaq.a42":
    "1. הנח את ה-WiFi Shield על גבי ה-Cyton או ה-Ganglion והפעל את הלוח. 2. חבר את המחשב לרשת ה-Wi-Fi של ה-Shield (SSID: OpenBCI-XXXX). 3. בהגדרות בחר את גרסת ה-WiFi המתאימה. 4. הזן IP 192.168.4.1 או השאר ריק לגילוי אוטומטי. 5. לחץ חבר. ה-WiFi Shield משדר ב-1000 Hz — הגדר מסנן עובר-נמוך ל-≤ 500 Hz.",
  "helpFaq.q43": "מהו לוח ה-Galea וכיצד מגדירים אותו?",
  "helpFaq.a43":
    "Galea של OpenBCI הוא אוזניות ביו-אותות מחקרי 24-ערוצים (EEG + EMG + AUX) המשדרות דרך UDP. 1. הפעל את Galea וחבר לרשת המקומית. 2. בהגדרות בחר «Galea — 24 ערוצים · UDP». 3. הזן כתובת IP או השאר ריק. 4. לחץ חבר. ערוצים 1–8 הם EEG; 9–16 EMG; 17–24 AUX. כל 24 נשמרים ב-CSV.",
  "helpFaq.q44": "האם ניתן להשתמש בשני מכשירי BCI בו-זמנית?",
  "helpFaq.a44":
    "כן — NeuroSkill™ יכול לקלוט זרם משני המכשירים בו-זמנית. המכשיר הראשון שמתחבר מניע את לוח המחוונים החי ואת צינור ה-ZUNA. נתוני המכשיר השני נשמרים ל-CSV לניתוח לא מקוון. תמיכה מלאה בניתוח רב-מכשירי בזמן אמת מתוכננת לגרסה עתידית.",
  "helpFaq.q45": "רק 4 מתוך 8 ערוצי ה-Cyton משמשים לניתוח חי — מדוע?",
  "helpFaq.a45":
    "צינור הניתוח בזמן אמת מתוכנן כרגע לקלט 4 ערוצים כדי להתאים לפורמט של Muse. עבור Cyton (8 ערוצים) ו-Cyton+Daisy (16 ערוצים), ערוצים 1–4 מזינים את הצינור החי; כל הערוצים נכתבים ל-CSV. תמיכה מלאה ברב-ערוצים נמצאת במפת הדרכים.",
  "helpFaq.q46": "כיצד משפרים את איכות האות בלוח OpenBCI?",
  "helpFaq.a46":
    "1. מרח ג'ל מוליך על כל נקודת אלקטרודה ופזר שיער לקבלת מגע ישיר עם הקרקפת. 2. בדוק עכבה עם OpenBCI GUI (מטרה: < 20 kΩ). 3. חבר את אלקטרודת SRB לאוסטיד (מאחורי האוזן). 4. השתמש בכבלים קצרים הרחק ממקורות חשמל. 5. הפעל מסנן notch בהגדרות → עיבוד אות (50 Hz לאירופה). 6. עבור Ganglion BLE: הרחק את הלוח מיציאות USB 3.0 הפולטות הפרעות ב-2.4 GHz.",
  "helpFaq.q47": "החיבור ל-OpenBCI מתנתק לעתים קרובות — כיצד לייצב אותו?",
  "helpFaq.a47":
    "Ganglion BLE: שמור את הלוח בטווח של 2 מ'; חבר את מתאם ה-BLE ליציאת USB 2.0 (USB 3.0 מפריע ל-2.4 GHz). Cyton USB: השתמש בכבל USB קצר ואיכותי המחובר ישירות למחשב. WiFi Shield: הימנע מחפיפה בין ערוץ 2.4 GHz של ה-Shield לנתב; מקם את המכשיר קרוב למחשב. בכלל: הימנע מהפעלת אפליקציות אלחוטיות אינטנסיביות (שיחות וידאו, סנכרון קבצים) בזמן ההקלטה.",
  "helpFaq.q48": "מה בדיוק מעקב הפעילות מתעד?",
  "helpFaq.a48":
    "מעקב חלון פעיל כותב שורה ל-activity.sqlite בכל פעם שהאפליקציה הקדמית או כותרת החלון משתנים: שם תצוגה של האפליקציה (למשל 'Safari', 'VS Code'), נתיב מלא לחבילה או לקובץ ההפעלה, כותרת החלון (עשויה להיות ריקה באפליקציות sandbox) וחותמת זמן Unix בשניות. מעקב מקלדת/עכבר כותב דגימה כל 60 שניות, אך רק אם הייתה פעילות מאז ההצבעה האחרונה: שתי חותמות זמן — אירוע מקלדת אחרון ואירוע עכבר/משטח מגע אחרון. לא מתועדות הקשות, טקסט שהוקלד, מיקומי סמן או יעדי לחיצה.",
  "helpFaq.q49": "מדוע macOS מבקש גישת נגישות למעקב קלט?",
  "helpFaq.a49":
    "מעקב מקלדת/עכבר משתמש ב-CGEventTap — ממשק API של macOS שמיירט אירועי קלט ברמת המערכת. אפל מחייבת הרשאת נגישות לכל אפליקציה שקוראת קלט גלובלי. ללא ההרשאה ה-tap נכשל בשקט: NeuroSkill ממשיך לעבוד נורמלית אך חותמות הזמן נשארות באפס. להענקת גישה: הגדרות מערכת ← פרטיות ואבטחה ← נגישות ← NeuroSkill ← הפעל. אם אינך רוצה להעניק אותה, פשוט השבת את 'מעקב אחר פעילות מקלדת ועכבר' בהגדרות — זה מונע כליל את התקנת ה-hook.",
  "helpFaq.q50": "כיצד למחוק נתוני מעקב פעילות?",
  "helpFaq.a50":
    "כל נתוני מעקב הפעילות נמצאים בקובץ אחד: ~/.skill/activity.sqlite. למחיקת הכל: לצאת מ-NeuroSkill, למחוק את הקובץ, ולהפעיל מחדש — מסד נתונים ריק נוצר אוטומטית. להפסקת איסוף עתידי ללא מחיקת נתונים קיימים, כבה את שני המתגים בהגדרות ← מעקב פעילות (נכנס לתוקף מיידית). למחיקה סלקטיבית של שורות, פתח את הקובץ בדפדפן SQLite ושתמש ב-DELETE FROM active_windows או DELETE FROM input_activity.",

  "helpTabs.tts": "Voice",

  "helpApi.cmdSay": "say",
  "helpApi.cmdSayParams": "text: string (נדרש)",
  "helpApi.cmdSayDesc":
    "הפעלת דיבור דרך TTS מקומי. Fire-and-forget — חוזר מיד בזמן שהשמע מתנגן ברקע. מאתחל את המנוע בקריאה הראשונה.",

  "helpFaq.q33": "איך מפעילים דיבור TTS מסקריפט?",
  "helpFaq.a33":
    'השתמשו ב-WebSocket או HTTP API. WebSocket: {"command":"say","text":"ההודעה שלך"}. HTTP: curl -X POST http://localhost:<port>/say -H \'Content-Type: application/json\' -d \'{"text":"ההודעה שלך"}\'. Fire-and-forget — מגיב מיד.',
  "helpFaq.q34": "למה אין צליל מ-TTS?",
  "helpFaq.a34":
    "ודאו ש-espeak-ng מותקן ובנתיב PATH. ודאו שהאודיו לא מושתק. בהרצה הראשונה המודל (~30 MB) חייב להוריד. הפעילו רישום TTS בהגדרות → קול.",
  "helpFaq.q35": "אפשר לשנות את קול או שפת ה-TTS?",
  "helpFaq.a35":
    "הגרסה הנוכחית משתמשת בקול Jasper (en-us) מ-KittenML/kitten-tts-mini-0.8. רק טקסט באנגלית מפונמז כראוי. קולות ושפות נוספים מתוכננים.",
  "helpFaq.q36": "האם TTS דורש חיבור לאינטרנט?",
  "helpFaq.a36":
    "רק פעם אחת, להורדת המודל הראשוני (~30 MB) מ-HuggingFace Hub. לאחר מכן כל הסינתזה פועלת לחלוטין ללא חיבור. המודל נשמר במטמון ב-~/.cache/huggingface/hub/ ונטען מחדש בכל הפעלה.",
  "helpFaq.q51": "מדוע {app} מבקשת הרשאת נגישות ב-macOS?",
  "helpFaq.a51":
    "{app} משתמשת ב-API של macOS CGEventTap לרישום חותמת הזמן האחרונה של לחיצת מקש או תנועת עכבר. הדבר משמש לחישוב חותמות זמן של פעילות בלוח מעקב הפעילות. נשמרות רק חותמות זמן — ללא הקשות מקשים, ללא מיקומי סמן. ללא הרשאה התכונה מושבתת בשקט.",
  "helpFaq.q52": "האם {app} זקוקה להרשאת בלוטות׳?",
  "helpFaq.a52":
    "כן. {app} משתמשת ב-Bluetooth Low Energy‏ (BLE) לחיבור למכשיר BCI. ב-macOS תופיע בקשת הרשאה חד-פעמית בניסיון הסריקה הראשון. ב-Linux וב-Windows לא נדרשת הרשאת בלוטות׳ מפורשת.",
  "helpFaq.q53": "כיצד מעניקים הרשאת נגישות ב-macOS?",
  "helpFaq.a53":
    "פתח הגדרות מערכת ← פרטיות ואבטחה ← נגישות. מצא את {app} ברשימה והפעל את המתג. ניתן גם ללחוץ על «פתח הגדרות נגישות» בכרטיסיית ההרשאות באפליקציה.",
  "helpFaq.q54": "מה קורה אם אני מסרב להרשאת נגישות?",
  "helpFaq.a54":
    "חותמות הזמן של פעילות מקלדת ועכבר לא יירשמו ויישארו אפס. כל שאר התכונות — שידור EEG, עוצמות רצועות, כיול, TTS, חיפוש — ממשיכות לפעול כרגיל. ניתן להשבית את התכונה לחלוטין בהגדרות ← מעקב פעילות.",
  "helpFaq.q55": "האם ניתן לבטל הרשאות לאחר מתן?",
  "helpFaq.a55":
    "כן. פתח הגדרות מערכת ← פרטיות ואבטחה ← נגישות (או התראות) והשבת את {app}. התכונה הרלוונטית תפסיק לפעול מיידית ללא צורך בהפעלה מחדש.",

  "helpTabs.dashboard": "לוח בקרה",
  "helpTabs.electrodes": "אלקטרודות",
  "helpTabs.settings": "הגדרות",
  "helpTabs.windows": "חלונות",
  "helpTabs.api": "API",
  "helpTabs.privacy": "פרטיות",
  "helpTabs.references": "מקורות",
  "helpTabs.faq": "שאלות נפוצות",

  "helpDash.mainWindow": "חלון ראשי",
  "helpDash.mainWindowDesc":
    "החלון הראשי הוא לוח הבקרה המרכזי. הוא מציג נתוני EEG בזמן אמת, מצב מכשיר ואיכות אות. הוא תמיד נגיש דרך שורת התפריטים.",
  "helpDash.statusHero": "כרטיס סטטוס",
  "helpDash.statusHeroBody":
    "הכרטיס העליון מציג את מצב החיבור החי של מכשיר ה-BCI שלכם. טבעת צבעונית ותג מציינים אם המכשיר מנותק, סורק, מחובר או אם הבלוטות׳ כבוי. כשמחובר, שם המכשיר, מספר סידורי וכתובת MAC מוצגים (לחצו לגלות/להסתיר).",
  "helpDash.battery": "סוללה",
  "helpDash.batteryBody":
    "סרגל התקדמות המציג את רמת הטעינה הנוכחית של אוזניות ה-BCI המחוברות. הצבע משתנה מירוק (גבוה) דרך כתום לאדום (נמוך) ככל שהטעינה יורדת.",
  "helpDash.signalQuality": "איכות אות",
  "helpDash.signalQualityBody":
    "ארבע נקודות צבעוניות — אחת לכל אלקטרודת EEG (TP9, AF7, AF8, TP10). ירוק = מגע עור טוב ורעש נמוך. צהוב = סביר (חפצי תנועה מסוימים). אדום = חלש (רעש גבוה / אלקטרודה רופפת). אפור = אין אות. האיכות מחושבת מחלון RMS מתגלגל על נתוני EEG גולמיים.",
  "helpDash.eegChannelGrid": "רשת ערוצי EEG",
  "helpDash.eegChannelGridBody":
    "ארבעה כרטיסים המציגים את ערך הדגימה האחרון (ב-µV) לכל ערוץ, בקידוד צבעים המתאים לתרשים גל הגלים למטה.",
  "helpDash.uptimeSamples": "זמן פעילות ודגימות",
  "helpDash.uptimeSamplesBody":
    "זמן פעילות סופר שניות שעון קיר מאז תחילת ההקלטה הנוכחית. דגימות הוא מספר דגימות EEG הגולמיות שהתקבלו מהאוזניות בהקלטה זו.",
  "helpDash.csvRecording": "הקלטת CSV",
  "helpDash.csvRecordingBody":
    "כשמחובר, מחוון REC מציג את שם קובץ ה-CSV הנכתב ל-{dataDir}/. דגימות EEG גולמיות (ללא סינון) נשמרות ברציפות — קובץ אחד להקלטה.",
  "helpDash.bandPowers": "עוצמות תדר",
  "helpDash.bandPowersBody":
    "תרשים עמודות חי המציג את העוצמה היחסית בכל פס תדר EEG סטנדרטי: דלתא (1–4 Hz), תטא (4–8 Hz), אלפא (8–13 Hz), בטא (13–30 Hz), וגמא (30–50 Hz). מתעדכן בכ-4 Hz מ-FFT עם חלון Hann של 512 דגימות. כל ערוץ מוצג בנפרד.",
  "helpDash.faa": "אסימטריית אלפא חזיתית (FAA)",
  "helpDash.faaBody":
    "מד מעוגן במרכז המציג את מדד אסימטריית אלפא חזיתית בזמן אמת: ln(AF8 α) − ln(AF7 α). ערכים חיוביים מצביעים על עוצמת אלפא ימנית-חזיתית גבוהה יותר, המקושרת למוטיבציית גישה של חצי הכדור השמאלי. ערכים שליליים מצביעים על נטיית נסיגה. הערך מוחלק עם ממוצע נע מעריכי ובדרך כלל נע בין −1 ל-+1. FAA נשמר יחד עם כל אפוכה של הטמעה בת 5 שניות ב-eeg.sqlite.",
  "helpDash.eegWaveforms": "גלי EEG",
  "helpDash.eegWaveformsBody":
    "תרשים תחום זמן גולל של אות EEG מסונן לכל ארבעת הערוצים. מתחת לכל גל יש סרט ספקטרוגרמה המציג את תוכן התדרים לאורך זמן. התרשים מציג את כ-4 השניות האחרונות.",
  "helpDash.gpuUtilisation": "ניצול GPU",
  "helpDash.gpuUtilisationBody":
    "תרשים קטן בראש החלון הראשי המציג ניצול מקודד ומפענח GPU. נראה רק כשמקודד הטמעות ZUNA פעיל. עוזר לאמת שצינור ה-wgpu פועל.",
  "helpDash.trayIconStates": "מצבי סמל מגש",
  "helpDash.trayGrey": "אפור — מנותק",
  "helpDash.trayGreyDesc": "בלוטות׳ דולק; אין מכשיר BCI מחובר.",
  "helpDash.trayAmber": "כתום — סורק",
  "helpDash.trayAmberDesc": "מחפש מכשיר BCI או מנסה להתחבר.",
  "helpDash.trayGreen": "ירוק — מחובר",
  "helpDash.trayGreenDesc": "משדר נתוני EEG חיים ממכשיר ה-BCI שלכם.",
  "helpDash.trayRed": "אדום — בלוטות׳ כבוי",
  "helpDash.trayRedDesc": "רדיו הבלוטות׳ כבוי. אין אפשרות לסרוק או להתחבר.",
  "helpDash.community": "קהילה",
  "helpDash.communityDesc":
    "הצטרפו לקהילת Discord של NeuroSkill כדי לשאול שאלות, לשתף משוב וליצור קשר עם משתמשים ומפתחים אחרים.",
  "helpDash.discordLink": "הצטרפו ל-Discord שלנו",

  "helpSettings.settingsTab": "לשונית הגדרות",
  "helpSettings.settingsTabDesc": "הגדרת העדפות מכשיר, עיבוד אות, פרמטרי הטמעה, כיול, קיצורי דרך ורישום.",
  "helpSettings.pairedDevices": "מכשירים מותאמים",
  "helpSettings.pairedDevicesBody":
    "רשימת כל מכשירי ה-BCI שהאפליקציה ראתה. ניתן להגדיר מכשיר מועדף (יעד חיבור אוטומטי), לשכוח מכשירים או לסרוק חדשים. עוצמת אות RSSI מוצגת למכשירים שנראו לאחרונה.",
  "helpSettings.signalProcessing": "עיבוד אות",
  "helpSettings.signalProcessingBody":
    "הגדרת שרשרת סינון EEG בזמן אמת: סף מסנן עליון (מסיר רעש בתדר גבוה), סף מסנן תחתון (מסיר סחיפת DC), ומסנן נוטש קו חשמל (מסיר זמזום 50 או 60 Hz והרמוניקות). שינויים חלים מיידית על תצוגת גל הגלים ועוצמות התדר.",
  "helpSettings.eegEmbedding": "הטמעת EEG",
  "helpSettings.eegEmbeddingBody":
    "כוונון החפיפה בין אפוכות הטמעה רצופות בנות 5 שניות. חפיפה גבוהה יותר פירושה יותר הטמעות לדקה (רזולוציה זמנית עדינה יותר בחיפוש) במחיר אחסון וחישוב נוספים.",
  "helpSettings.calibration": "כיול",
  "helpSettings.calibrationBody":
    'הגדרת משימת הכיול: תוויות פעולה (למשל "עיניים פקוחות", "עיניים עצומות"), משכי שלבים, מספר חזרות, והאם להתחיל כיול אוטומטית בהפעלת האפליקציה.',
  "helpSettings.globalShortcuts": "קיצורי דרך גלובליים",
  "helpSettings.globalShortcutsBody":
    "הגדרת קיצורי מקלדת מערכתיים לפתיחת חלונות תווית, חיפוש, הגדרות וכיול מכל אפליקציה. משתמש בפורמט מאיץ סטנדרטי (למשל CmdOrCtrl+Shift+L).",
  "helpSettings.debugLogging": "רישום ניפוי",
  "helpSettings.debugLoggingBody":
    "הפעלת/כיבוי רישום לפי תת-מערכת לקובץ היומן היומי ב-{dataDir}/logs/. תת-מערכות כוללות embedder, devices, websocket, csv, filter ו-bands.",
  "helpSettings.updates": "עדכונים",
  "helpSettings.updatesBody": "בדיקה והתקנה של עדכוני אפליקציה. משתמש במעדכן המובנה של Tauri עם אימות חתימה Ed25519.",
  "helpSettings.appearanceTab": "מראה",
  "helpSettings.appearanceTabBody":
    "בחירת מצב צבע (מערכת / בהיר / כהה), הפעלת ניגודיות גבוהה לגבולות וטקסט חזקים יותר, ובחירת ערכת צבעי תרשים לגלי EEG ותצוגות עוצמת תדר. פלטות בטוחות לעיוורי צבעים זמינות. שפה משתנה גם כאן דרך בוחר השפה.",
  "helpSettings.goalsTab": "יעדים",
  "helpSettings.goalsTabBody":
    "הגדרת יעד הקלטה יומי בדקות. סרגל התקדמות מופיע בלוח הבקרה בזמן שידור, והודעה מופעלת כשמגיעים ליעד. תרשים 30 הימים האחרונים מציג אילו ימים הושגו (ירוק), הושגה מחצית (כתום), התקדמות כלשהי (עמום), או פספוס (ללא).",
  "helpSettings.embeddingsTab": "הטמעות טקסט",
  "helpSettings.embeddingsTabBody":
    "בחירת מודל sentence-transformer המשמש להטמעת טקסט התוויות לחיפוש סמנטי. מודלים קטנים יותר (≤384 ממדים, למשל all-MiniLM-L6-v2) מהירים ומספיקים לחיפוש אישי. מודלים גדולים יותר מייצרים ייצוגים עשירים יותר במחיר גודל הורדה וזמן חישוב. המשקולות מורדות פעם אחת מ-HuggingFace ונשמרות מקומית. לאחר החלפת מודלים, הריצו הטמעה מחדש של כל התוויות.",
  "helpSettings.shortcutsTab": "קיצורי דרך",
  "helpSettings.shortcutsTabBody":
    "הגדרת קיצורי מקלדת גלובליים (מקשי קיצור מערכתיים) לפתיחת חלונות תווית, חיפוש, הגדרות וכיול. מציג גם את כל קיצורי הדרך בתוך האפליקציה (⌘K לפלטת פקודות, ? לשכבת קיצורים, ⌘↵ לשליחת תווית). קיצורים משתמשים בפורמט מאיץ סטנדרטי — למשל CmdOrCtrl+Shift+L.",
  "helpSettings.activitySection": "מעקב פעילות",
  "helpSettings.activitySectionDesc":
    "NeuroSkill יכול לרשום אופציונלית איזו אפליקציה בחזית ומתי המקלדת והעכבר שימשו לאחרונה. שתי התכונות כבויות כברירת מחדל, מקומיות לחלוטין, וניתנות להגדרה בנפרד בהגדרות → מעקב פעילות.",
  "helpSettings.activeWindowHelp": "מעקב חלון פעיל",
  "helpSettings.activeWindowHelpBody":
    'תהליכון רקע מתעורר כל שנייה ושואל את מערכת ההפעלה איזו אפליקציה בחזית כעת. כשהשם או כותרת החלון משתנים, שורה אחת מוכנסת ל-activity.sqlite: שם התצוגה של האפליקציה (למשל "Safari"), הנתיב המלא לחבילת האפליקציה או קובץ ההפעלה, כותרת החלון הקדמי (למשל שם המסמך או דף האינטרנט הנוכחי), וחותמת זמן Unix-שניות. אם נשארים באותו חלון, לא נכתבת שורה חדשה — זמן סרק באפליקציה בודדת לא מייצר פעילות מסד נתונים. ב-macOS המעקב משתמש ב-osascript; לא נדרשת הרשאת נגישות לשם ונתיב האפליקציה, אך כותרת החלון עשויה להיות ריקה לאפליקציות ב-sandbox. ב-Linux משתמש ב-xdotool ו-xprop (דורש מושב X11). ב-Windows משתמש בקריאת PowerShell GetForegroundWindow.',
  "helpSettings.inputActivityHelp": "מעקב פעילות מקלדת ועכבר",
  "helpSettings.inputActivityHelpBody":
    'הוק קלט גלובלי (rdev) מאזין לכל לחיצת מקש ואירוע עכבר או משטח מגע ברחבי המערכת. הוא לא מקליט מה הקלדתם, אילו מקשים לחצתם, או לאן הסמן זז — הוא רק מעדכן שתי חותמות זמן Unix-שניות בזיכרון: אחת לאירוע המקלדת האחרון ואחת לאירוע העכבר/משטח המגע האחרון. אלה נשטפים ל-activity.sqlite כל 60 שניות, אבל רק כשלפחות ערך אחד השתנה מאז השטיפה האחרונה, כך שתקופות סרק לא משאירות עקבות. לוח ההגדרות מקבל אירוע עדכון חי (מרוסן לפעם אחת בשנייה לכל היותר) כך ששדות "מקלדת אחרונה" ו"עכבר אחרון" משקפים פעילות בכמעט-זמן-אמת.',
  "helpSettings.activityStorageHelp": "היכן הנתונים מאוחסנים",
  "helpSettings.activityStorageHelpBody":
    "כל נתוני הפעילות נמצאים בקובץ SQLite בודד: ~/.skill/activity.sqlite. הם לעולם לא מועברים, מסונכרנים, או נכללים באנליטיקה כלשהי. שני טבלאות מתוחזקות: active_windows (שורה אחת לכל שינוי מיקוד חלון, עם שם אפליקציה, נתיב, כותרת וחותמת זמן) ו-input_activity (שורה אחת לכל שטיפת 60 שניות כשזוהתה פעילות, עם חותמות זמן מקלדת אחרונה ועכבר אחרון). לשני הטבלאות יש אינדקס יורד על עמודת חותמת הזמן. מצב יומן WAL מופעל כך שכתיבות רקע לעולם לא חוסמות קריאות. ניתן לפתוח, לבדוק, לייצא או למחוק את הקובץ בכל עת עם כל דפדפן SQLite.",
  "helpSettings.activityPermissionsHelp": "הרשאות מערכת הפעלה נדרשות",
  "helpSettings.activityPermissionsHelpBody":
    "macOS — מעקב חלון פעיל (שם ונתיב אפליקציה) לא דורש הרשאות מיוחדות. מעקב מקלדת ועכבר משתמש ב-CGEventTap הדורש גישת נגישות: פתחו הגדרות מערכת → פרטיות ואבטחה → נגישות, מצאו NeuroSkill ברשימה והפעילו. ללא הרשה זו ההוק נכשל בשקט — חותמות הזמן נשארות אפס ושאר האפליקציה לא מושפעת כלל. ניתן לכבות את המתג בהגדרות → מעקב פעילות למניעת בקשת ההרשאה. Linux — שתי התכונות דורשות מושב X11. מעקב חלון פעיל משתמש ב-xdotool ו-xprop, המותקנים מראש ברוב הפצות שולחן העבודה. מעקב קלט משתמש בהרחבת XRecord מ-libxtst. אם כלי כלשהו חסר, התכונה רושמת אזהרה ומכבה עצמה. Windows — לא נדרשות הרשאות מיוחדות. מעקב חלון פעיל משתמש ב-GetForegroundWindow דרך PowerShell; מעקב קלט משתמש ב-SetWindowsHookEx.",
  "helpSettings.activityDisablingHelp": "כיבוי ומחיקת נתונים",
  "helpSettings.activityDisablingHelpBody":
    "שני המתגים בהגדרות → מעקב פעילות חלים מיידית — לא נדרש הפעלה מחדש. כיבוי מעקב חלון פעיל עוצר הכנסת שורות חדשות ל-active_windows ומנקה את מצב החלון הנוכחי בזיכרון. כיבוי מעקב קלט עוצר את ה-callback של rdev מעדכון חותמות זמן ומונע שטיפות עתידיות ל-input_activity; שורות קיימות לא מוסרות אוטומטית. למחיקת כל ההיסטוריה שנאספה: צאו מהאפליקציה, מחקו ~/.skill/activity.sqlite, והפעילו מחדש. מסד נתונים ריק ייווצר אוטומטית בהפעלה הבאה.",
  "helpSettings.umapTab": "UMAP",
  "helpSettings.umapTabBody":
    "פרמטרי בקרה להטלת UMAP תלת-ממדית בהשוואת הקלטות: מספר שכנים (שולט על מבנה מקומי מול גלובלי), מרחק מינימלי (כמה צפוף האשכול), והמטריקה (קוסינוס או אוקלידית). מספרי שכנים גבוהים שומרים על יותר טופולוגיה גלובלית; מספרים נמוכים חושפים אשכולות מקומיים עדינים. ההטלות רצות במשימת רקע והתוצאות נשמרות במטמון.",
  "helpSettings.eegModelTab": "לשונית מודל EEG",

  "helpTabs.llm": "LLM",

  "helpLlm.overviewSection": "סקירה",
  "helpLlm.overviewSectionDesc":
    "NeuroSkill כולל שרת LLM מקומי אופציונלי — עוזר AI פרטי ותואם OpenAI ללא שליחת נתונים לענן.",
  "helpLlm.whatIsTitle": "מהי תכונת ה-LLM?",
  "helpLlm.whatIsBody":
    "תכונת ה-LLM מטמיעה שרת הסקה מבוסס llama.cpp באפליקציה. הוא מגיש נקודות קצה תואמות OpenAI (/v1/chat/completions, /v1/completions, /v1/embeddings, /v1/models, /health) על אותו פורט כמו ה-WebSocket API. כל לקוח תואם OpenAI יכול להתחבר.",
  "helpLlm.privacyTitle": "פרטיות ושימוש לא מקוון",
  "helpLlm.privacyBody":
    "כל ההסקה רצה מקומית. שום מידע לא עוזב את localhost. רק הורדת המודל הראשונית מ-HuggingFace Hub דורשת אינטרנט. לאחר מכן ניתן לעבוד לגמרי אופליין.",
  "helpLlm.compatTitle": "API תואם OpenAI",
  "helpLlm.compatBody":
    "השרת מדבר באותו פרוטוקול כמו OpenAI API. כל ספרייה עם פרמטר base_url (openai-python, openai-node, LangChain, LlamaIndex) עובדת מיד. הגדירו base_url ל-http://localhost:<port>/v1.",
  "helpLlm.modelsSection": "ניהול מודלים",
  "helpLlm.modelsSectionDesc": "דפדפו, הורידו והפעילו מודלי שפה מכומתים GGUF מהקטלוג המובנה.",
  "helpLlm.catalogTitle": "קטלוג מודלים",
  "helpLlm.catalogBody":
    "הקטלוג מפרט משפחות מודלים (Qwen, Llama, Gemma, Phi) עם וריאנטים של כימות. השתמשו בתפריט הנפתח לדפדוף. מודלים עם ★ הם ברירת המחדל המומלצת.",
  "helpLlm.quantsTitle": "רמות כימות",
  "helpLlm.quantsBody":
    "כל מודל זמין ברמות כימות GGUF שונות (Q4_K_M, Q5_K_M, Q6_K, Q8_0 וכו'). כימותים נמוכים קטנים ומהירים אך מקריבים איכות. Q4_K_M — הפשרה הטובה. Q8_0 כמעט ללא אובדן אך דורש כפליים זיכרון. BF16/F16/F32 ללא כימות.",
  "helpLlm.hardwareFitTitle": "תגי התאמת חומרה",
  "helpLlm.hardwareFitBody":
    "תג צבעוני לכל שורה: 🟢 מצוין — נכנס ל-VRAM עם מרווח. 🟡 טוב — מרווח צר. 🟠 צפוף — העברה חלקית ל-CPU. 🔴 לא נכנס. מתחשב ב-VRAM, RAM, גודל מודל ותקורת הקשר.",
  "helpLlm.visionTitle": "מודלים חזותיים / מולטימודליים",
  "helpLlm.visionBody":
    "משפחות Vision/Multimodal כוללות מקרין מולטימודלי אופציונלי (mmproj). הורידו את שניהם לאפשור קלט תמונות בצ'אט. המקרין מרחיב את מודל הטקסט — לא מודל עצמאי.",
  "helpLlm.downloadTitle": "הורדה ומחיקה",
  "helpLlm.downloadBody":
    "לחצו 'הורד' להורדת מודל מ-HuggingFace Hub. פס התקדמות מציג את המצב בזמן אמת. ניתן לבטל בכל עת. מודלים שהורדו נשמרים מקומית וניתן למחוק אותם. 'רענן מטמון' לסריקה מחדש.",
  "helpLlm.inferenceSection": "הגדרות הסקה",
  "helpLlm.inferenceSectionDesc": "כוונו את אופן טעינת המודלים והרצתם בשרת.",
  "helpLlm.gpuLayersTitle": "שכבות GPU",
  "helpLlm.gpuLayersBody":
    "מספר שכבות transformer שמועברות ל-GPU. 'הכל' למהירות מרבית, 0 ל-CPU בלבד. ערכים ביניים מחלקים בין GPU ל-CPU — שימושי כשהמודל חורג מעט מה-VRAM.",
  "helpLlm.ctxSizeTitle": "גודל הקשר",
  "helpLlm.ctxSizeBody":
    "גודל מטמון KV בטוקנים. 'אוטו' משתמש בברירת המחדל. הקשרים גדולים יותר שומרים יותר היסטוריה אך צורכים יותר זיכרון. בשגיאות זיכרון הקטינו ל-4K או 2K.",
  "helpLlm.parallelTitle": "בקשות מקביליות",
  "helpLlm.parallelBody":
    "מספר מרבי של לולאות פענוח מקביליות. ערכים גבוהים מאפשרים שיתוף שרת בין לקוחות אך מגדילים צריכת שיא. למשתמש יחיד 1 מספיק.",
  "helpLlm.apiKeyTitle": "מפתח API",
  "helpLlm.apiKeyBody":
    "טוקן Bearer אופציונלי לבקשות /v1/*. השאירו ריק לגישה פתוחה ב-localhost. הגדירו מפתח אם חושפים את הפורט ברשת מקומית.",
  "helpLlm.toolsSection": "כלים מובנים",
  "helpLlm.toolsSectionDesc": "הצ'אט יכול לקרוא לכלים מקומיים לאיסוף מידע או ביצוע פעולות בשמכם.",
  "helpLlm.toolsOverviewTitle": "איך כלים עובדים",
  "helpLlm.toolsOverviewBody":
    "המודל יכול לבקש להפעיל כלים בשיחה. האפליקציה מריצה אותם מקומית ומחזירה את התוצאה. כלים מופעלים רק על פי בקשה מפורשת — לעולם לא ברקע.",
  "helpLlm.toolsSafeTitle": "כלים בטוחים",
  "helpLlm.toolsSafeBody":
    "תאריך, מיקום, חיפוש אינטרנט, אחזור אינטרנט וקריאת קובץ — כלי קריאה בלבד. תאריך מחזיר תאריך ושעה. מיקום — גאולוקציה לפי IP. חיפוש — DuckDuckGo. אחזור — טקסט של URL ציבורי. קריאה — קבצים מקומיים עם עימוד.",
  "helpLlm.toolsDangerTitle": "כלים מורשים (⚠️)",
  "helpLlm.toolsDangerBody":
    "Bash, כתיבת קובץ ועריכת קובץ יכולים לשנות את המערכת. Bash מבצע פקודות shell. כתיבה יוצרת/דורסת קבצים. עריכה מבצעת חפש-והחלף. מושבתים כברירת מחדל עם תג אזהרה. הפעילו רק אם מבינים את הסיכונים.",
  "helpLlm.toolsExecModeTitle": "מצב ביצוע ומגבלות",
  "helpLlm.toolsExecModeBody":
    "מצב מקבילי קורא למספר כלים בו-זמנית (מהיר). מצב סדרתי מריץ אחד בכל פעם (בטוח יותר). 'מקסימום סיבובים' מגביל מעברי כלי/תוצאה. 'מקסימום קריאות לסיבוב' מגביל הפעלות מקביליות.",
  "helpLlm.chatSection": "צ'אט ויומנים",
  "helpLlm.chatSectionDesc": "עבדו עם המודל ועקבו אחרי פעילות השרת.",
  "helpLlm.chatWindowTitle": "חלון צ'אט",
  "helpLlm.chatWindowBody":
    "פתחו את הצ'אט מכרטיס שרת ה-LLM או ה-tray. ממשק עם Markdown, הדגשת קוד והדמיית קריאות כלים. שיחות זמניות — לא נשמרות. מודלים עם חזון מקבלים תמונות בגרירה או כפתור צרופה.",
  "helpLlm.chatApiTitle": "שימוש בלקוחות חיצוניים",
  "helpLlm.chatApiBody":
    "כוונו כל ממשק OpenAI ל-http://localhost:<port>/v1, הגדירו API key אם הוגדר ובחרו מודל מ-/v1/models. אפשרויות: Open WebUI, Chatbot UI, Continue (VS Code), curl/httpie.",
  "helpLlm.serverLogsTitle": "יומני שרת",
  "helpLlm.serverLogsBody":
    "מציג היומנים משדר את פלט השרת בזמן אמת: התקדמות טעינה, מהירות טוקנים, שגיאות. מצב 'מפורט' לאבחון llama.cpp. גלילה אוטומטית — ניתן לעצור בגלילה ידנית למעלה.",

  "helpTabs.hooks": "הוקים",

  "helpHooks.overviewSection": "סקירה",
  "helpHooks.overviewSectionDesc":
    "הוקים פרואקטיביים מפעילים אוטומטית פעולות כשדפוסי ה-EEG האחרונים תואמים למילות מפתח או מצבי מוח ספציפיים.",
  "helpHooks.whatIsTitle": "מה הם הוקים פרואקטיביים?",
  "helpHooks.whatIsBody":
    "הוק פרואקטיבי עוקב אחרי ה-embeddings של תוויות ה-EEG בזמן אמת. כשמרחק הקוסינוס יורד מתחת לסף, ההוק מופעל — פקודה, התראה, TTS או אירוע WebSocket. מאפשר אוטומציות נוירו-פידבק ללא כתיבת קוד.",
  "helpHooks.howItWorksTitle": "איך זה עובד",
  "helpHooks.howItWorksBody":
    "כל כמה שניות האפליקציה מחשבת embeddings של EEG ומשווה למילות מפתח דרך HNSW. זמן קירור מונע הפעלה חוזרת. הכל מקומי לחלוטין — שום מידע לא עוזב את המחשב.",
  "helpHooks.scenariosTitle": "תרחישים",
  "helpHooks.scenariosBody":
    "כל הוק מוגבל לתרחיש — קוגניטיבי, רגשי, פיזי או הכל. קוגניטיבי: ריכוז, הסחת דעת, עייפות נפשית. רגשי: מתח, רוגע, תסכול. פיזי: ישנוניות, עייפות גופנית. 'הכל' מתאים ללא קשר לקטגוריה.",
  "helpHooks.configSection": "הגדרת הוק",
  "helpHooks.configSectionDesc": "לכל הוק יש שדות ששולטים מתי ואיך הוא מופעל.",
  "helpHooks.nameTitle": "שם הוק",
  "helpHooks.nameBody":
    "שם תיאורי וייחודי להוק (לדוגמה 'Deep Work Guard', 'Calm Recovery'). משמש ביומן ההיסטוריה ובאירועי WebSocket.",
  "helpHooks.keywordsTitle": "מילות מפתח",
  "helpHooks.keywordsBody":
    "מילות מפתח או ביטויים קצרים שמתארים את מצב המוח (לדוגמה 'focus', 'deep work', 'stress', 'tired'). מוטמעים באותו מודל sentence-transformer. ההוק מופעל כשה-embeddings האחרונים קרובים.",
  "helpHooks.keywordSugTitle": "הצעות מילות מפתח",
  "helpHooks.keywordSugBody":
    "בזמן ההקלדה האפליקציה מציעה מונחים מהיסטוריית התוויות — התאמה מעורפלת וסמנטית. תג מקור: 'מעורפל', 'סמנטי' או שניהם. ↑/↓ ו-Enter לקבלה.",
  "helpHooks.distanceTitle": "סף מרחק",
  "helpHooks.distanceBody":
    "מרחק קוסינוס מרבי (0–1). ערכים נמוכים = מחמיר, גבוהים = מקל. טיפוסי: 0.08 (מאוד מחמיר) עד 0.25 (מקל). התחילו ב-0.12–0.16 וכוונו עם כלי ההצעה.",
  "helpHooks.distanceSugTitle": "כלי הצעת מרחק",
  "helpHooks.distanceSugBody":
    "לחצו 'הצע סף' לניתוח נתוני ה-EEG מול מילות המפתח. הכלי מחשב התפלגות מרחקים (min, p25, p50, p75, max) וממליץ על סף. סרגל אחוזונים חזותי. 'החל' להשתמש בערך.",
  "helpHooks.recentLimitTitle": "הפניות אחרונות",
  "helpHooks.recentLimitBody":
    "מספר דגימות embedding אחרונות להשוואה (ברירת מחדל: 12). ערכים גבוהים = החלקת שיאים, נמוכים = תגובתיות. טווח: 10–20.",
  "helpHooks.commandTitle": "פקודה",
  "helpHooks.commandBody":
    "מחרוזת פקודה אופציונלית באירוע ה-WebSocket (לדוגמה 'focus_reset', 'calm_breath'). כלי אוטומציה חיצוניים יכולים להגיב ולהפעיל פעולות או סקריפטים.",
  "helpHooks.textTitle": "טקסט תוכן",
  "helpHooks.textBody":
    "הודעה אופציונלית באירוע ההפעלה (לדוגמה 'קחו הפסקה של 2 דקות.'). מוצגת בהתראות וניתנת להקראה דרך TTS.",
  "helpHooks.advancedSection": "מתקדם",
  "helpHooks.advancedSectionDesc": "טיפים, היסטוריה ואינטגרציה עם כלים חיצוניים.",
  "helpHooks.examplesTitle": "דוגמאות מהירות",
  "helpHooks.examplesBody":
    "תבניות מוכנות: Deep Work Guard (ריכוז קוגניטיבי), Calm Recovery (הקלת מתח), Body Break (עייפות פיזית). לחצו להוספה עם ערכים מוכנים. התאימו לדפוסי ה-EEG שלכם.",
  "helpHooks.historyTitle": "היסטוריית הפעלות הוק",
  "helpHooks.historyBody":
    "יומן מתקפל המתעד כל הפעלה: חותמת זמן, תווית, מרחק קוסינוס, פקודה, מילות מפתח. לביקורת התנהגות, אימות סיפים וניפוי חיוביים שגויים. עימוד מובנה.",
  "helpHooks.wsEventsTitle": "אירועי WebSocket",
  "helpHooks.wsEventsBody":
    "אירוע JSON משודר דרך ה-WebSocket API: שם הוק, פקודה, טקסט, תווית, מרחק, חותמת זמן. לקוחות חיצוניים יכולים להאזין — עמעום אורות, השהיית מוזיקה, הודעת Slack וכו'.",
  "helpHooks.tipsTitle": "טיפים לכוונון",
  "helpHooks.tipsBody":
    "התחילו עם הוק אחד ומילות מפתח שתואמות לתוויות קיימות. השתמשו בכלי ההצעה לסף התחלתי. עקבו אחרי ההיסטוריה ליום וכוונו: הורידו סף לחיוביים שגויים, העלו אם לא מופעל. מילות מפתח ספציפיות משפרות דיוק.",
};

export default help;
