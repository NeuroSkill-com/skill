// SPDX-License-Identifier: GPL-3.0-only
/** HE — "validation" namespace. */
const validation: Record<string, string> = {
  "settingsTabs.validation": "אימות",
  "validation.title": "אימות ומחקר",
  "validation.intro":
    "כלי מחקר אופציונליים המכיילים את מאמן ההפסקות וציון הריכוז מול מדדים חיצוניים. אינם נדרשים לשימוש ב-NeuroSkill.",
  "validation.disclaimer":
    "כלי מחקר בלבד — לא מכשיר רפואי. לא אושר על ידי FDA, CE או כל גוף רגולטורי. לא לשימוש קליני.",

  "validation.master.title": "שערים גלובליים",
  "validation.master.respectFlow": "כבד מצב Flow",
  "validation.master.respectFlowDesc": "כשאתה נכנס ל-Flow, כל ההודעות מוסתרות. מופעל כברירת מחדל — השאר אותו כך.",
  "validation.master.quietBefore": "שעות שקטות התחלה",
  "validation.master.quietAfter": "שעות שקטות סוף",
  "validation.master.quietDesc": "שעון מקומי. אין הודעות מחוץ לחלון זה. התחלה = סוף משבית את השעות השקטות לחלוטין.",

  "validation.kss.title": "סולם הנמנום של קרולינסקה (KSS)",
  "validation.kss.desc": "דיווח עצמי של 5 שניות (1-9) על נמנום רגעי. מכייל את מאמן ההפסקות מול המצב הסובייקטיבי.",
  "validation.kss.enabled": "אפשר הודעות KSS",
  "validation.kss.maxPerDay": "מקס הודעות ביום",
  "validation.kss.minInterval": "מינימום דקות בין הודעות",
  "validation.kss.triggerBreakCoach": "הפעל כשמאמן ההפסקות מזהה עייפות",
  "validation.kss.triggerRandom": "הפעל דגימות בקרה אקראיות מדי פעם",
  "validation.kss.triggerRandomDesc": "נדרש לחישוב ROC/AUC — ללא מקרים שליליים נראה רק מקרים חיוביים של עייפות.",
  "validation.kss.randomWeight": "משקל דגימות אקראיות (0-1)",

  "validation.tlx.title": "NASA-TLX (עומס עבודה, 6 סולמות גולמיים)",
  "validation.tlx.desc": "דיווח עצמי של 60 שניות עם 6 תת-סולמות לאחר יחידת עבודה. מודד עומס — משלים לנמנום KSS.",
  "validation.tlx.enabled": "אפשר הודעות NASA-TLX",
  "validation.tlx.maxPerDay": "מקס הודעות ביום",
  "validation.tlx.minTaskMin": "אורך משימה מינימלי (דק) לשאלה",
  "validation.tlx.endOfDay": "הפעל גם סיכום עומס בסוף היום",

  "validation.tlx.form.title": "דרג את המשימה שזה עתה הסתיימה",
  "validation.tlx.mental": "דרישה מנטלית",
  "validation.tlx.physical": "דרישה פיזית",
  "validation.tlx.temporal": "דרישת זמן",
  "validation.tlx.performance": "ביצועים",
  "validation.tlx.effort": "מאמץ",
  "validation.tlx.frustration": "תסכול",

  "validation.pvt.title": "משימת ערנות פסיכומוטורית (PVT)",
  "validation.pvt.desc":
    "משימת זמן תגובה של 3 דקות. המדד האובייקטיבי של ערנות — איטי לאיסוף אבל האות החזק ביותר בספרות.",
  "validation.pvt.enabled": "אפשר תזכורות PVT שבועיות",
  "validation.pvt.weeklyReminder": "הצג תזכורת שורה אחת כשאין PVT השבוע",
  "validation.pvt.runNow": "הפעל PVT עכשיו (3 דק')",
  "validation.pvt.task.start": "התחל",
  "validation.pvt.task.cancel": "בטל",
  "validation.pvt.task.close": "סגור",

  "validation.eeg.title": "מדד עייפות EEG (Jap et al. 2009)",
  "validation.eeg.desc": "מחושב ברציפות מזרם עוצמת הפס כשאוזניות NeuroSkill מחוברות. נוסחה: (α + θ) / β. פסיבי — חינם.",
  "validation.eeg.enabled": "חשב מדד עייפות EEG",
  "validation.eeg.windowSecs": "חלון מתגלגל (שניות)",
  "validation.eeg.current": "ערך נוכחי",
  "validation.eeg.noHeadset": "אין אוזניות EEG בשידור",

  "validation.calibrationWeek.title": "שבוע כיול",
  "validation.calibrationWeek.desc":
    "פרץ אופציונלי של 7 ימים בתדירות דגימה גבוהה יותר. מעלה KSS ל-8 ביום, מפעיל TLX לאחר כל בלוק Flow ≥ 20 דק', מבקש PVT אחד באמצע השבוע. חוזר אוטומטית להגדרות הרגילות שלך ביום ה-8.",
  "validation.calibrationWeek.start": "התחל שבוע כיול",

  "validation.results.title": "תוצאות אחרונות",
  "validation.save.saved": "נשמר",
};
export default validation;
