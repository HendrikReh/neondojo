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
4. In **Übersicht → Bewertung** oder im einzelnen Reiter **Bewertung** startet **Position analysieren** einen echten dauerhaften GNUbg-Auftrag mit 0, 1, 2, 3 oder 4 Ply. Standard ist 2-Ply. Die Ausgabe enthält bis zu fünf Zugkandidaten, Cubeful Equity und Gewinnwahrscheinlichkeiten.
5. Unter **Bewertung → Dice Distribution** berechnet GNUbg dieselbe Stellung mit allen 21 ungeordneten Würfelpaaren. Die Matrix zeigt alle 36 geordneten Würfe; ein Pasch erhält Gewicht 1, andere Paare Gewicht 2. Das gewichtete Mittel ist die erwartete Equity. Blockierte Würfe werden als echter Zugaussetzer bewertet.
6. **Spiel analysieren** wertet standardmäßig nur das ausgewählte Spiel mit 0–4 Ply aus; Standard ist 2-Ply. Unter **Analyseumfang → Gesamtes Match** bleibt **Gesamte Partie analysieren** für alle Spiele verfügbar. Aufträge, Fortschritt, Statistiken und Fehlerübersicht gehören zum gewählten Umfang. Auch leere Spiele und reine Dopplerentscheidungen sind auswählbar und analysierbar. Die **Übersicht** zeigt Fortschritt, Spielerstatistik und filterbare Fehler gleichzeitig; der einzelne Reiter **Fehler** bleibt verfügbar. Jede Checker-Entscheidung zeigt den gespielten Zug, den besten Zug, beide Equities und den Verlust; Cube-Entscheidungen enthalten auch nicht genutzte Doppelangebote sowie Double/Take/Drop. Ein Klick auf die Zugnummer öffnet die zugehörige Brettstellung.
7. **Spielwürfe** zeigt separat die tatsächlich aufgezeichneten Würfe des ausgewählten Spiels. **Matchdatenbank → Position finden** sucht nach einer exakten GNUbg Position-ID. **Einstellungen → Brett** wählt mit Vorschau zwischen Forest/Walnut/Slate und den lokalen BGBlitz-Themes aus `themes/*.zip`. **Einstellungen → Allgemein → Sprache** schaltet zwischen Deutsch, English, Español, Français, Italiano und Polski um; die Einträge sind alphabetisch sortiert. Neue Installationen starten auf Englisch; vorhandene explizite Sprachwahlen bleiben erhalten.

Die Positionssuche identifiziert die Brettstellung aus Sicht des Spielers am Zug. Spielstand, Doppler und Würfel stehen in der zusätzlichen GNUbg Match-ID und werden für die Analyse berücksichtigt. In der Matchdatenbank erscheint jedes passende Match einmal; beim Öffnen wird das erste Vorkommen der gesuchten Stellung ausgewählt. Die Sammlung gespeicherter Positionen kann dieselbe Brettstellung in unterschiedlichen Matchsituationen enthalten. Ähnlichkeitssuche und ein grafischer Positionseditor sind nicht Bestandteil dieses Standes.

## Analysearbeitsplatz und Einstellungen

BG Tutor ist für die informationsdichte Partieanalyse am Desktop ausgelegt. Brett, Zugliste, Kandidaten, Bewertungen und Statistiken sollen auf großen Fenstern möglichst gleichzeitig sichtbar und mit Maus und Tastatur effizient bedienbar sein. Desktop-Nutzbarkeit und Informationsdichte bestimmen die weitere UX-Entwicklung und deren Abnahmekriterien. Mobilgeräte sind keine Zielplattform; zusätzliche mobile Layouts und mobile Testanforderungen gehören nicht zum Projektumfang.

Alle vier Arbeitsbereiche verwenden eine gemeinsame Sans-Serif-Schrift und dieselbe [Typografieskala](docs/typography.md): Seitentitel 24 px, Bereichstitel 14 px, Bedienelemente und Fließtext 13 px sowie Tabelleninhalt 12 px. Schriftgrößen und Gewichte werden zentral über CSS-Tokens gepflegt; Positionskennungen bleiben in Monospace, Brettgrafik und Markenlogo behalten ihre eigenen Schriften. Vorschau, Dialoge und Lernfragen besitzen klar definierte eigene Titelgrößen.

Die [Navigationsgestaltung](docs/ui-navigation.md) gilt durchgehend: dezente Textreiter auf transparentem Hintergrund, farbiger 2-px-Unterstrich und hervorgehobene Schrift beim aktiven Bereich. Dasselbe Muster verwenden Analyse-Reiter, Quellen- und Buchauswahl, Checker/Cube, Initial/Arrows/Final, Notizbereiche, Einstellungskategorien und Datenbankauswahl. Hover, Tastaturfokus und deaktivierte Einträge sind gemeinsam definiert und folgen dem gewählten UI-Theme. Aktionsbuttons und inhaltliche Markierungen behalten ihre jeweiligen visuellen Rollen.

Die kompakte Hauptnavigation trennt **Analyse**, **Matchdatenbank**, **Positionen** und **Lernen & Anki**. Der aktive Bereich erhält eine farbige Textmarkierung mit Unterstrich; die Einstellungen stehen dezent rechts. Die Analyse-Kopfzeile zeigt Spielernamen prominent sowie Match-ID, Matchlänge, gespeicherten Spielstand/Endstand, Spieldatum, Event und PR je Spieler dezent in einer Desktop-Zeile. Datum und Event kommen aus gespeicherten Metadaten; fehlende Werte bleiben n/a. Die Match-PR folgt derselben vollständigen nativen Matchanalyse wie in der Datenbank und wird nach deren Abschluss aktualisiert. Dateiformat und Quellexport entfallen in der Kopfzeile. Die Standardansicht **Übersicht** zeigt Zugliste, Brett, Kandidaten, Fehler und Statistiken gleichzeitig. Der Analysearbeitsplatz nutzt die volle Fensterbreite: links die Zugliste, in der Mitte Brett und Kandidaten mit Fehlerliste darunter, rechts Spieler- und Partiestatistik. Lange Listen scrollen in ihrem Bereich; der aktive Zug wird im Verlauf sichtbar gehalten. Ein optionaler Brettfokus blendet die Zugliste aus. Bewertung, Statistik, Fehler, Notizen und Lernkarten behalten ihre einzelnen Reiter. Die Haupt- und Detailreiter sind mit Pfeiltasten sowie Home/End bedienbar. Die letzte Match-/Spiel-/Positionsauswahl wird lokal wiederhergestellt, ohne eine neue Analyse zu starten.

Die **Fehlerübersicht pro Zug** startet in allen Ansichten mit **Nur Auffälligkeiten (≥ 0,04)**; **Filter zurücksetzen** stellt diesen Fehlerfilter wieder her. Korrekte beziehungsweise erzwungene Entscheidungen mit 0,000 Verlust sind damit keine Fehlerlistentreffer. Wird ausdrücklich die vollständige Entscheidungsliste gewählt, etwa über die Gesamtzahlen in der Statistik oder durch Ausschalten des Fehlerfilters, heißt sie **Entscheidungen pro Zug**. Bei einer Einzelspielanalyse zeigt der Spielfilter ausdrücklich das aktuelle Spiel, auch nach einem Statistik-Klick oder dem Zurücksetzen der Filter. Statistik-Klicks im bereits geöffneten Spiel behalten den Brettzug bei. Bei der gesamten Matchanalyse bleiben „Alle Spiele“ und gezielte Filter einzelner Spiele verfügbar. Bestehende Analysewerte werden dabei nicht verändert.

