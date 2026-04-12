# Fenster
{app} nutzt separate Fenster für verschiedene Aufgaben. Jedes kann über das Kontextmenü oder Tastenkürzel geöffnet werden.

## 🏷  Label-Fenster
Geöffnet über Menü, Tastenkürzel oder Tag-Button. Geben Sie ein Label ein, um den aktuellen EEG-Moment zu annotieren.

## 🔍  Such-Fenster
Das Such-Fenster bietet drei Modi — EEG-Ähnlichkeit, Text und Interaktiv — die Ihre Aufzeichnungen auf unterschiedliche Weise abfragen.

## EEG-Ähnlichkeitssuche
Wählen Sie einen Zeitraum und suchen Sie per Nächste-Nachbarn-Suche über alle ZUNA-Einbettungen in diesem Fenster. Der HNSW-Index liefert die k ähnlichsten 5-Sekunden-EEG-Epochen aus Ihrer gesamten Aufzeichnungshistorie, sortiert nach Kosinusabstand. Kleinerer Abstand = ähnlicherer Gehirnzustand. Labels, die einen Ergebnis-Zeitstempel überlappen, werden direkt angezeigt.

## Texteinbettungssuche
Geben Sie ein beliebiges Konzept, eine Aktivität oder einen mentalen Zustand in natürlicher Sprache ein (z. B. "tiefe Konzentration", "ängstlich", "Meditation mit geschlossenen Augen"). Die Abfrage wird durch dasselbe Satz-Transformer-Modell eingebettet, das für die Label-Indizierung verwendet wird, und mit all Ihren Anmerkungen über Kosinusähnlichkeit im HNSW-Label-Index abgeglichen. Ergebnisse sind Ihre eigenen Labels, nach semantischer Nähe sortiert — keine Schlüsselwortsuche. Ein 3D-kNN-Graph visualisiert die Nachbarschaftsstruktur.

## Interaktive multimodale Suche
Geben Sie ein Freitextkonzept ein, und {app} führt eine vierstufige multimodale Pipeline aus: (1) Die Abfrage wird eingebettet. (2) Die text-k semantisch ähnlichsten Labels werden gefunden. (3) Für jedes Label berechnet {app} die mittlere EEG-Einbettung seines Aufnahmefensters und sucht die eeg-k ähnlichsten EEG-Epochen. (4) Für jeden EEG-Nachbarn werden Anmerkungen innerhalb von ±Reichweite Minuten als "gefundene Labels" gesammelt. Das Ergebnis ist ein gerichteter Graph mit vier Ebenen — Abfrage → Texttreffer → EEG-Nachbarn → Gefundene Labels — als interaktive 3D-Visualisierung, exportierbar als SVG oder Graphviz DOT.

## 🎯  Kalibrierungs-Fenster
Führt eine geführte Kalibrierungsaufgabe durch. Erfordert ein verbundenes, streamendes BCI-Gerät.

## ⚙  Einstellungen-Fenster
Vier Tabs: Einstellungen, Tastenkürzel (globale Hotkeys, Befehlspalette, In-App-Kürzel), EEG-Modell (Encoder & HNSW-Status). Öffnen über das Tray-Menü oder den Zahnrad-Button.

## ?  Hilfe-Fenster
Dieses Fenster. Eine vollständige Referenz für jeden Teil der {app}-Oberfläche.

## 🧭  Einrichtungsassistent
Ein fünfstufiger Ersteinrichtungs-Assistent, der durch Bluetooth-Kopplung, Headset-Sitz und erste Kalibrierung führt. Öffnet sich automatisch beim ersten Start; kann jederzeit über die Befehlspalette (⌘K → Einrichtungsassistent) erneut geöffnet werden.

## 🌐  API-Status-Fenster
Ein Live-Dashboard mit allen aktuell verbundenen WebSocket-Clients und einem scrollbaren Anfragen-Protokoll. Zeigt Server-Port, Protokoll und mDNS-Erkennung. Enthält Schnellverbindungs-Snippets für ws:// und dns-sd. Aktualisiert sich alle 2 Sekunden automatisch. Öffnen über das Tray-Menü oder die Befehlspalette.

## 🌙 Schlafphasen
Für Sitzungen ab 30 Minuten zeigt die Verlaufsansicht ein automatisch generiertes Hypnogramm. Hinweis: Consumer-BCI-Headsets wie Muse haben 4 Trockenelektroden — die Schlafphaseneinteilung ist näherungsweise, kein klinisches PSG.

## ⚖  Vergleichen
Pick any two time ranges on the timeline and compare their average band-power distributions, relaxation/engagement scores, and Frontal Alpha Asymmetry side by side. Includes sleep staging, advanced metrics, and Brain Nebula™ — a 3D UMAP projection showing how similar the two periods are in high-dimensional EEG space. Open from the tray menu or command palette (⌘K → Compare).

# Overlays & Befehlspalette
Schnellzugriff-Overlays, die in jedem Fenster per Tastenkürzel verfügbar sind.

## ⌨  Befehlspalette (⌘K / Strg+K)
Ein Schnellzugriff-Dropdown mit allen ausführbaren Aktionen der App. Tippen zum Filtern, ↑↓ zum Navigieren, Enter zum Ausführen. Verfügbar in jedem Fenster. Befehle umfassen Fenster öffnen (Einstellungen, Hilfe, Suche, Label, Verlauf, Kalibrierung), Geräteaktionen (Verbindung wiederholen, Bluetooth-Einstellungen) und Dienstprogramme (Tastenkürzel anzeigen, Updates prüfen).

## ?  Tastenkürzel-Overlay
Drücke ? in jedem Fenster (außerhalb von Textfeldern), um ein schwebendes Overlay mit allen Tastenkürzeln anzuzeigen — globale Kürzel aus Einstellungen → Tastenkürzel, sowie In-App-Tasten wie ⌘K für die Befehlspalette und ⌘Enter zum Speichern von Labels. Erneut ? oder Esc zum Schließen.
