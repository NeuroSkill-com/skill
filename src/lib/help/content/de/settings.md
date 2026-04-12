# Einstellungen-Tab
Konfigurieren Sie Geräteeinstellungen, Signalverarbeitung, Embedding-Parameter, Kalibrierung, Tastenkürzel und Protokollierung.

## Gekoppelte Geräte
Listet alle erkannten BCI-Geräte auf. Sie können ein bevorzugtes Gerät festlegen, Geräte vergessen oder nach neuen suchen.

## Signalverarbeitung
Konfigurieren Sie die Echtzeit-EEG-Filterkette: Tiefpass, Hochpass und Netzfrequenzfilter.

## EEG-Embedding
Passen Sie die Überlappung zwischen aufeinanderfolgenden 5-Sekunden-Epochen an.

## Kalibrierung
Konfigurieren Sie die Kalibrierungsaufgabe: Aktionsbezeichnungen, Phasendauer, Wiederholungen und Autostart.

## Kalibrierungs-Sprachführung (TTS)
Während der Kalibrierung kündigt die App jede Phase namentlich an, indem sie die englische Text-to-Speech-Funktion auf dem Gerät verwendet. Die Engine wird von KittenTTS (tract-onnx, ~30 MB) mit Espeak-NG-Phonemisierung betrieben. Das Modell wird beim ersten Start vom HuggingFace Hub heruntergeladen und lokal zwischengespeichert – danach verlassen keine Daten Ihr Gerät. Die Sprache wird ausgelöst für: Sitzungsstart, jede Aktionsphase, jede Pause („Pause. Weiter: …“) und Sitzungsabschluss. Erfordert espeak-ng auf PATH (brew / apt / apk install espeak-ng). Nur Englisch.

## Globale Tastenkürzel
Systemweite Tastenkürzel zum Öffnen der Label-, Such-, Einstellungs- und Kalibrierungsfenster.

## Debug-Protokollierung
Protokollierung pro Subsystem in der täglichen Logdatei {dataDir}/logs/ ein-/ausschalten.

## Aktualisierungen
App-Updates prüfen und installieren. Nutzt Tauris Update-System mit Ed25519-Signaturprüfung.

## Erscheinungsbild
Farbmodus wählen (System / Hell / Dunkel), Hochkontrast für stärkere Rahmen und Text aktivieren sowie ein Farbschema für EEG-Wellenformen und Bandleistungsdiagramme festlegen. Farbenblindheits-sichere Paletten stehen zur Verfügung. Die Sprache wird hier über die Sprachauswahl geändert.

## Ziele
Tägliches Aufnahmeziel in Minuten festlegen. Ein Fortschrittsbalken erscheint während der Aufnahme im Dashboard, und bei Zielerreichung erfolgt eine Benachrichtigung. Das 30-Tage-Diagramm zeigt Tage mit erfülltem (grün), halbem (gelb), teilweisem (gedimmt) oder fehlendem (leer) Fortschritt.

## Texteinbettungen
Sentence-Transformer-Modell für die semantische Suche auswählen. Kleinere Modelle (≤384-dim) sind schnell; größere erzeugen reichhaltigere Repräsentationen. Gewichte werden einmalig von HuggingFace heruntergeladen und lokal gecacht. Nach Modellwechsel "Alle Labels neu einbetten" ausführen.

## Tastenkürzel
Globale Tastenkürzel für Label-, Such-, Einstellungs- und Kalibrierungsfenster konfigurieren. Zeigt auch alle In-App-Kürzel (⌘K für Befehlspalette, ? für Kürzel-Overlay, ⌘↵ zum Label absenden). Format: z. B. CmdOrCtrl+Shift+L.

# Aktivitätsverfolgung
NeuroSkill kann optional aufzeichnen, welche App im Vordergrund ist und wann Tastatur und Maus zuletzt benutzt wurden. Beide Funktionen sind standardmäßig aktiviert, vollständig lokal und in Einstellungen → Aktivitätsverfolgung unabhängig konfigurierbar.

