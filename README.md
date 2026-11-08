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
6. **Partiewürfe** zeigt separat die tatsächlich aufgezeichneten Würfe. **Position finden** sucht nach einer exakten GNUbg Position-ID. **Forest/Walnut/Slate** wechseln das Brettdesign; die Auswahl bleibt im Browser gespeichert.

Die Positionssuche identifiziert die Brettstellung aus Sicht des Spielers am Zug. Spielstand, Doppler und Würfel stehen in der zusätzlichen GNUbg Match-ID und werden für die Analyse berücksichtigt. Identische Bretter in verschiedenen Matchsituationen erscheinen daher als getrennte Treffer. Ähnlichkeitssuche und ein grafischer Positionseditor sind nicht Bestandteil dieses Standes.

## Importformate

| Format | Verhalten |
| --- | --- |
| `.mat` | GNUbg normalisiert Jellyfish-MAT zu SGF. Parserdiagnosen werden zusätzlich zum Exitcode geprüft, damit beschädigte Dateien nicht als gültiger Teilimport gespeichert werden. |
| `.sgf` | Nativer Parser für Standard-Backgammon, Escape-Sequenzen, mehrere Spiele, Setup-Positionen, Doppler, Scores, Crawford/Jacoby und Zugrekonstruktion. UTF-8; mehrdeutige Variantenbäume werden abgelehnt. |
| `.xg` | Nativer Decoder für binäre XG2-Dateien mit Save-Version 24–40: RichGame-Container, begrenzte Zlib-Dekompression, CRC, Datensätze und Konvertierung zu SGF. Mit einer echten Version-31-Datei über 460 Positionen geprüft. |

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

```sh
curl -F file=@backend/tests/fixtures/tutorial.mat http://127.0.0.1:8787/api/imports
curl 'http://127.0.0.1:8787/api/matches?q=Tutor'
```

Aufträge durchlaufen `queued → running → completed|failed`. Ein Worker pro Backend begrenzt die parallele Analyse. PostgreSQL `FOR UPDATE SKIP LOCKED` verhindert doppelte Beanspruchung, aktive identische Aufträge werden zusammengeführt. Ein GNUbg-Prozess bekommt maximal 90 Sekunden; eine Worker-Lease gilt 120 Sekunden und wird nach jedem Würfelpaar verlängert. Verwaiste Aufträge werden nach Ablauf erneut versucht, maximal drei Versuche. Ein normaler Enginefehler beendet den Auftrag als `failed`; die Oberfläche kann einen neuen Auftrag starten.

SSE-Ereignisse heißen `queued`, `running`, `progress`, `completed` und `failed`. `id:` ist eine dauerhafte Ereignisnummer, `data:` enthält `JobProgress`-JSON. `Last-Event-ID` setzt nach der angegebenen ID fort; ohne Header werden alle bisherigen Ereignisse abgespielt. Es folgen Keepalives, bis der Auftrag endet. SIGINT/SIGTERM beendet auch aktive Streams und laufende Kindprozesse. Der nächste Start übernimmt liegengebliebene Aufträge nach Lease-Ablauf. Der Browser merkt sich die zuletzt gestarteten Aufträge je Position und gleicht SSE zusätzlich durch Polling ab.

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
npm run build --prefix web
```

Echte Engine-, PostgreSQL- und Performancetests (Compose muss laufen):

```sh
DATABASE_URL=postgres://bgtutor:bgtutor_local@127.0.0.1:54329/bgtutor cargo test --workspace -- --ignored --nocapture
```

Die SQLx-Tests erstellen und entfernen eigene Datenbanken. Sie prüfen Import/Duplikate, REST-Paginierung, echte Analyse, SSE-Replay/Shutdown, aktive Job-Deduplizierung einschließlich gleichzeitigem Abschluss, atomare Beanspruchung durch zwei Worker, Lease-Recovery, 21 Würfelpaare und Gewichte. Parser-Tests vergleichen alle 460 Zugpositionen der offiziellen XG-Referenz mit dem unabhängig durch GNUbg importierten MAT.

Der Performancetest erzeugt 50.000 Partien und 250.000 Positionen und prüft die tatsächlichen PostgreSQL-Pläne: GIN-Trigrammindex für eine selektive Namenssuche und B-tree für die exakte Position-ID. Eine lokale Messung am 8. November 2026 ergab **0,279 ms / 0,036 ms** Datenbank-Ausführungszeit. Das sind selektive, warme Testabfragen ohne HTTP-/UI-Zeit, kein allgemeines Latenzversprechen.

Browserprüfung bei laufendem Backend und Vite; verwendet installiertes Google Chrome:

```sh
npm run test:e2e --prefix web
```

Der Test importiert eine klar benannte Demo-Partie in die lokale Bibliothek und prüft Analyse, Würfelmatrix, Suche, Theme-Persistenz, Wiederaufnahme von Ergebnissen und ein 390 px breites Layout. Screenshots liegen unter `web/test-results/`.

## Struktur und nächste Schritte

- `backend/src/import/`: SGF/XG, Regelprüfung und GNUbg-IDs
- `backend/src/engine.rs`: isolierte GNUbg-Prozesse und strukturierte Ergebnisse
- `backend/src/store.rs`, `jobs.rs`: SQLx-Zugriff, transaktionale Jobzustände und Events
- `backend/src/api.rs`: REST, SSE, OpenAPI
- `backend/migrations/`: Schema und Suchindizes
- `web/src/`: Client, SVG-Brett, Ergebnisse und aus OpenAPI erzeugte Typen

Dieser Stand analysiert **ausgewählte Zugpositionen**. Vollständige Partieanalyse, Cube-Entscheidungsaufträge, Rollouts, Analyse-Caching und eine Tauri-Hülle sind Folgeausbau. Ein späterer Tauri-Client kann das Backend als Sidecar starten und dieselben HTTP-Endpunkte verwenden; Rust-Domainlogik liegt nicht im UI.
