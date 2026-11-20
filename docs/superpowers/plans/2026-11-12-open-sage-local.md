# Open Sage lokal in NeonDojo — Architektur und Umsetzungsplan

Stand: 12. November 2026. Umsetzung im isolierten Worktree `codex/open-sage-local` auf Basis `d3b1434`; vorhandene und parallel entstandene Änderungen im Hauptworktree bleiben erhalten.

Verifizierte Präzisierung gegenüber dem Entwurf: Open Sage wird ausschließlich mit **2-ply** angeboten (Spiel 2 Threads, Analyse 4 Threads). 1-ply ignoriert in diesem Release das angeforderte Threadbudget; 3/4-ply bleiben ungeprüft deaktiviert. Der gehashte ARM64-Build und die Prüfgrenzen stehen in [docs/open-sage-local.md](../../open-sage-local.md). Der folgende ursprüngliche Plan bleibt als Abnahme- und Architekturkontext erhalten; unten dokumentierte Implementierungsentscheidungen gehen den ursprünglichen Profilannahmen vor.

**Ziel:** GNU Backgammon und Open Sage getrennt für Spiel, Analyse und Benchmark auswählen und vollständig lokal betreiben.

**Architektur:** Ein gemeinsamer Engine-Vertrag verbindet die vorhandene GNUbg-Anbindung und einen vom Backend verwalteten Open-Sage-Prozess. GNUbg bleibt zunächst Regel-, Würfel- und SGF-Modul beim Spielen; die gewählte Spiel-Engine trifft die Bot-Entscheidungen. PostgreSQL, persistierte Aufträge, REST und SSE bleiben die bestehenden Anwendungsgrenzen.

**Technik:** Rust/Tokio/Axum/SQLx, PostgreSQL, React/TypeScript; für Open Sage ein natives ARM64-Python mit dem Python/C++-Paket `bgsage` in einer eigenen virtuellen Umgebung.

Dieser Plan beschreibt Architektur, Arbeitspakete und Abnahme. Konkrete Adapter-Signaturen werden nach dem lokalen Kompatibilitätsnachweis gegen das festgelegte Paket ausgearbeitet. Der Benutzer hat die Umsetzung nach dem lokalen Kompatibilitätstest beauftragt. Engineinstallation und Codeintegration sind erfolgt; Datenmigrationen wurden ausschließlich in isolierten Tests ausgeführt. Commit und Push sind nicht beauftragt.

## 1. Bestätigte Entscheidungen und Umfang

Vom Benutzer bestätigt:

- Getrennte Engine-Vorgaben für Spiel, Analyse und Benchmark.
- GNUbg darf beim Spielen mit Open Sage weiterhin Regeln, Würfel und Spielstände verwalten.

Daraus folgt dieses Bedienmodell:

| Bereich | Auswahl und Wirkung |
| --- | --- |
| Spielen | Bot-Checkerzüge, Double/Take/Pass und Annahme einer Aufgabe verwenden die gewählte Spiel-Engine. |
| Analyse | Position, Würfelverteilung, Spiel und Match sowie Tutor/Coach, Hinweise und Lernberichte verwenden die gewählte Analyse-Engine. |
| Benchmark | Eigene Engine- und Profilwahl für Analysezeit und Spielzuglatenz; verändert weder Spiel- noch Analysevorgaben. |

Beispiel: Gegen Open Sage spielen, Hinweise mit GNUbg berechnen und anschließend Open Sage messen. Vor dem Start sind Spiel-Engine und Analyse-Engine sichtbar. Eine neue Spielsitzung speichert beide Konfigurationen; spätere Einstellungsänderungen beeinflussen sie nicht. Ein unabhängig gestarteter Analyseauftrag übernimmt die dann gewählte Analysekonfiguration.

Bestehende Installationen behalten GNUbg und ihre Tiefen. Open Sage wird nach erfolgreicher Installation auswählbar. Bei Ausfall kein stiller Wechsel zur anderen Engine. Laufende Partien und Aufträge behalten ihre Engine; historische Ergebnisse bleiben zugänglich.

Erste Freigabe: N-ply-Analyse, die bestehenden Spielmodi, Money Games und Matches, beide Benchmarkarten. Rollout-Bedienung, Training eigener Netze, Turniere zwischen Bots und ein Betrieb ganz ohne GNUbg sind eigene spätere Erweiterungen.

## 2. Befund am aktuellen Repository

Untersucht wurde `/Users/hendrik/Developer/bgtutor`, Ausgangs-HEAD `4e24d15`, einschließlich sichtbarer lokaler Arbeit. Das Arbeitsverzeichnis unter `Documents/ChatGPT/Backgammon` ist kein Git-Repository.