## Aktives Fenster verfolgen
Ein Hintergrund-Thread wacht jede Sekunde auf und fragt das Betriebssystem, welche Anwendung aktuell im Vordergrund ist. Wenn sich der App-Name oder der Fenstertitel ändert, wird eine Zeile in activity.sqlite eingefügt: der Anzeigename der Anwendung, der vollständige Pfad zum App-Bundle oder zur ausführbaren Datei, der Titel des vordersten Fensters und ein Unix-Sekunden-Zeitstempel. Solange Sie im selben Fenster bleiben, wird keine neue Zeile geschrieben. Unter macOS verwendet der Tracker osascript; für App-Name und Pfad ist keine Berechtigungsgenehmigung erforderlich, der Fenstertitel kann bei Sandbox-Apps leer sein. Unter Linux werden xdotool und xprop verwendet (X11-Sitzung erforderlich). Unter Windows kommt ein PowerShell-GetForegroundWindow-Aufruf zum Einsatz.

## Tastatur- & Mausaktivität verfolgen
Ein globaler Input-Hook (rdev) hört auf alle Tastendruck- und Maus-/Trackpad-Ereignisse systemweit. Es wird nicht aufgezeichnet, was Sie tippen, welche Tasten Sie drücken oder wohin der Cursor bewegt wird — es werden nur zwei Unix-Sekunden-Zeitstempel im Speicher aktualisiert: einer für das letzte Tastaturereignis, einer für das letzte Maus-/Trackpad-Ereignis. Diese werden alle 60 Sekunden in activity.sqlite geschrieben, aber nur wenn sich mindestens ein Wert seit dem letzten Schreiben geändert hat. Das Einstellungsfeld erhält ein gedrosseltes Update-Ereignis (höchstens einmal pro Sekunde).

## Datenspeicherung
Alle Aktivitätsdaten liegen in einer einzigen SQLite-Datei: ~/.skill/activity.sqlite. Sie wird niemals übertragen, synchronisiert oder in Analysen einbezogen. Zwei Tabellen: active_windows (eine Zeile pro Fensterfokuswechsel) und input_activity (eine Zeile pro 60-Sekunden-Flush bei erkannter Aktivität). Beide Tabellen haben einen absteigenden Index auf der Zeitstempelspalte. WAL-Journal-Modus ist aktiviert. Die Datei kann jederzeit mit einem SQLite-Browser geöffnet, exportiert oder gelöscht werden.

## Erforderliche Betriebssystemberechtigungen
macOS — Aktive-Fenster-Verfolgung (App-Name und Pfad) benötigt keine besonderen Berechtigungen. Tastatur- und Mausverfolgung verwendet einen CGEventTap, der Bedienungshilfen-Zugriff erfordert: Systemeinstellungen → Datenschutz & Sicherheit → Bedienungshilfen → NeuroSkill aktivieren. Ohne diese Berechtigung schlägt der Hook lautlos fehl — Zeitstempel bleiben bei null, der Rest der App funktioniert normal. Linux — Erfordert X11. Aktive-Fenster-Verfolgung verwendet xdotool und xprop. Input-Verfolgung verwendet die XRecord-Erweiterung aus libxtst. Windows — Keine besonderen Berechtigungen erforderlich.

## Deaktivieren & Daten löschen
Beide Umschalter in Einstellungen → Aktivitätsverfolgung wirken sofort — kein Neustart erforderlich. Deaktivieren der Aktive-Fenster-Verfolgung stoppt neue Zeilen in active_windows und löscht den In-Memory-Zustand. Deaktivieren der Input-Verfolgung stoppt den rdev-Callback und verhindert weitere Flushes in input_activity. Um die gesamte Historie zu löschen: App beenden, ~/.skill/activity.sqlite löschen, dann neu starten — eine leere Datenbank wird automatisch erstellt.

# UMAP

