# BG Tutor

Lokale Backgammon-Bibliothek mit einem eigenständigen **Rust/Tokio/Axum-Backend**, **PostgreSQL/SQLx**, **GNUbg** und einem **React/TypeScript-Client**. Das Backend läuft ohne Browser und ohne Tauri. Der Client verwendet ausschließlich REST und SSE; Tauri kann später denselben Client als Desktop-App hosten.

## Starten

Voraussetzungen: Rust (getestet mit 1.94.1), Node.js 22+ und npm, Docker Compose sowie ein lokales `gnubg` **einschließlich Netzgewichten**. Getestet mit GNUbg 1.05.002 auf macOS. `gnubg --version` muss funktionieren; bei abweichendem Installationsort `GNUBG_BIN` in `.env` setzen. Hier liegt es unter `/opt/local/bin/gnubg`.

Im Repository:

```sh
cp .env.example .env
docker compose up -d --wait db
cargo run -p bgtutor
```

In einem zweiten Terminal:

```sh
cd web
npm ci
npm run dev
```

- Oberfläche: <http://127.0.0.1:5173>
- API/Health: <http://127.0.0.1:8787/api/health>
- OpenAPI 3.1: <http://127.0.0.1:8787/api/openapi.json>
- PostgreSQL: `127.0.0.1:54329`; Docker-Volume `bgtutor_postgres_data`

Migrationen laufen beim Backend-Start automatisch. Cargo benötigt zum Kompilieren keine laufende Datenbank. Nur die `.env` im Startverzeichnis wird geladen; `.env`-Dateien übergeordneter Projekte bleiben außen vor. Konfiguration siehe [.env.example](.env.example).

Alternativ kann das Backend auch den gebauten Client ausliefern:

```sh
npm ci --prefix web
npm run build --prefix web
BGTUTOR_WEB_DIR=web/dist cargo run -p bgtutor --release
```

Dann genügt <http://127.0.0.1:8787>. Der Dienst und die Datenbank binden standardmäßig nur an Loopback. Dieser erste Stand besitzt keine Benutzerverwaltung; eine Netzwerkfreigabe benötigt eine eigene Zugriffskontrolle.

## Durchgängiger Ablauf

1. `backend/tests/fixtures/tutorial.mat` über **Partie importieren** laden, oder die echte Referenzpartie `WC2014.xg` im selben Verzeichnis verwenden.
2. Nach Spieler oder Dateiname suchen (mindestens drei Zeichen). Suchtreffer werden mit stabilen Cursorn seitenweise geladen.
3. Eine Position im Zugverlauf wählen. Das Brett zeigt die Stellung **vor** dem gespielten Zug; Spielstand und Doppler werden mit angezeigt. Punktnummern und Zugnotation folgen dem Spieler am Zug; die Pip-Zahlen berücksichtigen die Bar.
4. **Position analysieren** startet einen echten dauerhaften GNUbg-Auftrag mit 0, 1 oder 2 Ply. Die Ausgabe enthält bis zu fünf Zugkandidaten, Cubeful Equity und Gewinnwahrscheinlichkeiten.
5. **Dice Distribution** berechnet dieselbe Stellung mit allen 21 ungeordneten Würfelpaaren. Die Matrix zeigt alle 36 geordneten Würfe; ein Pasch erhält Gewicht 1, andere Paare Gewicht 2. Das gewichtete Mittel ist die erwartete Equity. Blockierte Würfe werden als echter Zugaussetzer bewertet.
6. **Gesamte Partie analysieren** wertet alle Spiele mit 0–2 Ply aus. Nach jedem Spiel erscheinen Fortschritt, Spielerstatistik und eine filterbare Fehlerübersicht. Jede Checker-Entscheidung zeigt den gespielten Zug, den besten Zug, beide Equities und den Verlust; Cube-Entscheidungen enthalten auch nicht genutzte Doppelangebote sowie Double/Take/Drop. Ein Klick auf die Zugnummer öffnet die zugehörige Brettstellung.
7. **Partiewürfe** zeigt separat die tatsächlich aufgezeichneten Würfe. **Position finden** sucht nach einer exakten GNUbg Position-ID. **Brett** wechselt zwischen Forest/Walnut/Slate und den lokalen BGBlitz-Themes aus `themes/*.zip`. **Sprache** schaltet die vollständige Oberfläche zwischen Deutsch, Español, Français, Polski, Italiano und English um. Beide Einstellungen bleiben im Browser gespeichert; Standard ist Deutsch.

