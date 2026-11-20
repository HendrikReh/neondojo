# Lokaler Open-Sage-Kompatibilitätsnachweis

Open Sage `bgsage==2.0.20260907` mit NumPy `2.5.4` wurde am 12. November 2026 unter nativem macOS ARM64 und CPython 3.12.13 geprüft. Dieser ursprüngliche Wheel-Nachweis gilt für **2-ply**. Der zusätzlich geprüfte lokale Fork ist unten für **1-/2-ply** dokumentiert. Dies ist ein lokaler Nachweis für diesen Build und diese Referenzstellungen, keine Aussage über Spielstärke oder allgemeine GNUbg-Gleichwertigkeit.

## Reproduzierbare Installation

Voraussetzungen: nativer ARM64-Interpreter Python 3.12, macOS mindestens 14 (NumPy-Wheel), genügend lokaler Speicher. Kein Rosetta-Interpreter und kein Source-Build. Der Lock enthält die SHA256-Hashes der beiden konkreten ARM64-Wheels; Modelle und Bearoff-Daten sind im Sage-Wheel enthalten.

Aus dem Repository-Root mit frei gewähltem externem Installationsverzeichnis:

```sh
python3.12 -m venv "$HOME/Library/Application Support/Neondojo/open-sage-venv"
"$HOME/Library/Application Support/Neondojo/open-sage-venv/bin/python" -m pip install \
  --only-binary=:all: --require-hashes \
  -r tools/open-sage/requirements-macos-arm64.lock
```

Für eine Installation ohne Netzwerk die bereits heruntergeladenen, unveränderten Wheels in ein lokales Verzeichnis legen und zusätzlich `--no-index --find-links /path/to/wheels` verwenden. Der Probeprozess installiert nichts und lädt nichts nach.

```sh
python3 -m unittest discover -s tools -p test_probe_open_sage.py
"$HOME/Library/Application Support/Neondojo/open-sage-venv/bin/python" \
  tools/probe_open_sage.py --levels 2ply --output /tmp/open-sage-2ply.json
```

Exitcode 0 bedeutet, dass **alle angeforderten** Prüfungen bestanden wurden. Fehlende Abhängigkeiten, falsche Architektur/Python-Version, fehlende/leere/veränderte Assets, native Abstürze, Zeitüberschreitung und fehlgeschlagene Referenzfälle ergeben einen Fehlerbericht und Exitcode 1. Standardzeitlimit: 300 Sekunden. `--timeout` kann es begrenzen. Der gestartete Worker wird bei Überschreitung beendet.

## Geprüfter Umfang und Quellenidentität

[2-ply-Bericht](../tools/open-sage/probe-macos-arm64-2ply.json) enthält Plattform, Paketversionen, Produktionsmodell `stage11`, Strategie `backgame_pair_phased`, 24 Modellslots sowie SHA256 jedes Python-Moduls, der nativen Bibliothek und aller Assets. Dateien werden gegen die SHA256-Einträge des installierten Wheel-RECORD geprüft. Das Installations-Lock schützt die ursprünglichen Wheels; ein gemeinsam manipuliertes RECORD ist durch diese lokale Integritätsprüfung allein nicht erkennbar.

Die Funktionsprüfung umfasst:

- Eröffnung 31: legale Kandidaten und bester Zug `8/5 6/5`.
- Beide Perspektiven, asymmetrische Bars, legaler Bareintritt mit Schlag und Notation.
- Money Game mit/ohne Jacoby, Cube-Besitz bei Spieler/Gegner/zentriert, Beavers explizit aus.
- Asymmetrischer Matchscore, Crawford aus beiden Perspektiven und Post-Crawford.
- Erzwungenes Aussetzen, Bearoff und Post-Move-Auswertung.
- Terminale einfache Siege/Niederlagen in Money, Match und Crawford liefern Wahrscheinlichkeit 1/0 und Equity +1/-1. Negative Match-Equity widerlegt eine unmittelbare Interpretation als MWC; weitere fachliche Umrechnungen benötigen eigene Referenzen.
- Sicher gewonnene Ein-Checker-Stellung: Money mit Cube 2 liefert No Double ≈1, Double/Take ≈2 und Double/Pass ≈1 in Einheiten des Würfels **vor** dem Angebot. Die Antwortperspektive negiert die ursprünglichen DT-/DP-Werte (Take −2, Pass −1); das Brett wird dafür nicht erneut umgedreht und ausgewertet. Bei 1-away/3-away saturieren die Matchwerte für Cube 1 und 2 jeweils bei ≈1. Die Probe prüft keine instabile Double-Empfehlung bei gleichen No-Double-/Pass-Werten.
- Ein zuvor im cubeless Filter nur 1-ply bewerteter gespielter Zug wird durch `force_boards` tatsächlich auf 2-ply gehoben. Im cubeful 2-ply-Pfad bewertet dieser Build ohnehin alle Kandidaten auf voller Tiefe.
- Endliche Equities und Wahrscheinlichkeiten sowie verschachtelte Gammon-/Backgammon-Grenzen bei allen geprüften Ergebnissen.

Die gewöhnlichen Tests benötigen weder NumPy noch Sage und prüfen insbesondere Fehlerbehandlung, Hashabweichungen, Prozess-Timeout, Netzwerksperre und übergangene Threadgrenzen.

## Einschränkung des ursprünglichen Wheels: 1-ply-Threadbudget

Der [vollständige 1-/2-ply-Bericht](../tools/open-sage/probe-macos-arm64.json) ist absichtlich **fehlgeschlagen**. Bei `BgBotAnalyzer(eval_level='1ply', parallel_threads=2)` beziehungsweise `4` erzeugt der cubeful Checkerpfad einen Pool mit Limit **10** auf dem Testrechner. Der öffentliche Parameter wird beim Erzeugen von `_OnePlyAnalyzer` nicht weitergereicht; `_CubefulAnalyzer.checker_play_analytics` fällt auf `max(2, os.cpu_count())` zurück. `BGBOT_MULTIPLY_THREADS` begrenzt diesen konkreten Pfad ebenfalls nicht. Eine alternative öffentliche 1-ply-Begrenzung wurde in diesem Build nicht gefunden.

