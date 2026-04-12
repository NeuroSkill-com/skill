## Wo werden meine Daten gespeichert?
Alles wird lokal in {dataDir}/ gespeichert — CSV-Aufnahmen, HNSW-Indizes, SQLite-Datenbanken, Labels, Logs und Einstellungen.

## Was macht der ZUNA-Encoder?
ZUNA ist eines von mehreren EEG-Embedding-Backends in {app}. Es ist ein GPU-beschleunigter Transformer-Encoder, der 5-Sekunden-EEG-Epochen in kompakte Embedding-Vektoren umwandelt. Diese Vektoren erfassen die neuronale Signatur jedes Moments und treiben die Ähnlichkeitssuche an. Weitere Backends sind LUNA und NeuroRVQ.

## Warum erfordert die Kalibrierung ein verbundenes Gerät?
Die Kalibrierung zeichnet gelabelte EEG-Daten auf. Ohne Live-Streaming gäbe es kein Neuralsignal zum Zuordnen.

## Wie verbinde ich mich von Python / Node.js?
Entdecken Sie den WebSocket-Port via mDNS, dann öffnen Sie eine Standard-WebSocket-Verbindung. Siehe API-Tab.

## Was bedeuten die Signalqualitäts-Indikatoren?
Jeder Punkt steht für eine EEG-Elektrode. Grün = guter Kontakt. Gelb = Artefakte. Rot = schlechter Kontakt. Grau = kein Signal.

## Kann ich die Netzfrequenz ändern?
Ja — Einstellungen → Signalverarbeitung: 50 Hz (Europa) oder 60 Hz (Amerika, Japan).

## Wie setze ich ein gekoppeltes Gerät zurück?
Einstellungen → Gekoppelte Geräte → × zum Vergessen.

## Warum wird das Systemsymbol rot?
Bluetooth ist ausgeschaltet. Aktivieren Sie es in den Systemeinstellungen → Bluetooth.

## Die App dreht sich, verbindet sich aber nicht — was tun?
1. Gerät einschalten (Muse: Taste halten bis zur Vibration; Ganglion/Cyton: blaue LED). 2. Innerhalb von 5 m bleiben. 3. Bei Bedarf aus- und wieder einschalten.

## Warum hat sich mein Gerät automatisch getrennt?
Wenn nach dem Empfang mindestens eines EEG-Frames 30 Sekunden lang keine Daten eintreffen, betrachtet {app} das Gerät als stillschweigend getrennt (z. B. außerhalb der BLE-Reichweite oder ohne saubere Trennung ausgeschaltet). Das Systemsymbol wechselt zurück zu Grau und der Scan wird automatisch fortgesetzt.

## Wie erteile ich die Bluetooth-Berechtigung?
macOS zeigt einen Berechtigungsdialog. Falls abgelehnt: Systemeinstellungen → Datenschutz → Bluetooth.

## Welche Metriken werden in der Datenbank gespeichert?
Jede 2,5-Sekunden-Epoche speichert: den EEG-Embedding-Vektor, relative Bandleistungen (Delta, Theta, Alpha, Beta, Gamma, High-Gamma) kanalgemittelt, Bandleistungen pro Kanal als JSON, abgeleitete Scores (Entspannung, Engagement), FAA, Kreuzband-Verhältnisse (TAR, BAR, DTR), Spektralform (PSE, APF, BPS, SNR), Kohärenz, Mu-Unterdrückung, Stimmungsindex und PPG-Durchschnittswerte falls verfügbar.

## Was ist der Sitzungsvergleich?
Vergleichen (⌘⇧M) vergleicht zwei Zeitbereiche nebeneinander: Bandleistungsbalken mit Differenzen, alle Scores und Verhältnisse, FAA, Schlafphasen-Hypnogramme und Brain Nebula™ — eine 3D-UMAP-Embedding-Projektion.

## Was ist Brain Nebula™?
Brain Nebula™ (technisch: UMAP Embedding Distribution) projiziert hochdimensionale EEG-Embeddings in 3D, sodass ähnliche Hirnzustände als benachbarte Punkte erscheinen. Bereich A (blau) und B (bernstein) bilden unterschiedliche Cluster. Sie können rotieren, zoomen und beschriftete Punkte anklicken, um zeitliche Verbindungen zu sehen.

