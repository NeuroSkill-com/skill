// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** HE "onboarding" namespace translations. */
const onboarding: Record<string, string> = {
  "onboarding.title": "ברוכים הבאים ל-{app}",
  "onboarding.step.welcome": "ברוכים הבאים",
  "onboarding.step.bluetooth": "בלוטות'",
  "onboarding.step.fit": "בדיקת התאמה",
  "onboarding.step.calibration": "כיול",
  "onboarding.step.models": "מודלים",
  "onboarding.step.tray": "מגש",
  "onboarding.step.permissions": "הרשאות",
  "onboarding.step.extensions": "הרחבות",
  "onboarding.step.enable_bluetooth": "הפעלת Bluetooth",
  "onboarding.step.done": "סיום",
  "onboarding.newBadge": "חדש",
  "onboarding.fontSizeLabel": "גודל טקסט",
  "onboarding.fontSizeDecrease": "הקטנת גודל טקסט",
  "onboarding.fontSizeIncrease": "הגדלת גודל טקסט",
  "onboarding.welcomeBackTitle": "ברוכים השבים ל-{app}",
  "onboarding.whatsNewTitle": "מה חדש מאז ההתקנה האחרונה",
  "onboarding.whatsNewBody":
    'הוספנו כמה צעדים חדשים מאז שהפעלת את האשף בפעם הקודמת. ההגדרה הקיימת שלך (Bluetooth, כיול, מודלים) ללא שינוי — אפשר לדפדף במהירות. הצעדים החדשים מסומנים כאן ומתויגים "חדש" בסרגל ההתקדמות:',
  "onboarding.trayHint": "מצא את אייקון האפליקציה בשורת התפריטים / במגש",
  "onboarding.permissionsHint": "אופציונלי: לאפשר זיהוי אפליקציה פעילה, קבצים ולוח גזירה",
  "onboarding.extensionsHint": "אופציונלי: התקן עוזרי VS Code, דפדפן ו-shell",
  "onboarding.welcomeTitle": "ברוכים הבאים ל-{app}",
  "onboarding.welcomeBody": "{app} מקליט, מנתח ומאנדקס נתוני EEG מכל מכשיר BCI נתמך. בוא נגדיר הכל בכמה צעדים קצרים.",
  "onboarding.bluetoothHint": "חבר את מכשיר ה-BCI",
  "onboarding.fitHint": "בדוק את איכות מגע החיישנים",
  "onboarding.calibrationHint": "הפעל כיול מהיר",
  "onboarding.modelsHint": "הורד מודלי AI מקומיים מומלצים",
  "onboarding.bluetoothTitle": "חבר את מכשיר ה-BCI",
  "onboarding.bluetoothBody": "הפעל את מכשיר ה-BCI והנח אותו על הראש. {app} יחפש מכשירים בקרבת מקום ויתחבר אוטומטית.",
  "onboarding.btConnected": "מחובר ל-{name}",
  "onboarding.btScanning": "סורק…",
  "onboarding.btReady": "מוכן לסריקה",
  "onboarding.btScan": "סרוק",
  "onboarding.btInstructions": "איך להתחבר",
  "onboarding.btStep1": "הפעל את מכשיר ה-BCI (לחיצה ארוכה, מתג או כפתור בהתאם למכשיר).",
  "onboarding.btStep2": "הנח את המכשיר על הראש — החיישנים צריכים לנוח מאחורי האוזניים ועל המצח.",
  "onboarding.btStep3": "לחץ על סרוק למעלה. {app} ימצא ויתחבר אוטומטית למכשיר הקרוב ביותר.",
  "onboarding.btSuccess": "המכשיר מחובר! אפשר להמשיך.",
  "onboarding.fitTitle": "בדיקת התאמת המכשיר",
  "onboarding.fitBody": "מגע חיישנים טוב חיוני לנתוני EEG נקיים. כל ארבעת החיישנים צריכים להציג ירוק או צהוב.",
  "onboarding.sensorQuality": "איכות חיישנים בזמן אמת",
  "onboarding.quality.good": "טוב",
  "onboarding.quality.fair": "סביר",
  "onboarding.quality.poor": "חלש",
  "onboarding.quality.no_signal": "אין אות",
  "onboarding.fitNeedsBt": "חבר את המכשיר קודם כדי לראות נתוני חיישנים בזמן אמת.",
  "onboarding.fitTips": "טיפים למגע טוב יותר",
  "onboarding.fitTip1": "חיישני אוזן (TP9/TP10): הכנס מאחורי האוזניים וקצת מעליהן. הסר שיער שמכסה את החיישנים.",
  "onboarding.fitTip2": "חיישני מצח (AF7/AF8): צריכים לשכב שטוח על עור נקי — נגב עם מטלית יבשה אם צריך.",
  "onboarding.fitTip3": "אם המגע חלש, הרטב קלות את החיישנים עם אצבע לחה. זה משפר את המוליכות.",
  "onboarding.fitGood": "התאמה מצוינת! לכל החיישנים יש מגע טוב.",
  "onboarding.calibrationTitle": "הפעלת כיול",
  "onboarding.calibrationBody":
    "הכיול מקליט EEG מתויג בזמן שאתה מחליף בין שני מצבים מנטליים. זה עוזר ל-{app} ללמוד את דפוסי הבסיס של המוח שלך.",
  "onboarding.openCalibration": "פתח כיול",
  "onboarding.calibrationNeedsBt": "חבר את המכשיר קודם כדי להפעיל כיול.",
  "onboarding.calibrationSkip": "אפשר לדלג ולכייל מאוחר יותר מתפריט המגש או ההגדרות.",
  "onboarding.enableBluetoothTitle": "הפעל Bluetooth ב‑Mac שלך",
  "onboarding.enableBluetoothBody":
    "{app} זקוק להפעלת מתאם ה‑Bluetooth במחשב כדי למצוא ולחבר את מכשיר ה‑BCI. אנא הפעל את ה‑Bluetooth בהגדרות המערכת אם הוא כבוי.",
  "onboarding.enableBluetoothStatus": "מתאם Bluetooth",
  "onboarding.enableBluetoothHint":
    "פתח את הגדרות ה‑Bluetooth והפעל אותו. אם אתה מריץ בפיתוח דרך ה‑Terminal, וודא שהמתאם המערכת מופעל.",
  "onboarding.enableBluetoothOpen": "פתח הגדרות Bluetooth",
  "onboarding.modelsTitle": "הורדת מודלים מומלצים",
  "onboarding.modelsBody":
    "לחוויה המקומית הטובה ביותר, הורד עכשיו את ברירות המחדל הבאות: Qwen3.5 4B (Q4_K_M), מקודד ZUNA, NeuTTS ו-Kitten TTS.",
  "onboarding.models.downloadAll": "הורד את הסט המומלץ",
  "onboarding.models.download": "הורד",
  "onboarding.models.downloading": "מוריד…",
  "onboarding.models.downloaded": "הורד",
  "onboarding.models.qwenTitle": "Qwen3.5 4B (Q4_K_M)",
  "onboarding.models.qwenDesc":
    "מודל צ'אט מומלץ. משתמש ב-Q4_K_M לאיזון הטוב ביותר בין איכות למהירות ברוב המחשבים הניידים.",
  "onboarding.models.zunaTitle": "מקודד EEG של ZUNA",
  "onboarding.models.zunaDesc": "נדרש להטמעות EEG, היסטוריה סמנטית ואנליטיקת מצבי מוח.",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc": "מנוע קול רב-לשוני מומלץ עם איכות טובה יותר ותמיכה בשכפול.",
  "onboarding.models.kittenTitle": "Kitten TTS",
  "onboarding.models.kittenDesc": "מנוע קול קליל ומהיר, שימושי כגיבוי מהיר ולמערכות עם משאבים מוגבלים.",
  "onboarding.trayTitle": "מצא את האפליקציה במגש",
  "onboarding.trayBody":
    "{app} פועלת ברקע. לאחר ההגדרה, הסמל בשורת התפריטים (macOS) או במגש המערכת (Windows/Linux) הוא הדרך לחזור לאפליקציה.",
  "onboarding.tray.states": "הסמל משנה צבע בהתאם למצב:",
  "onboarding.tray.grey": "אפור — מנותק",
  "onboarding.tray.amber": "ענבר — סורק או מתחבר",
  "onboarding.tray.green": "ירוק — מחובר ומקליט",
  "onboarding.tray.red": "אדום — Bluetooth כבוי",
  "onboarding.tray.open": "לחץ על סמל המגש בכל עת להצגה או הסתרה של הלוח.",
  "onboarding.tray.menu": "לחיצה ימנית (או שמאלית ב-Windows/Linux) פותחת תפריט מהיר — חיבור, תיוג, כיול ועוד.",
  "onboarding.extensionsTitle": "הרחבות נלוות",
  "onboarding.extensionsBody":
    "{app} יכולה למשוך הקשר נוסף מהעורך, הדפדפן והטרמינל. כל אינטגרציה היא חתיכה נפרדת שאפשר להתקין או לדלג עליה — אף אחת לא נדרשת לפעולת ה-EEG.",
  "onboarding.extensionsPrivacy":
    "אותה ערובת פרטיות כמו לכל השאר: כל הרחבה מדווחת ל-daemon המקומי דרך פורט localhost, והנתונים נכתבים ל-activity.sqlite במחשב הזה. שום דבר לא נשלח ל-NeuroSkill או למישהו אחר.",
  "onboarding.extensionsSkip":
    "הכל אופציונלי. אפשר להתקין, לעדכן או להסיר כל אחד מאלה מאוחר יותר ב-הגדרות → הרחבות וב-הגדרות → טרמינל.",
  "onboarding.extensions.vscodeTitle": "עורך מבוסס VS Code",
  "onboarding.extensions.vscodeDesc":
    "מוסיף מעקב עריכה לפי קובץ, הצעות AI inline ושילוב עם dev loop. עובד עם VS Code, VSCodium, Cursor, Windsurf, Trae, Positron — כל fork מותקן מזוהה אוטומטית.",
  "onboarding.extensions.browserTitle": "הרחבת דפדפן",
  "onboarding.extensions.browserDesc":
    "מתעד לשונית פעילה, זמן פוקוס דף ודפוסי קריאה מהדפדפן שלך. תמיכת sideload ל-Chrome, Firefox, Edge ו-Safari (Safari דורש שלב חתימה נוסף).",
  "onboarding.extensions.terminalTitle": "Hooks של טרמינל / shell",
  "onboarding.extensions.terminalDesc":
    "מוסיף hook קטן של preexec/precmd ל-shell שלך כדי שהאפליקציה תוכל לקשר זמני פקודות עם מצב הריכוז. בחר zsh, bash, fish או PowerShell — משנה את קובץ ה-rc שלך עם שורת source אחת בלבד, ניתן להסיר לחלוטין מאוחר יותר.",

  "onboarding.permissionsTitle": "מעקב פעילות אופציונלי",
  "onboarding.permissionsBody":
    '{app} יכולה לתעד במה עבדת כדי לקשר את נתוני ה-EEG והפוקוס שלך עם ההקשר האמיתי — "איבדתי ריכוז בזמן כתיבת ה-PR הזה" במקום סתם "איבדתי ריכוז ב-15:00". כבוי כברירת מחדל ולחלוטין אופציונלי.',
  "onboarding.permissionsPrivacy":
    "הכל נשאר על המחשב הזה. הפעילות המתועדת נכתבת לקובץ activity.sqlite מקומי ולעולם לא נשלחת לשום שרת — לא ל-NeuroSkill, לא לאף אחד. אפשר לכבות כל אפשרות בכל עת; הנתונים שכבר נרשמו נשארים על הדיסק עד שתמחק אותם.",
  "onboarding.permissionsSkip": "הכל כבוי כברירת מחדל. אפשר להפעיל כל אחד מאלה מאוחר יותר ב-הגדרות → מעקב פעילות.",
  "onboarding.permissionsActiveWindowDesc":
    "מתעד את האפליקציה בחזית, כותרת החלון, לשונית הדפדפן הפעילה ונתיב הקובץ הפתוח בעורך. macOS תבקש גישת נגישות / אוטומציה לכל דפדפן ועורך.",
  "onboarding.permissionsInputDesc":
    "מתעד רק חותמות זמן של שימוש במקלדת/עכבר — לעולם לא אילו מקשים, לעולם לא מיקומים, לעולם לא תוכן. לא נדרשת הרשאת מערכת.",
  "onboarding.permissionsFileDesc":
    "צופה ב-Documents, Desktop, Downloads ובתיקיות פיתוח נפוצות לאירועי יצירה/שינוי/מחיקה. מתעד רק נתיבים וחותמות זמן — תוכן הקבצים לעולם לא נקרא. macOS עשויה לבקש גישה מלאה לדיסק.",
  "onboarding.permissionsScreenshotsDesc":
    'מצלם את המסך במרווחים, מריץ OCR על טקסט ומאנדקס את שניהם לחיפוש חזותי ולשאילתות "מה היה על המסך שלי ב-15:00". macOS מבקשת הקלטת מסך. כיוון מרווח, איכות ו-OCR בהגדרות → צילומי מסך.',
  "onboarding.permissionsLocationDesc":
    "מתעד את מיקום המכשיר לצד בלוקי ריכוז (בית מול משרד מול בית קפה) כך שמעברי מקום יוכלו להיות מקושרים למצב הריכוז שלך. macOS מבקשת שירותי מיקום. נשמר מקומית; לעולם לא מועלה.",
  "onboarding.permissionsCalendarDesc":
    "קורא מטא-נתונים של אירועי לוח שנה (כותרת, זמן, משך, מספר משתתפים) כדי לקשר צפיפות פגישות עם ירידות ריכוז. macOS מבקשת גישה ללוח שנה בשימוש הראשון. תוכן האירועים לעולם לא מועלה.",
  "onboarding.permissionsClipboardDesc":
    "מתעד מתי לוח הגזירה משתנה (איזו אפליקציה, סוג תוכן, גודל). התוכן לעולם לא נקרא. macOS בלבד; יבקש גישת אוטומציה.",
  "onboarding.downloadsComplete": "כל ההורדות הושלמו!",
  "onboarding.downloadsCompleteBody":
    "המודלים המומלצים הורדו והם מוכנים לשימוש. כדי להוריד מודלים נוספים או להחליף לאחרים, פתח את",
  "onboarding.downloadMoreSettings": "הגדרות האפליקציה",
  "onboarding.doneTitle": "הכל מוכן!",
  "onboarding.doneBody": "{app} פועל בשורת התפריט שלך. הנה כמה דברים שכדאי לדעת:",
  "onboarding.doneTip.tray": "{app} נמצא בשורת התפריט. לחץ על הסמל כדי להציג/להסתיר את לוח המחוונים.",
  "onboarding.doneTip.shortcuts": "השתמש ב-⌘K לפתיחת פלטת הפקודות, או ? לצפייה בכל קיצורי המקשים.",
  "onboarding.doneTip.help": "פתח עזרה מתפריט המגש לקבלת הפניה מלאה של כל התכונות.",
  "onboarding.back": "חזרה",
  "onboarding.next": "הבא",
  "onboarding.getStarted": "בואו נתחיל",
  "onboarding.finish": "סיום",
  "onboarding.models.ocrTitle": "OCR Models",
  "onboarding.models.ocrDesc":
    "Text detection + recognition models for extracting text from screenshots. Enables text search across captured screens (~10 MB each).",
  "onboarding.screenRecTitle": "הרשאת הקלטת מסך",
  "onboarding.screenRecDesc":
    "נדרשת ב-macOS ללכידת חלונות יישומים אחרים עבור מערכת צילומי המסך. ללא הרשאה זו, צילומי המסך עלולים להיות ריקים.",
  "onboarding.screenRecOpen": "פתח הגדרות",
};

export default onboarding;