Der Probe beobachtet dafür den echten `ThreadPoolExecutor`-Konstruktor und delegiert unverändert an ihn. Er verändert weder Poolgröße noch Berechnung. Dies misst das konfigurierte Kandidatenpool-Limit, nicht sämtliche gleichzeitig laufenden nativen Threads. Bei 2-ply wurden die angeforderten Limits 2 und 4 eingehalten. 1-ply bleibt bis zu einer nachgewiesenen upstream-Korrektur beziehungsweise einer ausdrücklich gewählten, reproduzierbaren Lösung deaktiviert; private Enginefelder werden nicht angepasst. 3-/4-ply und Rollouts sind nicht freigegeben oder vermessen.

```sh
# Reproduziert auch den bekannten Fehler; Exitcode 1 ist hier erwartet.
"$HOME/Library/Application Support/Neondojo/open-sage-venv/bin/python" \
  tools/probe_open_sage.py --levels 1ply 2ply --output /tmp/open-sage-all.json
```

## Board- und Pass-Vertrag

Sage verwendet 26 Einträge: Punkte 1–24 mit positiven eigenen und negativen gegnerischen Checkern; eigene Zugrichtung abwärts. Index 25 ist die eigene Bar, Index 0 die gegnerische Bar. **Beide Barwerte sind nichtnegativ.** `flip_board` tauscht die Bars ohne Vorzeichenwechsel und spiegelt/negiert die Punkte.

Entgegen dem installierten Python-Docstring liefert `possible_moves` beim geprüften blockierten Wurf eine Liste mit dem **unveränderten Brett**, nicht eine leere Liste. `checker_play` bewertet diesen Pass-Kandidaten. Die öffentliche Notationsfunktion liefert dafür `""`. Der Adapter muss diesen expliziten Pass erkennen und darf keinen fehlenden Analysewert als Nullverlust darstellen.

## Offline-Prüfung und Messgrenzen

Vor jedem Engineimport sperrt ein Python-Audit-Hook Socketverbindungen und DNS; ein separater Subprozess-Test bestätigt die Sperre. Alle realen Proben liefen in der gewöhnlichen Ausführungssandbox. Der zusätzliche Versuch, per macOS `sandbox-exec` systemweit `network*` zu verbieten, scheiterte bereits beim Anwenden des Sandboxprofils mit `Operation not permitted`. Deshalb wird **keine native OS-Netzwerksperre behauptet**. Der Python-Hook kann direkte native Netzwerk-Systemaufrufe nicht abfangen.

Die JSON-Berichte enthalten frischen Prozessstart einschließlich aller Prüfungen, Importzeit, erste Analyzerinitialisierung sowie jeweils drei warme Eröffnungs-31-Auswertungen nach einem Warmup mit angeforderten 2/4 Threads. Die OS-Dateicaches wurden nicht geleert. 1-ply-Messungen im vollständigen Bericht haben wegen des beschriebenen Fehlers tatsächlich Poollimit 10 und sind kein 2-/4-Thread-Vergleich. 2-ply-Zeiten betreffen nur diese einzelne Stellung mit Standard-Zugfilter; keine Partiedurchsatz- oder UI-Latenzaussage. RSS ist der High-Water-Wert des gesamten Workerprozesses über Importe, Hashprüfung und alle nacheinander erzeugten Analyzer, kein Speicherwert pro Anfrage.

Keine Anwendungstabellen, Benutzerpartien oder Enginequellen wurden durch diesen Probe verändert. Die tatsächliche Auswahl und Integration in Analyse, Spielen und Benchmark sind getrennte Anwendungsschritte.

## Anwendungsadapter und numerische Grenzen

Der Rust-Adapter bündelt einen lokalen JSON-Sidecar; je ein Worker für das Spielen und einer für alle übrigen Anfragen wird pro aktivem, unveränderlichem Profil wiederverwendet. Beide Slots haben eine eigene Sperre: Ein Bot-Zug wartet nicht auf einen laufenden Analyseblock, und während beide rechnen, teilen sich die Spiel- und Analyse-Threads die CPU. Analyseblöcke, sonstige Anfragen, Installationsprüfungen und Neustarts bleiben auf dem allgemeinen Slot seriell; Benchmarks schließen über den gemeinsamen Engine-Zugriffsschutz weiterhin jede Engine-Arbeit aus. Die Frist einer Einzelanfrage schließt das Warten auf ihren Worker ein. Vor und nach jeder Einzelanfrage werden Paket-/RECORD-/Modellidentität geprüft. Vollständige Spielanalysen verwenden stattdessen die unten beschriebenen atomaren Blöcke mit vollständiger Prüfung an beiden Grenzen. Profilwechsel und explizite Benchmark-Neustarts beenden und reapen den vorherigen Prozess. Timeout, Abbruch, fehlerhafte Protokollrahmen und ungültige typisierte Ergebnisse verwerfen den Worker. Der gemeinsame Engine-Zugriffsschutz umfasst auch den Sidecar.

Im vollständigen WC2014-Referenzspiel 5 lieferte die Float32-Berechnung einer Bearoff-Variante `1.0000000074505806` als Gewinnwahrscheinlichkeit und `-7.450580596923828e-09` als Verlust-Gammon/-Backgammon-Wahrscheinlichkeit. Der Adapter begrenzt ausschließlich solche endlichen Abweichungen von den Endpunkten 0/1 bis `1e-7` (kleiner als ein Float32-Epsilon) auf den jeweiligen Endpunkt. Größere Abweichungen, nichtendliche Werte und inkonsistente verschachtelte Wahrscheinlichkeiten bleiben Fehler. Equities werden dabei nicht verändert. Die unabhängige Diagnoseprobe behält standardmäßig ihre strikte Prüfung.

