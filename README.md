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
2. Im Tab **Matchdatenbank** nach Spieler oder Dateiname suchen (mindestens drei Zeichen). Suchtreffer werden mit stabilen Cursorn seitenweise geladen.
3. Im Tab **Analyse** unter **Spiel auswählen** ein einzelnes Spiel des Matches wählen; die Pfeile daneben wechseln zum vorherigen/nächsten Spiel. Die Auswahl zeigt Spielnummer, Startspielstand und Zuganzahl, gegebenenfalls Crawford. Der Zugverlauf und seine Pfeile bleiben innerhalb dieses Spiels; lange Spiele laden ihre Seiten automatisch. Eine Position im Zugverlauf wählen. Das Brett zeigt die Stellung **vor** dem gespielten Zug; Spielstand und Doppler werden mit angezeigt. Punktnummern und Zugnotation folgen dem Spieler am Zug; die Pip-Zahlen berücksichtigen die Bar.
4. Im rechten Reiter **Bewertung** startet **Position analysieren** einen echten dauerhaften GNUbg-Auftrag mit 0, 1 oder 2 Ply. Die Ausgabe enthält bis zu fünf Zugkandidaten, Cubeful Equity und Gewinnwahrscheinlichkeiten.
5. Unter **Bewertung → Dice Distribution** berechnet GNUbg dieselbe Stellung mit allen 21 ungeordneten Würfelpaaren. Die Matrix zeigt alle 36 geordneten Würfe; ein Pasch erhält Gewicht 1, andere Paare Gewicht 2. Das gewichtete Mittel ist die erwartete Equity. Blockierte Würfe werden als echter Zugaussetzer bewertet.
6. **Spiel analysieren** wertet standardmäßig nur das ausgewählte Spiel mit 0–2 Ply aus. Unter **Analyseumfang → Gesamtes Match** bleibt **Gesamte Partie analysieren** für alle Spiele verfügbar. Aufträge, Fortschritt, Statistiken und Fehlerübersicht gehören zum gewählten Umfang. Auch leere Spiele und reine Dopplerentscheidungen sind auswählbar und analysierbar. Im rechten Reiter **Fehler** erscheinen Fortschritt, Spielerstatistik und eine filterbare Fehlerübersicht. Jede Checker-Entscheidung zeigt den gespielten Zug, den besten Zug, beide Equities und den Verlust; Cube-Entscheidungen enthalten auch nicht genutzte Doppelangebote sowie Double/Take/Drop. Ein Klick auf die Zugnummer öffnet die zugehörige Brettstellung.
7. **Spielwürfe** zeigt separat die tatsächlich aufgezeichneten Würfe des ausgewählten Spiels. **Matchdatenbank → Position finden** sucht nach einer exakten GNUbg Position-ID. **Einstellungen → Brett** wählt mit Vorschau zwischen Forest/Walnut/Slate und den lokalen BGBlitz-Themes aus `themes/*.zip`. **Einstellungen → Allgemein → Sprache** schaltet zwischen Deutsch, English, Español, Français, Italiano und Polski um; die Einträge sind alphabetisch sortiert. Neue Installationen starten auf Englisch; vorhandene explizite Sprachwahlen bleiben erhalten.

Die Positionssuche identifiziert die Brettstellung aus Sicht des Spielers am Zug. Spielstand, Doppler und Würfel stehen in der zusätzlichen GNUbg Match-ID und werden für die Analyse berücksichtigt. Identische Bretter in verschiedenen Matchsituationen erscheinen daher als getrennte Treffer. Ähnlichkeitssuche und ein grafischer Positionseditor sind nicht Bestandteil dieses Standes.

## Analysearbeitsplatz und Einstellungen

Die Hauptnavigation trennt **Analyse**, **Matchdatenbank** und **Lernen & Anki**. Brett, Zugverlauf und Positionsbewertung stehen im Analysearbeitsplatz nebeneinander; ein optionaler Brettfokus blendet die Zugliste aus. Bewertung, Fehler, Notizen und Lernkarten haben eigene Reiter. Bei kleinen Fenstern werden die Bereiche untereinander angeordnet. Die Haupt- und Detailreiter sind mit Pfeiltasten sowie Home/End bedienbar. Die letzte Match-/Spiel-/Positionsauswahl wird lokal wiederhergestellt, ohne eine neue Analyse zu starten.

Ansichten bleiben beim Tabwechsel eingebunden: Suchfilter, ungespeicherte Notizen und SSE-Abonnements bleiben erhalten. Akzeptierte Aufträge erscheinen auch dann im globalen Fortschritt, wenn während der POST-Antwort die Position oder das Spiel gewechselt wird.

**Einstellungen** öffnet einen eigenen modalen Dialog mit den Bereichen Allgemein, Analyse, Spielen und Brett. Neben Sprache und Theme enthält er einen persönlichen Spielernamen, den GNUbg-Programmpfad sowie getrennte Analyse- und künftige Spielprofile (0–2 Ply). Der Spielbereich konfiguriert nur zukünftige Spiele; Mensch gegen Engine und BGSage sind noch kein spielbares Feature. Die vorhandene Engineauswahl zeigt GNUbg und kennzeichnet BGSage als geplant/nicht verfügbar.