Die Layoutgrößen beziehen sich auf die nutzbare logische Fensterfläche, auch unter macOS **More Space**. Die Desktopprüfung umfasst 1800×1169, 2560×1440 und 3840×2160; es wird keine physische Pixelauflösung als CSS-Auflösung vorausgesetzt. Kennzahlen filtern in der Übersicht die bereits sichtbare Fehlerliste; die Herkunft und Abdeckung der Statistik bleiben erkennbar.

Der Zugverlauf zeigt pro Zug zwei kompakte Zeilen: die Notation und darunter Spielernamen sowie Würfel. Rechts steht der lokalisierte Equity-Verlust mit drei Nachkommastellen. **Grün**, **Gelb**, **Orange** und **Rot** unterscheiden gute Züge, Ungenauigkeiten, Fehler und Blunder; die Fehlergrenzen bleiben 0,04 / 0,08 / 0,16. Ein echter Nullverlust erscheint als 0,000, ein noch nicht ausgewerteter Zug als neutraler Strich. Zugängliche Quellen-/Tiefenangaben bleiben erhalten. Der ausgewählte Zug erhält unabhängig von seiner Bewertung einen dezenten Hintergrund und einen Akzent am linken Rand. Die Werte werden gesammelt für das ausgewählte Spiel geladen, mit nativen Aufträgen und importierten Analysen als getrennt erkennbaren Quellen. Ein verspätetes Ergebnis darf kein anderes Spiel einfärben.

Ansichten bleiben beim Tabwechsel eingebunden: Suchfilter, ungespeicherte Notizen und SSE-Abonnements bleiben erhalten. Akzeptierte Aufträge werden auch dann im Hintergrund weiterverfolgt, wenn während der POST-Antwort die Position oder das Spiel gewechselt wird. Die redundante globale Laufanzeige entfällt; Fortschritt und Fehler bleiben beim jeweiligen Analyseumfang sichtbar. Abschlüsse aktualisieren weiterhin die Zugbewertungen.

**Einstellungen** öffnet einen eigenen modalen Dialog mit den Bereichen Allgemein, Analyse, Spielen und Brett. Neben Sprache und Theme enthält er einen persönlichen Spielernamen, den GNUbg-Programmpfad sowie ein Analyseprofil (0–4 Ply, Standard 2-Ply) und ein getrenntes künftiges Spielprofil (0–2 Ply). Der Spielbereich konfiguriert nur zukünftige Spiele; Mensch gegen Engine und BGSage sind noch kein spielbares Feature. Die vorhandene Engineauswahl zeigt GNUbg und kennzeichnet BGSage als geplant/nicht verfügbar.

Enginepfad, Spielername und Profile werden mit Revisionsprüfung in PostgreSQL gespeichert. Eine gleichzeitige Änderung liefert einen sichtbaren Konflikt und erhält den Entwurf. **Installation prüfen** und Speichern prüfen die Version und eine echte GNUbg-Positionsbewertung einschließlich Netzgewichten mit insgesamt zehn Sekunden Zeitlimit. Der Pfad gehört zum Rechner des Rust-Backends. Ungültige Einstellungen ersetzen keine funktionierende Konfiguration. Sprache und Theme bleiben lokale Browserpräferenzen.

Migration 0019 erweitert die Analysetiefe auf 3-Ply und setzt den ursprünglichen, nie gespeicherten Standard auf 2-Ply. Explizit gespeicherte Profile bleiben erhalten. Bestehende Aufträge und Ergebnisse behalten ihre ursprüngliche Tiefe und Enginekonfiguration. Die Engine kann bei eindeutigem Ergebnis die Suche früh beenden; die eingestellte Tiefe ist das Suchprofil. Migration 0020 ergänzt 4-Ply für Positionen, einzelne Spiele, ganze Matches und Dice Distribution; sie ändert weder gespeicherte Profile noch den 2-Ply-Standard. Höhere Tiefen benötigen insbesondere bei kompletten Matches und allen 21 Würfelpaaren mehr Rechenzeit. Native Tiefentests prüfen tatsächliche GNUbg-Ausgaben und Partieexporte statt nur der angeforderten Tiefe in den Auftragsmetadaten.

Jeder neue Analyseauftrag erhält eine unveränderliche Enginekonfiguration; explizite oder gespeicherte Ply stehen am Auftrag. Aktive Deduplizierung berücksichtigt neben Ziel/Umfang/Ply auch den Enginepfad und die Version. Läuft ein älterer Auftrag nach einer externen Ersetzung des Binary am gleichen Pfad weiter, schlägt er mit Diagnose fehl, statt Ergebnisse verschiedener Versionen zu mischen. Bereits gespeicherte Match-Checkpoints bleiben erhalten. Legacy-Aufträge ohne Snapshot verwenden weiterhin die beim Backendstart konfigurierte Engine. MAT-Importe verwenden den aktuell gespeicherten GNUbg-Pfad.

## Position Browser und gespeicherte Suchen

**Positionen** ist die persönliche Sammlung bewusst gemerkter Stellungen. Importierte Partien füllen sie nicht automatisch. **Position merken** neben dem Brett im Analysearbeitsplatz übernimmt die konkrete Entscheidung mit ihrem Kontext; **Aus Sammlung entfernen** entfernt nur diese Markierung. Ursprüngliche Matchdaten, Notizen, Analysen und Lernkarten bleiben beim Entfernen erhalten. Die Sammlung startet leer; bestehende Tags oder Analysen werden nicht als ausdrückliche Auswahl interpretiert. Einzelne oder mehrere ausgewählte Einträge lassen sich in der Sammlung entfernen. Gemerkte Positionen bleiben auch nach dem Löschen ihres Ursprungsmatches oder der Quelldatenbank erhalten: Spieler, Wurf, Spielstand, ursprüngliche Match-ID, zugehörige Notizen und vorhandene Positionsanalysen werden eigenständig aufbewahrt. Sie lassen sich weiterhin einzeln analysieren, kommentieren und in Lernmodus oder Anki verwenden. Originaldateien und übrige Matchzüge werden gelöscht; Match-/Spielanalyse und Quellexport gehören zu weiterhin vorhandenen Originalpartien. Die Suche umfasst Datenbank, Freitext, Spieler am Zug, Matchname, Turnier, Runde, exakte Tags, Kommentare, Checker-ID, Matchpunkte, Wurf, Spielstand, Crawford, Cube-Wert sowie vorhandene lokale Analyse und nativen Equity-Verlust. Ein einzelner Würfel passt auf beide Würfel; ein Paar wird unabhängig von seiner Reihenfolge gesucht. Fehlende Analysewerte bleiben unbekannt. Native Fehlerwerte nennen ihre tatsächliche Engine und Tiefe. Importierte/Buchanalysen werden dafür nicht als lokale Fehlerbewertung ausgegeben.