## Warum zeigt Brain Nebula™ zuerst eine zufällige Wolke?
Die UMAP-Projektion ist rechenintensiv und läuft in einer Hintergrund-Warteschlange. Während der Berechnung wird eine zufällige Platzhalter-Wolke angezeigt. Sobald die Projektion fertig ist, animieren die Punkte sanft zu ihren endgültigen Positionen.

## Was sind Labels und wie werden sie verwendet?
Labels sind benutzerdefinierte Tags (z.B. 'Meditation', 'Lesen'), die einem Moment zugeordnet werden. Im UMAP-Viewer erscheinen beschriftete Punkte größer mit farbigen Ringen — klicken Sie darauf, um das Label zeitlich zu verfolgen.

## Was ist frontale Alpha-Asymmetrie (FAA)?
FAA = ln(AF8 α) − ln(AF7 α). Positive Werte deuten auf Annäherungsmotivation hin, negative auf Vermeidung.

## Wie funktioniert die Schlafphasen-Erkennung?
Jede EEG-Epoche wird anhand der relativen Delta-, Theta-, Alpha- und Beta-Leistung als Wach, N1, N2, N3 oder REM klassifiziert. Der Vergleich zeigt ein Hypnogramm je Sitzung.

## Welche Tastenkürzel gibt es?
⌘⇧O — {app}-Fenster öffnen. ⌘⇧M — Sitzungsvergleich öffnen. Anpassbar unter Einstellungen → Tastenkürzel.

## Was ist die WebSocket-API?
{app} bietet eine JSON-WebSocket-API im lokalen Netzwerk (mDNS: _skill._tcp). Befehle: status, label, search, compare, sessions, sleep, umap (Projektion einreihen), umap_poll (Ergebnis abrufen).

## Was sind Fokus-, Entspannungs- und Engagement-Scores?
Fokus = β/(α+θ), Entspannung = α/(β+θ), Engagement = β/(α+θ) mit sanfterer Kurve. Alle über Sigmoid auf 0–100 abgebildet.

## Was sind TAR, BAR und DTR?
TAR (Theta/Alpha) — höher = schläfriger. BAR (Beta/Alpha) — höher = gestresster/fokussierter. DTR (Delta/Theta) — höher = tieferer Schlaf. Alle kanalgemittelt.

## Was sind PSE, APF, BPS und SNR?
PSE (Spektrale Entropie, 0–1) — spektrale Komplexität. APF (Alpha-Spitzenfrequenz, Hz). BPS (Bandleistungs-Steigung) — 1/f-Exponent. SNR (Signal-Rausch-Verhältnis, dB).

## Was ist das Theta/Beta-Verhältnis (TBR)?
TBR ist das Verhältnis der absoluten Theta- zur Beta-Leistung. Höhere Werte deuten auf reduzierte kortikale Erregung hin — erhöhtes TBR ist mit Schläfrigkeit und Aufmerksamkeitsdysregulation verbunden. Referenz: Angelidis et al. (2016).

## Was sind Hjorth-Parameter?
Drei Zeitbereichsmerkmale nach Hjorth (1970): Aktivität (Signalvarianz / Gesamtleistung), Mobilität (Schätzung der mittleren Frequenz) und Komplexität (Bandbreite / Abweichung von einer reinen Sinuswelle). Sie sind recheneffizient und werden häufig in EEG-ML-Pipelines verwendet.

## Welche nichtlinearen Komplexitätsmaße werden berechnet?
Vier Maße: Permutationsentropie (ordinale Musterkomplexität, Bandt & Pompe 2002), Higuchi Fraktale Dimension (fraktale Signalstruktur, Higuchi 1988), DFA-Exponent (langreichweitige zeitliche Korrelationen, Peng et al. 1994) und Stichprobenentropie (Signalregularität, Richman & Moorman 2000). Alle werden über die 4 EEG-Kanäle gemittelt.

## Was sind SEF95, Spektraler Schwerpunkt, PAC und Lateralitätsindex?
SEF95 (Spektrale Kantenfrequenz) ist die Frequenz, unterhalb derer 95% der Gesamtleistung liegt — wird in der Anästhesieüberwachung verwendet. Der Spektrale Schwerpunkt ist die leistungsgewichtete mittlere Frequenz (Erregungsindikator). PAC (Phasen-Amplituden-Kopplung) misst die Theta-Gamma-Kreuzfrequenz-Interaktion, die mit Gedächtniskodierung verbunden ist. Der Lateralitätsindex ist die allgemeine Links/Rechts-Leistungsasymmetrie über alle Bänder.