Enginepfad, Spielername und Profile werden mit Revisionsprüfung in PostgreSQL gespeichert. Eine gleichzeitige Änderung liefert einen sichtbaren Konflikt und erhält den Entwurf. **Installation prüfen** und Speichern prüfen die Version und eine echte GNUbg-Positionsbewertung einschließlich Netzgewichten mit insgesamt zehn Sekunden Zeitlimit. Der Pfad gehört zum Rechner des Rust-Backends. Ungültige Einstellungen ersetzen keine funktionierende Konfiguration. Sprache und Theme bleiben lokale Browserpräferenzen.

Jeder neue Analyseauftrag erhält eine unveränderliche Enginekonfiguration; explizite oder gespeicherte Ply stehen am Auftrag. Aktive Deduplizierung berücksichtigt neben Ziel/Umfang/Ply auch den Enginepfad und die Version. Läuft ein älterer Auftrag nach einer externen Ersetzung des Binary am gleichen Pfad weiter, schlägt er mit Diagnose fehl, statt Ergebnisse verschiedener Versionen zu mischen. Bereits gespeicherte Match-Checkpoints bleiben erhalten. Legacy-Aufträge ohne Snapshot verwenden weiterhin die beim Backendstart konfigurierte Engine. MAT-Importe verwenden den aktuell gespeicherten GNUbg-Pfad.

## Zugvorschau und interaktive Statistiken

Der Analysearbeitsplatz bietet **Vor dem Zug**, **Zugpfeile** und **Nach dem Zug**. Standardmäßig zeigen grüne, kontrastreich umrandete Pfeile den gespielten Zug mit legalen Zwischenpunkten. Checker-Kandidaten aus Datei, BackgammonDB oder neuer Engineanalyse besitzen eigene Vorschau-Schaltflächen und „Besten Zug ansehen“. Quelle und Rang bleiben am Brett sichtbar; „Gespielter Zug“ setzt den Vergleich zurück. Die Nachher-Ansicht hält die Brettorientierung fest und blendet die Würfel aus. Bar-Eintritt, Hits, Mehrfachzüge, Doppelwürfe und Bear-off werden vor der Darstellung legal geprüft. Ungültige oder unbekannte Notation fällt auf die gespeicherte Ausgangsstellung zurück. Alle Themes verwenden dieselbe Projektion.

Vorschauen ändern keine Stellung-IDs, Analyseziele, Notizen oder Lernkarten: Diese beziehen sich weiter auf die Entscheidung vor dem Zug. Positions- und Quellenwechsel verwerfen fremde Kandidaten.

Der Reiter **Statistik** zeigt zum gewählten GNUbg-Auftrag Spielerwerte und eine Partieübersicht. Metriken öffnen die sichtbare Entscheidungsliste mit passendem Spieler-, Checker-/Cube-, Fehlerschwere- und Partiefilter. Partie-Schaltflächen öffnen genau das ausgewählte Spiel. Filterwechsel beginnen die Pagination neu. Native PR bleibt von Quellanalysen und Buchwerten getrennt; fehlende native Daten werden als nicht verfügbar angezeigt. Neue Aufträge speichern vollständige Partiestatistiken atomar. Ältere Aufträge erhalten aus ihren Entscheidungen rekonstruierte Summen, jedoch keine erfundene Partie-PR.

## Quellanalysen und Opening Books

Importe behalten vorhandene Analysen in eigenen Datensätzen: Engine/Version soweit aufgezeichnet, originale Tiefen-/Profilbezeichnung, Methode, Kandidaten, Cube-Antworten, Equity-Einheit und unveränderte Rohwerte. XG, native GNUbg-SGF und verfügbare BGBlitz-Informationen werden gelesen. MAT ohne Analysedaten erhält keine erfundene Analyse. Unbekannte Engines oder Cube-Einheiten bleiben unbekannt. GNUbg-MWC wird nicht als normalisierte Equity ausgegeben. Neue Berechnungen und unabhängige Matchkopien überschreiben keine Quellresultate. Auch frühere Importe können die Analyse einmalig aus ihren gespeicherten Originaldateien rekonstruieren.

Im Analyseinspektor sind **Importierte Analyse**, **Eröffnungsbuch** und **Lokale Engine** getrennte Quellen. Cube-Quellanalysen tatsächlicher Double-/Take-/Drop-Aktionen sind zusätzlich im Dialog „Cube- und Zugnotizen“ zugänglich. Buchabfragen verwenden den vollständigen Kontext; die 80-Bit-Checkerstellung allein reicht nicht.

**Einstellungen → Analyse → Eröffnungsbücher** öffnet eine eigene Buchverwaltung. Sie speichert Anbieter und Dateipfade mit Versionsprüfung und erhält ungespeicherte Einstellungen im übergeordneten Dialog. Die Dateipfade gehören zum Backend-Rechner. Automatisch erlaubt den Vergleich beider Bücher; direkte Anbieterwahl und Aus sind ebenfalls verfügbar. Eine manuelle Engineanalyse berechnet weiterhin das gewählte native GNUbg-Profil. Andere Buch-Engines oder -Versionen werden nicht als identischer nativer Cachehit ausgegeben und nicht in Match-PR gemischt.