Die Positionssuche identifiziert die Brettstellung aus Sicht des Spielers am Zug. Spielstand, Doppler und Würfel stehen in der zusätzlichen GNUbg Match-ID und werden für die Analyse berücksichtigt. Identische Bretter in verschiedenen Matchsituationen erscheinen daher als getrennte Treffer. Ähnlichkeitssuche und ein grafischer Positionseditor sind nicht Bestandteil dieses Standes.

## Board-Themes und Sprachen

Die Originalgrafiken der lokal bereitgestellten BGBlitz-Archive werden automatisch bei `npm run dev` und `npm run build` eingelesen. Dafür wird **Python 3.10+** ohne zusätzliche Pakete benötigt. Die Darstellung verwendet echte Hintergründe, Checker-Varianten, Würfel, Doppler und Nummernleisten. Neue Archive in `themes/` ablegen und den Entwicklungsserver neu starten oder `npm run themes:prepare --prefix web` ausführen. Ohne lokale ZIPs bleiben die drei eingebauten Themes verfügbar. Format, Herkunft und Rechte: [themes/README.md](themes/README.md).

Die Sprachwahl ist direkt neben der Theme-Auswahl verfügbar. Texte, Status, Fehlermeldungen, Beschriftungen und Zahlenformate wechseln sofort; bereits sichtbare Analyseergebnisse bleiben erhalten. Spieler-/Dateinamen und originale GNUbg-Zugnotationen bleiben ihre ursprünglichen Daten. Native technische Engine-Diagnosen erscheinen zusammen mit einer lokalisierten Fehlermeldung. Schlüssel und Platzhalter aller sechs Kataloge werden typisiert und beim Start geprüft. Die Übersetzungen sind noch nicht muttersprachlich lektoriert.

## Tags, Kommentare und Lernen

Partien und importierte Positionen haben jeweils eigene **Tags und Kommentare**. „Notizen speichern“ schreibt sie nach PostgreSQL; beim Blättern bleiben ungespeicherte Entwürfe in der aktuellen Sitzung erhalten. Parallele Änderungen erzeugen einen sichtbaren Konflikt, ohne den Entwurf zu überschreiben. Kommentare sind Klartext/Markdown und werden als Text dargestellt. Tags werden vereinheitlicht; Grenzen: 30 Tags zu je 64 Zeichen und 20.000 Kommentarzeichen.

**Tags & Kommentare durchsuchen** findet beide Zieltypen. Freitext ab drei Zeichen, exakter Tag und Zieltyp sind kombinierbar; Treffer öffnen die betreffende Partie oder Zugposition. Die normale Bibliothekssuche findet ebenfalls Partien über ihre eigenen Notizen und die Notizen ihrer Positionen. Suchindizes erhalten die schnelle Suche auch bei großen Bibliotheken.

Unter **Positionen als Lernkarten** ein Deck erstellen oder auswählen. Eine bereits analysierte Einzelposition hinzufügen, oder Fehler aus einer abgeschlossenen Partieanalyse nach Spieler und Mindestverlust übernehmen. Checker-Züge, Double-/No-double- und Take-/Pass-Entscheidungen werden unterstützt. Kartenidentität berücksichtigt Brett, vollständigen Matchkontext und Fragetyp; derselbe Lerninhalt kann mehreren Decks angehören. Erneutes Hinzufügen aktualisiert seine Analysequelle und erhält den Lernfortschritt. Die Tags und Kommentare der aktuellen Partie-/Positionsquelle werden beim Aufdecken und Export gelesen. Eine Deckauswahl ist auf 1.000 Karten begrenzt; pro Anfrage werden maximal 100 Karten gelistet.

**Lernen & Anki** zeigt fällige Karten mit dem gewählten Board-Theme, Matchlänge, Spielstand und Spieler am Zug; Money Games nennen zusätzlich den Jacoby-Status. Erst die eigene Antwort überlegen, dann die Lösung aufdecken und mit **Nochmal/Schwer/Gut/Einfach** bewerten. Die Wiederholungsplanung läuft in Rust mit `fsrs`, Zielbehaltensrate **90 %**: Nochmal nach **60 Sekunden**, die erste schwere Bewertung nach **10 Minuten**, spätere Hard-/Good-/Easy-Intervalle aus FSRS in ganzen Tagen. Easy liegt mindestens einen Tag nach Good. Zustand, Bewertungshistorie und Fälligkeit liegen in PostgreSQL. Gleichzeitige oder wiederholte Bewertungsanfragen können denselben Review nicht doppelt anwenden.

