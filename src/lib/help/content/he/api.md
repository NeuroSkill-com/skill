# סקירה

## שידור חי
{app} משדר מדדי EEG מעובדים וסטטוס מכשיר דרך שרת WebSocket מקומי. אירועים: eeg-bands (~4 Hz), device-status (~1 Hz), label-created.

## פקודות
לקוחות יכולים לשלוח פקודות JSON דרך WebSocket: status, calibrate, label, search, sessions, compare, sleep, umap/umap_poll. תגובות ב-JSON עם שדה "ok" בוליאני.

# מדריך פקודות

## status
_(ללא)_

מחזיר מצב מכשיר, מידע סשן, מוני הטמעות ואיכות אות.

## calibrate
_(ללא)_

פותח חלון כיול. דורש מכשיר מחובר.

## label
text (מחרוזת, חובה); label_start_utc (u64, אופציונלי)

מכניס תווית מתויגת בזמן למסד הנתונים.

## search
start_utc, end_utc (u64, חובה); k, ef (u64, אופציונלי)

מחפש k שכנים קרובים באינדקס HNSW.

## compare
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, נדרש)

משווה שני טווחי זמן על ידי החזרת מדדי עוצמת פסים מצטברים (עוצמות יחסיות, ציוני מיקוד/הרפיה/מעורבות ו-FAA) עבור כל אחד. מחזיר { a: SessionMetrics, b: SessionMetrics }.

## sessions
_(ללא)_

מציג את כל הפגישות של embeddings ממסדי הנתונים היומיים. טווחי הקלטה רציפים (פער > 2 דק' = פגישה חדשה). החדשות ביותר ראשונות.

## sleep
start_utc, end_utc (u64, נדרש)

מסווג כל אפוק לשלב שינה (ער/N1/N2/N3/REM). מחזיר היפנוגרמה + סיכום.

## umap
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, נדרש)

מכניס לתור משימת הטלת UMAP תלת-ממדית. מחזיר job_id לבדיקה. לא חוסם.

## umap_poll
job_id (מחרוזת, נדרש)

בודק תוצאת משימת UMAP. מחזיר { status: pending | done, points?: [...] }.

## say
text: string (נדרש)

הפעלת דיבור דרך TTS מקומי. Fire-and-forget — חוזר מיד בזמן שהשמע מתנגן ברקע. מאתחל את המנוע בקריאה הראשונה.