## Welche PPG-Metriken werden berechnet?
Bei Muse 2/S (mit PPG-Sensor): Herzfrequenz (bpm), RMSSD/SDNN/pNN50 (Herzfrequenzvariabilität — parasympathischer Tonus), LF/HF-Verhältnis (sympathovagales Gleichgewicht), Atemfrequenz (aus PPG-Hüllkurve), SpO₂-Schätzung (unkalibriert), Perfusionsindex (periphere Durchblutung) und Baevsky-Stressindex (autonomer Stress).

## Wie verwende ich den Fokus-Timer?
Öffnen Sie den Fokus-Timer über das Tray-Menü, die Befehlspalette (⌘K → "Fokus-Timer") oder den globalen Kurzbefehl (standardmäßig ⌘⇧P). Wählen Sie ein Preset — Pomodoro (25/5), Tiefes Arbeiten (50/10) oder Kurzfokus (15/5) — oder legen Sie benutzerdefinierte Dauern fest. Aktivieren Sie "EEG automatisch beschriften", damit NeuroSkill™ EEG-Aufnahmen am Anfang und Ende jeder Fokusphase automatisch markiert. Sitzungspunkte verfolgen abgeschlossene Runden. Ihre Einstellungen werden automatisch gespeichert.

## Wie verwalte ich meine Annotationen?
Öffnen Sie das Labels-Fenster über die Befehlspalette (⌘K → "Alle Labels"). Es zeigt alle Annotationen mit Inline-Textbearbeitung (Label anklicken, ⌘↵ zum Speichern oder Esc zum Abbrechen), Löschen (mit Bestätigung) und Metadaten mit dem EEG-Zeitbereich. Verwenden Sie das Suchfeld zum Filtern. Labels werden bei großen Archiven in Seiten à 50 angezeigt.

## Wie vergleiche ich zwei Sitzungen nebeneinander?
Klicken Sie auf der Verlaufsseite auf "Schnellvergleich", um den Vergleichsmodus zu aktivieren. Auf jeder Sitzungszeile erscheinen Kontrollkästchen — wählen Sie genau zwei aus und klicken Sie auf "Ausgewählte vergleichen". Alternativ öffnen Sie den Vergleich über das Tray oder die Befehlspalette und wählen die Sitzungen manuell über Dropdowns.

## Wie funktioniert die Texteinbettungssuche?
Ihre Abfrage wird durch dasselbe Satz-Transformer-Modell in einen Vektor umgewandelt, der auch Ihre Labels indiziert. Dieser Vektor wird dann per Nächste-Nachbarn-Suche mit dem HNSW-Label-Index abgeglichen. Ergebnisse sind Ihre eigenen Anmerkungen, nach semantischer Ähnlichkeit geordnet — die Suche nach "ruhig und fokussiert" findet Labels wie "tiefes Lesen" oder "Meditation", auch wenn diese Wörter nie in Ihrer Abfrage vorkamen. Erfordert das heruntergeladene Einbettungsmodell und den aufgebauten Label-Index (Einstellungen → Einbettungen).

## Wie funktioniert die interaktive multimodale Suche?
Die interaktive Suche verbindet Text, EEG und Zeit in einer einzigen Abfrage. Schritt 1: Textabfrage wird eingebettet. Schritt 2: Die text-k semantisch ähnlichsten Labels werden gefunden. Schritt 3: Für jedes Label berechnet {app} die mittlere EEG-Einbettung über sein Aufnahmefenster und ruft die eeg-k nächsten EEG-Epochen aus allen Tages-Indizes ab — von Sprache in Gehirnzustand-Raum. Schritt 4: Für jeden gefundenen EEG-Moment werden Anmerkungen innerhalb von ±Reichweite Minuten als "gefundene Labels" gesammelt. Die vier Knotenebenen (Abfrage → Texttreffer → EEG-Nachbarn → Gefundene Labels) werden als gerichteter Graph dargestellt. Als SVG oder DOT exportierbar.

## Wie löse ich TTS-Sprache aus einem Skript oder Automatisierungstool aus?
Verwenden Sie die WebSocket- oder HTTP-API. WebSocket: {"command":"say","text":"Ihre Nachricht"}. HTTP (curl): curl -X POST http://localhost:<port>/say -H 'Content-Type: application/json' -d '{"text":"Ihre Nachricht"}'. Der say-Befehl ist Fire-and-Forget — antwortet sofort, während Audio im Hintergrund abgespielt wird.