Beim Spielen entscheidet Sage anhand ausgewerteter Ergebnisbretter; die tatsächlich ausgeführte Notation stammt aus GNUbgs legalem Zugkatalog. GNUbg führt beide Spieler im externen Modus als Menschen und verwaltet Regeln, Würfel, Cube und Spielstände. Bot-, Analyse- und Regeln-Engine werden getrennt in der Sitzung festgehalten. Double/Take/Pass verwendet durchgehend den Kontext des ursprünglichen Doublers; Antworten negieren dessen Take-/Pass-Equities. Eine angebotene Aufgabe vergleicht Sage mit einer terminalen Stellung derselben Gewinnstufe im unveränderten Match-/Cube-Kontext; GNUbg führt anschließend die explizite Annahme/Ablehnung aus. Sage liefert keine GNUbg-PR-Statistik.

Zusätzliche Prüfungen (gewöhnliche Python-Tests benötigen keine Sage-Installation):

```sh
python3 -m unittest discover -s tools -p 'test_*open_sage*.py'
cargo test -p neondojo engines:: --lib
NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python cargo test -p neondojo --test open_sage -- --ignored
NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python cargo test -p neondojo sage_executes_reference_checker_turns_using_gnubg_rules --lib -- --ignored
# Nur gegen einen isolierten PostgreSQL-Testserver, siehe docs/development/testing.md:
OPEN_SAGE_TEST_PYTHON=/path/to/venv/bin/python cargo test -p neondojo --test open_sage_play -- --ignored
```

NeonDojo verwendet ausschließlich die Fork-Installation. `NEONDOJO_SAGE_PYTHON` und `OPEN_SAGE_TEST_PYTHON` erwarten deren Interpreter (lokal `local-data/open-sage-fork-2.0.20261119/venv/bin/python`). Mit dem ursprünglichen Wheel scheitert `native_checker_cube_and_match_analysis` am oben beschriebenen 1-ply-Threadbudget, und `open_sage_settings` erwartet die Tiefen 1 bis 4. Kein Test benötigt das ursprüngliche Wheel.

Die Testziele laufen einzeln und mit `--test-threads=1`. Ein gemeinsamer Aufruf von `cargo test --workspace -- --include-ignored` mit gesetzten Sage-Variablen lässt die Prozess-Tests in `engines::process` scheitern, weil sie sich den globalen Worker mit den echten Sage-Tests teilen.

## Verwendung in NeonDojo

