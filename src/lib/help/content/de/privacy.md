# Datenschutz-Übersicht
{app} ist vollständig lokal konzipiert. Ihre EEG-Daten, Embeddings, Labels und Einstellungen verlassen nie Ihren Computer.

# Datenspeicherung

## Alle Daten bleiben auf Ihrem Gerät
Alle von {app} aufgezeichneten Daten — rohe EEG-Samples (CSV), EEG-Embeddings (SQLite + HNSW-Index), Text-Labels, Kalibrierungszeitstempel, Logs und Einstellungen — werden lokal in {dataDir}/ gespeichert. Keine Daten werden an einen Cloud-Dienst, Server oder Drittanbieter hochgeladen.

## Keine Benutzerkonten
{app} erfordert keine Registrierung, Anmeldung oder Kontoerstellung.

## Speicherort
Alle Dateien befinden sich unter {dataDir}/ auf macOS und Linux. Jeder Tag hat ein eigenes JJJJMMTT-Unterverzeichnis.

# Netzwerkaktivität

## Keine Telemetrie oder Analytik
{app} sammelt keine Nutzungsanalysen, Absturzberichte oder Verhaltensbeobachtungen.

## Nur lokaler WebSocket-Server
{app} betreibt einen WebSocket-Server auf Ihrer lokalen Netzwerkschnittstelle für LAN-Streaming.

## mDNS / Bonjour-Dienst
{app} registriert einen _skill._tcp.local. mDNS-Dienst für die automatische Erkennung im lokalen Netzwerk.

## Update-Prüfungen
Wenn Sie in den Einstellungen auf „Nach Updates suchen" klicken, kontaktiert {app} den konfigurierten Update-Endpunkt, um nach einer neueren Version zu suchen. Dies ist die einzige ausgehende Internetanfrage der App, und sie erfolgt nur, wenn Sie sie ausdrücklich auslösen. Update-Pakete werden vor der Installation mit einer Ed25519-Signatur verifiziert.

# Bluetooth & Sicherheit

## Bluetooth Low Energy (BLE)
{app} kommuniziert mit Ihrem BCI-Gerät über BLE oder USB-Seriell mit dem Standard-System-Stack.

## Systemberechtigungen
Bluetooth-Zugriff erfordert eine explizite Systemberechtigung.

## Gerätekennungen
Seriennummer und MAC-Adresse des BCI-Headsets werden nur lokal gespeichert.

# Verarbeitung auf dem Gerät

## GPU-Inferenz bleibt lokal
Der EEG-Embedding-Encoder läuft vollständig auf Ihrer lokalen GPU über wgpu. Modellgewichte werden aus dem lokalen Hugging Face Cache (~/.cache/huggingface/) geladen. Keine EEG-Daten werden an eine externe Inferenz-API oder Cloud-GPU gesendet. Texteinbettungen für die Label-Suche verwenden nomic-embed-text-v1.5, ebenfalls lokal zwischengespeichert.

## Filterung und Analyse
Die gesamte Signalverarbeitung läuft lokal auf Ihrer CPU/GPU.

## Nächste-Nachbarn-Suche
Der HNSW-Vektor-Index wird vollständig auf Ihrem Gerät erstellt und abgefragt.

# Ihre Daten, Ihre Kontrolle

## Zugriff
Alle Ihre Daten sind in {dataDir}/ in Standardformaten (CSV, SQLite, binärer HNSW).

## Löschen
Löschen Sie jede Datei unter {dataDir}/ jederzeit. Keine Cloud-Backups.

## Exportieren
CSV-Aufnahmen und SQLite-Datenbanken sind portable Standardformate.

## Verschlüsselung
{app} verschlüsselt Daten nicht im Ruhezustand. Nutzen Sie die Festplattenverschlüsselung Ihres Betriebssystems.

# Aktivitätsverfolgung

## Aktivitätsverfolgung
Wenn aktiviert, zeichnet NeuroSkill auf, welche Anwendung im Vordergrund ist und wann Tastatur und Maus zuletzt benutzt wurden. Diese Daten verbleiben vollständig auf Ihrem Gerät in ~/.skill/activity.sqlite — sie werden niemals an einen Server gesendet, nicht remote protokolliert und nicht in Analysen einbezogen. Aktive-Fenster-Verfolgung erfasst: App-Name, ausführbarer Pfad, Fenstertitel und Unix-Zeitstempel. Tastatur- und Mausverfolgung erfasst nur zwei Zeitstempel — niemals Tastenanschläge, eingegebenen Text, Cursor-Koordinaten oder Klickziele. Beide Funktionen können in Einstellungen → Aktivitätsverfolgung unabhängig deaktiviert werden.

## Bedienungshilfen-Berechtigung (macOS)
Unter macOS erfordert die Tastatur- und Mausverfolgung die Bedienungshilfen-Berechtigung, da sie einen CGEventTap installiert. Apple verlangt diese Berechtigung für jede App, die globale Eingaben liest. Ohne sie schlägt der Hook lautlos fehl: Die App funktioniert weiterhin normal, nur die Input-Zeitstempel bleiben bei null. Aktive-Fenster-Verfolgung verwendet osascript und benötigt keine Bedienungshilfen-Berechtigung.

# Zusammenfassung

## No cloud
Keine Cloud. Alle Daten werden lokal in {dataDir}/ gespeichert.

## No telemetry
Keine Telemetrie. Keine Analytik oder Nutzungsverfolgung.

## No accounts
Keine Konten. Keine Registrierung oder Identifikatoren.

## One optional network request
Eine optionale Netzwerkanfrage. Update-Prüfungen nur auf Wunsch.

## Fully on-device
Vollständig auf dem Gerät. GPU-Inferenz, Signalverarbeitung und Suche lokal.

## Activity tracking is local-only
Aktivitätsverfolgung nur lokal. Fensterfokus und Input-Zeitstempel werden in activity.sqlite auf Ihrem Gerät gespeichert und verlassen es nie.