**Anki-Deck herunterladen** erzeugt ein eigenständiges `.apkg` vollständig in Rust: stabiles Notizmodell und GUIDs, eigene SVG-Brettdiagramme, Spielstand/Doppler/Würfel, Lösungen, Tags und Kommentare. Es benötigt keine BGBlitz-Bilder und keinen Python-Prozess zur Laufzeit. Die Beschriftung folgt der ausgewählten Sprache. Wiederholter Import aktualisiert vorhandene Notizen; ein persistierter Inhaltsfingerabdruck sorgt auch bei schnellen Kommentar- oder Sprachänderungen für eine neuere Anki-Änderungszeit. Der Export enthält keine BG-Tutor-Review-Historie. **Anki und BG Tutor führen ihre Lernstände getrennt.** Ein bidirektionaler Abgleich, AnkiConnect und der Import fremder Decks gehören nicht zu dieser ersten Ausbaustufe.

Der Aufbau orientiert sich an persönlichen/thematischen Fehlerdecks, wie sie [Dirk Schiemann](https://www.dirk-schiemann.de/the-anki-method.html), [XG2Anki](https://xg2anki.de/) und [AnkiGammon](https://ankigammon.com/) beschreiben. Anki-Pakete und Aktualisierungen: [Anki-Handbuch](https://docs.ankiweb.net/manual/importing/packaged-decks). Wiederholungsbibliothek: [FSRS-rs](https://docs.rs/fsrs/latest/fsrs/).

Die API bietet Annotationen unter `/api/matches/{id}/annotation` und `/api/positions/{id}/annotation` (GET/PUT), Suche unter `/api/annotations/search`, Tag-Vorschläge unter `/api/tags` sowie Decks, Karten, fällige Karten, Aufdecken und Bewertungen unter `/api/decks` und `/api/cards`. Der vollständige Vertrag steht in OpenAPI; der Client verwendet daraus erzeugte Typen.

Optionaler Test mit Ankis eigener Python-Importbibliothek in einem **temporären, leeren Profil**:

```sh
python tools/verify_anki_package.py deck.apkg --expected-cards 10
# Für ein aktualisiertes Paket derselben Karten:
python tools/verify_anki_package.py vorher.apkg --updated nachher.apkg
```

Dafür wird separat das offizielle Python-Paket `anki` benötigt; die Prüfung verändert keine vorhandene Anki-Sammlung. Getestet wurde mit Anki 26.9.4, einschließlich Import, Duplikaterkennung, HTML-Escaping und aktualisiertem Kommentar/Sprachinhalt.

## Importformate

| Format | Verhalten |
| --- | --- |
| `.mat` | GNUbg normalisiert Jellyfish-MAT zu SGF. Parserdiagnosen werden zusätzlich zum Exitcode geprüft, damit beschädigte Dateien nicht als gültiger Teilimport gespeichert werden. |
| `.sgf` | Nativer Parser für Standard-Backgammon, Escape-Sequenzen, mehrere Spiele, Setup-Positionen, Doppler, Scores, Crawford/Jacoby und Zugrekonstruktion. UTF-8; mehrdeutige Variantenbäume werden abgelehnt. |
| `.xg` | Nativer Decoder für binäre XG2-Dateien mit Save-Version 24–40: RichGame-Container, begrenzte Zlib-Dekompression, CRC, Datensätze und Konvertierung zu SGF. Unterstützt inaktive Cube-Platzhalter, abgeschlossene Spiele in offenen Money-Sessions ohne Match-Enddatensatz sowie zusammengefasste Checker-Züge aus Textimporten. Geprüft mit realen Dateien der Versionen 26, 30 und 31; die Referenz mit 460 Positionen wird gegen GNUbg verglichen. |

XG1, bearbeitete/custom XG-Startstellungen, automatische Doppel, Beavers/Raccoons und andere Spielvarianten werden mit einer Fehlermeldung abgelehnt. Vorhandene XG-Analysen, Rollouts und Kommentare werden noch nicht übernommen. Die Originaldatei bleibt gespeichert. Details und Quellen: [Import-Fixtures](backend/tests/fixtures/README.md).

Ein Upload enthält genau eine Datei; Limit: **16 MiB einschließlich Multipart-Rahmen**. SHA-256 erkennt identische Dateien. Original, kanonische SGF und Positionen werden in einer Transaktion gespeichert. Dateinamen und Inhalte werden niemals als Shellbefehle ausgeführt. Parser und Archive haben zusätzliche Mengen-/Größenlimits.

## API und Aufträge

| Methode | Pfad | Zweck |
| --- | --- | --- |
| GET | `/api/health` | PostgreSQL- und GNUbg-Verfügbarkeit |
| POST | `/api/imports` | Multipart-Feld `file`; 201 neu, 200 Duplikat |
| GET | `/api/matches?q=&after=&limit=` | Partiensuche; Limit 1–100 |
| GET | `/api/matches/{id}` | Partiemetadaten |
| GET | `/api/matches/{id}/source` | Kanonische SGF herunterladen |
| GET | `/api/matches/{id}/dice` | Beobachtete Würfelhäufigkeiten |
| GET | `/api/positions?match_id=&position_id=&after=&limit=` | Partiepositionen oder exakte Brettsuche |
| POST | `/api/jobs` | `{ "position_id": "<Positions-UUID>", "kind": "analysis", "plies": 0 }` |
| GET | `/api/jobs/{id}` | Zustand, Fortschritt, Ergebnis oder Fehler |
| GET | `/api/jobs/{id}/events` | Persistenter SSE-Strom |
| POST | `/api/matches/{id}/analysis` | Ganze Partie analysieren: `{ "plies": 0 }`; aktive identische Aufträge werden zusammengeführt |
| GET | `/api/matches/{id}/analysis` | Letzter Partieauftrag mit Teil-/Endergebnis, sonst `null` |
| GET | `/api/jobs/{id}/moves?after=&limit=&errors_only=&player=&kind=` | Paginierte Zugbewertung; `player` 0/1, `kind` checker/cube, `errors_only=true` ab 0,04 Equity |

```sh
curl -F file=@backend/tests/fixtures/tutorial.mat http://127.0.0.1:8787/api/imports
curl 'http://127.0.0.1:8787/api/matches?q=Tutor'
```

Aufträge durchlaufen `queued → running → completed|failed`. Ein Worker pro Backend begrenzt die parallele Analyse. PostgreSQL `FOR UPDATE SKIP LOCKED` verhindert doppelte Beanspruchung, aktive identische Aufträge werden zusammengeführt. Positionsanalyse und Import haben ein Prozesslimit von 90 Sekunden, die native Analyse eines ganzen Spiels 600 Sekunden. Eine Worker-Lease gilt 120 Sekunden und wird während der Arbeit alle 20 Sekunden erneuert. Verwaiste Aufträge werden nach Ablauf erneut versucht, maximal drei Versuche. Ein normaler Enginefehler beendet den Auftrag als `failed`; die Oberfläche kann einen neuen Auftrag starten.

SSE-Ereignisse heißen `queued`, `running`, `progress`, `completed` und `failed`. `id:` ist eine dauerhafte Ereignisnummer, `data:` enthält `JobProgress`-JSON. `Last-Event-ID` setzt nach der angegebenen ID fort; ohne Header werden alle bisherigen Ereignisse abgespielt. Es folgen Keepalives, bis der Auftrag endet. SIGINT/SIGTERM beendet auch aktive Streams und laufende Kindprozesse. Der nächste Start übernimmt liegengebliebene Aufträge nach Lease-Ablauf. Der Browser merkt sich die zuletzt gestarteten Aufträge je Position und gleicht SSE zusätzlich durch Polling ab. Den letzten Partieauftrag lädt er direkt aus der Datenbank; die Anzeige bleibt deshalb auch ohne Browser-Cache verfügbar.

Partieaufträge speichern Entscheidungen, Teilstatistik und SSE-Fortschritt nach jedem vollständig analysierten Spiel in einer Transaktion. `Job` und SSE enthalten `completed_games` und `total_games`; der Prozentwert bezieht sich auf Spiele, nicht auf Laufzeit. Während eines langen Spiels bleibt dieser Wert unverändert. Nach einem Neustart wird nach Lease-Ablauf beim nächsten unvollständigen Spiel fortgesetzt. Ein alter Worker kann keine Checkpoints oder Endergebnisse schreiben. Bei einem Enginefehler bleiben fertige Spiele und ihre Fehlerübersicht abrufbar; ein neu gestarteter Auftrag analysiert die Partie erneut.

Die Bewertung verwendet GNUbgs native `analyse game`-Ausgabe mit sechs Nachkommastellen, einschließlich der tatsächlich gespielten Alternative außerhalb der besten fünf Kandidaten. Vorhandene Analyseannotationen werden für die Berechnung entfernt. Geerbte Matchdaten und Setup-Stellungen werden für GNUbg explizit normalisiert; Brett- und Matchkontext werden für jede Checker-/Cube-Aktion zurückgeprüft. Gemischte SGF-Sammlungen können auch leere oder reine Cube-Spiele enthalten. Ein fehlender oder unvollständiger nativer Export führt zu einem Fehler; native Exportdateien sind auf jeweils 64 MiB begrenzt. Erzwungenes Aussetzen hat Verlust 0 und keine alternative Equity (`evaluation_status=forced`). Fehlerklassen: ungenau ab 0,04, Fehler ab 0,08, grober Fehler ab 0,16 normierter Equity. Die Summen pro Spieler sind **kein PR**. Cube-Equities werden aus Sicht des entscheidenden Spielers in Einheiten des Dopplers vor dem Angebot ausgewiesen. Reine Cube-Entscheidungen haben noch keinen anklickbaren Checker-Datensatz.

Die OpenAPI-Datei [docs/openapi.json](docs/openapi.json) und die tatsächlich verwendeten TypeScript-Typen werden aus den Rust-Handlern erzeugt, ohne laufenden Server:

```sh
npm run api:types --prefix web
```

## Verifikation

Schnelle Tests und Builds:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test --prefix web
npm run test:themes --prefix web
npm run build --prefix web
```

Echte Engine-, PostgreSQL- und Performancetests (Compose muss laufen):

```sh
DATABASE_URL=postgres://bgtutor:bgtutor_local@127.0.0.1:54329/bgtutor cargo test --workspace -- --ignored --nocapture
```

Die SQLx-Tests erstellen und entfernen eigene Datenbanken. Sie prüfen Import/Duplikate, REST-Paginierung, echte Analyse, SSE-Replay/Shutdown, aktive Job-Deduplizierung einschließlich gleichzeitigem Abschluss, atomare Beanspruchung durch zwei Worker, Lease-Recovery, 21 Würfelpaare und Gewichte. Die Partieanalyse wird über alle zehn Spiele und 460 Züge der Referenzpartie geprüft, einschließlich schlechter Checker-Züge, erzwungenem Aussetzen, falschem Double/Drop, 0–2 Ply, fortsetzbaren Checkpoints und dem Ausschluss veralteter Worker. Weitere Regressionstests prüfen geerbte SGF-Matchdaten, leere/Cube-Spiele, Setups mit Schwarz am Zug, stille lange Engine-Prozesse und die gleichzeitige Fortschrittsspeicherung/Lease-Verlängerung. Parser-Tests vergleichen alle 460 Zugpositionen der offiziellen XG-Referenz mit dem unabhängig durch GNUbg importierten MAT.

Der Performancetest erzeugt 50.000 Partien und 250.000 Positionen und prüft die tatsächlichen PostgreSQL-Pläne: GIN-Trigrammindex für eine selektive Namenssuche und B-tree für die exakte Position-ID. Eine lokale Messung am 8. November 2026 ergab **0,279 ms / 0,036 ms** Datenbank-Ausführungszeit. Das sind selektive, warme Testabfragen ohne HTTP-/UI-Zeit, kein allgemeines Latenzversprechen.

Browserprüfung bei laufendem Backend und Vite; verwendet installiertes Google Chrome:

```sh
npm run test:e2e --prefix web
```

Die Browserprüfungen importieren klar benannte Demo-Partien in die lokale Bibliothek und prüfen Analyse, Würfelmatrix, Suche, alle sechs Sprachen, lokale ZIP-Themes, Wiederaufnahme von Ergebnissen und ein 390 px breites Layout. Weitere Abläufe prüfen Notizen mit Versionskonflikten, direkte Positionssprünge, Deckanlage, verdeckte Lernantworten, persistierte Bewertungen, Checker-/Cube-Karten und Anki-Downloads. Eine verzögerte API-Antwort prüft außerdem, dass beim schnellen Wechsel zwischen analysierten Positionen die zuletzt angeklickte Position ausgewählt bleibt. Screenshots liegen unter `web/test-results/`.

## Struktur und nächste Schritte

- `backend/src/import/`: SGF/XG, Regelprüfung und GNUbg-IDs
- `backend/src/engine.rs`, `match_analysis.rs`: isolierte GNUbg-Prozesse und native Checker-/Cube-Auswertung
- `backend/src/match_jobs.rs`: dauerhafte Spiel-Checkpoints und Spielerstatistik
- `backend/src/store.rs`, `jobs.rs`: SQLx-Zugriff, transaktionale Jobzustände und Events
- `backend/src/api.rs`: REST, SSE, OpenAPI
- `backend/migrations/`: Schema und Suchindizes
- `web/src/`: Client, SVG-Brett, Ergebnisse und aus OpenAPI erzeugte Typen

Dieser Stand analysiert ausgewählte Positionen und vollständige Partien einschließlich Cube-Entscheidungen. Rollouts, Analyse-Caching und eine Tauri-Hülle sind Folgeausbau. Ein späterer Tauri-Client kann das Backend als Sidecar starten und dieselben HTTP-Endpunkte verwenden; Rust-Domainlogik liegt nicht im UI.
