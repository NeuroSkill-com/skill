// SPDX-License-Identifier: GPL-3.0-only
/** HE "virtual-eeg" namespace — סימולטור מכשיר EEG וירטואלי. */
const virtualEeg: Record<string, string> = {
  "settingsTabs.virtualEeg": "EEG וירטואלי",

  "veeg.title": "מכשיר EEG וירטואלי",
  "veeg.desc": "סמלו אוזניות EEG לבדיקות, הדגמות ופיתוח. מייצר נתונים סינתטיים שעוברים דרך כל צינור עיבוד האות.",

  "veeg.status": "מצב",
  "veeg.running": "פועל",
  "veeg.stopped": "עצור",
  "veeg.start": "הפעלה",
  "veeg.stop": "עצירה",

  "veeg.channels": "ערוצים",
  "veeg.channelsDesc": "מספר אלקטרודות EEG לסימולציה.",
  "veeg.sampleRate": "קצב דגימה (Hz)",
  "veeg.sampleRateDesc": "דגימות לשנייה לכל ערוץ.",

  "veeg.template": "תבנית אות",
  "veeg.templateDesc": "בחר את סוג האות הסינתטי שייוצר.",
  "veeg.templateSine": "גלי סינוס",
  "veeg.templateSineDesc": "גלי סינוס נקיים ברצועות תדר סטנדרטיות (דלתא, תטא, אלפא, בטא, גמא).",
  "veeg.templateGoodQuality": "EEG באיכות טובה",
  "veeg.templateGoodQualityDesc": "EEG מנוחה מציאותי עם קצב אלפא דומיננטי ורעש ורוד ברקע.",
  "veeg.templateBadQuality": "EEG באיכות גרועה",
  "veeg.templateBadQualityDesc": "אות רועש עם ארטיפקטים של שרירים, רעש קו 50/60 Hz וקפיצות אלקטרודה.",
  "veeg.templateInterruptions": "חיבור לסירוגין",
  "veeg.templateInterruptionsDesc": "אות טוב עם נפילות תקופתיות המדמות אלקטרודות רופפות או הפרעות אלחוטיות.",
  "veeg.templateFile": "מקובץ",
  "veeg.templateFileDesc": "השמעה חוזרת של דגימות מקובץ CSV או EDF.",

  "veeg.quality": "איכות אות",
  "veeg.qualityDesc": "כוונן את יחס אות לרעש. גבוה יותר = אות נקי יותר.",
  "veeg.qualityPoor": "גרוע",
  "veeg.qualityFair": "סביר",
  "veeg.qualityGood": "טוב",
  "veeg.qualityExcellent": "מצוין",

  "veeg.chooseFile": "בחר קובץ",
  "veeg.noFile": "לא נבחר קובץ",
  "veeg.fileLoaded": "{name} ({channels} ערוצים, {samples} דגימות)",

  "veeg.advanced": "מתקדם",
  "veeg.amplitudeUv": "משרעת (µV)",
  "veeg.amplitudeDesc": "משרעת שיא-לשיא של האותות המיוצרים.",
  "veeg.noiseUv": "רצפת רעש (µV)",
  "veeg.noiseDesc": "משרעת RMS של רעש גאוסיאני תוספתי.",
  "veeg.lineNoise": "רעש קו",
  "veeg.lineNoiseDesc": "הוסף הפרעת רשת חשמלית של 50 Hz או 60 Hz.",
  "veeg.lineNoise50": "50 Hz",
  "veeg.lineNoise60": "60 Hz",
  "veeg.lineNoiseNone": "ללא",
  "veeg.dropoutProb": "הסתברות נפילה",
  "veeg.dropoutDesc": "סיכוי לנפילת אות לשנייה (0 = ללא, 1 = קבוע).",

  "veeg.preview": "תצוגה מקדימה של אות",
  "veeg.previewDesc": "תצוגה מקדימה חיה של 4 הערוצים הראשונים.",

  // ── חלון מכשירים וירטואליים ───────────────────────────────────────────────────
  "window.title.virtualDevices": "{app} – מכשירים וירטואליים",

  "vdev.title": "מכשירים וירטואליים",
  "vdev.desc":
    "בדוק את NeuroSkill ללא חומרת EEG פיזית. בחר תבנית מוגדרת מראש שמתאימה למכשיר אמיתי או הגדר מקור אות סינתטי משלך.",

  "vdev.presets": "תבניות מכשיר",
  "vdev.statusRunning": "מכשיר וירטואלי משדר",
  "vdev.statusStopped": "אין מכשיר וירטואלי פעיל",
  "vdev.selected": "מוכן",
  "vdev.configure": "הגדר",
  "vdev.customConfig": "הגדרה מותאמת אישית",

  "vdev.presetMuse": "Muse S",
  "vdev.presetMuseDesc": "פריסת סרט ראש 4 ערוצים — TP9, AF7, AF8, TP10.",
  "vdev.presetCyton": "OpenBCI Cyton",
  "vdev.presetCytonDesc": "אות מחקרי 8 ערוצים, מונטאז׳ פרונטלי/מרכזי מלא.",
  "vdev.presetCap32": "כובע EEG 32 ערוצים",
  "vdev.presetCap32Desc": "מערכת בינלאומית 10-20 מלאה, 32 אלקטרודות.",
  "vdev.presetAlpha": "אלפא חזק",
  "vdev.presetAlphaDesc": "קצב אלפא בולט של 10 Hz — קו בסיס רגוע עם עיניים עצומות.",
  "vdev.presetArtifact": "בדיקת ארטיפקטים",
  "vdev.presetArtifactDesc": "אות רועש עם ארטיפקטים של שרירים ורעש קו 50 Hz.",
  "vdev.presetDropout": "בדיקת נפילות",
  "vdev.presetDropoutDesc": "אובדן אות תקופתי המדמה אלקטרודות רופפות.",
  "vdev.presetMinimal": "מינימלי (ערוץ אחד)",
  "vdev.presetMinimalDesc": "גל סינוס בערוץ יחיד — העומס הקל ביותר האפשרי.",
  "vdev.presetCustom": "מותאם אישית",
  "vdev.presetCustomDesc": "הגדר מספר ערוצים, קצב, תבנית ורמת רעש משלך.",

  "vdev.lslSourceTitle": "מקור LSL וירטואלי",
  "vdev.lslRunning": "משדר EEG סינתטי דרך LSL",
  "vdev.lslStopped": "מקור LSL וירטואלי עצור",
  "vdev.lslDesc": "מפעיל מקור Lab Streaming Layer מקומי כדי לבדוק גילוי וחיבור של זרמי LSL.",
  "vdev.lslHint":
    'פתח הגדרות → לשונית LSL ולחץ על "סרוק רשת" כדי לראות את SkillVirtualEEG ברשימת הזרמים, ואז התחבר אליו.',
  "vdev.lslStarted": "מקור LSL וירטואלי משדר כעת ברשת המקומית.",

  // לוח מצב
  "vdev.statusSource": "מקור LSL",
  "vdev.statusSession": "הפעלה",
  "vdev.sessionConnected": "מחובר",
  "vdev.sessionConnecting": "מתחבר…",
  "vdev.sessionDisconnected": "מנותק",
  "vdev.startBtn": "הפעל מכשיר וירטואלי",
  "vdev.stopBtn": "עצור מכשיר וירטואלי",
  "vdev.autoConnect": "חיבור אוטומטי ללוח הבקרה",
  "vdev.autoConnectDesc": "חבר את לוח הבקרה למקור זה מיד לאחר ההפעלה.",

  // תצוגה מקדימה
  "vdev.previewOffline": "תצוגה מקדימה של אות (לא מקוון)",
  "vdev.previewOfflineDesc":
    "תצוגה מקדימה של צורת גל בצד הלקוח — מציגה את צורת האות לפני החיבור. עדיין לא נשלחים נתונים.",

  // תבנית מותאמת אישית — ערוצים / קצב
  "vdev.cfgChannels": "ערוצים",
  "vdev.cfgChannelsDesc": "מספר אלקטרודות EEG לסימולציה.",
  "vdev.cfgRate": "קצב דגימה",
  "vdev.cfgRateDesc": "דגימות לשנייה לכל ערוץ.",

  // תבנית מותאמת אישית — איכות אות
  "vdev.cfgQuality": "איכות אות",
  "vdev.cfgQualityDesc": "יחס אות לרעש. גבוה יותר = אות נקי יותר.",

  // תבנית מותאמת אישית — תבנית
  "vdev.cfgTemplate": "תבנית אות",
  "vdev.cfgTemplateSine": "גלי סינוס",
  "vdev.cfgTemplateSineDesc": "גלי סינוס טהורים בתדרי דלתא, תטא, אלפא, בטא וגמא.",
  "vdev.cfgTemplateGood": "EEG באיכות טובה",
  "vdev.cfgTemplateGoodDesc": "מצב מנוחה מציאותי עם אלפא דומיננטי ורעש ורוד ברקע.",
  "vdev.cfgTemplateBad": "EEG באיכות גרועה",
  "vdev.cfgTemplateBadDesc": "אות רועש עם ארטיפקטים של שרירים, רעש קו וקפיצות אלקטרודה.",
  "vdev.cfgTemplateInterruptions": "חיבור לסירוגין",
  "vdev.cfgTemplateInterruptionsDesc": "אות טוב עם נפילות תקופתיות המדמות אלקטרודות רופפות.",

  // תבנית מותאמת אישית — מתקדם
  "vdev.cfgAdvanced": "מתקדם",
  "vdev.cfgAmplitude": "משרעת (µV)",
  "vdev.cfgAmplitudeDesc": "משרעת שיא-לשיא של האות המדומה.",
  "vdev.cfgNoise": "רצפת רעש (µV)",
  "vdev.cfgNoiseDesc": "משרעת RMS של רעש גאוסיאני תוספתי ברקע.",
  "vdev.cfgLineNoise": "רעש קו",
  "vdev.cfgLineNoiseDesc": "הזרק הפרעת רשת חשמלית של 50 Hz או 60 Hz.",
  "vdev.cfgLineNoiseNone": "ללא",
  "vdev.cfgLineNoise50": "50 Hz",
  "vdev.cfgLineNoise60": "60 Hz",
  "vdev.cfgDropout": "הסתברות נפילה",
  "vdev.cfgDropoutDesc": "סיכוי לנפילת אות לשנייה (0 = אף פעם, 1 = קבוע).",
};

export default virtualEeg;
