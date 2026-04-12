## Wie wird ein Hook ausgelöst?
Der Worker vergleicht jedes neue EEG-Embedding mit aktuellen Label-Exemplaren aus Keyword- und Text-Ähnlichkeit. Liegt die beste Kosinus-Distanz unter deinem Schwellwert, wird der Hook ausgelöst.

## Warum wird das Tray-Icon rot?
Bluetooth ist ausgeschaltet. Aktivieren Sie es in Systemeinstellungen → Bluetooth.

## Die App dreht sich, verbindet sich aber nie?
1. BCI-Gerät einschalten (Muse: Taste halten; Ganglion/Cyton: blaue LED). 2. Innerhalb von 5 m bleiben. 3. Bei Bedarf aus- und wieder einschalten.

## Wie erteile ich die Bluetooth-Berechtigung?
Systemeinstellungen → Datenschutz & Sicherheit → Bluetooth → {app} aktivieren.

## Kann ich EEG-Daten im Netzwerk empfangen?
Ja — abgeleitete Metriken (~4 Hz) und Status (~1 Hz) über WebSocket. Rohdaten werden nicht übertragen.

## Wo werden meine Aufnahmen gespeichert?
In {dataDir}/ — CSV, SQLite, HNSW-Dateien nach Datum geordnet.

## Was bedeuten die Signalqualitäts-Punkte?
Grün = guter Kontakt. Gelb = mäßig. Rot = schlecht. Grau = kein Signal.

## Was ist der Netzfrequenz-Kerbfilter?
Entfernt 50/60 Hz Netzrauschen aus der Anzeige.

## Welche Metriken werden gespeichert?
Bandleistungen, abgeleitete Scores, FAA, Verhältnisse, Spektralform, Kohärenz, Hjorth, Komplexität, PPG-Metriken — pro 2,5-s-Epoche.

## Was ist Sitzungsvergleich?
Vergleicht zwei Sitzungen nebeneinander: Bandleistungen, Scores, FAA, Schlaf, UMAP.

## Was ist der 3D-UMAP-Viewer?
Projiziert EEG-Embeddings in 3D, damit ähnliche Hirnzustände clustern.

## Warum zeigt UMAP eine zufällige Wolke?
UMAP läuft im Hintergrund. Platzhalter wird angezeigt, bis die Projektion fertig ist.

## Was sind Labels?
Benutzerdefinierte Tags, die an Momente während der Aufnahme angehängt werden.

## Was ist FAA?
ln(AF8 α) − ln(AF7 α). Positiv = Annäherungsmotivation, negativ = Rückzug.

## Wie funktioniert Schlaf-Staging?
Klassifiziert Epochen als Wach/N1/N2/N3/REM anhand von Bandleistungsverhältnissen.

## Tastenkombinationen?
⌘⇧O — {app} öffnen. ⌘⇧M — Sitzungsvergleich. Anpassbar in Einstellungen → Tastenkombinationen.

## Was ist die WebSocket-API?
JSON-API im LAN (mDNS: _skill._tcp). Befehle: status, label, search, compare, sessions, sleep, umap, umap_poll.

## Was sind Fokus / Entspannung / Engagement?
Abgeleitete Scores aus Bandleistungsverhältnissen, auf 0–100 per Sigmoid abgebildet.

## Was sind TAR, BAR, DTR?
Kreuzband-Verhältnisse: Theta/Alpha, Beta/Alpha, Delta/Theta.

## Was sind PSE, APF, BPS, SNR?
Spektralform-Merkmale: Entropie, Alpha-Spitzenfrequenz, Steigung, Signal-Rausch-Verhältnis.