- **BackgammonDB:** lokales SQLite-Buch mit 4.230.443 Entscheidungen und 351 Kontexten. Es liefert für Stellung **und Wurf** bis zu acht geordnete Züge, daneben eine separate Cube-Frage ohne Würfel. Die Baumabdeckung ist beschnitten und abhängig vom Spielstand; ein Miss ist normal. Quelle: GNUbg1.08.003, reguläre 3-ply-Bewertungen, keine pauschalen Rollouts. Daten unter CC BY4.0; Attribution erscheint beim Ergebnis. [Daten und Format](https://github.com/chalkstreamdev/bgdb-opening-book), [Buchbeschreibung](https://backgammondb.com/opening-book).
- **XG Opening Book3.70:** vollständige offizielle `.ob`-Datei, 53.210 Bewertungsdatensätze. Das Buch bewertet erreichte Stellungen **vor einem Wurf**, speichert keine Zugliste und kein Double-/Take-Verdikt. Es wird als Stellungsreferenz angeboten, mit tatsächlicher Engine, ursprünglichem Profil/Rollout und dem Autor jedes Eintrags. Native Werte aus Sicht des vorherigen Ziehenden werden nachvollziehbar auf den Spieler am Zug gespiegelt. Unklare historische Tiefencodes werden nicht erfunden. Die Quelle erlaubt freie Nutzung unter Erhalt und Anzeige der Rollout-Autoren. [Offizielle Quelle und Nutzungsbedingungen](https://www.extremegammon.com/GetOb.aspx).

Beaver, Jacoby, Crawford, Away-Scores, Cube-Wert und Cube-Besitz werden berücksichtigt. XG-Moneygame-Regeln werden aus dem Originalheader gelesen; eine fehlende oder nicht unterstützte Regel wird nicht durch einen ähnlichen Buchkontext ersetzt. Das BackgammonDB-Buch enthält keine Beaver-Kontexte. Ein XG-Stellungswert ist keine Bewertung des zufällig daneben angezeigten Würfelwurfs; beide Buchansichten sagen ausdrücklich, welche Frage sie beantworten.

Die realen Buchdateien liegen lokal unter `~/Library/Application Support/BG Tutor/`, nicht im Repository. Für andere Installationen die offiziellen Datendateien herunterladen, außerhalb Git ablegen und den Backend-Pfad im Buchdialog eintragen. BGDB-Daten müssen als serving-SQLite-Datei entpackt sein, XG als `OpeningbookV2.ob`. Die Check-/Install-APIs prüfen das jeweilige Format; sie führen keine fremden Programme aus.

## Private Demo-Daten neu aufbauen

`games/bgf`, `games/mat` und `games/xg` sind ausdrücklich Git-ignoriert. Die drei Demo-Bestände werden über den normalen REST-Importer vorbereitet. Das Werkzeug erstellt und überprüft vorher einen PostgreSQL-Dump außerhalb Git und schreibt den vollständigen Importbericht ebenfalls dorthin. Es prüft die Übereinstimmung von API- und Containerdatenbank. Vor dem Austausch werden unter Transaktionssperre Cluster-/Datenbankidentität, alle drei Bestände, Match-IDs, Herkunft und SHA256 geprüft. Fehlgeschlagene Validierung verändert die aktive Bibliothek nicht; ein erneuter Aufruf nach bereits erfolgtem Austausch wird verweigert.

```sh
python3 tools/rebuild_demo.py stage --allow-rejected
# Backend anschließend regulär stoppen; den ausgegebenen Manifestpfad verwenden:
python3 tools/rebuild_demo.py activate '/absoluter/Pfad/zum/demo-import-....json'
# Backend wieder starten.
```

`--allow-rejected` bedeutet, nach Prüfung der einzeln dokumentierten fehlerhaften Dateien die gültigen Matches zu verwenden. Netzwerk-/Serverfehler führen weiterhin zum Abbruch. Das Werkzeug ersetzt die Matchbestände; Lernkarten und Wiederholungsverlauf sind unabhängige Daten und werden nicht gelöscht. Das BGF-Demo verwendet den festen Standardbestand, MAT und XG haben eigene Bestände. Öffentliche Quellen und die Produkt-/UX-Empfehlungen stehen in [der BackgammonDB-Auswertung](docs/research/2026-11-08-backgammondb-product-ux.md).

Die lokale Demo-Neuinitialisierung vom 8. November 2026 ist abgeschlossen: **Demo BGF: 5**, **Demo MAT: 5**, **Demo XG: 23** Matches. 72.959 vorhandene Checker-/Cube-Analysen und 98 zusätzliche Quellstatistik-Datensätze sind gespeichert. Neun XG-Dateien mit ungültigen beziehungsweise inkonsistenten Zugdaten wurden zurückgewiesen und im privaten Importbericht dokumentiert. Die bisherige Bibliothek ist vollständig gesichert; Lernkarten und Wiederholungen blieben unverändert.

## Datenbanken und Matchverwaltung

Die Matchdatenbank verwaltet benannte, getrennte Bestände innerhalb derselben PostgreSQL-Instanz. Vorhandene Matches liegen nach der Migration im Standardbestand „My matches“. Neue Bestände lassen sich anlegen, beschreiben, umbenennen und löschen; der Standardbestand bleibt erhalten. Die Auswahl „Alle Datenbanken“ erlaubt die gemeinsame Suche, Importe gehen dann in den Standardbestand. Sonst bestimmt der gewählte Bestand das Importziel. Mehrfachimporte verarbeiten Dateien einzeln und melden Duplikate und Fehler je Datei.

Die sortierbare Matchliste zeigt Spieler, **PR (GNUbg)**, Spielstand, Matchlänge beziehungsweise Moneygame, gespieltes und Importdatum, Turnier, Analysezustand, Tags und Kommentare. Eine Zeilenauswahl beziehungsweise „Details“ öffnet eine Brettvorschau mit Partie- und Zugauswahl. Die Match-Schaltfläche, Enter oder ein Doppelklick öffnet den Analysearbeitsplatz. Metadaten wie Anzeigename, Datum, Turnier, Runde, Ort und Ergebnis sind mit Versionsprüfung editierbar; Originaldatei und Analysekontext bleiben erhalten. Gespielte Daten werden nur aus eindeutigen SGF-Quellangaben übernommen. Fehlende Angaben bleiben unbekannt.

Kombinierbare Filter umfassen Spieler, Datum (gespielt/importiert), exakten Tag, Kommentar, Position-ID, Turnier, Matchname, Match/Moneygame, Ergebnisstatus, Gewinnerfarbe beziehungsweise Remis, Runde und Matchpunkte. Notiz- und Positionssuche beachten den gewählten Bestand. Pagination bindet Sortierung und Filter an den Cursor. Angezeigte PR-Werte stammen ausschließlich aus einer vollständigen Matchanalyse mit nativen GNUbg-Statistiken; einzelne Spielanalysen werden als Teilanalyse geführt. PR verwendet normalisierten Equity-Verlust × 500 / (nicht erzwungene Checkerzüge + nahe oder tatsächliche Cube-Entscheidungen). Die Entscheidungszählung unterscheidet sich von XG/BMAB. Fehlende, ältere oder unvollständige Statistiken ergeben keinen erfundenen PR. Siehe [GNUbg-Statistik](https://www.gnu.org/software/gnubg/manual/gnubg.html#gnubg-matches_review_stat_overall).

Mehrfachauswahl unterstützt **Kopieren, Verschieben, Tags, Export und Löschen**. Je Aktion sind bis zu 100 Matches möglich, beim Export 50 und insgesamt 64 MiB. Kopieren erzeugt eigenständige Matches einschließlich Positionen, Partie-/Zug-IDs, Tags, Kommentaren und der letzten abgeschlossenen Analyse je Umfang. Lernfortschritt und aktive Aufträge werden nicht kopiert. Duplikate werden je Datenbank anhand des Originaldatei-Hashes erkannt und niemals mit Quellnotizen überschrieben. Verschieben erhält die IDs; bei einem Zielduplikat bleibt das Quellmatch bestehen. Aktionen melden den Ausgang je Eintrag. Löschen erfordert eine konkrete Bestätigung und lässt Matches mit wartender/laufender Analyse unangetastet. Lernkarten und deren Wiederholungsverlauf bleiben eigenständige Datensätze.

Export liefert Originaldateien oder kanonische SGF, bei mehreren Matches ein ZIP. Dies ist ein Quellexport; BG-Tutor-Notizen und Lernverlauf sind darin nicht enthalten. Für eine vollständige Sicherung dient weiterhin das PostgreSQL-Backup. Die übernommenen ChessBase-Ideen sind benannte Bestände, sortierbare Listen, Mehrfachverwaltung und eine schnelle Brettvorschau: [Vorschaufenster](https://help.chessbase.com/CBase/16/Eng/preview_window_in_the_database.htm), [Bestände organisieren](https://en.chessbase.com/post/chessbase-18-tips-for-beginners-part-7-organising-databases).


## Gespeicherte Match- und Positionsdaten

Der Import speichert die Originaldatei, kanonische SGF und eigene normalisierte Positionszeilen gemeinsam in PostgreSQL. Jede Position enthält 26 vorzeichenbehaftete Checker-Anzahlen einschließlich Bar, Spieler am Zug, Würfel, Spielnummer, Zugnummer und gespielten Zug. Ein gesonderter Spielkatalog enthält auch Spiele ohne Checkerzüge samt Startspielstand und Crawford-Status. Bei älteren Importen wird dieser Katalog beim ersten Zugriff einmalig aus der gespeicherten SGF ergänzt.

Die Brettstellung besitzt zusätzlich die kompakte **80-Bit-GNUbg-Position-ID**, gespeichert als 14 Base64-Zeichen mit B-tree-Index. Sie kodiert Checker-Anzahlen und ist kein reines Belegungs-Bitboard. Die separate **72-Bit-GNUbg-Match-ID** enthält unter anderem Spielstand, Matchlänge, Doppler, Würfel und Regeln. Exakte Positionssuchen verwenden den Index; dafür wird die Ursprungsdatei nicht erneut geparst. Ein weiterer Index auf Match, Spiel und Reihenfolge ermöglicht den direkten Zugriff auf ein spätes Spiel, ohne vorherige Spiele zu laden.

## Board-Themes und Sprachen

Die Originalgrafiken der lokal bereitgestellten BGBlitz-Archive werden automatisch bei `npm run dev` und `npm run build` eingelesen. Dafür wird **Python 3.10+** ohne zusätzliche Pakete benötigt. Die Darstellung verwendet echte Hintergründe, Checker-Varianten, Würfel, Doppler und Nummernleisten. Neue Archive in `themes/` ablegen und den Entwicklungsserver neu starten oder `npm run themes:prepare --prefix web` ausführen. Ohne lokale ZIPs bleiben die drei eingebauten Themes verfügbar. Format, Herkunft und Rechte: [themes/README.md](themes/README.md).

Sprache und Theme werden im eigenen Einstellungsdialog als Entwurf bearbeitet und beim **Speichern** übernommen. **Abbrechen** verwirft die Vorschau. Texte, Status, Fehlermeldungen, Beschriftungen und Zahlenformate wechseln nach dem Speichern; bereits sichtbare Analyseergebnisse bleiben erhalten. Spieler-/Dateinamen und originale GNUbg-Zugnotationen bleiben ihre ursprünglichen Daten. Native technische Engine-Diagnosen erscheinen zusammen mit einer lokalisierten Fehlermeldung. Schlüssel und Platzhalter aller sechs Kataloge werden typisiert und beim Start geprüft. Die Übersetzungen sind noch nicht muttersprachlich lektoriert.

## Tags, Kommentare und Lernen

Match, einzelne Partie und Checkerposition haben getrennte Notizen. „Cube- und Zugnotizen“ öffnet die tatsächlichen Quellaktionen einschließlich Double, Take, Drop und bestätigter Aufgabe mit ihrer Stellung vor der Aktion. Der angebotene Cube erscheint bei offenen Verdopplungen mit dem angebotenen Wert in der Brettmitte. Auch Partien ohne Checkerzug sind kommentierbar. Die Notizsuche berücksichtigt alle vier Ziele (Match, Partie, Position, expliziter Zug) und kann nach Bestand eingeschränkt werden. Entwürfe bleiben bei Navigation und Dialogwechsel erhalten; Versionskonflikte überschreiben keine parallelen Notizen.

Der gemeinsame Notizbereich unter **Analyse → Notizen** hat die Reiter **Match** und **Position**, mit genau einem sichtbaren Editor. Matches und importierte Positionen haben jeweils eigene **Tags und Kommentare**. „Notizen speichern“ schreibt sie nach PostgreSQL; beim Blättern bleiben ungespeicherte Entwürfe in der aktuellen Sitzung erhalten. Parallele Änderungen erzeugen einen sichtbaren Konflikt, ohne den Entwurf zu überschreiben. Kommentare sind Klartext/Markdown und werden als Text dargestellt. Tags werden vereinheitlicht; Grenzen: 30 Tags zu je 64 Zeichen und 20.000 Kommentarzeichen.

**Tags & Kommentare durchsuchen** findet beide Zieltypen. Freitext ab drei Zeichen, exakter Tag und Zieltyp sind kombinierbar; Treffer öffnen die betreffende Partie oder Zugposition. Die normale Bibliothekssuche findet ebenfalls Partien über ihre eigenen Notizen und die Notizen ihrer Positionen. Suchindizes erhalten die schnelle Suche auch bei großen Bibliotheken.

Unter **Analyse → Lernkarten → Positionen als Lernkarten** ein Deck erstellen oder auswählen. Eine bereits analysierte Einzelposition hinzufügen, oder Fehler des ausgewählten Spiels nach Spieler und Mindestverlust übernehmen. Dafür wird dessen abgeschlossene Spielanalyse verwendet; alternativ werden die Ergebnisse einer abgeschlossenen Matchanalyse auf dieses Spiel gefiltert. Checker-Züge, Double-/No-double- und Take-/Pass-Entscheidungen werden unterstützt. Kartenidentität berücksichtigt Brett, vollständigen Matchkontext und Fragetyp; derselbe Lerninhalt kann mehreren Decks angehören. Erneutes Hinzufügen aktualisiert seine Analysequelle und erhält den Lernfortschritt. Die Tags und Kommentare der aktuellen Partie-/Positionsquelle werden beim Aufdecken und Export gelesen. Eine Deckauswahl ist auf 1.000 Karten begrenzt; pro Anfrage werden maximal 100 Karten gelistet.

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
| `.mat` | GNUbg normalisiert Jellyfish-MAT zu SGF. Erkannte Aufgabezeilen werden spaltentreu normalisiert; zusammengesetztes Auswürfeln wird in legale Einzelschritte aufgelöst. Parserdiagnosen und der vollständige Zugverlauf werden geprüft, damit beschädigte Dateien nicht als Teilimport gespeichert werden. |
| `.sgf` | Nativer Parser für Standard-Backgammon, Escape-Sequenzen, mehrere Spiele, Setup-Positionen, Doppler, Scores, Crawford/Jacoby und Zugrekonstruktion. UTF-8; mehrdeutige Variantenbäume werden abgelehnt. |
| `.xg` | Nativer Decoder für binäre XG2-Dateien mit Save-Version 24–40: RichGame-Container, begrenzte Zlib-Dekompression, CRC und SGF-Konvertierung. Unterstützt Cube-Platzhalter, offene Money-Sessions, zusammengefasste Checker-Züge und gültige Zwischenstände von Punktpartien, auch mit explizit laufendem letztem Spiel. |
| `.bgf` | Nativer Rust-Decoder für BGBlitz BGF 1.0: JSON-Kopf, optional Gzip und JSON-/Smile-Nutzdaten. Übernimmt gespielte Züge, Aussetzen, Double/Take/Drop, Farben, Scores und Crawford/Jacoby. Die erzeugte SGF wird vollständig nachgespielt; vorhandene Fremdanalysen werden nicht als GNUbg-Ergebnisse übernommen. |

XG1, bearbeitete/custom XG-Startstellungen, automatische Doppel, Beavers/Raccoons und andere Spielvarianten werden mit einer Fehlermeldung abgelehnt. Vorhandene XG-/BGF-Analysen, Rollouts und Kommentare werden noch nicht übernommen. BGF-Sondervarianten, veränderte Startstellungen und begrenzte Dopplerregeln werden ausdrücklich abgelehnt. Die Originaldatei bleibt gespeichert. Details und Quellen: [Import-Fixtures](backend/tests/fixtures/README.md).

Ein Upload enthält genau eine Datei; Limit: **16 MiB einschließlich Multipart-Rahmen**. SHA-256 erkennt identische Dateien. Original, kanonische SGF und Positionen werden in einer Transaktion gespeichert. Dateinamen und Inhalte werden niemals als Shellbefehle ausgeführt. Parser und Archive haben zusätzliche Mengen-/Größenlimits.

## API und Aufträge

| Methode | Pfad | Zweck |
| --- | --- | --- |
| GET/PUT | `/api/settings` | Persistente Engine-/Spielereinstellungen mit Revisionsschutz |
| GET | `/api/engines` | Verfügbare Engines, Versionen und Analysestufen |
| GET/PUT | `/api/opening-book` | Buchanbieter, BGDB-/XG-Dateipfade, Verfügbarkeit und Versionsschutz |
| POST | `/api/opening-book/check`, `/api/opening-book/install` | Lokale BGDB-Datei prüfen beziehungsweise atomar installieren |
| POST | `/api/opening-book/xg/check`, `/api/opening-book/xg/install` | Offizielle XG-Buchdatei prüfen/installieren |
| GET | `/api/positions/{id}/opening-book` | Kontextsensitive Buchantwort oder expliziter Miss; XG ist eine gesonderte Stellungsreferenz |
| GET | `/api/matches/{id}/imported-analysis` | Analyseherkunft und Abdeckung der Quelldatei |
| GET | `/api/positions/{id}/imported-analysis`, `/api/moves/{id}/imported-analysis` | Unveränderte Checker-/Cube-Quellanalyse |
| POST | `/api/engines/gnubg/check` | Installation mit `{ "path": "..." }` prüfen |
| GET | `/api/health` | PostgreSQL- und GNUbg-Verfügbarkeit |
| POST | `/api/imports?database_id=` | Multipart-Feld `file`; 201 neu, 200 Duplikat im Zielbestand |
| GET/POST | `/api/databases` | Bestände mit Matchanzahl auflisten/anlegen |
| PUT/DELETE | `/api/databases/{id}` | Bearbeiten mit Version; Löschen mit `version` und explizitem `delete_matches=true` |
| GET | `/api/library/matches` | Erweiterte Bestands-/Metadatenfilter, Sortierung und gebundener Cursor; Limit 1–200 |
| PUT/PATCH | `/api/matches/{id}/metadata` | Editierbare Matchdetails mit Version |
| POST | `/api/library/actions` | `action`, `match_ids`, optional Quell-/Zielbestand und Tags; Einzelergebnisse |
| POST | `/api/library/export` | `match_ids`, `format: original|sgf`; Datei oder ZIP |
| GET | `/api/matches/{id}/games/{game_no}/decisions` | Stabile Partie-ID und tatsächliche Quellzüge samt Stellung |
| GET | `/api/moves/{id}` | Quellzug mit Vor-Aktions-Stellung |
| GET/PUT | `/api/games/{id}/annotation`, `/api/moves/{id}/annotation` | Partie-/Zugnotizen; Checkerzüge verwenden dieselben Positionsnotizen |
| GET | `/api/matches?q=&after=&limit=` | Partiensuche; Limit 1–100 |
| GET | `/api/matches/{id}` | Partiemetadaten |
| GET | `/api/matches/{id}/source` | Kanonische SGF herunterladen |
| GET | `/api/matches/{id}/games` | Spielkatalog mit Startspielständen, Crawford und Zuganzahl |
| GET | `/api/matches/{id}/dice?game_no=` | Beobachtete Würfelhäufigkeiten; optional einzelnes Spiel |
| GET | `/api/positions?database_id=&match_id=&game_no=&position_id=&after=&limit=` | Partiepositionen oder exakte Brettsuche |
| GET | `/api/positions/{id}` | Direkter Positionszugriff mit Match- und Spielnummer |
| POST | `/api/jobs` | `{ "position_id": "<Positions-UUID>", "kind": "analysis", "plies": 0 }` |
| GET | `/api/jobs/{id}` | Zustand, Fortschritt, Ergebnis oder Fehler |
| GET | `/api/jobs/{id}/events` | Persistenter SSE-Strom |
| POST | `/api/matches/{id}/analysis` | Ein Spiel: `{ "plies": 0, "game_no": 2 }`; gesamtes Match ohne `game_no`. Aktive Aufträge werden je Match, Spielumfang und Ply zusammengeführt |
| GET | `/api/matches/{id}/analysis?game_no=` | Letzter Auftrag des Spielumfangs; ohne `game_no` letzter Matchauftrag, sonst `null` |
| GET | `/api/jobs/{id}/moves?game_no=&after=&limit=&errors_only=&player=&kind=&severity=` | Paginierte Zugbewertung; kombinierbare Partie-, Spieler-, Checker-/Cube- und Fehlerschwerefilter; Cursorprüfung |
| GET | `/api/jobs/{id}/statistics` | Native Match-/Partiestatistik mit Analyseabdeckung und vorhandenen PR-Werten |

```sh
curl -F file=@backend/tests/fixtures/tutorial.mat http://127.0.0.1:8787/api/imports
curl 'http://127.0.0.1:8787/api/matches?q=Tutor'
```

Aufträge durchlaufen `queued → running → completed|failed`. Ein Worker pro Backend begrenzt die parallele Analyse. PostgreSQL `FOR UPDATE SKIP LOCKED` verhindert doppelte Beanspruchung, aktive identische Aufträge werden zusammengeführt. Positionsanalyse und Import haben ein Prozesslimit von 90 Sekunden, die native Analyse eines ganzen Spiels 600 Sekunden. Eine Worker-Lease gilt 120 Sekunden und wird während der Arbeit alle 20 Sekunden erneuert. Verwaiste Aufträge werden nach Ablauf erneut versucht, maximal drei Versuche. Ein normaler Enginefehler beendet den Auftrag als `failed`; die Oberfläche kann einen neuen Auftrag starten.

SSE-Ereignisse heißen `queued`, `running`, `progress`, `completed` und `failed`. `id:` ist eine dauerhafte Ereignisnummer, `data:` enthält `JobProgress`-JSON. `Last-Event-ID` setzt nach der angegebenen ID fort; ohne Header werden alle bisherigen Ereignisse abgespielt. Es folgen Keepalives, bis der Auftrag endet. SIGINT/SIGTERM beendet auch aktive Streams und laufende Kindprozesse. Der nächste Start übernimmt liegengebliebene Aufträge nach Lease-Ablauf. Der Browser merkt sich die zuletzt gestarteten Aufträge je Position und gleicht SSE zusätzlich durch Polling ab. Den letzten Partieauftrag lädt er direkt aus der Datenbank; die Anzeige bleibt deshalb auch ohne Browser-Cache verfügbar.

Partieaufträge speichern Entscheidungen, Teilstatistik und SSE-Fortschritt nach jedem vollständig analysierten Spiel in einer Transaktion. `Job` und SSE enthalten `completed_games` und `total_games`; der Prozentwert bezieht sich auf Spiele, nicht auf Laufzeit. Während eines langen Spiels bleibt dieser Wert unverändert. Nach einem Neustart wird nach Lease-Ablauf beim nächsten unvollständigen Spiel fortgesetzt. Ein alter Worker kann keine Checkpoints oder Endergebnisse schreiben. Bei einem Enginefehler bleiben fertige Spiele und ihre Fehlerübersicht abrufbar; ein neu gestarteter Auftrag analysiert die Partie erneut.

Die Bewertung verwendet GNUbgs native `analyse game`-Ausgabe mit sechs Nachkommastellen, einschließlich der tatsächlich gespielten Alternative außerhalb der besten fünf Kandidaten. Vorhandene Analyseannotationen werden für die Berechnung entfernt. Geerbte Matchdaten und Setup-Stellungen werden für GNUbg explizit normalisiert; Brett- und Matchkontext werden für jede Checker-/Cube-Aktion zurückgeprüft. Gemischte SGF-Sammlungen können auch leere oder reine Cube-Spiele enthalten. Ein fehlender oder unvollständiger nativer Export führt zu einem Fehler; native Exportdateien sind auf jeweils 64 MiB begrenzt. Erzwungenes Aussetzen hat Verlust 0 und keine alternative Equity (`evaluation_status=forced`). Fehlerklassen: ungenau ab 0,04, Fehler ab 0,08, grober Fehler ab 0,16 normierter Equity. Die reinen Equity-Summen pro Spieler sind kein PR; die gesonderte `performance_rating`-Statistik ergänzt native relevante Entscheidungszahlen und den gewichteten GNUbg-PR. Cube-Equities werden aus Sicht des entscheidenden Spielers in Einheiten des Dopplers vor dem Angebot ausgewiesen. Reine Cube-Entscheidungen erhalten eigene stabile Quellzug-IDs und sind über die Zugnotizen samt Stellung vor der Aktion zugänglich.

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

Echte Engine-, PostgreSQL- und Performancetests (Compose muss laufen). Der optionale lokale Importkorpus enthält inzwischen neun strikt abgewiesene XG-Dateien; dessen Lauf ist separat in [Importprüfung](docs/import-corpus-results.md) dokumentiert. Die übrigen Integrationstests können gezielt mit `--skip all_local_game_files_import_atomically_and_deduplicate` vollständig ausgeführt werden:

```sh
DATABASE_URL=postgres://bgtutor:bgtutor_local@127.0.0.1:54329/bgtutor cargo test --workspace -- --ignored --nocapture
```

Der optionale lokale Dateibestand wird ebenfalls über Multipart-REST in einer eigenen SQLx-Testdatenbank geprüft. Er liest `games/xg`, `games/mat` und `games/bgf` (alternativ `BGTUTOR_GAMES_DIR`). Ohne diese lokalen Dateien wird nur dieser optionale Bestandstest übersprungen. Ergebnis des bereitgestellten Bestands: **37/37 Dateien erfolgreich**, 17 verschiedene Inhalte und 20 korrekt erkannte Duplikate. [Vollständiger Bericht](docs/import-corpus-results.md).

```sh
DATABASE_URL=postgres://bgtutor:bgtutor_local@127.0.0.1:54329/bgtutor BGTUTOR_CORPUS_REPORT=/tmp/bgtutor-import-results.json cargo test -p bgtutor --test import_corpus -- --ignored --nocapture
```

Die SQLx-Tests erstellen und entfernen eigene Datenbanken. Zusätzliche Tests prüfen den Spielkatalog einschließlich Nachbefüllung älterer Matches, direkte Positionszugriffe, Scope-/Cursor-Isolation, echte Einzelspielanalyse, Wiederaufnahme und Lernkarten aus dem ausgewählten Spiel. Sie prüfen Import/Duplikate, REST-Paginierung, echte Analyse, SSE-Replay/Shutdown, aktive Job-Deduplizierung einschließlich gleichzeitigem Abschluss, atomare Beanspruchung durch zwei Worker, Lease-Recovery, 21 Würfelpaare und Gewichte. Die Partieanalyse wird über alle zehn Spiele und 460 Züge der Referenzpartie geprüft, einschließlich schlechter Checker-Züge, erzwungenem Aussetzen, falschem Double/Drop, 0–2 Ply, fortsetzbaren Checkpoints und dem Ausschluss veralteter Worker. Weitere Regressionstests prüfen geerbte SGF-Matchdaten, leere/Cube-Spiele, Setups mit Schwarz am Zug, stille lange Engine-Prozesse und die gleichzeitige Fortschrittsspeicherung/Lease-Verlängerung. Parser-Tests vergleichen alle 460 Zugpositionen der offiziellen XG-Referenz mit dem unabhängig durch GNUbg importierten MAT.

Der Performancetest erzeugt 50.000 Partien und 250.000 Positionen und prüft die tatsächlichen PostgreSQL-Pläne: GIN-Trigrammindex für eine selektive Namenssuche und B-tree für die exakte Position-ID. Eine lokale Messung am 8. November 2026 ergab **0,279 ms / 0,036 ms** Datenbank-Ausführungszeit. Das sind selektive, warme Testabfragen ohne HTTP-/UI-Zeit, kein allgemeines Latenzversprechen.

Browserprüfung bei laufendem Backend und Vite; verwendet installiertes Google Chrome:

```sh
npm run test:e2e --prefix web
```

Die Browserprüfungen prüfen außerdem direkte Spielauswahl, automatische Seitennavigation innerhalb eines langen Spiels, Grenzen des Zugverlaufs, Spiel-/Matchanalyse, verzögerte Kataloge, Filterrückkehr und einen einzigen Notizeditor mit getrennten Entwürfen. Sie importieren klar benannte Demo-Partien in die lokale Bibliothek und prüfen Analyse, Würfelmatrix, Suche, alle sechs Sprachen, lokale ZIP-Themes, Wiederaufnahme von Ergebnissen und ein 390 px breites Layout. Weitere Abläufe prüfen Notizen mit Versionskonflikten, direkte Positionssprünge, Deckanlage, verdeckte Lernantworten, persistierte Bewertungen, Checker-/Cube-Karten und Anki-Downloads. Eine verzögerte API-Antwort prüft außerdem, dass beim schnellen Wechsel zwischen analysierten Positionen die zuletzt angeklickte Position ausgewählt bleibt. Screenshots liegen unter `web/test-results/`.

## Struktur und nächste Schritte

- `backend/src/import/`: SGF/XG, Regelprüfung und GNUbg-IDs
- `backend/src/engine.rs`, `match_analysis.rs`: isolierte GNUbg-Prozesse und native Checker-/Cube-Auswertung
- `backend/src/match_jobs.rs`: dauerhafte Spiel-Checkpoints und Spielerstatistik
- `backend/src/store.rs`, `jobs.rs`: SQLx-Zugriff, transaktionale Jobzustände und Events
- `backend/src/api.rs`: REST, SSE, OpenAPI
- `backend/migrations/`: Schema und Suchindizes
- `web/src/`: Client, SVG-Brett, Ergebnisse und aus OpenAPI erzeugte Typen

Dieser Stand analysiert ausgewählte Positionen, einzelne Spiele und vollständige Matches einschließlich Cube-Entscheidungen. Rollouts, Analyse-Caching und eine Tauri-Hülle sind Folgeausbau. Ein späterer Tauri-Client kann das Backend als Sidecar starten und dieselben HTTP-Endpunkte verwenden; Rust-Domainlogik liegt nicht im UI.


Für isolierte Browserprüfungen kann `BGTUTOR_TEST_URL` auf einen separaten Backend-Server mit eigenem PostgreSQL-Testbestand und gebautem Frontend zeigen. Ohne Variable bleibt die lokale Vite-Adresse unverändert. Die privaten Demo-Bestände müssen dafür nicht verändert werden.