1. Die Fork-Umgebung installieren. Für diese lokale Umsetzung liegt sie dauerhaft unter `local-data/open-sage-fork-2.0.20261119/venv` im Hauptrepository; `local-data/` ist von Git ausgeschlossen. Es wurden keine Pakete oder Gewichte in den Quellcode kopiert.
2. Backend und Frontend aus dem Hauptrepository bauen und gemäß [README](../README.md#restart-neondojo-locally-on-port-8787) starten. Vor der ersten Anwendung von Migration `0041_open_sage.sql` die Benutzerdatenbank sichern. Die Migration ergänzt lokale Installationsmetadaten und Provider-Indizes; vorhandene Jobs bleiben GNUbg-kompatibel. Datenbank- und Browsertests verwenden weiterhin eigene Testdatenbanken.
3. Unter **Einstellungen → Analyse → Engines auf diesem Rechner → Lokale Open-Sage-Installation** den absoluten Pfad zum venv-Interpreter eintragen und **Prüfen und speichern** wählen. Danach Analyse-Engine und Spiel-Engine getrennt wählen und den Entwurf anwenden.
4. Unter **Einstellungen → Analyse → Engines auf diesem Rechner** die Rechenlimits **Open-Sage-Threads für Analysen** und **Open-Sage-Threads beim Spielen** jeweils von 2 bis zur ermittelten Zahl verfügbarer logischer CPUs einstellen. Bei **Standard** verwendet Open Sage 4 Threads für Analysen und Duelle und 2 beim Spielen. Neue Sitzungen, Aufträge und Duellserien speichern den jeweils gültigen Wert. Die Engine prüft das konfigurierte Limit mit einer echten Stellungsbewertung. Der geprüfte serielle 1-ply-Checkerpfad bleibt seriell.
5. Unter **Extras → Benchmark** Engine und Tiefe unabhängig einstellen. Das Referenzprotokoll verwendet weiterhin 4 Threads für Analyse und 2 für Spielentscheidungen.

Der Anwendungsprozess bleibt vollständig lokal. Der Sidecar verwendet keinen Downloadpfad; Installation und Updates sind ausdrücklich getrennte Schritte. Unbekannte oder geänderte Modelle, fehlende Python-Umgebung und Engineabstürze liefern Fehler, keine Ersatzbewertungen.

Benchmark-Protokoll Version 2 startet den Sage-Worker explizit neu, misst dessen Initialisierung separat und rechnet sie einmal in die Gesamtzeit ein. Danach wird der Worker innerhalb dieses Laufs wiederverwendet. Analysezeit umfasst alle zehn Referenzspiele und 460 Checkerzüge mit ihren Cube-Entscheidungen; Spiellatenz umfasst elf feste Checkerzüge mit GNUbg-Regelprozessen. Referenzen wurden nicht geändert. Drei vollständig kompatible Läufe ergeben Median und Spanne; eine einzelne Messung ist kein allgemeiner Geschwindigkeitsnachweis.

```sh
# SQLx erzeugt isolierte Datenbanken; DATABASE_URL auf den dedizierten Testserver setzen.
OPEN_SAGE_TEST_PYTHON=/path/to/venv/bin/python cargo test -p neondojo --test open_sage_settings -- --ignored --test-threads=1
NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python npm run test:e2e --prefix web -- e2e/open-sage-native.spec.ts
```

## Verifikation dieser Umsetzung

Am 12. November 2026 auf dem lokalen Apple-Silicon-Mac ausgeführt:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` und `cargo test --workspace` erfolgreich. Der vollständige Testlauf benötigte den expliziten lokalen Testserver, weil einige bestehende SQLx-Tests nicht als `ignored` markiert sind.
- 492 Frontendtests erfolgreich; Produktionsbuild und OpenAPI-/TypeScript-Generierung erfolgreich. Vite meldet weiterhin seine vorhandenen Bundle-/Import-Aufteilungshinweise.
- 121 gezielt aktivierte PostgreSQL-/GNUbg-Regressionsfälle zu Einstellungen, Konfiguration, Benchmarks, Prognosen, Ergebnislesern und Datenbanken erfolgreich; zusätzlich vier Checkpoint-/Lease-Wiederaufnahmetests.
- Sechs neue native Installations-/Auftrags-/Benchmark-/Provider-/ETA-Tests sowie vier native Spieltests erfolgreich. Die Benchmarks prüfen vollständige 460 Checkerzüge beziehungsweise elf ausgeführte Spielzüge. Spieltests umfassen beide Farben, vollständige Partien, Cube-Drop, Annahme/Ablehnung von Aufgaben, Fortsetzung, Einstellungen nach Start und gekreuzte Bot-/Hinweis-Engines.
- 13 betroffene Browserfälle zu Einstellungen, Benchmark und echter Sage-Auswahl erfolgreich; der zusätzliche Installationskonflikt mit Wiederladen und acht Aufgabe-Dialogfälle wurden ebenfalls erfolgreich geprüft. Desktop-Screenshots in allen drei Referenzgrößen visuell geprüft.
- 39 Python-/Theme-Tests durchlaufen: 34 erfolgreich, fünf vorhandene optionale Tests übersprungen. Nicht pauschal aktivierte optionale/native Tests des Gesamtprojekts bleiben als `ignored` gekennzeichnet.
- Provider-Lookup mit 10.000 synthetischen Analysejobs und einem passenden Sage-Ergebnis: `EXPLAIN (ANALYZE, BUFFERS)` verwendet `jobs_engine_position_ready`, sechs Shared-Hit-Blöcke, 0,027 ms Datenbank-Ausführungszeit. Das ist eine einzelne warme, hochselektive Testabfrage; keine API- oder UI-Latenzaussage.

Diese ursprüngliche Verifikation fand im isolierten Integrations-Worktree statt; der Benutzerbestand wurde dabei nicht migriert. Die anschließende Übernahme in den Hauptworktree erhält die dort ergänzten Pause-/Fortsetzungsfunktionen. Der native Sage-Test spielt beide Farben vollständig durch und prüft zusätzlich Pause/Fortsetzung nach Änderung der globalen Enginevorgaben: Engine-Snapshots, Brettstellung, Historie, Bibliotheksidentität und SGF-Checkpoint bleiben erhalten.


## Übernahme und lokale Aktivierung am 12. November 2026

Die Integration wurde auf den Hauptworktree `/Users/hendrik/Developer/bgtutor` mit Stand `ca6631f` übertragen. Die inzwischen vorhandenen Änderungen für Pause/Fortsetzung und Entscheidungsdialoge bleiben erhalten. Originaldateien beider Worktrees, der vorherige lokale Plan und Prüfprotokolle liegen unter `backups/open-sage-integration-20261112-064429/`. Der Integrations-Worktree bleibt als zusätzliche Referenz bestehen.

Vor dem Neustart wurde die PostgreSQL-Datenbank vollständig als `bgtutor-before-open-sage.dump` gesichert (172.823.656 Bytes, SHA256 in `database-sha256.txt`). Das Inhaltsverzeichnis wurde mit PostgreSQL 17 `pg_restore` geprüft; das ältere lokale `pg_restore` unterstützt dieses Archivformat nicht. Die Migration `0041` ist angewendet. Der Vergleich von Zeilenzahlen und Inhaltsdigests aller 31 bisherigen Tabellen vor/nach Aktivierung zeigt ausschließlich die erwarteten Änderungen an `_sqlx_migrations`, `app_settings` und `configuration_state`; `engine_installations` ist neu. Bei `benchmark_runs` wurden für den Vergleich die zwei neuen, leeren Metadatenspalten ausgenommen. Alle 62 Partien, fünf Spielsitzungen sowie vorhandene Analysen, Profile und Lernstände blieben unverändert. Es wurden keine Testpartien oder Testaufträge im Benutzerbestand angelegt.

Die gespeicherten Vorgaben für neue Spiele und Analysen sind auf ausdrücklichen Wunsch **Open Sage, 2-ply**. Die Benchmarkauswahl bleibt unabhängig. Vorhandene Spielsitzungen und Aufträge behalten ihre bisherigen Einstellungen; gespeicherte Profile wurden nicht überschrieben. Die verifizierte Python-Umgebung liegt unter `local-data/open-sage/venv/bin/python`.

Der optimierte Backendbuild läuft unter `http://127.0.0.1:8787` und liefert `web/dist` aus. Seine dauerhafte ausführbare Datei ist `local-data/bin/bgtutor`, das Protokoll `local-data/bgtutor.log`. Zum erneuten Bauen/Starten gelten die [README-Schritte](../README.md#restart-neondojo-locally-on-port-8787). Die zentrale `Cargo.toml` deaktiviert Stripping ausdrücklich für Entwicklungs- und Releasebuilds, um das Rust-LINKEDIT-Problem unter macOS 27 zu vermeiden. Der frühere manuelle Präfix `CARGO_PROFILE_RELEASE_STRIP=none` ist damit nicht mehr nötig; die Releaseoptimierung bleibt aktiv.

Nach dem Zusammenführen erneut erfolgreich geprüft:

- Rust-Formatierung, Clippy mit `-D warnings`, vollständige Workspace-Suite: 187 Tests erfolgreich, 333 optionale Tests regulär ignoriert.
- Zehn ausdrücklich aktivierte native Sage-Tests für Spiel, Installation, Aufträge und vollständige Benchmarks sowie der bestehende GNUbg-Pause-/Fortsetzungstest. Die beiden vollständigen Sage-Partien prüfen zusätzlich Pause/Fortsetzung mit gespeicherter Engineidentität.
- 492 Frontendtests, 43 betroffene Browsertests, OpenAPI-/TypeScript-Generierung und Frontend-Produktionsbuild. Die Browserdatenbanken waren vom Benutzerbestand getrennt.
- 39 Python-/Theme-Tests: 34 erfolgreich, fünf optionale Tests übersprungen.
- Optimierter Backendbuild mit deaktiviertem Strip-Schritt, tatsächliche Installationsprüfung über die laufende API, beide Engines im Katalog verfügbar und gespeicherte Sage-Vorgaben nachgelesen.


## Performance und Integritätsgrenzen

Neue Benchmarks verwenden `bgsage-match-v3-2ply-4threads` beziehungsweise
`bgsage-checker-turn-v3-2ply-2threads-gnubg-rules`. Historische Ergebnisse bleiben
unverändert und werden nicht mit neuen Protokollen aggregiert.

Eine vollständige Identitätsprüfung hasht jede Datei nur einmal innerhalb dieser
Prüfung. Die nächste Prüfung liest alle Dateien erneut; Manifest, Fingerprint und
bestehende Installations-Snapshots bleiben kompatibel. Eine Spielanalyse hält den
Worker exklusiv und prüft Installation, RECORD und Gewichte vollständig vor und
nach dem Spiel. Erst nach der Endprüfung darf der vollständige Block zurückgegeben
und gespeichert werden. Fehler, Timeout oder Abbruch verwerfen den Block und
beenden den Worker. Einzelpositionen behalten ihre bisherigen Prüfgrenzen.

Für eine Sage-Spielaktion bleibt der isolierte GNUbg-Regelprozess zwischen
Inspektion und Ausführung sowie während der begrenzten Bot-Aktionskette erhalten.
Jeder Folgeschritt prüft die exakte GNUbg-ID. Regeln, Würfel und SGF erzeugt weiterhin
GNUbg; Revision und endgültiger SGF/Snapshot werden wie bisher atomar gespeichert.
Der Prozess wird vor dem Speichern beendet; bei Fehlern bleibt der zuvor gespeicherte
Spielstand bestehen. Protokollzeilen, Artefakte, Laufzeit und Aktionszahl sind begrenzt.


## Nachmessung der drei Optimierungen – 12. November 2026

Zum Zeitpunkt der Nachmessung lag die Implementierung uncommitted im Hauptworktree. Vorhandene Änderungen an
Engine Matches und anderen Dateien wurden per Dreiwege-Zusammenführung erhalten;
separate Sicherungen liegen unter `backups/sage-optimization-20261112-073609` und
`backups/sage-performance-merge-20261112-075315`. Die laufende Benutzerinstanz wurde während der Nachmessung
nicht ersetzt oder neugestartet. Insbesondere wurde deren Datenbank nicht für Tests
oder Benchmarks verwendet und die unabhängige Migration 0042 dort nicht durch diese
Arbeit aktiviert. Gemessen wurde der optimierte Release-Build des zusammengeführten
Hauptworktrees auf Port 8891 mit `neondojo_e2e_sage_perf_20261112`.

Je drei Analyse- und Spielmessungen pro Engine, sequenziell in wechselnder Reihenfolge,
ohne parallele Builds oder Tests. Hardware: Apple M2 Pro, 10 logische CPUs, 16 GiB RAM,
macOS 27.0.1. GNUbg 1.08.003 20261108 mit einem Thread; Sage 2.0.20260907 mit vier
Analyse- beziehungsweise zwei Spielthreads. Tiefe jeweils numerisch 2-ply; das ist kein
Nachweis identischer Suchstärke. Die Engine-/Modellsnapshots und Referenzprüfsummen sind
gegenüber der Ausgangsmessung identisch. Die OS-Dateicaches wurden nicht geleert.

Analyse: zehn Spiele, 460 Checkerzüge einschließlich erzwungener Züge und Cube-Analyse.
Spiel: elf feste Checkerzüge mit bereits gewürfelten Würfeln. Zeiten enthalten die
vollständige NeonDojo-Strecke und bei Sage einen kalten Sidecarstart je Lauf; Wartezeit,
Installationstest, Datenbankschreiben und UI-Animation sind ausgeschlossen. Die Spielzahl
ist der Median der drei laufweisen Mittelwerte, keine Perzentile einzelner Benutzerzüge.

| Messung | Engine | Vorher: Median | Nachher: Median | Nachher: Min–Max | Zeitreduktion |
| --- | --- | ---: | ---: | ---: | ---: |
| Analyse gesamt (s) | gnubg | 20.215 | 18.442 | 18.368–18.636 | 8.8 % |
| Analyse gesamt (s) | bgsage | 49.750 | 12.915 | 12.893–12.955 | 74.0 % |
| Spielzug im Mittel (ms) | gnubg | 229.470 | 224.945 | 224.723–226.190 | 2.0 % |
| Spielzug im Mittel (ms) | bgsage | 484.310 | 302.656 | 301.610–304.460 | 37.5 % |

Sage benötigt damit 74,0 % weniger Analysezeit (3,85-facher Durchsatz) und 37,5 % weniger
Zeit pro Referenzzug (Faktor 1,60). Im aktuellen Vergleich liegt Sage bei der Analyse
30,0 % unter GNUbg, beim Spielzug weiterhin 34,5 % darüber. Auch die GNUbg-Kontrolle
wurde um 8,8 % beziehungsweise 2,0 % schneller: Der gesamte Vorher/Nachher-Unterschied
ist deshalb nicht ausschließlich der Implementierung zuzuschreiben. Normiert auf die
jeweilige GNUbg-Kontrolle sinkt das Sage/GNUbg-Zeitverhältnis um etwa 71,5 % bei Analyse
und 36,3 % beim Spiel. Es wurden nur Laufzeiten, keine Spielstärkeunterschiede gemessen.

Die drei Änderungen sind unabhängig nachvollziehbar: Eine native Identitätsprüfung
liest 42 statt 67 Dateien beziehungsweise 16.854.849 statt 31.035.709 Bytes; Hashes gelten
nur innerhalb dieser Prüfung. Die 738 Einzelentscheidungen des Referenzmatches laufen
in zehn exklusiven Spielblöcken mit vollständiger Anfangs-/Endprüfung. Die elf Sage-
Referenzzüge starten elf statt 22 GNUbg-Regelprozesse. Ein Regelprozess muss vor der
Rückgabe des Ergebnisses sauber enden; Absturz, Fehlerexit, stderr, Timeout, ungültige oder
überlange Antworten und Abbruch verwerfen die Aktion.

Prüfungen des Hauptworktrees: `cargo fmt --all --check`, Clippy für alle Targets ohne
Warnungen, 199 bestandene Workspace-Tests (338 optionale Tests ignoriert), 492
Frontendtests, Frontend-/Release-Build. Python-Gesamtlauf: 49 Tests, davon fünf vorhandene
optionale Tests übersprungen. Zusätzlich bestanden vier native Sage-Spieltests nach der
Zusammenführung, sechs Sage-Konfigurations-/Job-/Benchmarktests, die nativen Adaptertests
und der exakte Vergleich aller 460 Checker- und 278 Cube-Entscheidungen zwischen
Einzelanfragepfad und Spielblöcken. Neue Fehler-, Abbruch-, Abschluss- und Konkurrenztests
prüfen die Prozess- und Batchgrenzen. Keine UI-Verträge oder Ansichten wurden geändert;
ein zusätzlicher Browser-End-to-End-Lauf wurde nicht ausgeführt.

Die erweiterte GNUbg-Spieltestsuite hat 34 bestandene und vier fehlgeschlagene Tests.
Alle vier Fehler wurden am unveränderten Ausgangscommit `d2e0291` reproduziert: Die
Tests ändern `play_sessions.gnubg_path`, während neue Sitzungen den unveränderlichen
Engine-Snapshot verwenden; erwartet wird deshalb fälschlich HTTP 503 statt des tatsächlich
erfolgreichen HTTP 200. Die Tests wurden weder abgeschwächt noch in diesem Auftrag
umgebaut. Details stehen in den archivierten Protokollen.

Messdaten, vollständige Einzelwerte, Quell-/Binärhashes und Prüfprotokolle:
`local-data/benchmarks/20261112-075923-2ply-optimized/` (`results.json`, `summary.json`,
`comparison.json`, `source-manifest.json`). Ausgangsmessung:
`local-data/benchmarks/20261112-065856-2ply-comparison/`. Das separate Benchmark-Backend
wurde nach dem Lauf beendet; die Testdatenbank und Messartefakte bleiben zur Nachprüfung
erhalten. Während der Nachmessung wurden keine Änderungen committet oder gepusht.


### Aktivierung der Optimierungen

Die Optimierungen benötigen keine neue Datenbankmigration. Für die lokale Aktivierung
wird ausschließlich der geprüfte Open-Sage-Commit gebaut; unabhängige lokale Änderungen
und deren Migrationen bleiben im Hauptworktree erhalten. Vor dem Austausch von
`local-data/bin/bgtutor` werden die bisherige Binärdatei und ein vollständiger PostgreSQL-
Dump gesichert. Aktive Aufträge müssen vor dem Neustart abgeschlossen sein. Danach
werden API-Gesundheit, verfügbare Engines, unveränderte Einstellungen und Inhaltsdigests
sämtlicher Benutzertabellen geprüft. Die bestehenden Open-Sage-Vorgaben für Spiel und
Analyse bleiben bestehen. Aktivierungsnachweise liegen außerhalb von Git unter
`backups/sage-performance-activation-*/`.


## Gepatchter lokaler Fork und 1-ply – 12. November 2026

Die lokale Installation verwendet einen separat gebauten, nicht editierbaren ARM64-Wheel aus
`HendrikReh/bgsage`, Commit `2e71beed9c24b5bbc7564c03374d6d588ff24119` (Issue #6).
Basis: `139427d826dd67798226e7d23c003c90315306ba`. Der Paketname und die Upstream-Version
`2.0.20260907` bleiben erhalten; Commit, Wheel-Hash und vollständiger Laufzeit-Fingerprint
unterscheiden diesen Build vom ursprünglichen PyPI-Wheel.

Installation: `local-data/open-sage-fork-2e71bee/venv/bin/python`.
Die daneben gespeicherten `requirements.lock`, `wheels/`, `provenance.json` und
`probe-1ply-2ply.json` dokumentieren und reproduzieren exakt diesen lokalen Build.
Der Sage-Wheel hat SHA256 `d836dad115c39d8fc88e49c76500618a1fe421d3dadfb6420736324cfada2597`;
der Build ist für das geprüfte macOS 27 ARM64, Python 3.12 bestimmt.

```sh
python3.12 -m venv /path/to/new-venv
/path/to/new-venv/bin/python -m pip install --no-index --only-binary=:all: --require-hashes \
  --find-links local-data/open-sage-fork-2e71bee/wheels \
  -r local-data/open-sage-fork-2e71bee/requirements.lock
/path/to/new-venv/bin/python tools/probe_open_sage.py --levels 1ply 2ply --output /tmp/sage-fork-probe.json
```

Die Freigabe ergänzt einen optionalen `one_ply_verified`-Nachweis im unveränderlichen
Installations-/Auftragssnapshot. Alte Snapshots ohne dieses Feld bleiben 2-ply und behalten
bei der Serialisierung ihre bisherige Form. Die Installationsprüfung setzt den Nachweis erst
nach echter 1-ply-Auswertung mit eingehaltenem Threadbudget und identischer Artefaktidentität.
Jeder gestartete Worker prüft weiterhin das angeforderte Profil und das tatsächliche Budget;
RECORD-, Modell- und Native-Hashes sowie Prüfungen an Anfrage-/Spielblockgrenzen bleiben erhalten.
Eine fehlende oder fehlgeschlagene 1-ply-Prüfung lässt nur 2-ply verfügbar.

Spiel, Analyse, Benchmarks und Engine-Matches speichern die gewählte Tiefe pro Auftrag bzw.
Spieler getrennt. Benchmarkprotokolle unterscheiden 1-ply von 2-ply; vorhandene 2-ply-Protokolle
bleiben unverändert. Migration `0043` erweitert nur die zwei Sage-Tiefen-Constraints, ohne
Benutzereinstellungen oder Datenzeilen zu ändern. Die Vorgaben bleiben Open Sage mit 2-ply.

Die bisherige Umgebung `local-data/open-sage/venv` bleibt unverändert für alte Partien und
Aufträge erhalten. Ein Rückwechsel erfolgt über „Lokale Open-Sage-Installation → Prüfen und
speichern“ mit dem alten Python-Pfad; neue 1-ply-Vorgaben müssten dabei vorher auf 2-ply stehen.
Prüf-/Sicherungsartefakte liegen unter `backups/sage-fork-activation-20261112/`.

## Wiederverwendete Cube-Strategie und kürzerer Regelstart

Der lokale Fork `HendrikReh/bgsage` (Commit `5b146c7d86fa965f2185e3871f3c56df030fd76f`) hält
die native Cube-Strategie pro Analyzer vor. Sie wird beim ersten Cube-Aufruf
aus den verifizierten Gewichten geladen und bleibt von der Checker-Strategie
getrennt. Bearoff-Verarbeitung und temporäre Suchzustände bleiben an den
jeweiligen Aufruf gebunden. Cubeful 1-ply konvertiert die Kandidaten seriell.
NeonDojo prüft im realen Checkerpfad, dass der als seriell deklarierte Build
keinen Kandidatenpool startet. Bei bisherigen Builds und bei 2-ply gelten
weiter die beobachteten Poolgrenzen.

Für direkte GNUbg-Binärdateien liest der Regeladapter die Versionsmeldung
aus dem laufenden Prozess. Er prüft den aufgelösten Pfad und SHA-256 der
Binärdatei vor dem Start und nach sauberer Beendigung des Prozesses.
Für ausführbare Wrapper bleibt die getrennte Versionsprüfung vor und nach
der Aktion erforderlich. Die Prüfung des Checkpoints und der legalen Züge
sowie die Sage-Integritätsprüfungen an Spielblock- und Anfragegrenzen
gelten weiterhin.

Der vermessene Wheel (SHA-256
`20695d1835a7af26fcfb8fe25b3ddb094d391157cb70b0b72b68aae6d7dff2dc`) ist
mit der zugehörigen `requirements.lock` unter
`local-data/open-sage-fork-optimized-20695d1/venv/bin/python` installiert. Der Laufzeit-Fingerprint unterscheidet ihn
vom bisherigen Fork, auch wenn die Paketversion gleich bleibt. Frühere
Aufträge und Spielsitzungen benötigen weiterhin ihre gespeicherte
Installation. Die Testumgebung und die Rohdaten des Vergleichs befinden
sich lokal unter `local-data/benchmarks/*-sage-cube-reuse/`.

## Post-Crawford-Cube – 18. November 2026

Bis Commit `5b146c7` bewertete Open Sage „kein Doppel“ für den Zurückliegenden
nach Crawford zu niedrig. In 15 Positionen aus Engine-Duellen (3-away oder
4-away gegen 1-away, Cube zentriert) lag „kein Doppel“ bei 2-ply 0,058 bis
0,184 unter „Doppel/Take“. GNUbg (`hint` und Rollout mit 1296 Spielen) und
Open Sages eigener Rollout bewerten beide Aktionen dort gleich. Die
Empfehlung lautete trotzdem „doppeln“, aber die Analyse verbuchte für
„kein Doppel“ einen Fehler. Der Fork behebt das in PR #9: `cl2cf_match`
bewertet einen Cube, den der Zurückliegende noch drehen kann, als einmal
verdoppelt.

Der Build aus `340fc5346f1938d505f99f22101e059a30e9cfc6` liegt unter
`local-data/open-sage-fork-340fc53/` mit `src/`, `wheels/`, `requirements.lock`,
`provenance.json`, `probe-1ply-2ply.json` (bestanden) und `venv/`. Der
Sage-Wheel hat SHA256
`a9a264d1436f613dfdbecc7ec1bb5c624f8f6589ed26e09eaaaddc306bd1c00d`, der
Laufzeit-Fingerprint in NeonDojo lautet `7c9a021a…`. Analysen, die vorher mit
`5b146c7` liefen, behalten ihre Installation und ihre Werte. Für korrigierte
Post-Crawford-Cube-Werte müssen sie neu analysiert werden.

Am 18. November 2026 wurden 101 lokale Matches mit Open Sage 2-ply neu
analysiert. Alle 101 Jobs wurden abgeschlossen. Laut Engine-Snapshot liefen
sie mit `7c9a021a…`, die Laufzeit betrug etwa zwei Minuten. Verglichen wurden
6.452 Cube-Entscheidungen der vorherigen Analysen (`5b741451…`, Build
`5b146c7`). Gepaart wurde über Match, Spiel, Entscheidung und Spieler:

| Spiele | Entscheidungen | Andere beste Aktion | Cube-Fehler alt − neu | Fehler größer |
| --- | ---: | ---: | ---: | ---: |
| Post-Crawford | 77 | 27 | 1,507 | 0 |
| Übrige | 6.375 | 0 | 0,026 | 16 |

Nur in Post-Crawford-Spielen verschob sich „kein Doppel“ um mehr als 0,03,
und dort in allen 77 Entscheidungen. In den übrigen Spielen blieben die
Abweichungen klein: Bei „kein Doppel“ lag der Median bei 0,0003, bei
„Doppel/Take“ das Maximum bei 0,03. Die beste Aktion änderte sich dort
nirgends. Die Ursache dieser Restabweichungen ist nicht untersucht. Der
Build `340fc53` unterscheidet sich von `5b146c7` nicht nur durch den Fix.
Insgesamt sank der Cube-Fehler von 4,302 auf 2,769. Die Zahl der
Cube-Fehler über 0,02 sank von 68 auf 50. Bei den Zugentscheidungen blieb
der Gesamtfehler praktisch gleich (14,419 gegenüber 14,422). Die vorherigen
Analysen sind weiterhin gespeichert.

## Fork-Release 2.0.20261119 – 19. November 2026

Seit dem 19. November 2026 verwendet NeonDojo das Fork-Release
[`v2.0.20261119`](https://github.com/HendrikReh/bgsage/releases/tag/v2.0.20261119)
(Commit `0dee4f321c0437dc142da2ae68d26f7da3038db1`, Modell weiterhin
`stage11`). Es ist das erste Fork-Paket mit eigener Versionsnummer. Frühere
Fork-Builds meldeten weiterhin `2.0.20260907`. Die Installation liegt unter
`local-data/open-sage-fork-2.0.20261119/`. Sie enthält `wheels/`,
`requirements.lock`, `provenance.json`, `probe-1ply-2ply.json` (bestanden) und
`venv/`. Verwendet wird das offizielle cp312-Wheel aus dem GitHub-Release mit
SHA256 `c476a0b6edbc32021a0c7c4a7397ffcd2d869502c04bbf060a2a12b37baec4d9`.
Der Digest wurde gegen die Release-API geprüft. Der Laufzeit-Fingerprint in
NeonDojo lautet `9b3ffc5c…`.

Die unterstützten Paketversionen stehen in `SUPPORTED_VERSIONS` in
`tools/probe_open_sage.py`. Der Sidecar verwendet diese Prüfung, und
`SUPPORTED_SAGE_PACKAGE_VERSIONS` in `backend/src/engines/mod.rs` muss
dieselbe Liste enthalten; ein Unit-Test vergleicht beide. `2.0.20260907` bleibt
gültig, damit gespeicherte Job- und Duell-Snapshots und die Installation
`open-sage-fork-340fc53` weiter funktionieren.

Laut Release-Notes ändern sich die Ergebnisse: Bei 1- und 2-ply hängt die
Cube-Lebensdauer im Money-Kontaktspiel von Tiefe und Pipcount ab. Außerdem
durchsucht der cubeful 2-ply-Checkerpfad nur noch die Kandidaten, die den
Zugfilter überstehen, in voller Tiefe. Die übrigen Kandidaten liefert Open Sage
mit 1-ply-Werten in derselben Liste. NeonDojo mischt keine Tiefen. Der Sidecar
verwirft deshalb Kandidaten unter der angeforderten Tiefe, wie bisher schon
beim cubeless Pfad. Der gespielte Zug wird über `force_boards` immer in voller
Tiefe bewertet; liefert Open Sage ihn flacher, schlägt die Anfrage fehl.
Spielhinweise und die Bewertung des eigenen Zugs benötigen jeden legalen Zug.
Für sie fordert NeonDojo alle legalen Züge an (`all_legal`). Bei der
Eröffnung 31 dauert das in 2-ply etwa 35 ms statt 12 ms. Neue 2-ply-Analysen
zeigen dadurch weniger Kandidaten, im Testfall 5 statt 16. 3- und 4-ply
bewerten weiterhin alle Kandidaten in voller Tiefe. Die Probe-Prüfung
`forced_played_move_2ply` verlangt den gespielten Zug in voller Tiefe, nicht
mehr jeden Kandidaten.

Analysen und Duelle, die mit `7c9a021a…` liefen, behalten ihre Installation
und ihre Werte. Neue Werte erhalten sie nur durch eine neue Analyse.

## Warm-up für Partien

Beim Anlegen, Öffnen oder Fortsetzen einer aktiven Open-Sage-Partie kann NeonDojo
deren gespeichertes Bot-Profil im Hintergrund laden. Die HTTP-Antwort wartet
dabei nicht auf den Sidecar. Gleichzeitige Warm-up-Anfragen sind begrenzt.
Ein Zug verwendet weiterhin seinen gespeicherten Snapshot und startet bei
Bedarf selbst einen geprüften Worker.

Spiel und andere Sage-Anfragen haben je einen Platz für einen laufenden
Worker. Beide Plätze und vorübergehende Installationsprüfungen teilen sich
die bisherige Zugriffssperre. Eine Analyse oder Installationsprüfung
verdrängt den Spielprozess nicht. Änderungen am gespeicherten Spielprofil
erfordern einen neuen Prozess für dieses Profil. Benchmarkläufe starten ihren
eigenen Worker weiterhin kalt. Die Prüfungen von Artefakten, Threadgrenzen,
Anfragen, Antworten und Spielständen gelten wie bisher.


## 3-/4-ply-Analyse und -Duelle

Wähle in der Analyseansicht **Analyse-Engine → Open Sage** und als Tiefe
**4-ply**. Das gilt für ein einzelnes Spiel und für das gesamte Match.
Jeder Auftrag prüft und speichert Engine, Tiefe und Modell-Fingerprint.
Migration 0047 erlaubt außerdem 3- und 4-ply als gespeicherte Analysevorgabe.

Der installierte Fork `5b146c7d86fa965f2185e3871f3c56df030fd76f` wurde
für beide höheren Stufen mit einer realen Checker- und Cube-Bewertung im
5-Punkte-Kontext geprüft. Der Sidecar kontrolliert das Threadlimit, die
Installationsidentität und die von Open Sage ausgegebene Bewertungstiefe.
Die Ergebnisse gelten für das angeforderte Profil und den gespeicherten
Modell-Fingerprint. Normale Partien gegen den Computer verwenden weiterhin
die bisherigen Spielprofile.


Beim letzten Ausspielzug kann Sage ein vorzeitiges Endbrett anbieten, obwohl
GNUbg die Verwendung weiterer spielbarer Würfel verlangt. In diesem
Sonderfall verwendet NeonDojo das bestplatzierte gemeinsame Endbrett und
speichert die ausgeführte GNUbg-Zugnotation.