Die kompakte Positionstabelle nutzt die Desktop-Breite mit dauerhaft sichtbarer Brettvorschau rechts. **Spieler am Zug** und **Gegner** stehen in getrennten Spalten; Match-ID und Name, Datenbank, Partie, Zug, Wurf, Spielstand, Doppler, Verlust und Notizen sind direkt vergleichbar. Eine Zeile zeigt genau eine gespeicherte Positionsinstanz; Klick oder Pfeiltasten aktualisieren deren Vorschau, Enter oder Doppelklick öffnet die Analyse. Ein gemeinsamer Hover-Text liefert Kontext und Analyseherkunft. Spalten lassen sich über die Kopfzeilen in beide Richtungen sortieren und an den Rändern vergrößern oder verkleinern. Positionen speichern eigene Spaltenbreiten, unabhängig von der Matchdatenbank; Zurücksetzen stellt die an das Fenster angepasste Ansicht wieder her.

Die Spalte **Lernstapel** zeigt alphabetisch die Decks, in denen die Lernposition bereits enthalten ist. Sie ist sortierbar und größenveränderbar; vorhandene benutzerdefinierte Spaltenbreiten bleiben beim Ergänzen der Spalte erhalten. Vorschau und Zeilenübersicht zeigen die vollständigen Deck-Namen. Nach einer Übernahme aktualisiert sich die Liste, einschließlich bereits nachgeladener Seiten, Auswahl und Vorschau. Die Zuordnung berücksichtigt Brettstellung, Wurf, Spielstand und Doppler: Gleichwertige Vorkommen teilen ihre Deck-Zugehörigkeit, unterschiedliche Entscheidungskontexte bleiben getrennt. Sie bleibt auch nach dem Löschen des Ursprungsmatches verfügbar.

Für Fragen wie „Wie spielt Mochy diese Position?“ den sichtbaren Filter **Spieler am Zug** mit Checker-ID und Würfeln kombinieren. Beide Spielfarben werden berücksichtigt; Positionen, in denen nur der Gegner am Zug ist, bleiben ausgeschlossen. **Gespielte Züge anzeigen** blendet die tatsächlich aufgezeichnete Entscheidung als zusätzliche Spalte und in der Vorschau ein. Die Einstellung wird lokal gespeichert und ist anfangs ausgeschaltet, damit Lernfragen verdeckt bleiben. Sortierung und Richtung gehören zur gespeicherten Suche.

Treffer behalten ihre vollständigen Positions-/Match-IDs einschließlich Wurf, Score und Cube. Notizen werden nur von passenden Ebenen berücksichtigt: Match, betreffende Partie, Position und zugehöriger Checkerzug. Ein Kommentar zu einem anderen Zug macht nicht das ganze Match zum Treffer. Die erste Version durchsucht gespeicherte Checkerentscheidungen; Cube-Entscheidungen bleiben über Statistik und Cube-/Zugnotizen erreichbar. Eine Vorschau zeigt die Ausgangsstellung; deren Schaltfläche **Analyse öffnen** führt zum exakten Zug im Analysearbeitsplatz. In Tabellenzeilen entfällt diese redundante Schaltfläche; Enter und Doppelklick öffnen die Analyse weiterhin direkt.

Benannte Suchdefinitionen liegen mit Schema- und Änderungsrevision in PostgreSQL. Laden, neu speichern, aktualisieren/umbenennen und löschen stehen unter **Gespeicherte Suchen** bereit. Gleichzeitige Änderungen liefern HTTP 409 und erhalten den Entwurf. Ein Bezug zu einer gelöschten Datenbank wird sichtbar und führt nicht stillschweigend zu einer globalen Suche. Pagination bindet den Cursor an die vollständige Filterdefinition; beim Rückwechsel bleibt die geladene Ergebnisfolge mit Vorschau und Auswahl erhalten und wird aktualisiert.

Bis zu 200 ausgewählte Treffer lassen sich in einen neuen oder bestehenden Lernstapel übernehmen. Jede Position erhält ein Ergebnis: hinzugefügt, aktualisiert, bereits vertreten, Analyse erforderlich oder nicht mehr vorhanden. Nur verwendbare abgeschlossene lokale Lösungen liefern Lernantworten; fehlende oder nicht auswertbare Analysen erzeugen keine erfundene Karte. Identische Lernfragen teilen die vorhandene FSRS-Historie. **Dieses Deck lernen** öffnet den bestehenden Lernmodus ohne eine alte aufgedeckte Antwort mitzunehmen; **Gesamtes Deck als Anki herunterladen** verwendet den vorhandenen Rust-APKG-Export. Das Deck-Limit bleibt 1.000 Karten, ein Grenzverstoß übernimmt keine Teilmenge.

Der sichtbare Importknopf wurde aus dem Header entfernt. Import und Mehrfachimport bleiben in der Matchdatenbank verfügbar, ebenso der gemeinsame Dateieingang und Drop auf der Startansicht.

## Zugvorschau und interaktive Statistiken

Der Analysearbeitsplatz bietet **Initial**, **Arrows** und **Final**. Standardmäßig zeigen grüne, kontrastreich umrandete Pfeile den gespielten Zug mit legalen Zwischenpunkten. Checker-Kandidaten aus Datei, BackgammonDB oder neuer Engineanalyse besitzen eigene Vorschau-Schaltflächen und „Besten Zug ansehen“. Quelle und Rang einer Kandidatenvorschau bleiben am Brett sichtbar; **Vorschau zurücksetzen** beendet den Vergleich. Die redundante Zeile mit dem gespielten Zug unter den Brettschaltern entfällt. Die Nachher-Ansicht hält die Brettorientierung fest und blendet die Würfel aus. Bar-Eintritt, Hits, Mehrfachzüge, Doppelwürfe und Bear-off werden vor der Darstellung legal geprüft. Ungültige oder unbekannte Notation fällt auf die gespeicherte Ausgangsstellung zurück. Alle Themes verwenden dieselbe Projektion.

Vorschauen ändern keine Stellung-IDs, Analyseziele, Notizen oder Lernkarten: Diese beziehen sich weiter auf die Entscheidung vor dem Zug. Positions- und Quellenwechsel verwerfen fremde Kandidaten.

Der Reiter **Statistik** zeigt zum gewählten GNUbg-Auftrag Spielerwerte und eine Partieübersicht. Metriken öffnen die sichtbare Entscheidungsliste mit passendem Spieler-, Checker-/Cube-, Fehlerschwere- und Partiefilter. Partie-Schaltflächen öffnen genau das ausgewählte Spiel. Filterwechsel beginnen die Pagination neu. Native PR bleibt von Quellanalysen und Buchwerten getrennt; fehlende native Daten werden als nicht verfügbar angezeigt. Neue Aufträge speichern vollständige Partiestatistiken atomar. Ältere Aufträge erhalten aus ihren Entscheidungen rekonstruierte Summen, jedoch keine erfundene Partie-PR.

## Quellanalysen und Opening Books