## Warum gibt es keinen Ton von TTS?
Prüfen Sie, ob espeak-ng installiert und im PATH ist (brew install espeak-ng auf macOS, apt install espeak-ng auf Ubuntu). Prüfen Sie, ob der Audioausgang nicht stummgeschaltet ist. Beim ersten Start muss das Modell (~30 MB) erst heruntergeladen werden. Aktivieren Sie die TTS-Debug-Protokollierung unter Einstellungen → Sprache.

## Kann ich die TTS-Stimme oder Sprache ändern?
Die aktuelle Version verwendet die Jasper-Stimme (Englisch en-us) aus dem Modell KittenML/kitten-tts-mini-0.8. Nur englischer Text wird korrekt phonemisiert. Weitere Stimmen und Sprachunterstützung sind für zukünftige Versionen geplant.

## Benötigt TTS eine Internetverbindung?
Nur einmalig für den ersten ~30 MB großen Modell-Download von HuggingFace Hub. Danach läuft die gesamte Synthese vollständig offline. Das Modell wird unter ~/.cache/huggingface/hub/ zwischengespeichert.

## Welche OpenBCI-Boards unterstützt NeuroSkill™?
NeuroSkill™ unterstützt alle Boards im OpenBCI-Ökosystem: Ganglion (4K, BLE), Ganglion + WiFi-Shield (4K, 1 kHz), Cyton (8K, USB-Dongle), Cyton + WiFi-Shield (8K, 1 kHz), Cyton+Daisy (16K, USB-Dongle), Cyton+Daisy + WiFi-Shield (16K, 1 kHz) und Galea (24K, UDP). Alle können parallel zu einem anderen BCI-Gerät betrieben werden. Board in Einstellungen → OpenBCI auswählen und auf Verbinden klicken.

## Wie verbinde ich den Ganglion per Bluetooth?
1. Ganglion einschalten — blaue LED sollte langsam blinken. 2. In Einstellungen → OpenBCI 'Ganglion — 4K · BLE' wählen. 3. Einstellungen speichern, dann Verbinden klicken. NeuroSkill™ scannt bis zum konfigurierten Timeout (Standard 10 s). Board innerhalb von 3–5 m halten. Auf macOS beim ersten Mal die Bluetooth-Berechtigung erteilen.

## Mein Ganglion ist eingeschaltet, aber NeuroSkill™ findet ihn nicht — was tun?
1. Blaue LED blinkt? Wenn nicht, Taste drücken zum Aufwecken. 2. BLE-Scan-Timeout in Einstellungen erhöhen. 3. Board auf unter 2 m annähern. 4. NeuroSkill™ beenden und neu starten. 5. Bluetooth aus- und wieder einschalten. 6. Sicherstellen, dass kein anderes Programm (OpenBCI GUI) mit dem Ganglion verbunden ist — BLE erlaubt nur eine Verbindung gleichzeitig. 7. macOS: Bluetooth-Berechtigung in Systemeinstellungen → Datenschutz → Bluetooth prüfen.

## Wie verbinde ich einen Cyton per USB?
1. USB-Funk-Dongle an den Computer anschließen. 2. Cyton einschalten (Schalter auf PC). 3. In Einstellungen → OpenBCI 'Cyton — 8K · USB seriell' wählen. 4. Auf Aktualisieren klicken und Port auswählen (/dev/cu.usbserial-… auf macOS, /dev/ttyUSB0 auf Linux, COM3 auf Windows), oder leer lassen für automatische Erkennung. 5. Einstellungen speichern und Verbinden klicken.

## Der serielle Port wird nicht angezeigt oder ich erhalte einen Berechtigungsfehler — was tun?
macOS: Dongle erscheint als /dev/cu.usbserial-*. Falls nicht vorhanden, CP210x- oder FTDI-VCP-Treiber installieren. Linux: sudo usermod -aG dialout $USER ausführen und ab-/anmelden. Prüfen, ob /dev/ttyUSB0 nach dem Anstecken erscheint. Windows: CP2104-USB-UART-Treiber installieren; COM-Port erscheint in Geräte-Manager → Anschlüsse.

