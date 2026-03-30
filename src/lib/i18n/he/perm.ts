// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** HE "perm" namespace translations. */
const perm: Record<string, string> = {
  "perm.intro":
    "{app} משתמשת במספר הרשאות מערכת אופציונליות כדי לאפשר תכונות כגון חותמות זמן לפעילות מקלדת/עכבר והתראות. כל הנתונים נשארים במכשיר שלך.",
  "perm.granted": "הוענקה",
  "perm.denied": "לא הוענקה",
  "perm.unknown": "לא ידוע",
  "perm.notRequired": "לא נדרשת",
  "perm.systemManaged": "מנוהל על ידי המערכת",
  "perm.accessibility": "נגישות",
  "perm.accessibilityDesc":
    "מעקב פעילות מקלדת ועכבר משתמש ב-CGEventTap (macOS) לרישום חותמות הזמן של הלחיצה האחרונה על מקש ותנועת העכבר האחרונה. לא נשמרות הקשות מקשים או מיקומי סמן — רק חותמות זמן של Unix. הדבר מחייב הרשאת נגישות ב-macOS.",
  "perm.accessibilityOk": "ההרשאה הוענקה. חותמות זמן של פעילות מקלדת ועכבר נרשמות.",
  "perm.accessibilityPending": "בודק סטטוס הרשאה…",
  "perm.howToGrant": "כיצד להעניק הרשאה זו:",
  "perm.accessStep1": "לחץ על «פתח הגדרות נגישות» למטה.",
  "perm.accessStep2": "מצא את {app} ברשימה (או לחץ על + להוספה).",
  "perm.accessStep3": "הפעל את המתג.",
  "perm.accessStep4": "חזור לכאן — הסטטוס יתעדכן אוטומטית.",
  "perm.openAccessibilitySettings": "פתח הגדרות נגישות",
  "perm.bluetooth": "בלוטות׳",
  "perm.bluetoothDesc":
    "בלוטות׳ משמש לחיבור למכשיר BCI שלך (Muse, MW75 Neuro, OpenBCI Ganglion, IDUN Guardian וכו׳). ב-macOS המערכת מציגה בקשת הרשאה חד-פעמית בסריקה הראשונה. ב-Linux וב-Windows לא נדרשת הרשאה נפרדת.",
  "perm.openBluetoothSettings": "פתח הגדרות בלוטות׳",
  "perm.notifications": "התראות",
  "perm.notificationsDesc":
    "התראות נשלחות כשמגיעים ליעד ההקלטה היומי וכשזמינה עדכון תוכנה. ב-macOS וב-Windows המערכת מבקשת הרשאה בפעם הראשונה שנשלחת התראה.",
  "perm.openNotificationsSettings": "פתח הגדרות התראות",
  "perm.matrix": "סיכום הרשאות",
  "perm.feature": "תכונה",
  "perm.matrixBluetooth": "בלוטות׳ (מכשיר BCI)",
  "perm.matrixKeyboardMouse": "חותמות זמן מקלדת ועכבר",
  "perm.matrixActiveWindow": "מעקב חלון פעיל",
  "perm.matrixNotifications": "התראות",
  "perm.matrixNone": "לא נדרשת הרשאה",
  "perm.matrixAccessibility": "נדרשת נגישות",
  "perm.matrixOsPrompt": "המערכת תבקש בשימוש ראשון",
  "perm.legendNone": "לא נדרשת הרשאה",
  "perm.legendRequired": "נדרשת הרשאת מערכת — ללא הרשאה התכונה מושבתת בשקט",
  "perm.legendPrompt": "המערכת תבקש בקריאה הראשונה",
  "perm.why": "מדוע {app} צריכה הרשאות אלה?",
  "perm.whyBluetooth": "בלוטות׳",
  "perm.whyBluetoothDesc": "לגילוי מכשיר BCI ולשידור נתונים ממנו דרך BLE.",
  "perm.whyAccessibility": "נגישות",
  "perm.whyAccessibilityDesc":
    "לחיתום אירועי מקלדת ועכבר לצורך הקשר פעילות. רק זמן האירוע נשמר — לעולם לא מה שנהקש או היכן היה הסמן.",
  "perm.whyNotifications": "התראות",
  "perm.whyNotificationsDesc": "להתרעה כשמגיעים ליעד ההקלטה היומי וכשעדכונים זמינים.",
  "perm.privacyNote":
    "כל הנתונים מאוחסנים מקומית במכשיר שלך ולא מועברים לשום שרת. ניתן להשבית כל תכונה בהגדרות ← מעקב פעילות.",
  "perm.screenRecording": "הקלטת מסך",
  "perm.screenRecordingDesc":
    "נדרש ללכידת חלונות יישומים אחרים עבור מערכת הטמעת צילומי מסך. macOS מסתיר תוכן חלונות ללא הרשאה זו.",
  "perm.screenRecordingOk": "הרשאת הקלטת מסך אושרה. לכידת צילומי מסך תפעל כהלכה.",
  "perm.screenRecordingStep1": "פתח הגדרות מערכת → פרטיות ואבטחה → הקלטת מסך ואודיו מערכת",
  "perm.screenRecordingStep2": "מצא את NeuroSkill™ ברשימה והפעל",
  "perm.screenRecordingStep3": "ייתכן שתצטרך לצאת ולהפעיל מחדש את היישום כדי שהשינוי ייכנס לתוקף",
  "perm.openScreenRecordingSettings": "פתח הגדרות הקלטת מסך",
  "perm.whyScreenRecording": "הקלטת מסך",
  "perm.whyScreenRecordingDesc":
    "ללכידת החלון הפעיל לחיפוש דמיון חזותי וקורלציית EEG חוצת-מודאליות. רק צילומי מסך שנבחרו נשמרים — אף פעם לא הקלטה רציפה.",
  "perm.matrixScreenRecording": "לכידת צילום מסך",
  "perm.matrixScreenRecordingReq": "נדרשת הקלטת מסך",
  "perm.calendar": "לוח שנה",
  "perm.calendarDesc": "כלי לוח השנה יכולים לקרוא אירועים כדי לספק הקשר תזמון. ההרשאה מתבקשת על ידי macOS בעת הצורך.",
  "perm.requestCalendarPermission": "בקש הרשאת לוח שנה",
  "perm.openCalendarSettings": "פתח הגדרות פרטיות לוח שנה",
  "perm.location": "שירותי מיקום",
  "perm.locationDesc":
    "ב-macOS, שירותי מיקום משתמשים ב-CoreLocation (סלולרי/Wi-Fi/GPS) למיקום מדויק. ב-Linux ו-Windows האפליקציה משתמשת בגיאולוקציה לפי IP ללא צורך בהרשאה. אם שירותי מיקום נדחים, האפליקציה עוברת אוטומטית לגיאולוקציה לפי IP.",
  "perm.locationOk": "הרשאת מיקום אושרה. CoreLocation ישמש למיקום מדויק.",
  "perm.locationFallback": "מיקום לא מורשה — האפליקציה תשתמש בגיאולוקציה לפי IP (דיוק ברמת עיר).",
  "perm.locationStep1": "פתח הגדרות מערכת → פרטיות ואבטחה → שירותי מיקום",
  "perm.locationStep2": "מצא את {app} ברשימה והפעל",
  "perm.locationStep3": "חזור לכאן — הסטטוס יתעדכן אוטומטית",
  "perm.requestLocationPermission": "בקש הרשאת מיקום",
  "perm.openLocationSettings": "פתח הגדרות מיקום",
  "perm.whyLocation": "מיקום",
  "perm.whyLocationDesc":
    "לספק הקשר מיקום מדויק ל-LLM ולשמור נתוני GPS לצד נתוני בריאות. עובר לגיאולוקציה לפי IP אם נדחה.",
  "perm.matrixLocation": "מיקום (GPS / IP)",
  "perm.matrixLocationReq": "שירותי מיקום (אופציונלי — נסיגה ל-IP)",
  "perm.openInputMonitoringSettings": "פתח הגדרות ניטור קלט",
  "perm.openFocusSettings": "פתח הגדרות ריכוז",
  "perm.whyCalendar": "לוח שנה",
  "perm.whyCalendarDesc": "כדי לספק הקשר תזמון לכלי הבינה המלאכותית כך שהעוזר יוכל להתייחס לאירועים הקרובים שלך.",
  "perm.matrixCalendar": "אירועי לוח שנה",
  "perm.matrixCalendarReq": "נדרשת גישה ללוח שנה",
};

export default perm;