Importe behalten vorhandene Analysen in eigenen Datensätzen: Engine/Version soweit aufgezeichnet, originale Tiefen-/Profilbezeichnung, Methode, Kandidaten, Cube-Antworten, Equity-Einheit und unveränderte Rohwerte. XG, native GNUbg-SGF und verfügbare BGBlitz-Informationen werden gelesen. MAT ohne Analysedaten erhält keine erfundene Analyse. Unbekannte Engines oder Cube-Einheiten bleiben unbekannt. GNUbg-MWC wird nicht als normalisierte Equity ausgegeben. Neue Berechnungen und unabhängige Matchkopien überschreiben keine Quellresultate. Auch frühere Importe können die Analyse einmalig aus ihren gespeicherten Originaldateien rekonstruieren.

Im Analyseinspektor sind **Importierte Analyse**, **Eröffnungsbuch** und **Lokale Engine** getrennte Quellen. Cube-Quellanalysen tatsächlicher Double-/Take-/Drop-Aktionen sind zusätzlich im Dialog „Cube- und Zugnotizen“ zugänglich. Buchabfragen verwenden den vollständigen Kontext; die 80-Bit-Checkerstellung allein reicht nicht.

Die Buchverwaltung ist ausschließlich über **Einstellungen → Analyse → Eröffnungsbücher** erreichbar. Im Analysearbeitsplatz bleiben die positionsbezogene Quellenauswahl und Angaben zu Herkunft, Engine und Tiefe sichtbar. Die Buchverwaltung speichert Anbieter und Dateipfade mit Versionsprüfung und erhält ungespeicherte Einstellungen im übergeordneten Dialog. Die Dateipfade gehören zum Backend-Rechner. Automatisch erlaubt den Vergleich beider Bücher; direkte Anbieterwahl und Aus sind ebenfalls verfügbar. Eine manuelle Engineanalyse berechnet weiterhin das gewählte native GNUbg-Profil. Andere Buch-Engines oder -Versionen werden nicht als identischer nativer Cachehit ausgegeben und nicht in Match-PR gemischt.

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

Über **Spalten** oberhalb der Matchliste lässt sich jede Datenspalte ein- oder ausblenden. **Turnier** und **Kommentar** sind standardmäßig ausgeblendet; **Tags** ist eine eigene, sichtbare Spalte. **Alle Spalten anzeigen** und **Standardspalten** stehen im selben Menü bereit. Auswahl und Aktionen bleiben erreichbar, mindestens eine Datenspalte bleibt sichtbar. Sichtbarkeit und Breiten werden getrennt lokal im Browser gespeichert; ausgeblendete Spalten behalten ihre Breite. Bestehende Breiten der früheren Notizspalte werden auf Tags übernommen. Filter, Sortierung und Mehrfachauswahl bleiben beim Umschalten erhalten; eine ausgeblendete Sortierspalte wird oberhalb der Tabelle genannt. Auch ausgeblendete Angaben bleiben durchsuchbar und Teil der Zeilenzusammenfassung.

Die Matchdatenbank verwaltet benannte, getrennte Bestände innerhalb derselben PostgreSQL-Instanz. Vorhandene Matches liegen nach der Migration im Standardbestand „My matches“. Neue Bestände lassen sich anlegen, beschreiben, umbenennen und löschen; der Standardbestand bleibt erhalten. Die Auswahl „Alle Datenbanken“ erlaubt die gemeinsame Suche, Importe gehen dann in den Standardbestand. Sonst bestimmt der gewählte Bestand das Importziel. Mehrfachimporte verarbeiten Dateien einzeln und melden Duplikate und Fehler je Datei.

Die exakte Positionssuche in der Matchdatenbank filtert dieselbe Matchliste; getrennte Trefferboxen entfallen. Jedes passende Match erscheint einmal, auch bei mehreren Vorkommen. Match-Schaltfläche, Enter und Doppelklick öffnen bei aktivem Positionsfilter dessen erste passende Position. Der Positionsfilter lässt sich einzeln entfernen und mit allen übrigen Filtern, Sortierung und Mehrfachaktionen kombinieren. Die sortierbare Matchliste zeigt Spieler, **PR**, Spielstand, Matchlänge beziehungsweise Moneygame, gespieltes und Importdatum, Turnier, Analysezustand, Tags und Kommentare. Die kompakte Desktop-Tabelle nutzt die gesamte Breite neben der Datenbankauswahl; eine separate Brettvorschau entfällt. Die Spalte **ID** zeigt eine dauerhaft vergebene numerische Match-ID, unabhängig von Datenbank, Zeilenreihenfolge und Filtern. Umbenennen und Verschieben behalten die ID; Kopien erhalten eine eigene. Nummernlücken sind möglich. Die technische UUID bleibt für REST-Zugriffe erhalten. Die Spalte **Positionen** zeigt nur die Anzahl der gespeicherten Stellungen; das Herkunftsformat bleibt für Import und Export gespeichert. Namen, Tags und Kommentare bleiben einzeilig. Jede Matchzeile hat genau einen gemeinsamen Hover-Text mit Match-ID, Spielern, Matchlänge, Ergebnis, verfügbaren PR-Werten, Analyseprofil und Kontext. Tags und Kommentare werden darin als begrenzte Auszüge zusammengefasst; eigene Zell-Tooltips entfallen. Die rechten Ränder der Spaltenköpfe lassen sich ziehen; die gewählten Breiten werden lokal im Browser gespeichert. Auf einen fokussierten Spaltentrenner wirken Pfeiltasten (mit Umschalt in größeren Schritten), Pos1/Ende und Escape zum Abbrechen einer laufenden Größenänderung. **Spaltenbreiten zurücksetzen** stellt die an das aktuelle Fenster angepassten Standardbreiten wieder her. Breite Tabellen bleiben innerhalb der Matchliste horizontal scrollbar. Alle Datenspalten besitzen Sortierknöpfe mit Richtungspfeilen. Erster Klick sortiert aufsteigend, der nächste absteigend. **Tags** und **Kommentar** besitzen eigene Sortierknöpfe; Auswahl- und Aktionsspalten bleiben Bedienelemente. ID, PR, Positionszahl und Spielstand werden numerisch sortiert (Spielstand zuerst Weiß, dann Schwarz), Text ohne Beachtung der Groß-/Kleinschreibung. Fehlende Werte stehen in beiden Richtungen am Ende. Die Sortierung erfolgt vor der serverseitigen Pagination; ein Sortierwechsel beginnt wieder mit der ersten Seite und hebt die Mehrfachauswahl auf. Der Stift pro Zeile öffnet direkt den Metadateneditor. Die Match-Schaltfläche, Enter oder ein Doppelklick öffnet den Analysearbeitsplatz, Pfeiltasten wechseln zwischen den Zeilen. Metadaten wie Anzeigename, Datum, Turnier, Runde, Ort und Ergebnis sind mit Versionsprüfung editierbar; Originaldatei und Analysekontext bleiben erhalten. Gespielte Daten werden nur aus eindeutigen SGF-Quellangaben übernommen. Fehlende Angaben bleiben unbekannt.