## Wie nutze ich das OpenBCI WiFi-Shield?
1. WiFi-Shield auf Cyton oder Ganglion aufstecken und Board einschalten. 2. Computer mit dem WLAN des Shields verbinden (SSID: OpenBCI-XXXX). 3. In Einstellungen die passende WiFi-Board-Variante auswählen. 4. IP 192.168.4.1 eingeben oder leer lassen für automatische Erkennung. 5. Verbinden klicken. Das WiFi-Shield überträgt mit 1000 Hz — Tiefpassfilter auf ≤ 500 Hz einstellen.

## Was ist das Galea-Board und wie richte ich es ein?
Galea ist ein 24-Kanal-Forschungsheadset (EEG + EMG + AUX) von OpenBCI, das per UDP überträgt. 1. Galea einschalten und mit dem lokalen Netzwerk verbinden. 2. In Einstellungen → OpenBCI 'Galea — 24K · UDP' auswählen. 3. IP-Adresse eingeben oder leer lassen. 4. Verbinden klicken. Kanäle 1–8 sind EEG (Echtzeit-Analyse); 9–16 EMG; 17–24 AUX. Alle 24 Kanäle werden in CSV gespeichert.

## Kann ich zwei BCI-Geräte gleichzeitig verwenden?
Ja — NeuroSkill™ kann von beiden gleichzeitig streamen. Das zuerst verbundene Gerät steuert das Live-Dashboard, die Bandleistungsanzeige und die EEG-Embedding-Pipeline. Die Daten des zweiten Geräts werden zur Offline-Analyse in CSV aufgezeichnet. Gleichzeitige Multi-Geräte-Analyse in der Echtzeit-Pipeline ist für eine zukünftige Version geplant.

## Nur 4 der 8 Cyton-Kanäle werden für die Live-Analyse genutzt — warum?
Die Echtzeit-Analyse-Pipeline (Filter, Bandleistungen, EEG-Embeddings, Signalqualitätspunkte) ist derzeit für 4-Kanal-Eingaben ausgelegt, um dem Muse-Headset-Format zu entsprechen. Bei Cyton (8K) und Cyton+Daisy (16K) speisen die Kanäle 1–4 die Live-Pipeline; alle Kanäle werden für die Offline-Arbeit in CSV geschrieben. Vollständige Multi-Kanal-Pipeline-Unterstützung ist geplant.

## Wie verbessere ich die Signalqualität bei OpenBCI?
1. Leitfähiges Gel an jedem Elektrodenstandort auftragen und Haare beiseiteschieben für direkten Hautkontakt. 2. Impedanz mit dem OpenBCI GUI prüfen (Ziel: < 20 kΩ). 3. SRB-Elektrode am Mastoid (hinter dem Ohr) befestigen. 4. Elektrodenkabel kurz halten und von Stromkabeln fernhalten. 5. Kerbfilter in Einstellungen → Signalverarbeitung aktivieren (50 Hz für Europa). 6. Beim Ganglion BLE: Board von USB-3.0-Ports fernhalten — sie stören das 2,4-GHz-Band.

## Unterstützt {app} das AWEAR-Stirnband?
Ja. AWEAR ist ein Einkanal-BLE-EEG-Gerät mit einer Abtastrate von 256 Hz. Die Verbindung funktioniert wie bei anderen BLE-Geräten — schalten Sie das Stirnband ein, erteilen Sie bei Aufforderung die Bluetooth-Berechtigung, und {app} erkennt und verbindet sich automatisch. Der einzelne EEG-Kanal treibt die Echtzeit-Analyse-Pipeline an.

## Die OpenBCI-Verbindung bricht häufig ab — wie stabilisiere ich sie?
Ganglion BLE: Board innerhalb von 2 m halten; BLE-Adapter in einen USB-2.0-Port stecken (USB 3.0 stört 2,4 GHz). Cyton USB: Kurzes, hochwertiges USB-Kabel direkt am Computer anschließen, nicht über Hub. WiFi-Shield: WLAN-Kanal des Shields nicht mit dem Router überlappen lassen; Board nah am Computer positionieren. Allgemein: Keine wireless-intensiven Programme (Videokonferenzen, Datei-Sync) während der Aufnahme.

