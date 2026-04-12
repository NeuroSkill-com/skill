# Übersicht
NeuroSkill enthält einen optionalen lokalen LLM-Server, der Ihnen einen privaten, OpenAI-kompatiblen KI-Assistenten bietet, ohne Daten in die Cloud zu senden.

## Was ist die LLM-Funktion?
Die LLM-Funktion bettet einen auf llama.cpp basierenden Inferenzserver direkt in die App ein. Bei Aktivierung stellt er OpenAI-kompatible Endpunkte (/v1/chat/completions, /v1/completions, /v1/embeddings, /v1/models, /health) auf demselben lokalen Port wie die WebSocket-API bereit. Sie können jeden OpenAI-kompatiblen Client darauf ausrichten.

## Datenschutz & Offline-Nutzung
Alle Inferenz läuft auf Ihrem Rechner. Keine Tokens, Prompts oder Vervollständigungen verlassen je localhost. Die einzige Netzwerkaktivität ist der erstmalige Modell-Download von HuggingFace Hub. Sobald ein Modell lokal gecacht ist, können Sie sich vollständig vom Internet trennen.

## OpenAI-kompatible API
Der Server spricht dasselbe Protokoll wie die OpenAI-API. Jede Bibliothek mit base_url-Parameter (openai-python, openai-node, LangChain, LlamaIndex usw.) funktioniert sofort. Setzen Sie base_url auf http://localhost:<port>/v1 und lassen Sie den API-Schlüssel leer, sofern nicht konfiguriert.

# Modellverwaltung
GGUF-quantisierte Sprachmodelle aus dem integrierten Katalog durchsuchen, herunterladen und aktivieren.

## Modellkatalog
Der Katalog listet kuratierte Modellfamilien (z. B. Qwen, Llama, Gemma, Phi) mit mehreren Quantisierungsvarianten. Verwenden Sie das Dropdown zum Durchsuchen und wählen Sie eine Quantisierung zum Herunterladen. Mit ★ markierte Modelle sind die empfohlene Standardwahl.

## Quantisierungsstufen
Jedes Modell ist in mehreren GGUF-Quantisierungsstufen verfügbar (Q4_K_M, Q5_K_M, Q6_K, Q8_0 usw.). Niedrigere Quants sind kleiner und schneller, opfern aber Qualität. Q4_K_M ist meist der beste Kompromiss. Q8_0 ist nahezu verlustfrei, benötigt aber doppelt so viel Speicher. BF16/F16/F32 sind unquantisierte Referenzgewichte.

## Hardware-Kompatibilität
Jede Quantisierungszeile zeigt ein farbcodiertes Abzeichen: 🟢 Läuft hervorragend — passt vollständig in GPU-VRAM. 🟡 Läuft gut — knapper Spielraum. 🟠 Knapp — teilweises CPU-Offloading nötig. 🔴 Passt nicht — zu groß für den Speicher. Berücksichtigt GPU-VRAM, System-RAM, Modellgröße und Kontext-Overhead.

## Vision- / Multimodale Modelle
Familien mit dem Tag Vision oder Multimodal enthalten eine optionale multimodale Projektordatei (mmproj). Laden Sie sowohl das Textmodell als auch seinen Projektor herunter, um Bildeingaben im Chat-Fenster zu aktivieren. Der Projektor erweitert das Textmodell — er ist kein eigenständiges Modell.

## Herunterladen & Löschen
Klicken Sie auf 'Herunterladen', um ein Modell von HuggingFace Hub abzurufen. Ein Fortschrittsbalken zeigt den Echtzeit-Status. Sie können jederzeit abbrechen. Heruntergeladene Modelle werden lokal gespeichert und können gelöscht werden. Verwenden Sie 'Cache aktualisieren', um den Katalog erneut zu scannen.

# Inferenzeinstellungen
Optimieren Sie, wie der Server Modelle lädt und ausführt.

## GPU-Schichten
Steuert, wie viele Transformer-Schichten auf die GPU ausgelagert werden. 'Alle' für maximale Geschwindigkeit, 0 für reine CPU-Inferenz. Zwischenwerte teilen das Modell zwischen GPU und CPU auf — nützlich, wenn das Modell die VRAM-Kapazität knapp übersteigt.