Kombinierbare Filter umfassen Spieler, Datum (gespielt/importiert), exakten Tag, Kommentar, Position-ID, Turnier, Matchname, Match/Moneygame, Ergebnisstatus, Gewinnerfarbe beziehungsweise Remis, Runde und Matchpunkte. Notiz- und Positionssuche beachten den gewählten Bestand. Pagination bindet Sortierung und Filter an den Cursor. Angezeigte PR-Werte stammen ausschließlich aus einer vollständigen Matchanalyse mit nativen GNUbg-Statistiken; einzelne Spielanalysen werden als Teilanalyse geführt. PR verwendet normalisierten Equity-Verlust × 500 / (nicht erzwungene Checkerzüge + nahe oder tatsächliche Cube-Entscheidungen). Die Entscheidungszählung unterscheidet sich von XG/BMAB. Fehlende, ältere oder unvollständige Statistiken ergeben keinen erfundenen PR. Siehe [GNUbg-Statistik](https://www.gnu.org/software/gnubg/manual/gnubg.html#gnubg-matches_review_stat_overall).

Mehrfachauswahl unterstützt **Kopieren, Verschieben, Tags, Export und Löschen**. Je Aktion sind bis zu 100 Matches möglich, beim Export 50 und insgesamt 64 MiB. Kopieren erzeugt eigenständige Matches einschließlich Positionen, Partie-/Zug-IDs, Tags, Kommentaren und der letzten abgeschlossenen Analyse je Umfang. Lernfortschritt und aktive Aufträge werden nicht kopiert. Duplikate werden je Datenbank anhand des Originaldatei-Hashes erkannt und niemals mit Quellnotizen überschrieben. Verschieben erhält die IDs; bei einem Zielduplikat bleibt das Quellmatch bestehen. Aktionen melden den Ausgang je Eintrag. Löschen erfordert eine konkrete Bestätigung und lässt Matches mit wartender/laufender Analyse unangetastet. Lernkarten und deren Wiederholungsverlauf bleiben eigenständige Datensätze.

Export liefert Originaldateien oder kanonische SGF, bei mehreren Matches ein ZIP. Dies ist ein Quellexport; BG-Tutor-Notizen und Lernverlauf sind darin nicht enthalten. Für eine vollständige Sicherung dient weiterhin das PostgreSQL-Backup. Die übernommenen ChessBase-Ideen sind benannte Bestände, sortierbare Listen, Mehrfachverwaltung und eine schnelle Brettvorschau: [Vorschaufenster](https://help.chessbase.com/CBase/16/Eng/preview_window_in_the_database.htm), [Bestände organisieren](https://en.chessbase.com/post/chessbase-18-tips-for-beginners-part-7-organising-databases).


XG- und BGF-Importe erhalten das Ergebnis der letzten abgeschlossenen Partie. Migration 0018 und die begrenzte Nachbefüllung korrigieren bereits automatisch abgeleitete Ergebnisse aus den unveränderten Originaldateien; manuell gespeicherte Metadaten bleiben erhalten. Die Nachbefüllung verändert weder kanonische SGF noch Positionen, importierte Analyse, Notizen oder Lernfortschritt. Moneygames und nicht belegbare Ergebnisse bleiben unbekannt. Die Matchtabelle zeigt Spielernamen und gegebenenfalls einen eigenen Anzeigenamen statt einer zusätzlichen Dateinamenspalte.

## Gespeicherte Match- und Positionsdaten

Der Import speichert die Originaldatei, kanonische SGF und eigene normalisierte Positionszeilen gemeinsam in PostgreSQL. Jede Position enthält 26 vorzeichenbehaftete Checker-Anzahlen einschließlich Bar, Spieler am Zug, Würfel, Spielnummer, Zugnummer und gespielten Zug. Ein gesonderter Spielkatalog enthält auch Spiele ohne Checkerzüge samt Startspielstand und Crawford-Status. Bei älteren Importen wird dieser Katalog beim ersten Zugriff einmalig aus der gespeicherten SGF ergänzt.

Die Brettstellung besitzt zusätzlich die kompakte **80-Bit-GNUbg-Position-ID**, gespeichert als 14 Base64-Zeichen mit B-tree-Index. Sie kodiert Checker-Anzahlen und ist kein reines Belegungs-Bitboard. Die separate **72-Bit-GNUbg-Match-ID** enthält unter anderem Spielstand, Matchlänge, Doppler, Würfel und Regeln. Exakte Positionssuchen verwenden den Index; dafür wird die Ursprungsdatei nicht erneut geparst. Ein weiterer Index auf Match, Spiel und Reihenfolge ermöglicht den direkten Zugriff auf ein spätes Spiel, ohne vorherige Spiele zu laden.

## Board-Themes und Sprachen

Die Originalgrafiken der lokal bereitgestellten BGBlitz-Archive werden automatisch bei `npm run dev` und `npm run build` eingelesen. Dafür wird **Python 3.10+** ohne zusätzliche Pakete benötigt. Die Darstellung verwendet echte Hintergründe, Checker-Varianten, Würfel, Doppler und Nummernleisten. Neue Archive in `themes/` ablegen und den Entwicklungsserver neu starten oder `npm run themes:prepare --prefix web` ausführen. Ohne lokale ZIPs bleiben die drei eingebauten Themes verfügbar. Format, Herkunft und Rechte: [themes/README.md](themes/README.md).

Sprache und Theme werden im eigenen Einstellungsdialog als Entwurf bearbeitet und beim **Speichern** übernommen. **Abbrechen** verwirft die Vorschau. Texte, Status, Fehlermeldungen, Beschriftungen und Zahlenformate wechseln nach dem Speichern; bereits sichtbare Analyseergebnisse bleiben erhalten. Spieler-/Dateinamen und originale GNUbg-Zugnotationen bleiben ihre ursprünglichen Daten. Native technische Engine-Diagnosen erscheinen zusammen mit einer lokalisierten Fehlermeldung. Schlüssel und Platzhalter aller sechs Kataloge werden typisiert und beim Start geprüft. Die Übersetzungen sind noch nicht muttersprachlich lektoriert.

## Oberflächenthemes und Spielplan

Unter **Einstellungen → Darstellung** stehen **Graphite**, **Club** und **Paper** mit **Hell**, **Dunkel** oder **System** zur Auswahl. System folgt den Änderungen des Betriebssystems. Die Vorschau verändert nur ihren eigenen Ausschnitt; erst erfolgreiches Speichern übernimmt den Entwurf. Die Auswahl wird lokal für diesen Browser gespeichert und bereits vor dem ersten Rendern angewandt. Sie ist unabhängig vom BGBlitz-Brett-Theme, der Sprache und dem Analyseprofil. Auch Tabellen, Lernmodus, Dialoge und Fehleranzeigen verwenden die gemeinsamen Oberflächenfarben.

Am Analysebrett zeigt **Spielplan** je einen benannten Eintrag für Weiß und Schwarz: reines Rennen, Rennen mit Kontakt, Angriff/Blitz, Prime oder Ankerspiel. **Initial** und **Arrows** verwenden die Ausgangsstellung, **Final** die tatsächlich angezeigte Stellung nach dem Zug; bei einer nicht projizierbaren Zugnotation bleibt die Ausgangsstellung maßgeblich. Die Anzeige benötigt keinen Analyseauftrag. Sie bleibt auch für eigenständig gespeicherte Positionen ohne Ursprungsmatch verfügbar.

Die deterministischen Brettregeln sind in Rust aus der [öffentlichen Open-Sage-Referenz](https://github.com/markbgsage/bgsage/blob/139427d826dd67798226e7d23c003c90315306ba/cpp/src/encoding.cpp) übernommen; Herkunft und MPL-2.0-Lizenz stehen unter `backend/src/game_plan_source.txt` und `backend/licenses/`. `POST /api/game-plans` akzeptiert ausschließlich eine gültige 26-Punkte-Brettdarstellung und liefert beide Farbklassen samt Regelversion. Ein begrenzter Clientcache vermeidet erneute Berechnungen identischer Stellungen. Die Klassen beschreiben den Brettaufbau; Spielstand, Würfel und Dopplerbewertung fließen nicht ein. Konkrete Zug- und Cube-Empfehlungen kommen weiterhin aus der Engineanalyse.

## Stand der besprochenen Erweiterungen

| Funktion | Stand |
| --- | --- |
| Match-, Spiel- und Positionsanalyse mit GNUbg, SSE, Fehlerliste, 0–4 Ply | Vorhanden; Standard 2-Ply. |
| Mehrere Matchdatenbanken, unabhängige Kopien, Mehrfachverwaltung, numerische IDs, Sortierung, Spaltenauswahl | Vorhanden; Turnier und Kommentar sind optional. |
| Tags und Kommentare zu Match, Partie, Position und Quellzug; durchsuchbare Notizen | Vorhanden. |
| Persönliche Positionssammlung, gespeicherte Filter, Spielernamen und Vorschau; Erhalt nach Löschen der Quelle | Vorhanden. |
| BGBlitz-Brett-Themes, sechs Sprachen, Initial/Arrows/Final, Zugpfeile, anklickbare Statistiken | Vorhanden. |
| Lernmodus mit FSRS, Rust-Anki-Export, Deck-Zugehörigkeit und Karten-Brettvorschau | Vorhanden. |
| Graphite/Club/Paper als Oberflächenthemes mit Hell/Dunkel/System | Vorhanden; vom Brett-Theme getrennt, mit Vorschau und lokaler Speicherung. |
| Zwei getrennte Spielplananzeigen beim Brett, passend zu Initial/Final | Vorhanden; lokale Brettklassifikation, unabhängig von GNUbg. |
| Decks umbenennen und löschen | Vorhanden; mit Versionsprüfung und Bestätigung, ohne Verlust geteilter Karten oder Lernverläufe. |
| Fehlerschwere getrennt nach Checker und Cube in der nativen Statistik | Vorhanden; anklickbare Gesamt-/Checker-/Cube-Matrix für Spieler, Match und Partie, auch für vorhandene native Analysen. |

Bewusst spätere Ausbaustufen sind **Mensch gegen Engine**, Open Sage als vollständige Analyse-/Spielengine, Rollouts und die **Tauri-Hülle**. Die Spielplanklassifikation allein ist keine Integration der kompletten Sage-Engine.

Weitere besprochene Produktideen aus dem [BackgammonDB-Vergleich](docs/research/2026-11-08-backgammondb-product-ux.md) sind noch keine vollständig umgesetzten Funktionen: benannte Notebooks und ein Positionseditor/XGID-Eingang, automatische Spielphasen-Tags mit Suchfiltern, gemeinsame Checker-/Cube-Aktionsliste, persönliche langfristige Stärken-/Schwächenstatistik, ein Sitzungsrückblick mit gezielter Fehlerwiederholung sowie ein portables Bibliotheksbackup in der Oberfläche. Die vorhandene eigenständige Positionsaufbewahrung und die neue Spielplananzeige liefern dafür Grundlagen. AnkiConnect, fremde Decks importieren und bidirektionale Synchronisation gehören weiterhin nicht zur ersten Anki-Ausbaustufe.

## Getrennte Checker-/Cube-Fehlerstatistik

Die native Analysestatistik zeigt je Spieler eine kompakte Matrix **Gesamt / Checker / Cube** für Ungenauigkeiten, Doubtful, Bad und Very bad. Jeder verfügbare Wert öffnet die Fehlerliste mit passendem Spieler, Entscheidungstyp und Fehlerschwere. Ein Wert aus der Partieübersicht bindet den Filter zusätzlich an genau diese Partie; bestehende Gesamt-Aktionen bleiben erhalten. Auch echte Nullen lassen sich anklicken und ergeben eine leere Trefferliste.

Die zusätzlichen Zahlen werden aus den bereits gespeicherten, vollständig checkpointeten Analyseentscheidungen gelesen. Vorhandene Analysen benötigen keinen neuen GNUbg-Auftrag. Gesamtwerte, persistierte Analyse-JSON und native PR werden nicht umgeschrieben. Erzwungene, nicht auswertbare und verlustfreie Entscheidungen erhöhen die Fehlerzahlen nicht. Fehlt bei alten oder unvollständigen Ergebnissen ein belastbarer Nachweis der Entscheidungsabdeckung, erscheint **—** mit Erklärung statt einer erfundenen Null. Die API ergänzt `/api/jobs/{id}/statistics` um `error_breakdown` für Auftrag und Partien; die Fehlerliste berücksichtigt dieselben Einschränkungen einschließlich Cursorprüfung.

## Tags, Kommentare und Lernen

Unter **Lernen & Anki** stehen beim ausgewählten Deck **Umbenennen** und **Löschen** bereit. Umbenennen behält Deck-ID, Karten, Vorschauauswahl und Lernstand. Löschen erfordert eine ausdrückliche Bestätigung mit Deckname und Kartenanzahl; es entfernt nur dieses Deck und seine Zuordnungen. Karten, Wiederholungsverlauf, gespeicherte Positionen und andere Decks bleiben erhalten. Wird dieselbe Entscheidung später erneut in ein Deck übernommen, wird die vorhandene Karte mit ihrem Lernstand wiederverwendet. Der Anki-Download verwendet den aktuellen Decknamen.

Parallele Umbenennungen und neue Kartenzuordnungen ändern die Deckversion. Ein veralteter Dialog überschreibt oder löscht den aktuellen Stand nicht: Der Entwurf bleibt erhalten, **Deckangaben aktualisieren** aktualisiert die Grundlage, und erst eine erneute ausdrückliche Aktion schreibt die Änderung. Nach einer Deckänderung aktualisieren sich auch die Deck-Auswahl und Zugehörigkeitsanzeigen im Positionsbrowser. Eine laufende Lernansicht des gelöschten Decks wird geschlossen; verspätete Antworten können ihre Karten nicht zurückbringen. Die REST-Erweiterung bietet `PUT /api/decks/{id}` mit `name` und `expected_version` sowie `DELETE /api/decks/{id}?expected_version=n` mit Status204 bei Erfolg,409 bei veralteter Version und404 bei fehlendem Deck.

Die Ansicht **Lernen & Anki** folgt dem Aufbau der Positionssuche: Überschrift **Lernkarten** außerhalb der Panels, Suchformular oberhalb einer kompakten Ergebnistabelle und Brettvorschau rechts. **Alle Lernkarten** durchsucht die gemeinsame Kartensammlung über alle Decks; die Deckauswahl grenzt den Bestand ein. Mehrfach enthaltene Karten erscheinen einmal und nennen ihre Deckzugehörigkeiten. Auch Karten ohne verbliebene Deckzuordnung bleiben auffindbar. Gespeicherte Deckauswahlen werden weiterhin berücksichtigt.

Die serverseitige Suche berücksichtigt Spieler, Fragen, Positionskennungen, Decknamen, Tags und Kommentare vor der Paginierung. **Spieler am Zug**, exakter Tag, Kommentartext, exakte Checker-ID, Entscheidungsart und Lernstatus sind kombinierbare Filter. Neue Karten haben noch keine Wiederholung; fällige Karten können auch neu sein. Bereits wiederholt umfasst alle Karten mit mindestens einer abgeschlossenen Wiederholung. Filter betreffen die Trefferliste; Lernen und Anki-Export verwenden weiterhin das ausdrücklich ausgewählte Deck. Ohne ausgewähltes Deck sind diese Aktionen deaktiviert. Match-, Partie-, Positions- und Zugnotizen werden für die Suche frisch gelesen, auch bei erhaltenen Positionen nach dem Löschen der Quelle. Antworten werden durch die Suche nicht aufgedeckt, Fälligkeiten und Wiederholungsverlauf bleiben unverändert. `GET /api/cards` und `GET /api/decks/{id}/cards` unterstützen dieselben Filter, Gesamtzahl und UUID-Paginierung; alle Parameter und Ergebnistypen stehen in OpenAPI.

Die Kartenliste unter **Lernen & Anki** bietet rechts eine Brettvorschau der ausgewählten Karte mit Fragetyp, Spielern, Würfeln, Spielstand und Doppler im gewählten Board-Theme. Die erste Karte wird automatisch ausgewählt; Klick, Pfeiltasten und Pos1/Ende wechseln die Vorschau. Lange Listen scrollen unabhängig vom sichtbaren Brett. Weitere Karten lassen sich nachladen, ohne die Auswahl zu verlieren. Beim Deckwechsel wird die vorherige Vorschau entfernt. Das Durchsehen zeigt die Frageseite und verändert weder Lernfortschritt noch Fälligkeit; Antworten und Bewertungen gehören zum ausdrücklich gestarteten Lernmodus.

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
| GET | `/api/matches/{id}/move-assessments?game_no=&job_id=` | Gespeicherte Checkerbewertungen eines Spiels mit Quelle und Tiefe; optional auf gewählten nativen Auftrag begrenzt |
| GET | `/api/positions/{id}` | Direkter Positionszugriff mit Match- und Spielnummer |
| POST | `/api/jobs` | `{ "position_id": "<Positions-UUID>", "kind": "analysis", "plies": 0 }` |
| GET | `/api/jobs/{id}` | Zustand, Fortschritt, Ergebnis oder Fehler |
| GET | `/api/jobs/{id}/events` | Persistenter SSE-Strom |
| POST | `/api/matches/{id}/analysis` | Ein Spiel: `{ "plies": 0, "game_no": 2 }`; gesamtes Match ohne `game_no`. Aktive Aufträge werden je Match, Spielumfang und Ply zusammengeführt |
| GET | `/api/matches/{id}/analysis?game_no=` | Letzter Auftrag des Spielumfangs; ohne `game_no` letzter Matchauftrag, sonst `null` |
| GET | `/api/jobs/{id}/moves?game_no=&after=&limit=&errors_only=&player=&kind=&severity=` | Paginierte Zugbewertung; kombinierbare Partie-, Spieler-, Checker-/Cube- und Fehlerschwerefilter; Cursorprüfung |
| GET | `/api/jobs/{id}/statistics` | Native Match-/Partiestatistik mit Analyseabdeckung und vorhandenen PR-Werten |
| GET | `/api/position-browser?filters=&after=&limit=` | Kontextbezogene Checkerpositionssuche; URL-kodierte JSON-Definition mit schema_version 1 |
| GET/POST | `/api/position-filters` | Gespeicherte Suchdefinitionen auflisten/anlegen |
| PUT/DELETE | `/api/position-filters/{id}` | Mit expected_version aktualisieren oder löschen |
| POST | `/api/decks/{id}/positions` | Bis zu 200 Positions-UUIDs mit einzelnem Übernahmeergebnis in einen Lernstapel übernehmen |

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

Die SQLx-Tests erstellen und entfernen eigene Datenbanken. Zusätzliche Tests prüfen den Spielkatalog einschließlich Nachbefüllung älterer Matches, direkte Positionszugriffe, Scope-/Cursor-Isolation, echte Einzelspielanalyse, Wiederaufnahme und Lernkarten aus dem ausgewählten Spiel. Sie prüfen Import/Duplikate, REST-Paginierung, echte Analyse, SSE-Replay/Shutdown, aktive Job-Deduplizierung einschließlich gleichzeitigem Abschluss, atomare Beanspruchung durch zwei Worker, Lease-Recovery, 21 Würfelpaare und Gewichte. Die Partieanalyse wird über alle zehn Spiele und 460 Züge der Referenzpartie geprüft, einschließlich schlechter Checker-Züge, erzwungenem Aussetzen, falschem Double/Drop, fortsetzbaren Checkpoints und dem Ausschluss veralteter Worker. Separate native Tiefentests verwenden eine kurze Partie mit einer nicht trivialen Zugwahl: Sie prüfen echte 3-/4-Ply-Ausgaben für Positionen und Dice Distribution sowie Partieexporte für Spiel- und Matchaufträge. API- und Migrationstests sichern den Bereich 0–4, den Standard 2 und bestehende Profile ab. Weitere Regressionstests prüfen geerbte SGF-Matchdaten, leere/Cube-Spiele, Setups mit Schwarz am Zug, stille lange Engine-Prozesse und die gleichzeitige Fortschrittsspeicherung/Lease-Verlängerung. Parser-Tests vergleichen alle 460 Zugpositionen der offiziellen XG-Referenz mit dem unabhängig durch GNUbg importierten MAT.

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

## Engine-Benchmark

Unter **Extras → Benchmark** lässt sich die Analysegeschwindigkeit der verfügbaren
Engine mit einer oder mehreren Ply-Stufen messen. GNUbg unterstützt derzeit
0–4 Ply. Die gewählten Stufen laufen einzeln und in Auswahlreihenfolge im
Hintergrund; das Verlassen der Ansicht beendet keinen Lauf. **Abbrechen** beendet
auch den aktiven Engine-Prozess. Während der Messung warten andere von BG Tutor
veranlasste Engine-Aufrufe; ein Hinweis bleibt auch in anderen Arbeitsbereichen
sichtbar. Andere Programme auf dem Rechner können die Messung beeinflussen.

Die feste Referenz **WC2014 v1** ist das 11-Punkte-Match Akiko Yazawa gegen
Douglas Mayfield mit zehn Spielen und 460 Checkerzügen. Grundlage ist die
vorhandene unanalysierte GNUbg-SGF-Fixture; Herkunft und Reproduktion sind in
`backend/tests/fixtures/README.md` dokumentiert. Die Anwendung liefert eine eigene
unveränderliche Kopie mit ID, Version und SHA-256-Prüfsumme mit. Vorhandene
Bewertungen werden vor der tatsächlichen Analyse gelöscht.

Gemessen wird die Summe der vollständigen Spielanalyse-Pipelines einschließlich
Prozessstarts, Laden, Checker- und Cube-Analyse sowie Ergebnisaufbereitung.
Wartezeit, Installations-/Versionsprüfungen und Datenbankschreibvorgänge zählen
nicht dazu. **Ø Analysezeit pro Zug** ist diese Gesamtzeit geteilt durch alle
460 Checkerzüge, einschließlich erzwungener Züge; sie ist keine einzeln gemessene
Zuglatenz. Neue Benchmarkläufe verwenden das Messprotokoll
`gnubg-match-v2-unlimited` ohne feste Zeitgrenze. Ein manueller Abbruch beendet
weiterhin den aktiven Engine-Prozess. Ältere Ergebnisse behalten ihren damals
gespeicherten Timeout im unveränderlichen Protokoll-Snapshot.

Jeder Versuch speichert Datum, Backend-Maschine mit Hardwareangaben, Engine und
Version, Programmpfad, Ply, Referenz, Messprotokoll, Fortschritt und Status.
Erfolgreiche Läufe erhalten zusätzlich Gesamtzeit und Durchschnitt in
Millisekunden. Die Historie bleibt unabhängig von Bibliotheksmatches und
Einstellungsprofilen erhalten. Nach einem Backend-Neustart werden unterbrochene
Läufe als fehlgeschlagen gekennzeichnet; Teilzeiten werden niemals fortgesetzt.
Die Engine-Verfügbarkeitsanzeige nutzt während der Messung den bereits geprüften
Snapshot, damit sie keinen konkurrierenden Probeprozess startet.

| Methode | Endpoint | Wirkung |
| --- | --- | --- |
| GET | `/api/benchmarks/reference` | Versionierte Referenz und Messprotokoll |
| GET | `/api/benchmarks/status` | Kompakter Aktivstatus für den globalen Hinweis |
| GET | `/api/benchmarks` | Letzte 1.000 Versuche, neueste zuerst |
| POST | `/api/benchmarks` | Neue Versuche mit `{ "engine_id": "gnubg", "plies": [0, 2] }` |
| POST | `/api/benchmarks/{id}/cancel` | Wartenden/laufenden Versuch abbrechen; terminale Ergebnisse unverändert |

Neue Versuche während einer laufenden Messung werden mit HTTP 409 abgewiesen;
mehrere Stufen werden gemeinsam vor Beginn angelegt. Nicht verfügbare Engines
und ungültige Ply-Auswahlen ergeben HTTP 422. Benchmark-Endpunkte und Datentypen
sind Bestandteil der generierten OpenAPI-Spezifikation.

### Voraussichtliche Analysezeit

Vor dem Start einer Spiel- oder Matchanalyse steht unter Umfang und Ply-Auswahl
**„Geschätzte Analysezeit: ca. …“**. Grundlage ist die Zahl der Checkerzüge im
gewählten Umfang multipliziert mit der durchschnittlichen Zeit pro Zug aus dem
neuesten vollständig abgeschlossenen passenden Benchmark. Die Anzeige nennt
Benchmarkdatum, Engine-Version, Ply und Zuganzahl. Ein Wechsel von Match, Spiel,
Umfang, Ply oder Engine-Installation verwirft alte Antworten.
Während der Übermittlung und Ausführung wird keine Prognose vor dem Start gezeigt.

Passend bedeutet gleiche lokale Maschine und Hardware, gleiche Engine-Installation
und Version, gleicher Ply-Wert sowie kompatible Referenz- und Analyseparameter.
Die erfolgreich abgeschlossenen Protokolle mit früherem 600-Sekunden-Limit und
neuem unbegrenzten Zeitlimit sind für diese Hochrechnung kompatibel. Fehlerhafte,
unvollständige oder fremde Messungen dienen nicht als Kalibrierung.

Die Schätzung enthält wie der Benchmark Cube-Analyse und Prozessstarts; fremde
Aufträge in der Warteschlange zählen nicht dazu. Unterschiedliche Stellungstypen
können die tatsächliche Dauer deutlich verändern. Fehlen passende Messdaten oder
Checkerzüge, ist die Installation nicht ermittelbar oder schlägt die Anfrage fehl,
zeigt die Oberfläche einen verständlichen Hinweis. Die Analyse kann trotzdem
normal gestartet werden. Es wird kein neuer Benchmark für die Prognose ausgeführt.

`GET /api/matches/{id}/analysis/estimate?plies=2&game_no=1&engine_id=gnubg`
liefert die Prognose und ihre Messbasis. Ohne `game_no` gilt das gesamte Match;
ohne `plies` gilt die gespeicherte Standardtiefe. Die Anfrage ist rein lesend,
legt weder Jobs noch Metadaten an und beachtet die vorhandenen Quellen- und
Wettbewerbssperren. Die Schnittstelle ist in OpenAPI dokumentiert.

### Restzeit während der Analyse

Im Fortschrittsbereich einer laufenden Spiel- oder Matchanalyse erscheint zusätzlich
**„Geschätzte Restzeit: ca. …“**. Anfangs dient der gespeicherte passende Benchmark
als Grundlage. Nach jedem vollständig analysierten Spiel wird das gemessene
Analysetempo verwendet: bestätigte Analysezeit pro tatsächlich zeitgemessenem
Checkerzug × verbleibende Checkerzüge. Dadurch zählen große Spiele entsprechend
stärker als kurze. Innerhalb eines Spiels läuft ein Sekunden-Countdown; echte
Fortschrittsdaten liefert die Engine derzeit nach vollständigen Spielen.

Warteschlange und Warten auf die Engine zählen nicht als Analysezeit. Zeitwerte
werden mit monotoner Uhr gemessen und dauerhaft am Auftrag gespeichert. Neuladen
und Wiederaufnahme erhalten bestätigte Arbeit; ein unterbrochenes Teilspiel wird
neu gemessen. Auch ältere Aufträge mit bereits erledigten, aber noch ungemessenen
Zügen verfälschen den Durchschnitt nicht. Änderungen in den Einstellungen ändern die
Messbasis eines gestarteten Auftrags nicht.

Dauert ein Spiel länger als erwartet, steht dort **„Restzeit wird neu geschätzt …“**.
Ohne passende Messwerte bleibt die Restzeit zunächst unbekannt; nach dem ersten
verwertbaren Spiel ist eine Prognose trotzdem möglich. Nach Abschluss oder Fehler
verschwindet die aktive Restzeitanzeige.

`GET /api/jobs/{id}/remaining-time` liefert Zustand, Messbasis, bestätigte und
zeitgemessene Zugzahlen sowie die aktuelle Restzeit. Die rein lesende Schnittstelle
beachtet dieselben Quellen- und Wettbewerbssperren wie der Auftrag und startet
keine Engine. Aktive Messwerte werden ungefähr einmal pro Sekunde gespeichert;
die Oberfläche ergänzt dazwischen einen monotonen Countdown.
