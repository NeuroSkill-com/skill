# Übersicht
Proaktive Hooks lösen automatisch Aktionen aus, wenn Ihre aktuellen EEG-Muster bestimmten Schlüsselwörtern oder Gehirnzuständen entsprechen.

## Was sind Proaktive Hooks?
Ein Proaktiver Hook überwacht Ihre aktuellen EEG-Label-Embeddings in Echtzeit. Wenn die Kosinus-Distanz zwischen Gehirnzustands-Embeddings und Schlüsselwort-Embeddings unter einen konfigurierten Schwellenwert fällt, wird der Hook ausgelöst — er sendet einen Befehl, zeigt eine Benachrichtigung an, löst TTS aus oder sendet ein WebSocket-Ereignis. Ermöglicht Neurofeedback-Automatisierungen ohne Programmierung.

## So funktioniert es
Alle paar Sekunden berechnet die App EEG-Embeddings aus Ihren neuesten Gehirndaten. Diese werden über den HNSW-Index mit den Schlüsselwort-Embeddings jedes aktiven Hooks verglichen. Eine Abklingzeit verhindert wiederholtes Auslösen. Der Abgleich ist rein lokal — keine Daten verlassen Ihren Rechner.

## Szenarien
Jeder Hook kann auf ein Szenario beschränkt werden — Kognitiv, Emotional, Physisch oder Alle. Kognitive Hooks zielen auf Fokus, Ablenkung oder geistige Ermüdung. Emotionale Hooks auf Stress, Ruhe oder Frustration. Physische Hooks auf Schläfrigkeit oder physische Ermüdung. 'Alle' passt unabhängig von der Kategorie.

# Einen Hook konfigurieren
Jeder Hook hat mehrere Felder, die steuern, wann und wie er ausgelöst wird.

## Hook-Name
Ein beschreibender Name für den Hook (z. B. 'Deep Work Guard', 'Calm Recovery'). Der Name wird im Verlaufsprotokoll und in WebSocket-Ereignissen verwendet. Er muss unter allen Hooks eindeutig sein.

## Schlüsselwörter
Schlüsselwörter oder kurze Phrasen, die den zu erkennenden Gehirnzustand beschreiben (z. B. 'Fokus', 'tiefe Arbeit', 'Stress', 'müde'). Diese werden mit demselben Satz-Transformer-Modell wie Ihre EEG-Labels eingebettet. Der Hook wird ausgelöst, wenn aktuelle EEG-Embeddings den Schlüsselwort-Embeddings nahe sind.

## Schlüsselwort-Vorschläge
Während der Eingabe schlägt die App verwandte Begriffe aus Ihrem Label-Verlauf vor — über unscharfen Zeichenkettenabgleich und semantische Embedding-Ähnlichkeit. Vorschläge zeigen ein Quell-Abzeichen: 'unscharf', 'semantisch' oder beides. ↑/↓ und Enter zum Übernehmen.

## Distanzschwellenwert
Die maximale Kosinus-Distanz (0–1) zwischen EEG-Embeddings und Schlüsselwort-Embeddings. Niedrigere Werte = strenger, höhere = toleranter. Typisch: 0,08 (sehr streng) bis 0,25 (locker). Beginnen Sie bei 0,12–0,16 und stimmen Sie mit dem Vorschlagswerkzeug ab.

## Distanz-Vorschlagswerkzeug
Klicken Sie auf 'Schwellenwert vorschlagen', um Ihre EEG-Daten anhand der Hook-Schlüsselwörter zu analysieren. Das Werkzeug berechnet die Distanzverteilung (Min, p25, p50, p75, Max) und empfiehlt einen Schwellenwert. Ein Perzentilbalken zeigt aktuelle und vorgeschlagene Schwellenwerte. 'Anwenden' übernimmt den Wert.

## Letzte Referenzen
Anzahl der neuesten EEG-Embedding-Samples zum Vergleich (Standard: 12). Höhere Werte glätten vorübergehende Spitzen, erhöhen aber die Erkennungslatenz. Niedrigere reagieren schneller, können aber auf Artefakte ansprechen. Bereich: 10–20.

## Befehl
Ein optionaler Befehlsstring im WebSocket-Ereignis bei Auslösung (z. B. 'focus_reset', 'calm_breath'). Externe Automatisierungstools können darauf reagieren, um Aktionen, Benachrichtigungen oder Skripte auszulösen.

## Nutzlast-Text
Eine optionale menschenlesbare Nachricht im Auslöseereignis (z. B. 'Machen Sie eine 2-Minuten-Pause.'). Wird in Benachrichtigungen angezeigt und kann per TTS vorgelesen werden, wenn Sprachführung aktiviert ist.

# Erweitert
Tipps, Verlauf und Integration mit externen Werkzeugen.

## Schnellbeispiele
Fertige Hook-Vorlagen für gängige Anwendungsfälle: Deep Work Guard (kognitiver Fokus-Reset), Calm Recovery (emotionale Stressentlastung) und Body Break (physische Ermüdung). Klicken Sie zum Hinzufügen mit vorausgefüllten Werten. Passen Sie an Ihre persönlichen EEG-Muster an.

## Hook-Auslösungsverlauf
Das einklappbare Verlaufsprotokoll zeichnet jedes Auslöseereignis mit Zeitstempel, Label, Kosinus-Distanz, Befehl und Schlüsselwörtern auf. Zur Überprüfung des Verhaltens und zum Debuggen von Fehlalarmen. Zeilen aufklappen für Details. Seitensteuerungen zum Blättern.

## WebSocket-Ereignisse
Bei Auslösung sendet die App ein JSON-Ereignis über die WebSocket-API mit Hook-Name, Befehl, Text, Label, Distanz und Zeitstempel. Externe Clients können darauf lauschen, um Automatisierungen zu erstellen — z. B. Lichter dimmen, Musik pausieren oder in ein Dashboard loggen.

## Feinabstimmungs-Tipps
Beginnen Sie mit einem Hook und wenigen Schlüsselwörtern, die zu Labels passen. Verwenden Sie das Vorschlagswerkzeug für den Anfangsschwellenwert. Überwachen Sie den Verlauf einen Tag lang und passen Sie an: Schwellenwert senken bei Fehlalarmen, erhöhen wenn nie ausgelöst. Spezifischere Schlüsselwörter verbessern die Präzision.