## UMAP
UMAP-Projektionsparameter für den 3D-Vergleich einstellen: Anzahl Nachbarn (lokale vs. globale Struktur), Mindestabstand (Cluster-Dichte) und Metrik (Kosinus oder Euklidisch). Höhere Nachbarzahlen bewahren globale Topologie; niedrige enthüllen feine lokale Cluster. Projektionen laufen im Hintergrund.

# EEG-Modell-Tab
ZUNA-Encoder und HNSW-Vektor-Index-Status überwachen.

## Encoder-Status
Zeigt ob der ZUNA wgpu-Encoder geladen ist, die Architektur und den Pfad der Gewichtsdatei.

## Embeddings heute
Ein Live-Zähler der in den heutigen HNSW-Index eingebetteten 5-Sekunden-EEG-Epochen.

## HNSW-Parameter
M (Verbindungen pro Knoten) und ef_construction steuern den Qualitäts-/Geschwindigkeits-Kompromiss.

## Datennormalisierung
Der data_norm-Skalierungsfaktor für rohes EEG. Standard (10) ist für Muse 2 / Muse S kalibriert.

# OpenBCI-Geräte
Verbinde und konfiguriere jedes OpenBCI-Board — Ganglion, Cyton, Cyton+Daisy, WiFi-Shield-Varianten oder Galea — allein oder neben einem anderen BCI-Gerät.

## Board-Auswahl
Wählen Sie das OpenBCI-Board. Ganglion (4K, BLE) ist am portabelsten. Cyton (8K, USB) bietet mehr Kanäle. Cyton+Daisy verdoppelt auf 16K. WiFi-Shield ersetzt USB/BLE durch 1-kHz-WLAN. Galea (24K, UDP) ist ein Hochdichte-Forschungsboard. Alle Varianten können allein oder neben einem anderen BCI-Gerät betrieben werden.

## Ganglion BLE
Der Ganglion verbindet sich über Bluetooth Low Energy. Klicke auf Verbinden — NeuroSkill™ scannt bis zum konfigurierten Timeout nach dem nächsten werbenden Ganglion. Halte das Board innerhalb von 3–5 m und eingeschaltet (blaue LED blinkt). Nur ein Ganglion kann pro Bluetooth-Adapter aktiv sein.

## Serieller Port (Cyton / Cyton+Daisy)
Cyton-Boards kommunizieren über einen USB-Funk-Dongle. Lasse das Portfeld leer für automatische Erkennung, oder gib ihn explizit an (/dev/cu.usbserial-… auf macOS, /dev/ttyUSB0 auf Linux, COM3 auf Windows). Stecke den Dongle vor dem Verbinden ein. Unter Linux füge deinen Benutzer zur Gruppe dialout hinzu.

## WiFi-Shield
Das OpenBCI WiFi-Shield erstellt ein eigenes 2,4-GHz-WLAN (SSID: OpenBCI-XXXX). Verbinde deinen Computer mit diesem Netzwerk und gib die IP 192.168.4.1 ein. Alternativ kann das Shield in dein Heimnetz eingebunden werden — gib dann die zugewiesene IP ein. Leer lassen für automatische mDNS-Erkennung. Das WiFi-Shield überträgt mit 1 kHz — stelle den Tiefpassfilter auf ≤ 500 Hz ein.

## Galea
Galea ist ein 24-Kanal-Forschungsheadset (EEG + EMG + AUX), das per UDP überträgt. Gib die IP-Adresse des Galea-Geräts ein oder lasse das Feld leer, um Pakete von beliebigen Sendern zu akzeptieren. Kanäle 1–8 sind EEG (Echtzeit-Analyse); 9–16 sind EMG; 17–24 AUX. Alle 24 Kanäle werden in CSV gespeichert.

## Kanal-Labels & Presets
Weise jedem physischen Kanal standardisierte 10-20-Elektrodennamen zu. Nutze ein Preset (Frontal, Motor, Okzipital, Full 10-20) oder gib eigene Namen ein. Kanäle jenseits der ersten 4 werden nur in CSV gespeichert und treiben die Echtzeit-Analyse nicht an.
