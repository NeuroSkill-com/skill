# Hauptfenster
Das Hauptfenster ist das primäre Dashboard. Es zeigt EEG-Daten in Echtzeit, den Gerätestatus und die Signalqualität.

## Status-Anzeige
Die obere Karte zeigt den Live-Verbindungsstatus Ihres BCI-Geräts. Ein farbiger Ring und ein Badge zeigen den aktuellen Zustand an.

## Akku
Ein Fortschrittsbalken mit dem aktuellen Ladestand des verbundenen BCI-Headsets.

## Signalqualität
Vier farbige Punkte — einer pro EEG-Elektrode (TP9, AF7, AF8, TP10). Grün = gut. Gelb = mittel. Rot = schlecht. Grau = kein Signal.

## EEG-Kanal-Raster
Vier Karten mit dem neuesten Abtastwert (in µV) für jeden Kanal.

## Laufzeit & Abtastwerte
Laufzeit zählt die Sekunden seit Sitzungsbeginn. Abtastwerte ist die Gesamtzahl der empfangenen EEG-Samples.

## CSV-Aufzeichnung
Ein AUFN-Indikator zeigt den Dateinamen der CSV-Datei in {dataDir}/.

## Bandleistung
Ein Live-Balkendiagramm der relativen Leistung in jedem EEG-Frequenzband: Delta, Theta, Alpha, Beta und Gamma.

## Frontale Alpha-Asymmetrie (FAA)
Eine mittig verankerte Anzeige der Echtzeit-FAA: ln(AF8 α) − ln(AF7 α). Positive Werte zeigen stärkere rechts-frontale Alpha-Leistung an, was mit Annäherungsmotivation der linken Hemisphäre assoziiert wird. Negative Werte deuten auf Rückzugstendenz hin. Der Wert wird mit einem exponentiellen gleitenden Durchschnitt geglättet und liegt typischerweise zwischen −1 und +1. FAA wird zusammen mit jeder 5-Sekunden-Embedding-Epoche in eeg.sqlite gespeichert.

## EEG-Wellenformen
Ein scrollendes Zeitbereichsdiagramm des gefilterten EEG-Signals für alle vier Kanäle mit Spektrogramm.

## GPU-Auslastung
Ein kleines Diagramm oben im Hauptfenster mit der GPU-Auslastung. Nur sichtbar, wenn der ZUNA-Encoder aktiv ist.

# Systemsymbol-Zustände

## Grau — Getrennt
Bluetooth ist an; kein BCI-Gerät verbunden.

## Gelb — Suche
Suche nach einem BCI-Gerät oder Verbindungsversuch.

## Grün — Verbunden
Live-EEG-Daten werden von Ihrem BCI-Gerät gestreamt.

## Rot — Bluetooth aus
Bluetooth ist ausgeschaltet. Keine Suche oder Verbindung möglich.

# Community
Treten Sie der NeuroSkill-Discord-Community bei, um Fragen zu stellen, Feedback zu geben und sich mit anderen Nutzern und Entwicklern zu vernetzen.
