# הנחיה קולית מקומית (TTS)

## הנחיה קולית מקומית (TTS)
NeuroSkill™ כולל מנוע דיבור מקומי לחלוטין. הוא מכריז על שלבי כיול בקול (תוויות, הפסקות, סיום) ומופעל דרך WebSocket או HTTP API. הסינתזה מקומית — אין צורך באינטרנט לאחר הורדת המודל (~30 MB).

## איך זה עובד
עיבוד טקסט → חלוקה למשפטים (≤400 תווים) → פונמיזציה באמצעות libespeak-ng (ספריית C, בתוך התהליך, קול en-us) → טוקניזציה (IPA → IDs) → הסקת ONNX (KittenTTS) → 1 שנייה שקט → השמעה דרך rodio.

## Model
KittenML/kitten-tts-mini-0.8 מ-HuggingFace Hub. קול: Jasper (en-us). 24,000 Hz מונו float32. ONNX מכומת INT8 — CPU בלבד. נשמר במטמון לאחר הורדה ראשונה.

## דרישות
espeak-ng חייב להיות מותקן ובנתיב PATH. macOS: brew install espeak-ng. Ubuntu/Debian: apt install libespeak-ng-dev. Alpine: apk add espeak-ng-dev. Fedora: dnf install espeak-ng-devel.

## שילוב בכיול
כשמתחיל כיול, המנוע מתחמם ברקע (מוריד את המודל במידת הצורך). בכל שלב נקרא tts_speak. הדיבור לעולם אינו חוסם את הכיול — כל הקריאות הן fire-and-forget.

## API — פקודת say
הפעילו דיבור מכל סקריפט או סוכן LLM. הפקודה חוזרת מיד. WebSocket: {"command":"say","text":"ההודעה שלך"}. HTTP: POST /say עם body {"text":"ההודעה שלך"}. CLI: curl -X POST http://localhost:<port>/say -d '{"text":"שלום"}' -H 'Content-Type: application/json'.

## רישום לניפוי באגים
הפעילו רישום TTS בהגדרות → קול כדי לכתוב אירועי סינתזה (טקסט, דגימות, השהיה) לקובץ הלוג של NeuroSkill™. שימושי למדידת השהיה ואבחון בעיות.

## בדקו כאן
השתמשו בווידג'ט למטה כדי לבדוק את מנוע ה-TTS מחלון עזרה זה.