- `backend/src/engine.rs` enthält einen konkreten GNUbg-Prozessadapter und die globale Koordination zwischen normaler Engine-Arbeit und exklusiven Benchmarks.
- `backend/src/settings.rs` besitzt bereits Engine-Katalog und `EngineSnapshot`; dessen Pfad und Konstruktion sind GNUbg-spezifisch. `configuration.rs` erlaubt bisher nur GNUbg.
- `backend/src/store.rs` dedupliziert aktive Jobs mit einem Konfigurationsfingerabdruck. Mehrere Ergebnisabfragen wählen aber den letzten Auftrag ohne Engine-Filter.
- `backend/src/match_analysis.rs` analysiert über GNUbg-SGF und liefert native GNUbg-Statistik. `gnubg_emg_500_v1` ist ausdrücklich keine allgemeine Engine-PR.
- `backend/src/play.rs` und `play/native.py` verwenden GNUbg nicht nur zur Bewertung, sondern auch für Zustandsübergänge, Bot-Automatik, legale Züge und Aufgaben.
- `play/reports.rs` bindet Berichte derzeit an den GNUbg-Pfad und die Version der Spiel-Engine. Eine getrennte Analyse-Engine erfordert hier einen eigenen Snapshot.
- `web/src/SettingsDialog.tsx` zeigt `bgsage` bereits als deaktivierte Option. Der Benchmark hat eine eigene Engine-Auswahl, seine Spielzugmessung ist jedoch noch GNUbg-spezifisch.
- MAT-Konvertierung benötigt weiterhin GNUbg. Die GNUbg-Positions- und Match-IDs sind Datenformate und bleiben auch für Open-Sage-Ergebnisse bestehen.

Während der Untersuchung wurden die lokalen Änderungen an Spielzug-Benchmarks weiterbearbeitet. Betroffen sind unter anderem `benchmarks.rs`, `benchmarks/runner.rs`, `benchmarks/estimate.rs`, `play.rs`, Frontend und OpenAPI sowie die neue Migration `0039_play_latency_benchmark.sql`. Diese Arbeit nicht überschreiben. Vor Umsetzung erneut Status, Schnittstellen und aktuelle Migrationsnummer prüfen.

## 3. Open Sage: gesicherte Basis und Nachweisgrenzen

