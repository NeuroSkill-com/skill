# Übersicht

## Live-Streaming
{app} überträgt abgeleitete EEG-Metriken und Gerätestatus über einen lokalen WebSocket-Server. Events: eeg-bands (~4 Hz — 60+ Scores), device-status (~1 Hz), label-created.

## Befehle
Clients können JSON-Befehle über WebSocket senden: status, calibrate, label, search, sessions, compare, sleep, umap/umap_poll. Antworten als JSON mit "ok"-Boolean.

# Befehlsreferenz

## status
_(keine)_

Gibt Gerätestatus, Sitzungsinfo, Embedding-Zähler und Signalqualität zurück.

## calibrate
_(keine)_

Öffnet das Kalibrierungsfenster. Erfordert ein verbundenes Gerät.

## label
text (String, erforderlich); label_start_utc (u64, optional)

Fügt ein zeitgestempeltes Label in die Datenbank ein.

## search
start_utc, end_utc (u64, erforderlich); k, ef (u64, optional)

Sucht die k nächsten Nachbarn im HNSW-Index.

## compare
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, erforderlich)

Vergleicht zwei Zeiträume, indem aggregierte Bandleistungsmetriken (relative Leistungen, Fokus-/Entspannungs-/Engagement-Werte und FAA) für jeden zurückgegeben werden. Gibt { a: SessionMetrics, b: SessionMetrics } zurück.

## sessions
_(keine)_

Listet alle Embedding-Sitzungen aus den täglichen Datenbanken auf. Zusammenhängende Aufnahmebereiche (Lücke > 2 Min = neue Sitzung). Neueste zuerst.

## sleep
start_utc, end_utc (u64, erforderlich)

Klassifiziert jede Epoche in ein Schlafstadium (Wach/N1/N2/N3/REM). Gibt Hypnogramm + Zusammenfassung zurück.

## umap
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, erforderlich)

Stellt einen 3D-UMAP-Projektionsauftrag in die Warteschlange. Gibt job_id zum Abfragen zurück. Nicht-blockierend.

## umap_poll
job_id (String, erforderlich)

Fragt ein UMAP-Ergebnis ab. Gibt { status: pending | done, points?: [...] } zurück.

## say
text: string (erforderlich)

Text per geräteeigener TTS sprechen. Fire-and-Forget — kehrt sofort zurück, während Audio im Hintergrund abgespielt wird. Initialisiert die TTS-Engine beim ersten Aufruf.