## Was genau zeichnet die Aktivitätsverfolgung auf?
Aktive-Fenster-Verfolgung schreibt eine Zeile in activity.sqlite, wenn sich die vorderste App oder der Fenstertitel ändert: App-Anzeigename (z. B. "Safari", "VS Code"), vollständiger Pfad zum Bundle oder zur ausführbaren Datei, Fenstertitel (z. B. Dokumentname oder Webseite — kann bei Sandbox-Apps leer sein) und Unix-Sekunden-Zeitstempel. Tastatur- und Mausverfolgung schreibt alle 60 Sekunden einen Stichprobeneintrag, aber nur wenn seit dem letzten Flush Aktivität stattfand: zwei Zeitstempel — letztes Tastaturereignis und letztes Maus-/Trackpad-Ereignis. Es werden keine Tastenanschläge, kein eingegebener Text, keine Cursorpositionen und keine Klickziele aufgezeichnet.

## Warum fragt macOS nach Bedienungshilfen-Zugriff für die Input-Verfolgung?
Die Tastatur- und Mausverfolgung verwendet einen CGEventTap — eine macOS-API, die systemweite Eingabeereignisse abfängt. Apple verlangt die Bedienungshilfen-Berechtigung für jede App, die globale Eingaben liest. Ohne diese Berechtigung schlägt der Tap lautlos fehl: NeuroSkill funktioniert weiterhin normal, aber Tastatur- und Maus-Zeitstempel bleiben bei null. Zum Erteilen: Systemeinstellungen → Datenschutz & Sicherheit → Bedienungshilfen → NeuroSkill → aktivieren. Wenn Sie die Berechtigung nicht erteilen möchten, deaktivieren Sie einfach "Tastatur- & Mausaktivität verfolgen" in den Einstellungen — dadurch wird der Hook gar nicht erst installiert.

## Wie lösche ich die Aktivitätsverfolgungsdaten?
Alle Aktivitätsdaten liegen in einer einzigen Datei: ~/.skill/activity.sqlite. Um alles zu löschen: App beenden, Datei löschen, neu starten — eine leere Datenbank wird automatisch erstellt. Um künftige Erfassung zu stoppen, ohne bestehende Daten zu löschen, deaktivieren Sie beide Umschalter in Einstellungen → Aktivitätsverfolgung (sofort wirksam). Zum gezielten Entfernen von Zeilen öffnen Sie die Datei in einem SQLite-Browser und verwenden DELETE FROM active_windows oder DELETE FROM input_activity.

## Warum fordert {app} auf macOS die Berechtigung "Bedienungshilfen" an?
{app} nutzt die macOS-CGEventTap-API, um den letzten Zeitstempel einer Tastatureingabe oder Mausbewegung zu erfassen. Dieser dient zur Berechnung von Aktivitätszeitstempeln im Bereich Aktivitätsverfolgung. Es werden nur Zeitstempel gespeichert – keine Tastenanschläge, keine Cursorpositionen. Die Funktion deaktiviert sich lautlos, wenn die Berechtigung fehlt.

## Benötigt {app} eine Bluetooth-Berechtigung?
Ja. {app} nutzt Bluetooth Low Energy (BLE), um eine Verbindung zum BCI-Headset herzustellen. Auf macOS erscheint beim ersten Scan-Versuch ein einmaliger Berechtigungs-Dialog. Unter Linux und Windows ist keine explizite Bluetooth-Berechtigung erforderlich.

## Wie erteile ich auf macOS die Berechtigung "Bedienungshilfen"?
Öffnen Sie Systemeinstellungen → Datenschutz & Sicherheit → Bedienungshilfen. Suchen Sie {app} in der Liste und schalten Sie den Schalter ein. Sie können auch auf «Bedienungshilfen-Einstellungen öffnen» im Tab Berechtigungen innerhalb der App klicken.

## Was passiert, wenn ich die Berechtigung "Bedienungshilfen" verweigere?
Tastatur- und Maus-Aktivitätszeitstempel werden nicht erfasst und bleiben bei null. Alle anderen Funktionen – EEG-Streaming, Bandleistungen, Kalibrierung, TTS, Suche – funktionieren weiterhin normal. Sie können die Funktion unter Einstellungen → Aktivitätsverfolgung vollständig deaktivieren.

## Kann ich erteilte Berechtigungen widerrufen?
Ja. Öffnen Sie Systemeinstellungen → Datenschutz & Sicherheit → Bedienungshilfen (oder Benachrichtigungen) und deaktivieren Sie {app}. Die betreffende Funktion hört sofort auf zu funktionieren – ohne Neustart.