Gemeint ist [markbgsage/bgsage](https://github.com/markbgsage/bgsage), die Backgammon-Bibliothek. Sie bietet lokale Python/C++-Bewertungen, Mehrzug-Suche und Rollouts. Ihre Basisbewertung heißt 1-ply; GNUbg bezeichnet seine Basisbewertung als 0-ply. Die Nummern sind keine Aussage gleicher Spielstärke. [Projektbeschreibung](https://github.com/markbgsage/bgsage/blob/main/README.md)

Die geprüfte PyPI-Seite führt `2.0.20260907` mit macOS-ARM64-Wheels für CPython 3.10–3.14. Das Projekt nennt Python ab 3.10 und NumPy als Laufzeitabhängigkeit; die macOS-Buildkonfiguration berücksichtigt ARM64/NEON. Planungskandidat ist eine separate ARM64-Umgebung mit Python 3.12 und genau dieser Paketversion. Auf dem untersuchten Rechner melden die vorhandenen Werkzeuge ARM64 und Python 3.14.4; zusätzliche Interpreter wurden nicht eingerichtet. [Release-Dateien](https://pypi.org/project/bgsage/2.0.20260907/#files), [Paketkonfiguration](https://github.com/markbgsage/bgsage/blob/main/pyproject.toml)

Für Bewertung ist CPU-Betrieb vorgesehen; CUDA-Training ist kein Bestandteil der Integration. Die Erstinstallation benötigt heruntergeladene Pakete. Danach muss die Anwendung einschließlich Modell- und Bearoff-Dateien offline arbeiten können. Das Projekt bezeichnet Code, mitgelieferte Gewichte und Bearoff-Daten als MPL-2.0; Hinweise bei einer späteren Bündelung mitführen. [Projektbeschreibung und Lizenzangaben](https://github.com/markbgsage/bgsage/blob/main/README.md)

Die untersuchten API-Quellen stammen aus `main`, nicht aus einem lokal getesteten Wheel. `BgBotAnalyzer` bietet dort `checker_play`, `cube_action` und `post_move_analytics`; Kontextparameter umfassen Cube, Away-Scores, Crawford und Money-Regeln. `force_boards` kann den tatsächlich gespielten Zug trotz Suchfilter zur Bewertung aufnehmen. **Jede benötigte Fähigkeit muss gegen das ausgewählte Release geprüft werden.** Falls eine notwendige Funktion fehlt, einen geprüften unveränderlichen Upstream-Commit samt Build-Hash festlegen, nicht automatisch `main` installieren. [Analyzer](https://github.com/markbgsage/bgsage/blob/main/python/bgsage/analyzer.py)

Die aktuelle Simulationsschnittstelle ist kein Ersatz für die interaktive Spielsteuerung: Sie dokumentiert unter anderem keine Aufgaben und Einschränkungen beim Start vollständiger Matches. Deshalb baut dieser Plan nicht auf `simulate_match` als Spielsitzung auf. [Simulation](https://github.com/markbgsage/bgsage/blob/main/SIMULATION.md)

## 4. Architekturentscheidung

| Ansatz | Bewertung |
| --- | --- |
| Verwalteter Python-Prozess mit strukturiertem Protokoll | Empfohlen: öffentliche Python-API, getrennte Fehlerdomäne, kontrollierbarer Abbruch, vorab geladene Modelle. |
| Direkte C++-Einbindung in Rust | Zusätzlicher Build-/ABI-Aufwand; native Fehler betreffen den Backend-Prozess. Erst bei nachgewiesenem Transportengpass erwägen. |
| Eigenständiger lokaler HTTP-Dienst | Zusätzlicher Port, Dienststart und Versionsverwaltung; für diese lokale Anwendung unnötig. |

Der Browser spricht ausschließlich mit dem vorhandenen Rust-Backend. Dieses startet den Sidecar mit festem Interpreter und festem Skriptpfad, ohne Shell-Interpolation. Requests und Responses verwenden versionierte JSON-Nachrichten über stdin/stdout; stdout bleibt dem Protokoll vorbehalten, stderr liefert begrenzte Diagnoseausgaben.

Ein Worker hält genau einen aktiven Request. Er lädt das Modell einmal und wird für denselben Installations-/Profilfingerabdruck wiederverwendet. Anfangs nur ein rechnender Open-Sage-Worker, keine unbeschränkte Prozesssammlung. Die tatsächliche Threadzahl wird explizit begrenzt, geprüft und gespeichert. Ausgangspunkt für lokale Messungen: 2 Threads interaktiv und 4 für Hintergrundanalyse; erst nach Messung anpassen, nicht alle logischen Kerne ungeprüft belegen.

Requests enthalten ID, Operation, vollständigen Kontext und Profilfingerabdruck. Größenlimit, Startfrist, Berechnungsfrist und Fehlercodes sind Teil des Protokolls. Bei Timeout, Abbruch oder Lease-Verlust wird der aktive Prozess beendet und eingesammelt; der nächste Auftrag erhält einen neuen Worker. Keine Wiederverwendung einer möglicherweise versetzten Antwort. Berechnungen blockieren keine Tokio-Worker.

Die vorhandene gemeinsame Engine-Sperre umfasst künftig GNUbg **und** Open Sage, einschließlich Installationstests und kombinierter Spielzüge. Ein Benchmark wartet auf laufende Arbeit und misst erst nach exklusiver Übernahme. Verschachtelte Adapteraufrufe dürfen die Sperre nicht erneut blockierend erwerben.

## 5. Verträge, Profile und Persistenz

Neue klar abgegrenzte Module unter `backend/src/engines/`:

- `types.rs`: `EngineId` (`gnubg`, `bgsage`), Profile, Fähigkeiten, Kontext, Ergebnisherkunft und strukturierte Fehler.
- `registry.rs`: geprüfte Installationen und Adapterauswahl.
- `gnubg.rs`: Anbindung an die bestehende Implementierung ohne fachliche Neuimplementierung.
- `open_sage.rs` und `open_sage_worker.py`: Prozessverwaltung und schmale Bibliotheksanbindung.
- `context.rs`: einzige maßgebliche Konvertierung zwischen absolutem NeonDojo-Brett und Engine-Perspektive.

`engine.rs` kann zunächst als Kompatibilitätsfassade bestehen bleiben. Nur tatsächlich gemeinsame Prozess- und Koordinationslogik auslagern; GNUbg-SGF-Parsing bleibt GNUbg-spezifisch.

Der Katalog meldet Fähigkeiten getrennt: Checker, Cube, Position, Matchanalyse, Spielentscheidung und jeweilige Benchmarkart. Open-Sage-Spielverfügbarkeit berücksichtigt auch die weiterhin benötigte GNUbg-Regelinstallation. Versionsausgabe allein reicht nicht: Installationstest führt echte Checker-, Cube- und Bearoff-Bewertungen aus und prüft Modelle sowie Python/C++-Import.

Ein unveränderlicher Snapshot speichert Engine-ID, Release/Build, Interpreter-/Programmpfad, Modell- und Bearoff-Hashes, Adapter-/Protokollversion und vollständiges effektives Bewertungsprofil einschließlich Threadzahl und Suchfiltern. Bei GNUbg werden vorhandene Snapshots kompatibel gelesen; unbekannte historische Modellhashes bleiben unbekannt. Neue Sitzungen speichern zusätzlich Regelmodul- und Analyse-Snapshot. Updates installieren parallel versionierte Umgebungen, statt laufenden Sitzungen Dateien auszutauschen.

Die Profilauswahl verwendet engineeigene Kennungen, beispielsweise `gnubg:2ply` und `bgsage:3ply`. Kein pauschales `plies + 1` und keine automatische Gleichsetzung. Für Open Sage zunächst die nachgewiesenen N-ply-Stufen anbieten, Planungskandidaten 1–4 ply für Analyse und Benchmark sowie 1–2 ply für interaktives Spiel. Startvorgabe Open Sage: 1-ply beim Spielen, 2-ply bei Analyse. GNUbg behält 0–4 beziehungsweise 0–2 ply. Einstellungen pro Engine merken, damit ein Wechsel keine Tiefe zerstört.

Neue Migrationen erweitern Installation, Profile, Sitzungen, Job- und Benchmark-Snapshots. Keine vorhandene Migration ändern. Vorhandene `plies`-Felder für alte GNUbg-Daten erhalten; neue Profile explizit versionieren. Deduplizierung umfasst Ziel/Revision, vollständigen Entscheidungskontext, Umfang und effektives Profil. Leseabfragen und Cursor erhalten die ausgewählte Ergebnisquelle beziehungsweise Job-ID; „letzter Auftrag“ darf die Engine-Grenze nicht überschreiten.

Rechnerabhängige Installationspfade bleiben außerhalb portabler Benutzerprofile. `shared/configuration-options.json`, die vorhandenen Konfigurationsmodule und gemeinsame Normalisierungs-Fixtures verwenden. Den getrennten Benchmark-Standard mit Konfigurationsversion speichern; ein einmaliger Lauf überschreibt keine anderen Bereiche.

## 6. Fachliche Konvertierung und Analyse

NeonDojo speichert absolute Brettkoordinaten. Open Sage erwartet eine Spielerperspektive; auch die Bar-Darstellung muss explizit konvertiert werden. Ein unverändertes Durchreichen von 26 Zahlen ist nicht zulässig. Umkehrung, Spielerfarbe, Cube-Besitz und Away-Scores gemeinsam umrechnen. Quellen: [BG-Sage-Brettfunktionen](https://github.com/markbgsage/bgsage/blob/main/python/bgsage/board.py), [Simulationskontext](https://github.com/markbgsage/bgsage/blob/main/SIMULATION.md).

Referenztests decken beide Farben, beide Bars, Schlagen, mehrfach bewegte Steine, Pasch, höheren Einzelwürfel, Ausspielen und blockierte Würfe ab. Open Sage liefert für einen komplett blockierten Wurf laut Brett-API die unveränderte Stellung; diese muss als Pass behandelt werden, nicht als fehlendes Ergebnis. Kandidaten werden über resultierende Bretter mit legalen Zugfolgen verknüpft; vorhandene SGF-Regelprüfung und Notationswerkzeuge wiederverwenden.

Ergebnisse führen Perspektive und Einheit ausdrücklich mit. Die fünf Sage-Wahrscheinlichkeiten müssen in die vorhandene Darstellung mit zusätzlichem Verlustwert überführt werden; Gammonwerte enthalten Backgammons. Pre-roll und Post-move nicht vertauschen. [Ergebnistypen](https://github.com/markbgsage/bgsage/blob/main/python/bgsage/types.py)

Money-Equity, Match Winning Chance und normalisierte Match-Equity getrennt behandeln. Die dokumentierte Matchsuche rechnet intern mit MWC und konvertiert Ausgabewerte; tatsächliche Rückgabeeinheiten des festgelegten Builds mit Referenzfällen prüfen. Double/Take/Pass müssen aus derselben Perspektive und auf derselben Cube-Basis verglichen werden. Jacoby entsprechend dem Kontext setzen, im vorhandenen Spielmodus Beavers ausdrücklich ausschalten. [Bewertungsdefinitionen](https://github.com/markbgsage/bgsage/blob/main/MULTI-PLY.md)

Positions- und Würfeljobs verwenden den gemeinsamen Adapter. Würfelverteilung weiterhin 21 Paare, Gewichte 1/2, Summe 36; Pass-Stellungen gehören dazu. Ein nicht bewertbarer Wurf macht die Verteilung unvollständig, niemals scheinbar vollständig mit Nullwerten.

Für Spiel-/Matchanalyse liest ein neuer Open-Sage-Analysator die vorhandene normalisierte Partie und deren Cube-Ereignisse. Er bewertet beide Spieler, jeden gespielten Checkerzug auch außerhalb der besten Kandidaten und die relevanten Pre-roll-/Double-/Take-/Pass-Entscheidungen. Pro Spiel atomarer Checkpoint über `match_jobs.rs`; bestehende Lease-Prüfungen und Wiederaufnahme erhalten. Keine zweite Implementierung von Import, SGF-Replay oder Persistenz.

Open-Sage-Ergebnisse und GNUbg-Ergebnisse bleiben unabhängig auswählbar. Betroffen sind Analyse, Bibliotheksanzeige, gespeicherte Positionen, Berichte, wiederkehrende Fehler und Lernkarten. Bereits gespeicherte Lernantworten und Review-Historie werden durch einen Engine-Wechsel nicht ersetzt. Unvollständige Analyse bleibt sichtbar unvollständig.

**PR-Regel:** `gnubg_emg_500_v1` bleibt ausschließlich für vollständige GNUbg-Statistik gültig. Open Sage liefert zunächst Einzelbewertungen, Fehler und Abdeckung; eine nicht belegte PR bleibt „nicht verfügbar“. Eine eigene Sage-PR mit Zählregeln und unabhängig geprüfter Umrechnung wäre ein separater fachlicher Auftrag. Schwellwerte für Tutor und Fehlerklassen dürfen nur auf nachgewiesen kompatible normalisierte Verluste angewendet werden; dieser Nachweis ist Freigabebedingung für diese Modi.

## 7. Spielen mit GNUbg als Regelmodul

Die bestehende native Automatik muss gezielt aufgeteilt werden:

1. GNUbg erzeugt oder lädt den unveränderten Spielstand, würfelt und bestimmt zulässige Aktionen.
2. Für Open-Sage-Partien bleiben beide GNUbg-Spieler manuell gesteuert. Kein automatischer GNUbg-Botzug, keine automatische Take-/Pass-/Aufgabeentscheidung.
3. NeonDojo übergibt den Entscheidungskontext an die Spiel-Engine. Deren Antwort wird gegen den aktuellen Zustand und legale Zugfolgen geprüft.
4. GNUbg wendet ausschließlich die gewählte Aktion an und erzeugt SGF, Historie und Folgezustand. Die nächste Bot-Entscheidung läuft erneut durch den gewählten Adapter.
5. Sitzung, Archiv und relevante Berichte werden mit bestehender Versionsprüfung atomar gespeichert. Ein Fehler oder eine verspätete Antwort lässt den bestätigten Spielstand unangetastet.

Die bisher über `gnubg.hint(10000)` gewonnene Zugliste darf im Sage-Modus nicht unbemerkt zur Bot-Auswahl führen. Regelgenerierung und Bewertung trennen; wo die GNUbg-API zur Aufzählung Bewertungen mitliefert, diese nicht als Sage-Ergebnisse übernehmen und den Aufwand im Spielbenchmark mitzählen.

Bei einer menschlichen Aufgabe entscheidet die Spiel-Engine über Annahme. Dafür einmalige, getestete Fachregel: garantierten Ertrag der Aufgabe und Fortsetzung aus Bot-Perspektive in derselben Einheit vergleichen; Matchscore, Jacoby, Cube sowie einfache/Gammon/Backgammon-Aufgabe berücksichtigen. Keine GNUbg-Bewertung als Ersatz für Sage. Bot-Aufgabeangebote bleiben entsprechend bestehendem Produktverhalten ausgeschaltet. Prüfen, ob der gewählte GNUbg-Build explizite Annahme/Ablehnung im manuellen Modus unterstützt.

Hinweise, Tutor-Warnungen und Lernberichte verwenden den beim Sitzungsstart gespeicherten **Analyse**-Snapshot. Ihre sichtbare Quellenangabe kann daher von der Bot-Engine abweichen. Bestehende GNUbg-Sitzungen behalten ihre bisherige Auswertungssemantik.

## 8. Reproduzierbare Benchmarks

Analysebenchmark und Spielzuglatenz bleiben getrennt. Die bestehenden WC2014-Referenzen weiterverwenden: 10 Spiele/460 Checkerzüge für Analyse und die aktuell ergänzten 11 festen Checkerstellungen für Spielzuglatenz. Keine Benutzerpartien erzeugen oder verändern.

Neue Messprotokolle speichern Engine-/Regel-/Modellfingerabdrücke, effektives Profil, Threadzahl, Referenzhash, Cachezustand und Prozesslebenszyklus. Historische GNUbg-Protokolle bleiben unverändert. Ein neues Engine-Ergebnis darf nicht mit einem umbenannten alten GNUbg-Protokoll erscheinen.

Für die erste Sage-Messung beginnt jeder Lauf mit einem frischen Worker. Modellstart und Laden zählen einmal zur vollständigen Laufzeit; der Worker bleibt innerhalb des Laufs bestehen. Für Spielzüge zusätzlich einzelne warme Zugzeiten und Startkosten getrennt berichten. Bei GNUbg dessen tatsächliche Prozessstarts mitzählen. Die Vergleichsansicht beschreibt damit reale Anwendungspipelines; sie behauptet keinen isolierten Vergleich allein der Suchalgorithmen.

Queue-Wartezeit, Installationsprüfung, Datenbankpersistenz, Netzwerk und Animation bleiben außerhalb des Timers. Persistierte Analyseergebnisse nicht wiederverwenden; engineinterne Suchcaches und deren Reset-/Wiederverwendungsstrategie im Protokoll dokumentieren. Alle Referenzentscheidungen müssen vollständig bewertet sein, bevor eine Gesamtzeit als gültig gilt.

Für reproduzierbare Vergleichsmessungen drei getrennte Läufe je Kombination, Median und Streuung ausweisen; keine Teilzeiten abgebrochener Versuche kombinieren. Im Benchmark gilt eine explizite, für beide Engines dokumentierte Ressourcenbegrenzung. Geschwindigkeit misst keine Spielstärke.

Zeitprognosen in `benchmarks/estimate.rs` und `analysis_timing.rs` dürfen nur zur gleichen Maschine, Engine, Version, Modell, Profil und kompatiblen Pipeline passen. GNUbg-Messwerte niemals für Sage schätzen; ohne passende Messung „noch keine Schätzung“. Messungen der Spielzuglatenz bleiben von Matchanalyse-Prognosen ausgeschlossen.

## 9. Umsetzung in prüfbaren Arbeitspaketen

Reihenfolge: A → B → C → D → E → F → G → H. Vor jedem Eingriff aktuelle lokale Änderungen erneut lesen. Für fachliche Änderungen zuerst die unten genannten Verhaltensfälle als fehlende Tests festhalten, dann implementieren und gezielt prüfen. Keine ungefragten Commits.

### A — Lokalen Kompatibilitätsnachweis liefern

**Neue Dateien:** `tools/probe_open_sage.py`, `tools/test_probe_open_sage.py`, `docs/open-sage-local.md`; anschließend eine gehashte Paket-Lockdatei unter `tools/open-sage/`.

- Gewähltes Release in einer separaten ARM64-Umgebung prüfen; Interpreter, Wheel, native Erweiterung, Gewichte und Bearoff-Dateien identifizieren und hashen.
- Checker/Cube/Money/Match/Crawford/Pass sowie erzwungene Bewertung eines gespielten Kandidaten testen. Verfügbarkeit öffentlicher APIs gegen das Wheel dokumentieren.
- Offline-Probe nach einmaliger Bereitstellung durchführen; fehlende Dateien, falsche Architektur und ungültige Modelle müssen verständliche Fehler erzeugen.
- Warm-/Kaltstart, Speicherbedarf und Threadwirkung messen. Aus diesen Daten erlaubte interaktive Profile und Fristen festlegen.

**Abnahme:** Eine festgelegte Version kann lokal alle benötigten Operationen ausführen. Fehlt eine Pflichtfähigkeit, kein UI-Freischalten; einen fest gepinnten kompatiblen Build prüfen. Keine Aussage zur macOS-Lauffähigkeit allein aufgrund einer Versionsnummer.

### B — Engine-Vertrag und Konfiguration erweitern

**Dateien:** neue `backend/src/engines/`-Module; `backend/src/lib.rs`, `engine.rs`, `settings.rs`, `configuration.rs`, `configuration/validation.rs`, `configuration/store.rs`, `store.rs`, `model.rs`; neue Migration; `shared/configuration-options.json`, `tests/fixtures/configuration-normalization.json`.

- Engine-ID, Fähigkeiten, Bewertungsprofil und versionierte Snapshots einführen; alte GNUbg-JSON-Daten weiterhin lesen.
- Getrennte Bereichsvorgaben und Installationsrevisionen speichern; laufende Jobs/Sitzungen unveränderlich halten.
- Fingerabdrücke, Ergebnisquellen und Deduplizierungsindizes erweitern; Legacy-Fälle gesondert testen.

**Tests:** `backend/tests/configuration.rs`, neue `backend/tests/engine_selection.rs`, vorhandene Konfigurations-Tests im Frontend. Fälle: alte Profile, fehlende Installation, Profilwechsel, Revision 409, zwei Engines am gleichen Ziel und kein Wechsel laufender Arbeit.

### C — Sidecar und Adapter fachlich absichern

**Dateien:** `engines/open_sage.rs`, `engines/open_sage_worker.py`, `engines/context.rs`; neue `backend/tests/open_sage.rs`, `tests/fixtures/open-sage-context.json` und Protokolltests.

- JSON-Protokoll, Prozessgrenzen, Identitätsprüfung und Ressourcenbegrenzung implementieren.
- Unabhängige Referenzstellungen für Perspektive, Regeln, Wahrscheinlichkeiten und Equity sichern; Roundtrip-Tests allein reichen nicht.
- Crash, kaputtes JSON, Übergröße, Timeout, Abbruch und Versions-/Modellwechsel testen.

**Abnahme:** Keine ungültige Antwort erreicht Persistenz oder Spielzustand; GNUbg-Regressionsfälle bleiben unverändert. Kein exaktes numerisches Gleichsetzen beider Engines, sondern regelbezogene Invarianten und überprüfte Quellwerte.

### D — Positionen und vollständige Partien analysieren

**Dateien:** `jobs.rs`, `match_jobs.rs`, `match_analysis.rs`, neuer `engines/open_sage_match.rs`, `analysis_statistics.rs`, `store.rs`, `api.rs`; Ergebnisleser in `position_browser.rs`, `study.rs` und zugehörigen Quellenmodulen.

- Position/Würfelverteilung anbinden, danach Spiel/Match mit allen Checker- und Cube-Entscheidungen.
- Gespielten Zug vollständig bewerten und Quellenbindung bis zu Lernberichten erhalten.
- Bestehende Checkpoints, Deduplizierung, Lease-Recovery und Schutz vor alten Workern für beide Engines prüfen.
- PR-Verfügbarkeit und normalisierte Fehler explizit behandeln; keine gemischten Statistiken.

**Tests:** neue Open-Sage-Referenzfälle sowie `backend/tests/match_analysis.rs`, `analysis_statistics.rs`, `imported_analysis.rs`; Cube-only-Spiel, Endstellung, Pass, Wiederaufnahme nach Spielcheckpoint und identisches Brett mit anderem Matchkontext.

### E — Spielentscheidungen von Regelsteuerung trennen

**Dateien:** `play.rs`, `play/native.py`, neues `play/decisions.rs`, `play/feedback.rs`, `play/reports.rs`, `play/library.rs`.

- Manuellen Regelmodus für extern gelieferte Entscheidungen schaffen; bisherige GNUbg-Partien kompatibel halten.
- Open-Sage-Bot, Cube und Aufgabeentscheidung integrieren; unabhängigen Analyse-Snapshot für Hinweise und Berichte verwenden.
- Archiv, Fortsetzung, Undo-Entwürfe, Eröffnungswurf und Ergebnisdialoge unverändert bedienen.

**Tests:** `backend/tests/play.rs`, `play_history.rs`; neue E2E-Fälle für Sage-Spiel mit GNUbg-Hinweisen und die Gegenrichtung. Beide Farben, Match/Money, Crawford, Take/Pass, Aufgabe, Reload, Prozessfehler und konkurrierende Aktionen prüfen. Direkter Nachweis: Der gewählte Sage-Zug wird ausgeführt und kein GNUbg-Automatikzug vorgeschaltet.

### F — Benchmarks und Prognosen erweitern

**Dateien:** `benchmarks.rs`, `benchmarks/runner.rs`, `benchmarks/play_latency.rs`, `benchmarks/estimate.rs`, `analysis_timing.rs`, `backend/tests/benchmarks.rs`, `analysis_time_estimate.rs`, `analysis_remaining_time.rs`.

- Beide Benchmarkarten erhalten Engine und Profil aus ihrer eigenen Auswahl.
- Neue Protokolle, Workerlebenszyklus und Zeitgrenzen implementieren; exklusive Koordination über beide Engines prüfen.
- Vollständigkeit, Abbruch und Fingerabdrücke in Historie und Prognosen prüfen.

**Abnahme:** Referenzen unverändert, keine Bibliothekseinträge, keine konkurrierende Engine-Messung, keine Sage-Schätzung aus GNUbg-Zeiten. Historische Benchmarks bleiben mit ihrer ursprünglichen Bedeutung lesbar.

### G — Oberfläche und API vollständig umstellen

**Dateien:** `SettingsDialog.tsx`, `PlayWorkspace.tsx`, `BenchmarkWorkspace.tsx`, `AnalysisPanel.tsx`, `MatchAnalysisPanel.tsx`, `SourceAnalysisView.tsx`, `useAnalysisSources.ts`, Konfigurationsmodule und betroffene Message-Dateien.

- Pro Bereich Auswahl und passende Profile aus dem Katalog anzeigen; Installation, Nichtverfügbarkeit und nicht unterstützte Fähigkeiten verständlich darstellen.
- Quellen, Version und Profil an Ergebnissen sichtbar machen. Spielstart zeigt Bot und Analyse; Installation erklärt die GNUbg-Regelabhängigkeit.
- Lade-/Fehler-/Konfliktzustände, Entwürfe, Fokus und Schutz vor veralteten Antworten erhalten.
- Alle sechs Sprachen und vorhandene Typografie/Navigation verwenden; API-Typen aus Rust regenerieren.

**Tests:** Settings-/Konfigurations-Tests, `web/e2e/benchmark.spec.ts`, `analysis-sources.spec.ts` und neue `engine-selection.spec.ts`. Desktop-Sichten bei 1800×1169, 2560×1440 und 3840×2160 prüfen.

### H — Installation, Migration und Gesamtfreigabe

**Dateien:** `README.md`, `.env.example`, `docs/open-sage-local.md`, neue Lockdatei und Migrationstests.

- Versionierte lokale Umgebung unter `~/Library/Application Support/NeonDojo/engines/open-sage/` dokumentieren; Modell-/Paketdateien nicht im Git-Repository ablegen.
- Reproduzierbares Setup aus gehashten Wheels beschreiben; keine Installation beim normalen App-Start und kein automatisches Upgrade laufender Installationen.
- Migration auf einer isolierten Kopie historischer Daten testen: GNUbg-Partien, Ergebnisse, Profile, Notizen und Lernfortschritt bleiben erhalten.
- Abnahmematrix für beide Engines ausführen und Messergebnisse mit Hardware, Datenumfang, Profil und Prozessstrategie dokumentieren.

## 10. Prüfkommandos und Freigabekriterien

Bei der späteren Umsetzung vom Repository-Root aus; zuerst fokussierte Tests, danach:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run api:types --prefix web
npm test --prefix web
npm run test:themes --prefix web
npm run build --prefix web
npm run test:e2e --prefix web
```

`cargo test --workspace` benötigt gemäß README einen geeigneten Testserver. Native Integrationstests nur nach Prüfung von Zielserver/Isolation, GNUbg-Gewichten und eingebettetem Python sowie dem Open-Sage-Probebericht starten. Bestehende gezielte Kommandos:

```sh
cargo test -p neondojo --test play -- --ignored --nocapture
cargo test -p neondojo --test match_analysis -- --ignored --nocapture
cargo test -p neondojo --test benchmarks -- --ignored --nocapture
```

Nach Anlage des neuen Integrationstest-Targets zusätzlich:

```sh
cargo test -p neondojo --test open_sage -- --ignored --nocapture
```

Die neuen Python-Protokolltests erhalten einen eigenen dokumentierten Aufruf mit dem gewählten Sidecar-Interpreter; `test:themes` allein deckt sie nicht ab. Datenbank-Suchänderungen zusätzlich anhand repräsentativer isolierter Daten mit `EXPLAIN (ANALYZE, BUFFERS)` prüfen.

Freigabe erst, wenn Engine-Auswahl in allen drei Bereichen wirkt, alle vorhandenen Spielaktionen unterstützt sind, vollständige Analyse fachlich korrekt ist, Daten unverändert erhalten bleiben und Benchmark/Prognose nachweislich die richtige Engine verwenden. Fehlende Open-Sage-PR ist eine explizit ausgewiesene Einschränkung, keine fehlgeschlagene Analyse.

## 11. Ergebnis dieser Planung

Repository, relevante lokale Änderungen, README, Architektur- und UI-Dokumentation sowie Open-Sage-Primärquellen wurden gelesen. Context7 war wegen ungültiger API-Zugangsdaten nicht nutzbar; die Bibliotheksrecherche erfolgte direkt in Projektquellen und PyPI.

Kein lokaler Open-Sage-Import, keine Installation, Engine-Funktionsprüfung, Laufzeitmessung oder Anwendungstests wurden ausgeführt. Der erste Umsetzungsschritt ist deshalb Paket A; die ARM64-Wheels belegen die Bereitstellung, noch keine erfolgreiche Nutzung auf diesem Rechner.

## Umsetzungsstand

- Lokaler Kompatibilitätstest mit gehashten ARM64-Wheels, Referenzfällen, Offline-Audit und dokumentierter 1-ply-Einschränkung.
- Dispatcher, persistenter begrenzter Sidecar, Paket-/Modellprüfung, unveränderliche Snapshots und revisionierte lokale Installation.
- Analyse-/Spielauswahl und unabhängige Benchmarkauswahl; vorhandene GNUbg-Defaults bleiben erhalten.
- Positions-, Würfel-, Spiel-/Matchanalyse, Checkpoints und Provider-getrennte Ergebnisleser. Bestehende Priorisierung höherer Tiefe bleibt **innerhalb desselben Providers** bestehen.
- Open-Sage-Spielentscheidungen mit GNUbg-Regeln; getrennte Bot-/Analyse-/Regelidentitäten, Cube/Pass/Aufgabe, Hinweise und Berichte ohne erfundene GNUbg-PR.
- Neue Benchmarkprotokolle mit einmal eingerechnetem Kaltstart, vollständiger Referenzabdeckung, Snapshot-verträglicher Prognose und Median/Spanne dreier gleicher Läufe.
- Rust/OpenAPI-generierte Typen, sechs UI-Sprachen sowie Desktop-Prüfung bei 1800×1169, 2560×1440 und 3840×2160.

Konkrete Module: `backend/src/engines/{mod,context,game,process}.rs`, `engines/worker.py`, `settings/open_sage.rs`, `play/open_sage.rs`; Migration `0041_open_sage.sql` (Nummer 0040 ist durch parallele Arbeit am Hauptworktree belegt). Die Installations-/Snapshot-Identität nutzt einen Gesamthash der geprüften Paket- und Modelldateien statt redundanter Dateilisten in jedem Auftrag. Ein Wechsel im Einstellungsdialog merkt die dort gewählten Tiefen je Engine; gespeicherte Profile enthalten weiterhin ihre ausdrücklich gewählte Engine und Tiefe.


## Übernahme in den Hauptworktree

Am 12. November 2026 mit den zwischenzeitlichen Pause-/Fortsetzungsänderungen auf `main` zusammengeführt und erneut geprüft. Nach vollständiger Datenbanksicherung wurde Migration 0041 angewendet und Open Sage 2-ply auf Wunsch als Vorgabe für neue Spiele und Analysen aktiviert. Bestehende Partien, Aufträge, Profile und Lernstände bleiben erhalten. Prüfungen, Sicherung und tatsächlicher Laufzeitstand stehen in [open-sage-local.md](../../open-sage-local.md#übernahme-und-lokale-aktivierung-am-12-november-2026).