## Kontextgröße
Die KV-Cache-Größe in Token. 'Auto' verwendet den Standardwert des Modells. Größere Kontexte speichern mehr Gesprächsverlauf, verbrauchen aber mehr Speicher. Bei Speicherfehlern auf 4K oder 2K reduzieren.

## Parallele Anfragen
Maximale Anzahl gleichzeitiger Dekodierungsschleifen. Höhere Werte lassen mehrere Clients den Server teilen, erhöhen aber den Spitzenverbrauch. Für Einzelbenutzer-Setups reicht 1.

## API-Schlüssel
Ein optionales Bearer-Token, das bei jeder /v1/*-Anfrage erforderlich ist. Leer lassen für offenen Zugang auf localhost. Setzen Sie einen Schlüssel, wenn Sie den Port im lokalen Netzwerk freigeben möchten.

# Integrierte Werkzeuge
Der LLM-Chat kann lokale Werkzeuge aufrufen, um Informationen zu sammeln oder Aktionen in Ihrem Auftrag durchzuführen.

## So funktionieren Werkzeuge
Wenn die Werkzeugnutzung aktiviert ist, kann das Modell Werkzeugaufrufe anfordern. Die App führt das Werkzeug lokal aus und gibt das Ergebnis zurück, damit das Modell reale Informationen einbeziehen kann. Werkzeuge werden nur aufgerufen, wenn das Modell sie explizit anfordert — sie laufen nie im Hintergrund.

## Sichere Werkzeuge
Datum, Standort, Websuche, Web-Abruf und Datei lesen sind schreibgeschützte Werkzeuge, die Ihr System nicht verändern können. Datum gibt das aktuelle Datum und die Uhrzeit zurück. Standort liefert eine ungefähre IP-basierte Geolokalisierung. Websuche führt eine DuckDuckGo-Abfrage durch. Web-Abruf ruft den Textinhalt einer URL ab. Datei lesen liest lokale Dateien mit optionaler Paginierung.

## Privilegierte Werkzeuge (⚠️)
Bash, Datei schreiben und Datei bearbeiten können Ihr System verändern. Bash führt Shell-Befehle mit denselben Berechtigungen wie die App aus. Datei schreiben erstellt oder überschreibt Dateien. Datei bearbeiten führt Suchen-und-Ersetzen durch. Standardmäßig deaktiviert mit Warnabzeichen. Nur aktivieren, wenn Sie die Risiken verstehen.

## Ausführungsmodus & Grenzen
Parallelmodus ruft mehrere Werkzeuge gleichzeitig auf (schneller). Sequentieller Modus führt sie einzeln aus (sicherer bei Seiteneffekten). 'Max. Runden' begrenzt Werkzeugaufruf-Roundtrips pro Nachricht. 'Max. Aufrufe pro Runde' begrenzt gleichzeitige Aufrufe.

# Chat & Protokolle
Mit dem Modell interagieren und die Serveraktivität überwachen.

## Chat-Fenster
Öffnen Sie das Chat-Fenster über die LLM-Server-Karte oder das Tray-Menü. Es bietet Markdown-Rendering, Code-Hervorhebung und Werkzeugaufruf-Visualisierung. Gespräche sind flüchtig — nicht auf der Festplatte gespeichert. Visionsfähige Modelle akzeptieren Bildanhänge per Drag-and-Drop oder Anhang-Schaltfläche.

## Externe Clients verwenden
Da der Server OpenAI-kompatibel ist, können Sie jedes externe Chat-Frontend verwenden. Richten Sie es auf http://localhost:<port>/v1, setzen Sie einen API-Schlüssel falls konfiguriert und wählen Sie ein Modell aus /v1/models. Optionen: Open WebUI, Chatbot UI, Continue (VS Code), curl/httpie.

## Server-Protokolle
Der Protokoll-Viewer am unteren Rand des LLM-Einstellungsfelds streamt die Serverausgabe in Echtzeit. Er zeigt Ladefortschritt, Token-Geschwindigkeit und Fehler. Aktivieren Sie 'Ausführlich' für detaillierte llama.cpp-Diagnose. Protokolle scrollen automatisch — pausierbar durch manuelles Hochscrollen.
