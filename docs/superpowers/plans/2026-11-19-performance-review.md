# Umsetzungsplan für die NeonDojo-Performance

Die Wartezeit bei der Entscheidungssuche, bei Dice Equity und beim Wechsel zwischen Zügen senken. Die folgenden Arbeitspakete basieren auf dem Review von `main` bei `d3a21e1` am 19. November 2026. Jedes Paket lässt sich einzeln umsetzen und abnehmen.

## Rahmen

Rust, Axum und PostgreSQL bleiben für Ergebnisse, Quellen und Jobzustände maßgeblich. React lädt Daten passend zum sichtbaren Arbeitsbereich. Revisionsschutz, Analyseprovenienz, Cursor, unvollständige Ergebnisse und bestehende Nutzerdaten gehören zur Abnahme. Schemaänderungen verwenden neue Migrationen. Bei Vertragsänderungen `npm run api:types --prefix web` ausführen. Für Messungen und Tests separate Datenbanken verwenden. [Repository-Regeln](../../../AGENTS.md) · [Verifikation](../../development/testing.md)

## Reihenfolge

| Priorität | Arbeitspaket | Abnahmesignal |
| --- | --- | --- |
| P0 | Baselines am Review-Stand und am Startpunkt der Umsetzung erfassen | Gleiche Fixture, Engine und Messmethode je Vergleich |
| P1 | Entscheidungssuche über Revisionen aktualisieren | Auf warmem Bestand entfallen JSON-Hashes und erneute Projektion |
| P1 | GNUbg Dice Equity bündeln | Auswertungs- und Versionsstarts sind unabhängig von der Zahl der Würfe |
| P1 | Analyseauswahl über Zugwechsel stabil halten | Null erneute Requests für unveränderte Spielbewertungen und Matchstatistik |
| P2 | Nummerierte Fehlerliste schlank paginieren | Exakte Trefferzahl und Reihenfolge bei Detailarbeit nur für die Seite |
| P2 | Open-Sage-Spielzüge bei Hintergrundanalyse absichern | Zuglatenz unter Last gemessen und Wartezeit begrenzt |
| P3 | Notizen, Spielvalidierung, Jobmonitoring und Checkpoints vereinfachen | Weniger SQL- bzw. HTTP-Aufrufe pro Workflow |

## P0 · Baselines und Vergleich

- [x] Messrezepte A, B und C auf `d3a21e1` und `main` (`2cb191e`) wiederholt. Die Vergleiche mit den jeweiligen Implementierungsstartpunkten stehen bei den Arbeitspaketen.
- [ ] Je Paket einen fokussierten Regressionstest und eine wiederholbare Messung etablieren. Revisionswechsel, Competition-Sichtbarkeit, leere oder teilweise analysierte Spiele, erzwungene Pässe und verspätete Antworten in den jeweils betroffenen Pfaden prüfen.
- [ ] Für den Vergleich Commit, macOS, Hardware, PostgreSQL- und Engine-Version, Tiefe, Threads, Datenumfang und Trefferquote erfassen. SQL-Ausführungszeit, API-Antwortzeit und Zeit bis zur Browseranzeige jeweils separat ausweisen. Vergleichsläufe ohne parallele Builds oder Analyseaufträge durchführen.

Die Wiederholung am 21. November 2026 nutzte Release-Builds auf einem Mac mini M4 Pro mit macOS 27.0.1 und PostgreSQL 16.13 auf Loopback. Rezept A verglich 2.000 Matches, davon 1.000 mit passendem Spieler und insgesamt 200.000 Entscheidungen. Auf beiden Ständen waren genau 1.000 native Fingerabdrücke aktuell. Die drei warmen SQL-Läufe ergaben **1.247,7 → 14,2 ms** Median für die Aktualitätsprüfung. Die mittleren JSON-Größen betrugen 1.670 Textbytes und 655 gespeicherte Bytes.

Rezept C nutzte GNU Backgammon 1.08.003 mit einem Thread und je fünf frischen Aufträgen pro Tiefe und Stand. Die beobachtete Zeit bis zum Abschluss sank von **5,13 auf 0,58 s** bei 1-ply und von **6,67 auf 1,83 s** bei 3-ply. Pro Auftrag sanken die Versionsstarts von **43 auf 3** und die Auswertungsstarts von **21 auf 1**. Alle Stichproben enthielten die 21 erwarteten Würfelpaare mit Gesamtgewicht 36. Rezept B misst die HTTP-Antwortzeit. Seine Ergebnisse stehen bei der [nummerierten Fehlerliste](#p2--nummerierte-fehlerliste). Die Browserzeit wurde beim [Spielwechsel](#spielwechsel-und-aufgezeichnete-würfe) separat gemessen.

## P1 · Entscheidungssuche

In `recurring_mistakes::index::ensure` die aus Entscheidungs-JSON abgeleitete Prüfsumme ersetzen. `ensure` versorgt neben der Entscheidungssuche auch die Fehlergruppen (`ensure_and_group`), die deshalb zur Abnahme gehören. Den Index weiterhin an die wirksame Analyse, Quellrevision, Spielernamen, Wettbewerbssichtbarkeit und Klassifikatorversion binden. [Indexprüfung](../../../backend/src/recurring_mistakes/index.rs#L33) · [Katalog-View](../../../backend/migrations/0001_schema.sql) (früher `0074_catalog_imported_lookup.sql`, jetzt in der Schema-Baseline) · [Native Checkpoints](../../../backend/src/match_jobs.rs#L202)

**Stufe 1 · umgesetzt.** Der Anwendungscode fügt gespeicherte Entscheidungen nur ein oder löscht sie. Schreiber sind der Spiel-Checkpoint, die Kopie beim Restore, die Positionsretention und Kaskaden über `jobs` und `positions`. [Checkpoint](../../../backend/src/match_jobs.rs#L220) · [Restore](../../../backend/src/databases.rs#L1405) · [Retention](../../../backend/src/position_retention.rs#L80) Datenmigrationen wie `0079_xg_ply_convention` und mehrere Tests ändern `move_analyses` dagegen per `UPDATE`. Umsetzung:

- Der Fingerabdruck endet mit `count(*)` und `sum(id)` der Entscheidungen des wirksamen Jobs. `bigserial`-IDs werden nie wiederverwendet, daher fällt jedes Einfügen und Löschen auf, auch Löschen mit anschließendem Neueinfügen. Der Index `move_analyses_page(job_id,id)` liefert beide Werte, ohne das JSON zu lesen.
- Migration `0080_decision_index_row_fingerprint` legt einen Statement-Trigger an. Er entfernt bei jedem `UPDATE` auf `move_analyses` die Indexeinträge der betroffenen Matches. Aktualisierungen in Migrationen, Korrekturen und Tests führen damit zur Neuindexierung.
- Dieselbe Migration überträgt jeden Eintrag, dessen Hash noch zu den gespeicherten Entscheidungen passt, in das neue Format. Ohne diese Übertragung würde das Upgrade jeden Match veralten lassen. Ab 400 veralteten Matches pro Anfrage antwortet die Suche nur noch mit „Too many …“.
- In Stufe 1 konnte ein Rebuild nach einem gleichzeitigen `UPDATE` einen alten Stand veröffentlichen, weil ein `UPDATE` Anzahl und ID-Summe gleich lässt. Die Prüfung unter der Sperre erkannte Einfügen und Löschen.

Ergebnis auf Fixture A (19. November 2026, M4 Pro, PostgreSQL 16.13): Aktualitätsprüfung **~1.250 ms → ~122 ms** (−90 %). Vor der Übertragung waren 1.000 von 1.000 Kandidaten veraltet, danach 0.

**Stufe 2 · umgesetzt am 21. November 2026.** Migration `0089_decision_source_revisions` speichert die Revision je nativem Job und je importierter Analyse. Transaktions-Trigger erfassen Änderungen an Entscheidungen, Checkpoints und für Study relevanten Jobdaten. Eine Änderung der Namen oder des Spieldatums entfernt die alten Indexzeilen. Die Migration übernimmt vorhandene native Zeilenfingerabdrücke einmalig als Startwerte und korrigiert veraltete Spieldaten im Index. Der Schemafingerabdruck bestehender Backups bleibt gültig, weil die Migration ausschließlich abgeleitete Daten ändert.

Der Rebuild liest Fingerabdruck und Entscheidungen aus einem gemeinsamen Snapshot. Damit kann ein vorübergehender Analysewechsel keine Zeilen dauerhaft der falschen Quelle zuordnen. Beim Veröffentlichen prüft er Revision und Metadaten unter der Match- und Revisionssperre nochmals. Schließt eine andere Analyse erst nach dieser Prüfung ab und verdrängt die bisherige Auswahl, kann die bereits laufende Suche noch ihre bisherige Auswahl liefern. Die nächste Suche erkennt die Abweichung.

| PostgreSQL-Ausführungszeit, Median | Review `d3a21e1` | Vorher `be7f275` | Mit Revision |
| --- | ---: | ---: | ---: |
| Aktualitätsprüfung | 1.319,3 ms | 40,2 ms | 26,0 ms |
| Erste Seite, 50 Treffer | 12,4 ms | 12,9 ms | 12,4 ms |
| Exakte Trefferzahl | 8,8 ms | 8,9 ms | 8,9 ms |

Gemessen mit `EXPLAIN (ANALYZE, BUFFERS)` auf einem Mac mini M4 Pro mit macOS 27.0.1 und PostgreSQL 16.13. Die Fixture umfasst 2.000 Matches und 200.000 native Entscheidungen in 1.000 passenden Jobs. Die JSONs hatten im Mittel 1.670 Textbytes und 655 gespeicherte Bytes. Für Seite und Zählung waren 60.000 Entscheidungen des Spielers bei Mindestverlust 0,04 relevant. Je Phase und Stand folgten fünf Messungen auf einen Warmlauf. Die Aktualitätsprüfung brauchte rund 35 % weniger Zeit als am Implementierungsstart und 98 % weniger als beim reproduzierten Review-Stand. Die Prüfung der Katalogeignung liest weiterhin je Job einen passenden Eintrag aus dem Index von `move_analyses`. Die zusätzliche Aggregation aller 200 Entscheidungs-IDs je Job entfällt. Nach dem Upgrade waren alle 1.000 vorbereiteten Indexeinträge aktuell. Die Messwerte beschreiben PostgreSQL-Laufzeit.

Die Vorher-nachher-Messung verwendet dieselbe Fixture aus Rezept A. Für `be7f275` und den neuen Stand `CANDIDATES` samt Spielerfilter aus `ensure` übernehmen und Indexversion, Datenbankwahl und Spieler als `'1:perf'`, `NULL` und `ARRAY['perf player']` binden. Die Fingerabdrücke mit der Vorher-Abfrage in `decision_index_matches` eintragen. Den Nachher-Stand per Migration auf einer identisch vorbereiteten Datenbank prüfen. Für die Seitenselektion und die Zählung `FILTER` aus dem Suchendpunkt verwenden und je Entscheidung eine synthetische Indexzeile mit alternierendem Spielernamen einfügen. Als Spieldatum `coalesce(m.played_on,m.created_at::date)` übernehmen. Die ersten 50 Treffer mit Mindestverlust 0,04 absteigend lesen, die Trefferzahl separat messen.

**Katalogzählung · umgesetzt.** Die native Teilmenge von `analysis_catalog_sources` zählte alle Entscheidungen je Job, weil die Zähler im `WHERE` standen und der Planer sie deshalb nicht weglassen konnte. Migration `0081_catalog_native_lookup` entscheidet die Eignung wie 0074 für Importe über `EXISTS` und hängt die Zähler als weglassbaren `LEFT JOIN LATERAL` an. Ein Job bleibt genau dann geeignet, wenn eine seiner Entscheidungen zu einem Spiel mit Checkpoint gehört. Der Test `native_eligibility_needs_a_checkpointed_decision_and_selection_skips_the_counts` prüft diese Grenze und dass der Plan der wirksamen Auswahl keine Aggregation über `move_analyses` mehr enthält.

Ergebnis auf der lokalen Bibliothek (858 Matches, 65.796 Entscheidungen; Spieler mit 101 nativen Analysen; View in einer zurückgerollten Transaktion getauscht): Aktualitätsprüfung **~38 ms → ~13 ms** (−66 %). Katalog, wirksame Auswahl und Fingerabdrücke waren identisch. Auf Fixture A (isolierter Cluster, PostgreSQL 16.13, abwechselnd je 3 × 3 warme Läufe; Load-Average 50–65 durch fremde Prozesse): **153–367 ms → 44–103 ms**, Bestwerte −71 %, Mediane 293 → 49 ms. Katalog und Auswahl waren identisch, und alle 1.000 Einträge blieben aktuell.

**Veröffentlichung · umgesetzt in Stufe 1.** `store` las den Fingerabdruck unter der Match-Sperre nicht erneut. Ein langsamer Rebuild konnte deshalb einen neueren Eintrag mit früher geladenen Entscheidungen überschreiben. `ensure` und `store` lesen den Fingerabdruck jetzt über dieselbe Abfrage `CANDIDATES`. Unter der Sperre liest `store` ihn für den Match erneut:

- Unverändert: Der Eintrag wird veröffentlicht.
- Geändert, aber eine andere Anfrage hat den aktuellen Stand schon gespeichert: Es wird nichts geschrieben.
- Sonst: `publish` lädt die Entscheidungen unter dem neuen Fingerabdruck erneut. Nach drei Versuchen bittet die Anfrage um Wiederholung.

Das deckt auch ältere Importe ab, deren Laden `moves_cataloged` umschaltet. Die zusätzliche Abfrage kostet auf der lokalen Bibliothek ~0,4 ms je neu aufgebautem Match. Der Test `slower_rebuilds_never_publish_an_older_fingerprint` in `index.rs` schlägt ohne die Prüfung fehl.

Rezept A extrahiert die Aktualitätsabfrage aus dem jeweiligen Checkout. Auf dem Review-Stand misst es zusätzlich die Kontrollabfrage und die ID-Summe aus Stufe 1.

- [x] Zwei identische Suchen auf aktuellem Bestand lösen null Rebuilds aus und lesen im Pfad zur Aktualitätsprüfung kein natives Entscheidungs-JSON. Auswahllogik, Trefferzahl, Sortierung und Rückverweise stimmen mit der Baseline überein. Das Messziel für A war mindestens 70 % weniger PostgreSQL-Ausführungszeit gegenüber dem Review-Stand auf demselben warmen Datensatz. Die Wiederverwendung prüft `decision_index_follows_saved_decisions`.
- [x] Wechsel zwischen nativem und importiertem Ergebnis, spätere Checkpoints, Löschung, Positionsretention, Namensänderung, Restore und eine neue Klassifikatorversion invalidieren die betroffenen Einträge. Ein konkurrierender Rebuild überschreibt keine bereits veröffentlichte neuere Projektion. Bestehende Datenbanken werden migriert. Fehlergruppen liefern dieselben Gruppen und Raten wie zuvor. Stufe 2 prüft auch Korrekturen mit unveränderter Zeilen-ID, den Austausch eines Checkpoints und einen vorübergehenden Analysewechsel.
- [x] Die 62 fokussierten Unit- und Integrationstests für Index, wiederkehrende Fehler, Analysekatalog, Positionsretention und Backup mit isoliertem SQLx-Setup bestanden, einschließlich der echten GNUbg-Analyse. Messung A erfasst Aktualitätsprüfung, Seitenselektion und exaktes `COUNT` mit `EXPLAIN (ANALYZE, BUFFERS)`.

## P1 · GNUbg Dice Equity

`jobs::execute` ruft für jeden der 21 Würfe `ConfiguredEngine::analyze` auf und gilt damit für GNUbg und Open Sage. Jeder GNUbg-Aufruf startet eine Auswertung und zwei Versionsprüfungen. Die Bündelung gehört deshalb in die Engine-Schicht: eine Methode für die Würfelverteilung in `ConfiguredEngine` mit GNUbg-Batch in `engine.rs`. Open Sage behält zunächst den bestehenden Einzelpfad. `jobs.rs` behält Lease, Fortschritt und Persistenz. [Job-Schleife](../../../backend/src/jobs.rs#L197) · [Prozess und Versionsprüfung](../../../backend/src/engine.rs#L327) · [Pass-Fallback](../../../backend/src/engine.rs#L494)

1. **Zwischenschritt.** Die Versionsprüfung einmal vor dem ersten und einmal nach dem letzten Wurf eines Auftrags ausführen statt bei jedem Aufruf. Vor dem Abschluss veröffentlicht der Würfelauftrag kein Ergebnis, die Absicherung gegen einen Programmwechsel bleibt also gleichwertig. Das senkt die Starts der Anfangsstellung von 64 auf 23. *Umgesetzt und durch den Batch ersetzt:* Zunächst prüfte `Engine::analyze_rolls` die Version vor dem ersten und nach dem letzten Wurf und rief dazwischen `analyze` ohne Einzelprüfung auf. Messung C am 19. November 2026 (M4 Pro, GNUbg 1.08.003, je 5 frische Aufträge, Load-Average ~18 in beiden Läufen): Versionsaufrufe **43 → 3** inklusive Auftragsanlage, Auswertungen unverändert 21. Bis zum Abschluss bei 1-ply **Median 7,6 s (5,3–9,0) → 2,67 s (2,61–2,74)**, bei 3-ply **6,9 s (6,7–9,1) → 4,23 s (4,20–4,25)**. `e2e/dice-equity.spec.ts` bestand (8/8).
2. **Batch.** Alle Würfe in einer begrenzten GNUbg-Befehlsdatei auswerten. Für jeden Wurf die ursprüngliche Stellung samt Matchkontext neu setzen (`set gnubgid`), Ausgaben eindeutig dem Wurf zuordnen und je Wurf eine eigene Datei für die Vorschau-Zugpfade schreiben. GNUbg-Skripte haben kein `echo`. Die Trennung der Ausgabe je Wurf vor der Umsetzung festlegen, etwa über die feste Reihenfolge der `hint`-Blöcke oder Marker über die eingebettete Python-Unterstützung. Zeit- und Ausgabegrenzen für den gesamten Batch neu bemessen: Heute gelten 90 s und 8 MiB je Aufruf. *Umgesetzt:* `AnalysisEngine::analyze_rolls` nimmt die Würfe und meldet die Zahl fertiger Würfe; `jobs.rs` behält Lease, Fortschritt, Gewichte und Persistenz, Open Sage den Einzelpfad. `Engine::analyze_rolls` schreibt eine Befehlsdatei: Bewertungstiefe und Zugfilter einmal, dann je Wurf `set gnubgid`, `set dice`, `hint` und `save game hint-<i>.sgf`. Die Ausgabe wird an der Zeile „The dice have been set to a and b.“ getrennt, die `set dice` selbst ausgibt, und die Reihenfolge wird geprüft. Python ist dafür nicht nötig. GNUbg gibt die Ausgabe je Wurf ab, `run_observed` zählt die Marker daher während des Laufs, und der Fortschritt steigt weiter je Wurf. Die Versionsprüfung von `run_with_limit` umschließt den einen Prozess. Zeitgrenze 90 s je Wurf wie zuvor je Aufruf, Ausgabegrenze unverändert 8 MiB (ein Batch der Anfangsstellung schreibt rund 80 KB). Paritätstest gegen Einzelaufrufe: `real_gnubg_roll_batch_matches_individual_evaluations` (1-ply und 3-ply mit Zugfilter) vergleicht Kandidaten, Zugpfade, Equities und Wahrscheinlichkeiten je Wurf als JSON.
3. **Erzwungene Pässe.** Heute löst ein blockierter Wurf einen zweiten GNUbg-Lauf aus. Prüfen, ob die bestehende Regelvalidierung (`apply_checker_turn` mit leerem Zug, wie im Open-Sage-Pfad) blockierte Würfe vorab erkennt, sodass der Pass-Befehl direkt im Batch steht. [Regelprüfung im Open-Sage-Pfad](../../../backend/src/engines/mod.rs#L307) *Umgesetzt:* `engines::forced_pass` prüft mit `apply_checker_turn` und leerem Zug auf dem absoluten Brett (`learning::decode_decision_board`). Blockierte Würfe stehen mit den Pass-Befehlen am Ende des Batches, weil diese die Spielereinstellungen ändern. Meldet GNUbg für einen spielbaren Wurf keinen legalen Zug, scheitert der Auftrag. `real_gnubg_roll_batch_evaluates_forced_passes_in_place` deckt eine Stellung mit 6 blockierten und 15 spielbaren Würfen ab; Regelprüfung und GNUbg stimmen überein, und die Pass-Werte sind identisch mit dem bisherigen Zweitlauf.

Die gepinnte Engine-Identität und Threadzahl am Batch prüfen. Die bestehende Einzelpositionsanalyse dient als Referenz für Ergebnisparität. Bei 1-ply dominieren die Prozessstarts. Bei tieferen Profilen wächst der Anteil der Auswertung, und der relative Gewinn sinkt.

- [x] Alle 21 Würfe liefern bei identischer Engine-Version und Tiefe dieselben besten Kandidaten, Zugpfade und Equities innerhalb der bestehenden Ausgabepräzision. Gewicht 1 für Pasche, Gewicht 2 für andere Paare, Gesamtgewicht 36 und gewichteter Mittelwert stimmen. Eine blockierte Stellung liefert auch für erzwungene Pässe Werte aus der richtigen Perspektive.
- [x] Die Engine-Starts pro Auftrag sind konstant. Ziel für die Anfangsstellung ist ein Auswertungsprozess plus eine feste Zahl von Versionsprüfungen, nach dem Zwischenschritt höchstens 21 Auswertungen und 2 Prüfungen zuzüglich der Prüfung bei Auftragserstellung. Open-Sage-Würfelaufträge liefern unverändert Ergebnisse. Timeout, Programmwechsel, abgebrochener Batch und verlorene Lease verhindern die Veröffentlichung unvollständiger Ergebnisse. Persistierte Ergebnisse werden bei erneutem Öffnen wiederverwendet.
- [x] Messung C vor und nach der Änderung bei 1-ply und der üblichen Profiltiefe ausführen. Zusätzlich einen nativen Pass-Fall und `e2e/dice-equity.spec.ts` prüfen. Ausgabe-, Zeit- und Auftragsgrenzen anhand eines reproduzierbaren Fehlers testen. *Ergebnis Batch am 19. November 2026 (M4 Pro, GNUbg 1.08.003, je 5 frische Aufträge, Load-Average 6–10): 3 Versionsaufrufe inklusive Auftragsanlage und **1 Auswertungsprozess** statt 43 und 21. Bis zum Abschluss bei 1-ply **Median 0,61 s (0,59–0,63)** statt 7,6 s, bei 3-ply **1,79 s (1,77–1,81)** statt 6,9 s. Der beobachtete Fortschritt stieg bei 3-ply in 5-%-Schritten. Fehlerfälle: Versionswechsel im Batch (`dice_job_discards_all_rolls_when_the_version_changes_during_the_batch`), fehlender Wurf (`dice_job_fails_without_a_result_when_gnubg_skips_a_roll`) und abgebrochener Fortschritt wie bei verlorener Lease (`real_gnubg_roll_batch_stops_when_progress_cannot_be_saved`). `e2e/dice-equity.spec.ts` besteht (8/8).*

**Nachmessung zur Fortschrittsausgabe.** Auf `main` (`0c8d0d0`) durchsuchte jede Leseoperation erneut den gesamten Ausgabepuffer nach Wurfmarkern. Zusätzliche Marker konnten außerdem schon vor der Ergebnisprüfung vollständigen Fortschritt melden. Der Beobachter liest jetzt nur neu hinzugekommene Bytes mit dem für geteilte Marker nötigen Überlapp. Der letzte Wurf wird erst nach Prüfung der Batch-Ausgabe gemeldet. Die Regressionstests teilen Marker an unterschiedlichen Byte-Grenzen, überschreiten das Ausgabelimit von 8 MiB und erzwingen die skalierte Frist mit zwei Würfen zu je 500 ms. Der Timeout-Test prüft auch, dass der Prozess beendet und eingeholt wurde. Der Produktionswert bleibt 90 Sekunden je Wurf für die Berechnung der gesamten Batch-Frist.

Der synthetische Release-Benchmark spielt 21 Marker in festen 8-KiB-Stücken ab und vergleicht fünf Läufe pro Puffergröße. Auf dem Mac mini M4 Pro sank der Median der reinen Scanzeit bei 80 KiB von **1,170 auf 0,052 ms** und bei 8 MiB von **5.597 auf 2,82 ms**.

```bash
cargo test --release -p neondojo --lib engine::tests::benchmark_dice_progress_replay \
  -- --ignored --exact --nocapture
```

Messung C auf demselben Rechner mit macOS 27.0.1, PostgreSQL 16.13 und GNU Backgammon 1.08.003 bei einem Thread. Je Stand und Tiefe wurden fünf frische Aufträge in einer isolierten Datenbank abgeschlossen. Bei 1-ply lag der Median bei **568 → 574 ms** (560–633 → 556–589 ms), bei 3-ply bei **1.786 → 1.789 ms** (1.721–1.799 → 1.746–1.850 ms). Die Spannweiten überlappten. Jeder Auftrag lieferte die erwarteten 21 Wurfpaare mit Gesamtgewicht 36 und startete drei Versionsprüfungen und einen Auswertungsprozess.

## P1 · Analyse-Navigation

Heute bindet `useAnalysisCatalog` die geladene Seite an `[matchId, positionId]`. Bei jedem Zugwechsel ist `analysisId` deshalb kurz `undefined`. `useMoveAssessments` leert dabei die Zugfarben und lädt sie neu, und die Statistik lädt neu, auch wenn sie ausgeblendet ist. In `useAnalysisCatalog` die wirksame Auswahl und Quellrevision pro Match halten. Positionsabhängige Katalogseiten und Kandidaten haben eigene Schlüssel. Spielbewertungen und Statistik nach Match, Analyse-ID, tatsächlichem Spielbereich, Schwellenwerten und Ergebnisrevision aktualisieren. Auf Matchwechsel, Auswahländerung und Quellenersatz weiterhin gegen verspätete Antworten absichern. [Auswahl-Hook](../../../web/src/useAnalysisCatalog.ts#L11) · [Zugbewertungen](../../../web/src/useMoveAssessments.ts#L93) · [Statistik](../../../web/src/MatchAnalysisPanel.tsx#L263)

**Umgesetzt (19. November 2026).** Die Katalogantwort liefert `selected`, `selected_id` und `source_revision` je Match; nur `items` hängt von der Position ab. `useAnalysisCatalog` merkt sich deshalb die letzte Seite je Match und liefert Auswahl, Auswahl-ID und Quellrevision daraus, solange die Seite der neuen Position lädt. Eigene Schreibvorgänge (Auswahl, Löschen) leeren diese Erinnerung wie bisher die Seite, ein Fehler der aktuellen Seite blendet sie aus, und ein anderer Match nutzt sie nie. Damit bleiben `analysisId` und die Schlüssel von `useMoveAssessments` und der Statistik über Zugwechsel stabil; Spielwechsel, Checkpoints, Schwellenwerte und Ergebnisrevision ändern sie weiterhin. Der Test `stepping through moves keeps move colors and statistics without reloading them` in `e2e/game-selection.spec.ts` zählt beim Vor- und Zurückschalten um je zehn Züge mit dem alten Hook **42**, mit dem neuen **0** Requests an `move-assessments` und `analysis-statistics`. Die Zugfarben fehlen in keinem Zwischenzustand, und für jede neue Position wird das Analyseergebnis geladen.

- [x] Nach geladenem Match in einem Spiel zehn Züge vor- oder zurückwechseln. Playwright zählt für unveränderte Analyse und unverändertes Spiel **0** weitere Requests an `move-assessments` und `analysis-statistics`. Die Zugfarben der Zughistorie bleiben dabei ohne leeren Zwischenzustand sichtbar. Kandidaten und Brett folgen weiterhin dem Zug.
- [x] Beim Wechsel der Analyse oder der Quelle erscheinen nur passende Ergebnisse. Ein neues Spiel aktualisiert die Bewertungen seines Spielbereichs. Nach einem neuen Checkpoint aktualisiert sich die Statistik. Nach künstlich verzögerten Antworten bleiben Auswahl, Zugfarben und Ergebnisse konsistent.
- [x] `e2e/analysis-catalog.spec.ts` und `e2e/game-selection.spec.ts` erweitern. Anzahl und Antwortumfang der betroffenen Requests mit Playwright erfassen. Den sichtbaren Wechsel der Position auf den Referenz-Desktopgrößen kontrollieren.

**Abschluss am 21. November 2026.** Die Quellrevision invalidiert die abhängigen Ansichten auch bei gleicher Analyse-ID. Folgeseiten dürfen Auswahl und Checkpoints der ersten Katalogseite nicht überschreiben. Der Katalog fragt aktive ausgewählte Analysen auch außerhalb der ersten Seite weiter ab. Der Playwright-Abnahmelauf umfasst die verzögerte Antwort einer vorherigen Auswahl, den Wechsel des Spielbereichs und den Wechsel von einem auf zwei gespeicherte Checkpoints. Der Zugwechsel wurde bei 1800 × 1169, 2560 × 1440 und 3840 × 2160 geprüft.

**Nachmessung gegen `main` (`d978649`).** Google Chrome mit isoliertem PostgreSQL und `WC2014.xg` (10 Spiele, 460 Zugstellungen, Spiel 3 mit 30 Zügen). Gezählt werden `move-assessments` und `analysis-statistics`. Nach dem Warmlauf verursachen 20 Zugwechsel in beiden Ständen **0 Requests und 0 Antwortbytes**. Nach einem gezielten Anheben der Quellrevision in der Testdatenbank bei gleicher Match- und Analyse-ID unterbleibt das Neuladen auf `main` (**0 Requests und 0 Antwortbytes**, erwarteter Testfehler). Mit der Änderung lädt die Ansicht beide Endpunkte einmal (**2 Requests und 16.946 Antwortbytes**). Die Messung erfasst Browser-Requests und Antwortkörper ohne Header.

Zur Wiederholung den erweiterten Test in einem separaten Checkout für beide Stände verwenden. Auf dem Ausgangsstand ausschließlich die Testdatei übernehmen. Playwright startet für jeden Lauf eine eigene Testdatenbank gemäß [Testanleitung](../../development/testing.md#browser-tests).

```bash
npm run test:e2e --prefix web -- e2e/game-selection.spec.ts \
  --grep 'stepping through moves' --workers=1 --reporter=line
npm run test:e2e --prefix web -- e2e/analysis-catalog.spec.ts \
  e2e/game-selection.spec.ts --workers=1 --reporter=line
```

### Spielwechsel und aufgezeichnete Würfe

Auf `main` (`da42b9c`) wartete der Spielwechsel auf die Stellungen und die aufgezeichneten Würfe. Jetzt stehen Brett und Zughistorie nach dem Laden der Stellungen zur Verfügung. Die Statistik lädt beim Öffnen von **Spielwürfe** und bleibt beim Wechsel zwischen Auswertungstabs desselben Spiels verfügbar. Ein Spielwechsel verwirft die ausstehende Antwort. Fehler und erneuter Abruf bleiben im Tab.

Die Messung verwendet `WC2014.xg` mit 10 Spielen und 460 Zugstellungen. Nach einem Warmlauf wechselt der Test je Szenario zehnmal zwischen Spiel 3 und Spiel 4. Er misst im Browser vom `change`-Ereignis bis zum ersten Animationsframe, in dem die Überschrift am Brett das gewählte Spiel zeigt und der aktuelle Zug in der Zughistorie markiert ist. Pro Lauf zählt er die Anfragen an den Würfelstatistik-Endpunkt. Umgebung sind ein Mac mini M4 Pro mit macOS 27.0.1, Google Chrome 158 im Headless-Modus, das Playwright-Testbackend im Debug-Build und ein isoliertes PostgreSQL 16.13 auf Loopback.

| Verzögerung der Würfelantwort | HTTP-Anfragen vorher → nachher | Browser-Median vorher → nachher | Spannweite vorher → nachher |
| --- | ---: | ---: | ---: |
| Keine zusätzliche Verzögerung | 10 → 0 | 42,8 → 41,2 ms | 36,8–90,1 → 39,1–117,1 ms |
| 250 ms zusätzlich | 10 → 0 | 347,8 → 38,8 ms | 316,0–414,7 → 36,7–40,7 ms |

Die Spannweiten ohne künstliche Verzögerung überlappen. Im zweiten Szenario hält Playwright nur die Statistikantwort zurück. Die Zeitwerte erfassen den Browserablauf. Die Messung verwendet keine neue Engine-Auswertung.

Zur Wiederholung denselben Test in beiden Checkouts verwenden und auf dem Ausgangsstand nur die Testdatei übernehmen. Dort melden die Prüfungen für null Würfelabrufe die Regression, nachdem beide Messreihen protokolliert wurden. Der zweite Test prüft den ersten Abruf, die Wiederverwendung, eine verspätete Antwort nach Spielwechsel und die Wiederholung nach einem HTTP-Fehler. Playwright nutzt pro Lauf eine eigene Testdatenbank gemäß [Testanleitung](../../development/testing.md#browser-tests).

```bash
npm run test:e2e --prefix web -- e2e/observed-rolls-navigation.spec.ts \
  --workers=1 --reporter=line
```

## P2 · Nummerierte Fehlerliste

Aus dem gemeinsamen Review-Pfad eine schmale Relation für validierte Treffer und einen Detailpfad für die gewählte Seite ableiten. Für Importe die revisionsgebundene Projektion als Ausgangspunkt prüfen. Der Cache `imported_analysis_projections` wird heute nur für Checker-Entscheidungen gespeicherter Positionen gefüllt. Eine Wiederverwendung setzt daher voraus, dass er alle Entscheidungen des Matches einschließlich Cube-Kontext abdeckt, oder einen eigenen Pfad für die übrigen. PostgreSQL führt Sortierung, Trefferzählung und Seitenauswahl unter derselben Lesesicht aus. Wendet ein Filter die aktuellen Schwellenwerte an, dieselbe Klassifikation wie im bestehenden Review verwenden. [Seitenschleife](../../../backend/src/review_decisions.rs#L380) · [Importprojektion](../../../backend/src/imported_analysis/projections.rs#L42)

**Umgesetzt (19. November 2026).** Zeitverteilung vorher (WC2014, 738 importierte Entscheidungen, 4,2 MB Analyse-JSON): Eine nummerierte Seite führte die Review-Abfrage in zwei Blöcken über alle Zeilen aus (29 + 23 ms laut Statement-Log, jeweils mit vollem JSON); die Projektion aller Zeilen kostete in Release ~10 ms. `imported_analysis_projections` wird nicht wiederverwendet, weil es die vollständige Entscheidung als JSON und nur Checker gespeicherter Positionen enthält. Stattdessen speichert Migration `0082_review_hits` je Match und Analyse die gültigen Treffer (Ordnungsschlüssel, Spieler, `evaluated`, Verlust) in `review_hit_sets`/`review_hits`. `build_hits` erzeugt sie mit `project()` unter einer REPEATABLE-READ-Sicht und Advisory-Lock; der Fingerabdruck umfasst Projektionsversion, Analyse, Quellrevision, Format, Katalog-Flags, Anzahl und ID-Summe der nativen Zeilen, Checkpoints und die Zahl der Importdatensätze. Statement-Trigger verwerfen Sätze bei `UPDATE` auf `move_analyses` und `imported_analyses`. Die Tabellen stehen in `DERIVED_TABLES` des Backups. Nummerierte Seiten bis 100 Einträge vergleichen den Fingerabdruck in ihrer Lesesicht, wählen und zählen über die schmale Relation mit derselben Klassifikation (`review_severity`, `severity_selected`) und projizieren nur die Seite über `REVIEW_SQL` mit ID-Filter. Ist der Satz veraltet, wird er einmal gebaut und die Anfrage in neuer Sicht beantwortet; stimmt die Seite nicht mit dem Satz überein, gilt der bisherige Scan. Cursor-Seiten und Batch-Verbraucher scannen wie bisher.

Messung B, je drei abwechselnde Runden à 12 warme Läufe auf getrennten Datenbanken: nummeriert erste Seite **~48 → ~11 ms**, zehnte **~48 → ~10 ms**, Fehlerseite **~47 → ~12 ms** (−75 bis −78 %), Cursor unverändert ~27 ms. Die Antworten waren nach Normalisierung der IDs identisch. SQL-Pläne: schmale Auswahl 0,22 ms (27 Puffertreffer), Detailabfrage für 50 Treffer der zehnten Seite 6,3 ms. Der erste Aufbau des Satzes kostet etwa eine bisherige Seite (~92 ms bei laufender Fremdlast). Tests: `numbered_pages_follow_the_cursor_order_for_every_filter` vergleicht für zehn Filter alle nummerierten Seiten mit der Cursor-Folge; `numbered_pages_follow_native_checkpoints_and_in_place_updates` deckt einen unvollständigen nativen Checkpoint, einen neuen Checkpoint und eine In-place-Änderung ab.

- [x] Nummerierte und Cursor-Abfragen liefern je Filter aus denselben gespeicherten Daten dieselbe Folge stabiler IDs wie zuvor. Bei neu importierten Vergleichsdaten die Entscheidungen über Spiel, Ordnungsnummer und Art vergleichen. Gesamtzahl, letzte und leere Seite, Cube-vor-Checker-Reihenfolge sowie Sichtbarkeit von erzwungenen, fehlenden und unvollständigen Entscheidungen bleiben korrekt. Ein Cursor für alte Checkpoints oder Filter wird wie bisher abgewiesen.
- [x] Die Detailprojektion verarbeitet pro nummerierter Seite höchstens die Treffer der Seite. Zählung und Auswahl berücksichtigen weiterhin jede gültige Zeile. Vorgeschlagenes Messziel für B sind mindestens 30 % niedrigere Mediane für die erste, späte und gefilterte nummerierte Seite. Den Cursor-Median mitberichten und Verschlechterungen anhand mehrerer Läufe prüfen.
- [x] `analysis_catalog`, `move_assessments` und den vorhandenen isolierten `explain_real_xg_review_page`-Test ausführen. Einen nativen Fall mit unvollständigem Checkpoint ergänzen. Messung B wiederholen und die SQL-Pläne zusätzlich sichern.

**Nachmessung am 21. November 2026.** Beim Wechsel zwischen nummerierten Seiten und Cursor-Seiten konnte PostgreSQL für das gemeinsame, dauerhaft vorbereitete `REVIEW_SQL` auf einen generischen Plan wechseln. Im langsamen Fall protokollierte PostgreSQL über 210 ms für einen einzelnen Detail-SELECT. Der Kontrolllauf mit `plan_cache_mode=force_custom_plan` brachte nummerierte Seiten wieder auf 11–13 ms. Die Abfrage verwendet jetzt an allen drei Aufrufstellen ein nicht persistentes SQLx-Statement. PostgreSQL kann damit für die konkrete Quelle, den Cursor und die Treffer-IDs planen. Der SQLx-Regressionstest prüft auf einer einzelnen Verbindung die Ergebnisparität beim Wechsel der Seitentypen und die Abwesenheit eines dauerhaft vorbereiteten Review-Statements.

Rezept B mit unverändertem `plan_cache_mode` auf `main` (`2cb191e`) und mit diesem Patch. Drei abwechselnde Runden mit separaten Testdatenbanken und je zwölf warmen HTTP-Anfragen pro Fall und Runde. Je Testfall stimmten Reihenfolge und Fachdaten aller 50 Treffer in sämtlichen Läufen überein. Die ungefilterte Gesamttrefferzahl betrug 738, bei Fehlern 74.

| HTTP-Latenz | Median vorher | Median nachher | p95 vorher | p95 nachher |
| --- | ---: | ---: | ---: | ---: |
| Nummeriert, erste Seite | 213,4 ms | 12,0 ms | 230,2 ms | 13,3 ms |
| Nummeriert, zehnte Seite | 215,6 ms | 11,5 ms | 229,2 ms | 13,1 ms |
| Cursor, erste Seite | 66,0 ms | 29,1 ms | 69,1 ms | 31,8 ms |
| Nummeriert, Fehlerseite | 218,0 ms | 12,4 ms | 233,2 ms | 14,2 ms |

Auf `d3a21e1` lieferte derselbe Benchmark Mediane von 123,0 / 124,1 / 65,2 / 123,2 ms. Der dort separat gemessene erste SQL-Batch las 513 Zeilen in 9,6 ms.

## P2 · Open Sage unter Hintergrundlast

Die gemeinsame Sperre ist laut Codekommentar gewollt: Spiel- und Analyseslot teilen sie, damit die Auswertungsparallelität der Engine und das Benchmark-Gate unverändert bleiben. Eine Spielanfrage wartet deshalb bis zum Ende eines ganzen Spielblocks, und ihr Timeout beginnt erst danach. Den Abtausch zwischen Zuglatenz und CPU-Budget vor der Umsetzung als Produktentscheidung klären. Zuerst die Zeit bis zum Erwerb der Worker-Sperre und die reine Auswertungszeit separat erfassen. Bevorzugter Entwurf sind getrennte Spiel- und Analyseslots mit gemeinsamem CPU-Budget und einer Frist für die gesamte interaktive Anfrage. Falls das Budget gleichzeitige Auswertung ausschließt, verifizierte Analyseblöcke verkürzen und interaktive Anfragen zwischen Blöcken vorziehen. [Gemeinsame Sperre](../../../backend/src/engines/process.rs#L14) · [Batch-Integrität](../../../backend/src/engines/process.rs#L207) · [Play-Pfad](../../../backend/src/play/open_sage.rs#L248)

**Umgesetzt (19. November 2026).** Produktentscheidung: getrennte Spiel- und Analyseslots mit gemeinsamem CPU-Budget. `process.rs` ersetzt den gemeinsamen `WORKER` durch die Sperren `GENERAL` (Spielblöcke, sonstige Anfragen, Probes, Neustarts) und `PLAY` (Bot-Züge, Warm-up). Benchmarks bleiben über `with_shared_access`/`benchmark_exclusive` exklusiv. Die Frist von `request_costed` umfasst jetzt das Warten auf den Slot. Mit den Standardwerten (4 Analyse-, 2 Spiel-Threads) belegen beide zusammen 6 von 14 Kernen des M4 Pro; eine Summenprüfung über die Einstellungen ist nicht umgesetzt.

Messung (Engine-Schicht, Open Sage `2.0.20261119`, Bot 1-ply mit 2 Threads in der Anfangsstellung über 15 Würfe, warmer Worker; Analyse der WC2014-Referenzpartie mit 4 Threads in Schleife): ohne Hintergrundlast ~48 ms Median. Alt während 3-ply-Analyse (~3 s je Spielblock) **3.855 ms Median, 4.304 ms p95**; die reine Auswertung meldete der Worker mit unter 1 ms, der Rest war Warten auf die Sperre. Neu während 3-ply-Analyse **33 ms Median, 34 ms p95**, während 4-ply-Analyse (22,8 s je Spielblock) ebenfalls **33 / 34 ms**. Die alte 4-ply-Messung wurde nach über 10 Minuten abgebrochen.

- [x] Ein deterministischer Test hält einen Analyseblock offen und stellt gleichzeitig eine Spielanfrage. Bei verfügbarem Budget liefert sie vor Abschluss der Analyse ein gültiges Ergebnis. Bei serieller Auswertung erhält sie den nächsten Blockwechsel. Ein eigener Sättigungstest prüft die dokumentierte Frist einschließlich Wartezeit. Batch-Integrität, atomare Spiel-Checkpoints, Worker-Bereinigung und exklusive Benchmarks bleiben abgesichert.
- [x] Auf isoliertem Backend identische Open-Sage-Züge mit warmer Engine einmal ohne Hintergrundlast und einmal während einer laufenden Spielanalyse wiederholen. Threadlimits, Tiefe, Teststellung, Median und p95 für Wartezeit und Gesamtantwort festhalten. Danach dieselbe Gegenprobe mit dem neuen Ablauf fahren. Die Codeprüfung lieferte dafür noch keine gemessene Baseline. *Gemessen an der Engine-Schicht statt über das HTTP-Backend: Die Spielanfrage geht direkt über `SageEngine::for_play`, ohne GNUbg-Regelprozess und Sitzung. Tests: `moves_finish_while_an_analysis_block_holds_the_general_worker` hält einen Spielblock offen, die Spielanfrage endet sofort, eine allgemeine Anfrage wartet und endet an ihrer Frist; `a_move_ends_at_its_deadline_including_the_wait_for_the_play_worker` prüft die Frist inklusive Wartezeit und die Bereinigung des blockierten Workers.*

### Frist bei exklusiven Benchmarks

Auf `main` (`9e7c3c8`) begann der Timer einer NeonGammon-Einzelanfrage erst nach Erwerb des gemeinsamen Engine-Zugangs. Die eigentliche Spielaktion hielt diesen Zugang bereits über die gesamte Zugfolge. Jetzt umfasst die Frist in beiden Fällen auch die Wartezeit vor dem Start. Der native Teil einer Spielaktion hat insgesamt 180 Sekunden für den Zugang und die Verarbeitung durch GNUbg und NeonGammon. Bei Fristablauf vor dem Start bleibt der wartende Auftrag ohne Worker-Aufruf. Der Benchmark bleibt exklusiv und kann weiterhin seine eigenen Engine-Aufrufe ausführen.

Nachmessung auf einem Mac mini M4 Pro mit macOS 27.0.1 und Rust-Debug-Build. Ein warmer Python-3.14.7-Testworker beantwortet je sieben Identitätsanfragen über den Spiel-Worker. Die Frist beträgt 60 ms. Im zweiten Szenario hält eine Testaufgabe den exklusiven Engine-Zugang jeweils 250 ms. Gemessen wird die Dauer bis zur Rückgabe aus dem Rust-Adapter. Der Test benötigt keine Datenbank, keine Engine-Gewichte und keine HTTP-Anfrage.

| Szenario | Median vorher → nachher | Spannweite vorher → nachher | Ausgeführte Anfragen vorher → nachher |
| --- | ---: | ---: | ---: |
| Zugang frei | 0,20 → 0,21 ms | 0,18–0,27 → 0,18–0,29 ms | 7 → 7 |
| Zugang exklusiv belegt | 254,25 → 62,22 ms | 252,86–254,67 → 60,87–62,31 ms | 7 → 0 |

Zur Wiederholung auf `9e7c3c8` nur die unten genannte Benchmark-Funktion aus `backend/src/engines/process.rs` übernehmen und in beiden Checkouts denselben Befehl ausführen. Der Ausgangsstand meldet den erwarteten Testfehler nach der Ausgabe beider Messreihen. Der reguläre Abnahmetest `play_request_deadline_includes_exclusive_benchmark_wait` hält den Zugang bis zur Antwort oder einem separaten Watchdog blockiert. Er prüft, dass der vorhandene Spiel-Worker erhalten bleibt und der Benchmark darin weiterhin eine eigene Anfrage ausführen kann.

```bash
cargo test -p neondojo --lib \
  engines::process::tests::benchmark_play_request_gate_wait \
  -- --ignored --exact --nocapture
```

### Gemeinsames Threadbudget für NeonGammon

Auf `main` (`44a9b05`) konnten die beiden Worker gleichzeitig mehr konfigurierte Threads beanspruchen als der Rechner logische CPUs hat. Jetzt reserviert jede Auswertung das Threadbudget ihres gespeicherten Profils. Passen beide Anforderungen in das Budget, laufen sie parallel. Bei Überbelegung erhält eine wartende Spielanfrage die nächste Freigabe zwischen Einzelauswertungen. Der Analyse-Worker behält dabei seinen verifizierten Spielblock. Die Wartezeit zählt zur bestehenden Zugfrist. Profile, die von einem Rechner mit mehr logischen CPUs stammen, behalten ihre gespeicherte Threadzahl und reservieren das gesamte Budget, solange sie laufen.

Der Abnahmetest hält eine einzelne Auswertung in einem verifizierten Block offen. Er prüft erlaubte Parallelität, Fristablauf bei Überbelegung und die Wiederverwendung des warmen Spiel-Workers. Sobald Threads frei werden, kommt eine bereits wartende Spielanfrage vor der nächsten Analyseauswertung zum Zug. Der Test deckt auch ein Profil mit mehr Threads als logischen CPUs ab.

Messung auf einem Mac mini M4 Pro mit 14 logischen CPUs, macOS 27.0.1, Rust-Debug-Build und NeonGammon 3.1.0. Je Szenario stellt der Spiel-Worker nach kurzem Vorlauf 20 Checker-Anfragen aus Spiel 3 von `WC2014.gnubg.sgf`, während der Analyse-Worker dasselbe Spiel zweimal vollständig bei 3-ply analysiert. Beide Worker sind vor der Messung warm. Die Spielanfragen nutzen ebenfalls 3-ply. Die Spielzeit misst die Antwort am Rust-Adapter, die Engine-Zeit stammt aus dem Sidecar für die reine Auswertung. Die Analysezeit umfasst beide Spiele. Die Play- und Analyse-Prüfsummen waren für alle vier Läufe identisch.

| Analyse + Spiel | Spiel-Median vorher → nachher | Spiel-p95 vorher → nachher | Engine-Median vorher → nachher | Beide Analysen vorher → nachher |
| --- | ---: | ---: | ---: | ---: |
| 4 + 2 Threads | 193,26 → 191,76 ms | 516,88 → 507,22 ms | 160,75 → 157,90 ms | 6,97 → 6,89 s |
| 14 + 2 Threads | 203,49 → 202,36 ms | 467,28 → 477,89 ms | 177,13 → 146,62 ms | 3,62 → 7,39 s |

Bei 14 + 2 begrenzt die Reservierung die gleichzeitige Worker-Auslastung von 16 auf 14 konfigurierte Threads. Der Spiel-Median blieb ähnlich und der p95 stieg in diesem Lauf leicht. Die beiden Hintergrundanalysen dauerten länger, da die Worker bei dieser Kombination abwechselnd Auswertungen ausführen. Die Messung zählt konfigurierte Threadbudgets und erfasst keine Betriebssystem-Threadbelegung.

Zur Wiederholung auf `44a9b05` die Benchmark-Funktion in das Testmodul von `backend/src/engines/process.rs` übernehmen. Beide Läufe separat mit demselben 3.1.0-Interpreter und ohne parallele Builds ausführen.

```bash
NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python cargo test -p neondojo --lib \
  engines::process::tests::benchmark_play_with_analysis_thread_budget \
  -- --ignored --exact --nocapture
```

## P3 · Kleine Arbeitspakete

| Paket | Änderung | Abnahme |
| --- | --- | --- |
| Gespeicherte Positionsnotizen · *umgesetzt am 20. November 2026: `load_notes` liest die Notizen aller Positionen der Seite mit `p.id = ANY($1)` und gruppiert je Position, wie `load_deck_memberships`. 100 gespeicherte Positionen mit 50 Notizen auf WC2014: 26,5 → 22,2 ms Median bei identischer Antwort; 1 statt 100 Notizabfragen je Seite.* | Einen Batch über die Positions-IDs der Seite ausführen und nach Position gruppieren. [Notizabfrage](../../../backend/src/position_browser.rs#L758) | Eine Notizabfrage für eine volle Seite. Match-, Spiel-, Positions- und Zugnotizen behalten Zuordnung und Reihenfolge. Der vorhandene Test `notes_belong_to_the_exact_game_position_and_checker_move` besteht. |
| Spielvalidierung · *umgesetzt am 20. November 2026: `store::game_catalog` liefert den Katalog (bei fehlendem Katalog wie bisher einmalig aus dem SGF aufgebaut); `match_games` reichert nur noch für `/games` die Ergebnisse aus dem SGF an. `validate_game` parst kein SGF mehr. WC2014: `/api/positions?game_no=3` 4,9 → 1,2 ms, `/api/matches/{id}/analysis?game_no=3` 4,7 → 0,9 ms Median bei identischen Antworten.* | Existenz und Spielscope aus dem Spielekatalog lesen. Aufbereitung von altem SGF bei fehlendem Katalog wiederverwenden. [Validierung](../../../backend/src/api.rs#L437) | Auf katalogisierten Matches verarbeitet die Scope-Prüfung kein SGF. Gültige, unbekannte und reine Cube-Spiele sowie HTTP-Fehler verhalten sich wie zuvor. `e2e/game-selection.spec.ts` besteht. |
| Jobüberwachung · *umgesetzt am 20. November 2026: `web/src/job-watch.ts` führt je Job-ID eine gemeinsame Überwachung für `useJobs`, `useMatchAnalysis` und `useBackgroundJobs`. Sichtbare Ansichten öffnen einen Stream; Fortschritt kommt aus den SSE-Daten und wird nur übernommen, wenn er zum Versuch des letzten `running`-Events gehört und nicht zurückgeht. Vollständige Lesevorgänge nur zu Beginn, am Ende, nach Verbindungsfehlern und alle 15 s als Absicherung. Nur global verfolgte Aufträge (Bulk-Analysen) lesen weiter alle 3 s, weil Browser über HTTP/1.1 nur wenige Verbindungen je Server erlauben. 3-ply-Würfelauftrag mit 21 Fortschrittsereignissen: 25 → 3 vollständige GETs, 1 Stream (`e2e/job-watch.spec.ts`); Replay, Versuchswechsel, geteilte Überwachung, Wiederverbindung, geschlossener Stream, gelöschter und fertiger Auftrag in `job-watch.test.ts`.* | Je Job-ID eine Überwachung für globale und lokale Anzeigen nutzen. Fortschritt aus SSE übernehmen, vollständiges Ergebnis bei Abschluss oder erforderlicher Synchronisierung abrufen. Ohne `Last-Event-ID` spielt der Server beim Verbinden alle bisherigen Events ab, und heute löst jedes davon einen vollständigen GET aus. Serverseitig fragt jede SSE-Verbindung alle 350 ms `job_events` und den Job ab. Weniger Verbindungen entlasten also auch PostgreSQL. [SSE-Endpunkt](../../../backend/src/api.rs#L867) · [Lokale Überwachung](../../../web/src/useJobs.ts#L76) · [Globale Überwachung](../../../web/src/useBackgroundJobs.ts#L32) | Eine SSE-Verbindung pro beobachtetem Job. Ein Replay mit 21 Fortschrittsereignissen löst keine Serie vollständiger GETs aus. Wiederverbindung, verpasste Events, terminale Zustände und Fehler sind abgedeckt. |
| Checkpoint-Schreibvorgang · *umgesetzt am 21. November 2026* | Entscheidungszeilen werden vor der Lease-Sperre vorbereitet und in Batches von bis zu 500 Zeilen eingefügt. Die Inserts mit variabler Zeilenzahl belegen keinen SQLx-Statement-Cache. [Checkpoint-Transaktion](../../../backend/src/match_jobs.rs#L142) | Ein Entscheidungs-Insert je Batch. Entscheidungen, Revisionsmarke, Zusammenfassung, Timing und Fortschritt bleiben atomar. SQLx-Tests decken die gespeicherte Vollanalyse, Lease-Ablauf, Wiederaufnahme, Rollback und leere Spiele ab. |

### Jobereignisse ohne häufige Leerlaufabfragen

Auf `main` (`6e52172`) fragte jeder offene SSE-Stream alle 350 ms die gespeicherten Ereignisse und den Auftrag ab. Migration `0090_job_event_notifications` meldet neue Ereignisse gesammelt je Auftrag nach dem Commit. Eine gemeinsame Listener-Verbindung weckt die betroffenen Streams. Jeder Stream liest weiterhin ab seiner letzten Ereignis-ID aus der Tabelle. Nach einer Wiederverbindung oder einem überlaufenen Benachrichtigungspuffer prüft der Stream die gespeicherten Zeilen erneut. Im Leerlauf prüft er vorsorglich alle zehn Sekunden. Der Listener nutzt eine eigene Verbindung außerhalb des Anfrage-Pools. Die Migration lässt das Backup-Format kompatibel.

Der Abnahmetest nutzt einen Anfrage-Pool mit nur einer Verbindung. Er prüft die Auslieferung nach dem Commit, beendet die Listener-Verbindung und speichert 106 Ereignisse einschließlich des Terminalereignisses ohne Benachrichtigung. Die Prüfung nach der Wiederverbindung liefert sie in ID-Reihenfolge über die 100er-Seitengrenze hinweg. Ein erneuter Abruf mit `Last-Event-ID` setzt an der erwarteten Stelle fort. Ein bereits empfangenes Terminalereignis beendet die Verbindung ohne erneute Zustellung.

Der Benchmark öffnet zwölf Streams für einen ruhenden Auftrag, wartet 500 ms nach dem Start und zählt für 2,1 Sekunden die Leseabfragen über `pg_stat_statements`. Umgebung sind macOS 27.0.1 auf einem Mac mini M4 Pro, ein Rust-Debug-Build und isoliertes PostgreSQL 16.13 auf Loopback. Die Leser rufen den Axum-Handler auf und lesen seine SSE-Frames. Die Messung enthält keine Engine-Arbeit und erfasst PostgreSQL-Ausführungszeit.

| Leerlaufmessung | Vorher | Nachher |
| --- | ---: | ---: |
| Ereignisabfragen | 72 | 0 |
| Auftragsabfragen | 72 | 0 |
| PostgreSQL-Ausführungszeit | 1,169 ms | 0,000 ms |

Zur Wiederholung `shared_preload_libraries=pg_stat_statements` auf dem isolierten Testserver setzen und `DATABASE_URL` gemäß [Testanleitung](../../development/testing.md#postgresql-test-server) bereitstellen. Auf `6e52172` nur die neuen Testfunktionen und ihre Hilfsfunktionen aus `backend/tests/workflow.rs` übernehmen. Dort meldet die Leerlaufprüfung nach der Messung erwartungsgemäß einen Testfehler.

```bash
cargo test -p neondojo --test integration workflow::benchmark_idle_job_event_streams \
  -- --ignored --exact --nocapture
cargo test -p neondojo --test integration workflow::job_events_replay_committed_rows_after_listener_disconnect \
  -- --exact
```

### Gebündelte Statusabfragen für Sammelanalysen

Auf `main` (`15a43de`) startete die gemeinsame Jobüberwachung für jeden ausschließlich im Hintergrund beobachteten Auftrag eine eigene Statusabfrage alle drei Sekunden. Jetzt fragt ein gemeinsamer Timer diese Aufträge in Gruppen von höchstens 100 IDs ab. `POST /api/jobs/status` liest eine Gruppe in einer PostgreSQL-Abfrage und prüft den Wettbewerbsschutz für dieselben Auftragstypen wie der Einzelabruf. Sichtbare Ansichten nutzen weiter ihren vorhandenen SSE-Stream, und ein allein beobachteter Auftrag nutzt den Einzelabruf. Fehlende Aufträge enden in der Überwachung. Vorübergehend gesperrte Aufträge werden weiter geprüft.

Der Vitest-Vergleich beobachtet 24 laufende Aufträge und zählt die initialen Requests sowie fünf weitere Abfragen mit einer simulierten Uhr. Er misst **144 → 6 HTTP-Aufrufe** bei jeweils **144 angeforderten Auftragsdatensätzen** und null SSE-Verbindungen. Der Test misst die Transportaufrufe bei simulierten Antworten. Er erfasst keine Netzwerklatenz.

Zur Wiederholung auf `15a43de` nur den Test `24 background jobs use one request per refresh` aus `web/src/job-watch.test.ts` übernehmen. Der Ausgangsstand gibt die 144 früheren Aufrufe aus und meldet den erwarteten Testfehler.

```bash
npm test --prefix web -- src/job-watch.test.ts -t '24 background jobs' \
  --disableConsoleIntercept
```

### Nachmessung zum Checkpoint-Schreibvorgang

Ausgangsstand `main` (`70af44a)), Mac mini M4 Pro (14 Kerne, 64 GB, macOS 27.0.1), PostgreSQL 16.13 auf Loopback, Rust-Debug-Build, GNU Backgammon 1.08.003 bei 1-ply. `WC2014.gnubg.sgf` enthält 10 Spiele und 460 Checkerzüge. Das längste Spiel (Nr. 8) hat 83 Checkerzüge und 138 gespeicherte Entscheidungen. Der Benchmark berechnet das Spiel einmal je Testlauf und speichert dasselbe Ergebnis je Runde in 5 neuen Aufträgen zum Aufwärmen und 30 neuen Aufträgen zur Messung.

| Messgröße je Spiel | Ausgangsstand | Mit Batch |
| --- | ---: | ---: |
| SQL-Inserts in `move_analyses` | 138 | 1 |
| Checkpoint-Zeit, Median | 27,6 ms | 15,7 ms |
| Checkpoint-Zeit, p95 | 28,9 ms | 16,2 ms |
| PostgreSQL-Ausführungszeit der Entscheidungs-Inserts, Mittelwert | 9,4 ms | 2,9 ms |

Die Zeitwerte für Median und p95 sind jeweils die mittleren Werte aus drei abwechselnden Runden pro Stand. Die Mediane lagen in den drei Läufen bei 27,5–27,7 ms und 15,6–15,9 ms. Die Checkpoint-Zeit umfasst das Lesen der Spielpositionen, die Vorbereitung der Zeilen und den Commit. Die Job-Zeitmessung des laufenden Arbeiters ist in diesem direkten Aufruf deaktiviert. GNUbg läuft vor dem Warmlauf. PostgreSQL zählt Statements und misst deren Ausführung über `pg_stat_statements`. Entscheidungen und Zusammenfassung hatten in allen sechs Läufen dieselbe Prüfsumme.

Zur Wiederholung `shared_preload_libraries=pg_stat_statements` auf einem isolierten PostgreSQL-Testserver einstellen. Die Testrolle muss die Extension in ihrer SQLx-Datenbank anlegen können. `DATABASE_URL` gemäß [Anleitung für den Testserver](../../development/testing.md#postgresql-test-server) setzen. Auf dem Ausgangsstand nur die [Testdatei](../../../backend/src/match_jobs/checkpoint_tests.rs) und ihre `#[cfg(test)]`-Moduldeklaration übernehmen. Beide Stände separat bauen und nacheinander mit derselben GNUbg-Installation messen.

```bash
cargo test -p neondojo --lib match_jobs::checkpoint_tests::benchmark_game_checkpoint \
  -- --ignored --exact --nocapture
```

## Abschluss je Arbeitspaket

- [ ] Fokussierte Tests zuerst ausführen. Danach bei Rust-Änderungen `cargo fmt --all --check` und `cargo clippy --workspace --all-targets -- -D warnings` ausführen. Bei Frontend-Änderungen `npm test --prefix web` und `npm run build --prefix web` ausführen. Betroffene ignorierte SQLx- und native Tests explizit starten. [Verifikationsanleitung](../../development/testing.md)
- [ ] Nach der Änderung dieselbe Messung wiederholen. Metriken mit Commit und Messbedingungen dokumentieren. Dokumentation aktualisieren, wenn sich Verhalten oder Betrieb ändern. Bei einem beauftragten Check auf Port 8787 erst bauen und dann mit den bestehenden Datenbank- und Engine-Einstellungen neu starten. [Lokaler Neustart](../../../README.md#restart-neondojo-locally-on-port-8787)

## Messrezepte

Je Messung den Vorbereitungsblock und genau einen der Blöcke A, B oder C zusammen als Bash-Skript aus der Wurzel des zu messenden Checkouts ausführen. Die Blöcke aus dieser Planfassung für beide Stände verwenden. Voraussetzung sind ein verfügbares `pg_config`, Rust, Python 3, `curl`, `jq` und GNUbg mit installierten Gewichten. Das Rezept baut in `target/perf`. Wer beide Release-Binaries vorab bauen möchte, kann dem Rezept über `NEONDOJO_PERF_BIN` den absoluten Pfad zum passenden Binary übergeben. Der Test prüft in beiden Fällen, dass das gestartete Backend den Commit von `HEAD` meldet. Der Start erfolgt mit frischer Datenbank, eigenem PostgreSQL-Cluster und Loopback-Ports. Beim Skriptende werden Backend und Testcluster gestoppt. SQL-Pläne, Messumgebung und Logs stehen im ausgegebenen Messordner.

### Vorbereitung

```bash
set -euo pipefail
# Ohne gültige Locale bricht PostgreSQL auf macOS beim Start ab
# ("postmaster became multithreaded during startup").
export LC_ALL=C
perf_root="$(git rev-parse --show-toplevel)"
cd "$perf_root"
perf_dir="$(mktemp -d /tmp/neondojo-perf.XXXXXX)"
export NEONDOJO_PERF_DIR="$perf_dir"
printf '\nMessordner %s\n' "$perf_dir"
perf_pg="$(pg_config --bindir)"
perf_pg_port="$(python3 - <<'PY'
import socket
with socket.socket() as pg:
    pg.bind(("127.0.0.1", 0))
    print(pg.getsockname()[1])
PY
)"
perf_gnubg="$(command -v "${GNUBG_BIN:-gnubg}")"
export NEONDOJO_PERF_GNUBG="$(python3 -c 'import os, sys; print(os.path.abspath(sys.argv[1]))' "$perf_gnubg")"
cat > "$perf_dir/gnubg-counted" <<'SH'
#!/bin/sh
printf '%s\n' "$1" >> "$NEONDOJO_PERF_DIR/gnubg-launches"
exec "$NEONDOJO_PERF_GNUBG" "$@"
SH
chmod +x "$perf_dir/gnubg-counted"

# Der Review-Stand verwendet noch bgtutor für das Paket und die Servervariablen.
perf_package="$(sed -nE 's/^name = "(bgtutor|neondojo)"$/\1/p' backend/Cargo.toml)"
case "$perf_package" in
  bgtutor|neondojo) ;;
  *) printf 'Unbekanntes Backend-Paket: %s\n' "$perf_package" >&2; exit 1 ;;
esac
perf_bin="${NEONDOJO_PERF_BIN:-}"
if [ -z "$perf_bin" ]; then
  cargo build --locked --release --package "$perf_package" --bin "$perf_package" --target-dir target/perf
  perf_bin="$perf_root/target/perf/release/$perf_package"
fi
case "$perf_bin" in
  /*) test -x "$perf_bin" ;;
  *) printf 'Der Binary-Pfad muss absolut sein.\n' >&2; exit 1 ;;
esac
"$perf_pg/initdb" -D "$perf_dir/db" -U neondojo_perf --no-locale --encoding=UTF8 --auth=trust > "$perf_dir/initdb.log"
"$perf_pg/pg_ctl" -D "$perf_dir/db" -l "$perf_dir/postgres.log" \
  -o "-h 127.0.0.1 -p $perf_pg_port -k $perf_dir" -w start
perf_app_pid=''
perf_stop() {
  if [ -n "$perf_app_pid" ]; then
    kill "$perf_app_pid" 2>/dev/null || true
    wait "$perf_app_pid" 2>/dev/null || true
  fi
  "$perf_pg/pg_ctl" -D "$perf_dir/db" -m fast -w stop
}
trap perf_stop EXIT

perf_database="${perf_package}_e2e_perf"
export DATABASE_URL="postgresql://neondojo_perf@127.0.0.1:$perf_pg_port/$perf_database"
"$perf_pg/createdb" -h 127.0.0.1 -p "$perf_pg_port" -U neondojo_perf "$perf_database"
(
  cd "$perf_dir"
  exec env -u NEONDOJO_WEB_DIR -u BGTUTOR_WEB_DIR \
    BGTUTOR_BIND=127.0.0.1:0 NEONDOJO_BIND=127.0.0.1:0 \
    BGTUTOR_E2E_DATABASE_NAME="$perf_database" NEONDOJO_E2E_DATABASE_NAME="$perf_database" \
    RUST_LOG="$perf_package=info" GNUBG_BIN="$perf_dir/gnubg-counted" "$perf_bin"
) > "$perf_dir/backend.log" 2>&1 &
perf_app_pid=$!
for attempt in $(seq 1 120); do
  perf_api_port="$(sed -nE 's/.*listening.*127\.0\.0\.1:([0-9]+).*/\1/p' "$perf_dir/backend.log")"
  if [ -n "$perf_api_port" ]; then break; fi
  if ! kill -0 "$perf_app_pid" 2>/dev/null; then
    printf 'Backend-Start fehlgeschlagen, siehe %s/backend.log\n' "$perf_dir" >&2
    exit 1
  fi
  sleep 0.15
done
test -n "$perf_api_port"
export NEONDOJO_PERF_URL="http://127.0.0.1:$perf_api_port"
# Der Build bettet den Commit ein. Uncommittete Änderungen bleiben unsichtbar;
# den Arbeitsbaum deshalb zusätzlich mit git status protokollieren.
test "$(curl -fsS "$NEONDOJO_PERF_URL/api/version" | jq -er '.version')" = "$(git rev-parse HEAD)"
git status --short > "$perf_dir/git-status.txt"
test "$("$perf_pg/psql" -XAt "$DATABASE_URL" -c 'SELECT current_database()')" = "$perf_database"
{
  git rev-parse HEAD
  uname -sm
  if command -v sw_vers >/dev/null; then
    sw_vers -productVersion
    sysctl -n machdep.cpu.brand_string
  fi
  "$perf_pg/psql" -XAt "$DATABASE_URL" -c 'SHOW server_version'
  "$NEONDOJO_PERF_GNUBG" --version
  uptime
} > "$perf_dir/environment.txt"
```

### A · Aktualitätsprüfung des Entscheidungsindex

Auf beiden Ständen dieselbe synthetische Fixture anlegen. Sie bildet 2.000 Matches, 50 % Spieler-Selektivität und 200 Entscheidungen je passendem Job nach. Die Analyse-JSONs sind synthetische Nutzlasten für den SQL-Plan. Die Kontrollvariante isoliert die Kosten des Hashs. Im Review auf PostgreSQL 16.13 dauerte der Kandidaten-SELECT bei 1.658 Textbytes und 714 gespeicherten Bytes je JSON **1,59–1,62 s**, die Kontrollabfrage **155–162 ms**. Das folgende Rezept erzeugte bei der Verifikation **1.670 Textbytes und 655 gespeicherte Bytes** im Mittel.

```bash
"$perf_pg/psql" -X -v ON_ERROR_STOP=1 "$DATABASE_URL" <<'SQL'
INSERT INTO matches(id, filename, format, sha256, white, black, match_length,
                    position_count, original, sgf, source_metadata_loaded,
                    moves_cataloged, imported_analysis_loaded)
SELECT md5('match/' || n)::uuid, 'perf-' || n || '.sgf', 'sgf',
       repeat(md5('perf/' || n), 2),
       CASE WHEN n <= 1000 THEN 'Perf Player' ELSE 'Other Player' END,
       'Opponent', 7, 0, ''::bytea, '', true, true, true
FROM generate_series(1, 2000) AS s(n);

INSERT INTO jobs(id, match_uuid, kind, plies, status, progress, total_games, completed_games)
SELECT md5('job/' || n)::uuid, md5('match/' || n)::uuid, 'match', 3, 'completed', 100, 1, 1
FROM generate_series(1, 1000) AS s(n);

INSERT INTO match_analysis_games(job_id, game_no)
SELECT md5('job/' || n)::uuid, 1 FROM generate_series(1, 1000) AS s(n);

INSERT INTO move_analyses(job_id, game_no, decision_no, player, kind, equity_loss, data)
SELECT md5('job/' || n)::uuid, 1, d, d % 2, 'checker', (d % 10) / 100.0,
       jsonb_build_object('evaluation_status', 'evaluated', 'played', '24/20 13/10',
                          'best', '24/21 13/9', 'candidates', payload.candidates)
FROM generate_series(1, 1000) AS s(n)
CROSS JOIN generate_series(1, 200) AS decisions(d)
CROSS JOIN LATERAL (
  SELECT jsonb_agg(jsonb_build_object('notation', '24/21 13/9', 'equity', 0.11,
                       'path', md5(concat_ws('/', n, d, c)),
                       'probabilities', ARRAY[0.54, 0.21, 0.02, 0.48, 0.20, 0.02])
                   ORDER BY c) AS candidates
  FROM generate_series(1, 11) AS candidates(c)
) AS payload;

ANALYZE;
SELECT count(*) AS decisions, round(avg(octet_length(data::text))) AS text_bytes,
       round(avg(pg_column_size(data))) AS stored_bytes
FROM move_analyses;
SQL

python3 - <<'PY'
import os
from pathlib import Path
import re
import subprocess

source = Path("backend/src/recurring_mistakes/index.rs").read_text()
candidates = re.search(r'const CANDIDATES: &str = "(.*?)";', source, re.S)
if candidates:
    scope = re.search(r'"\{CANDIDATES\}( AND \(\$2::uuid.*?)"', source, re.S)[1]
    query = candidates[1] + scope
    parameters = ("'1:perf'", "NULL", "ARRAY['perf player']")
else:
    query = re.search(r'let candidates: Vec<Candidate> = sqlx::query_as\(\s*"(.*?)",', source, re.S)[1]
    parameters = ("NULL", "ARRAY['perf player']", "'1:perf'")
query = re.sub(r"\$([123])\b", lambda match: parameters[int(match[1]) - 1], query)
pg = Path(subprocess.check_output(["pg_config", "--bindir"], text=True).strip())
dir_path = Path(os.environ["NEONDOJO_PERF_DIR"])

def sql(statement):
    return subprocess.check_output(
        [pg / "psql", "-XAt", "-v", "ON_ERROR_STOP=1", os.environ["DATABASE_URL"]],
        input=statement, text=True,
    )

sql(f"INSERT INTO decision_index_matches(match_id,fingerprint) SELECT id,fingerprint FROM ({query}) c; ANALYZE;")
counts = sql(
    "SELECT count(*), count(*) FILTER (WHERE stored IS DISTINCT FROM fingerprint),"
    " count(*) FILTER (WHERE split_part(fingerprint,'|',3)='native'"
    " AND split_part(fingerprint,'|',4)=j.id::text)"
    f" FROM ({query}) c LEFT JOIN jobs j ON j.match_uuid=c.id;"
)
assert counts.strip() == "1000|0|1000", counts
print("candidates|stale|expected native job", counts.strip())


def measure(name, statement):
    for sample in range(4):
        result = sql(f"EXPLAIN (ANALYZE, BUFFERS) {statement};")
        (dir_path / f"index-{name}-{sample}.txt").write_text(result)
        if sample:
            print(name, re.search(r"Execution Time: .*", result)[0])

# Beide Produktionsabfragen unter denselben Bedingungen messen.
measure("candidate", query)
json_hash = (
    r"\(SELECT coalesce\(sum\(hashtextextended\(a\.data::text,0\)\),0\)::text\s+"
    r"FROM move_analyses a WHERE a\.job_id=sel\.job_id\)"
)
if not candidates:
    control, replacements = re.subn(json_hash, "'control'", query)
    assert replacements == 1
    ids = "(SELECT count(*)::text||':'||coalesce(sum(a.id),0)::text FROM move_analyses a WHERE a.job_id=sel.job_id)"
    measure("control", control)
    # VACUUM füllt die Visibility Map für den Index-Only Scan.
    sql("VACUUM (ANALYZE) move_analyses;")
    measure("idsum", re.sub(json_hash, ids, query))
PY
```

Gegenprobe am 19. November 2026 auf `d3a21e1` (M4 Pro, PostgreSQL 16.13, gleiche Fixture): Kandidat **1,28–1,30 s**, Kontrolle **~110 ms** (ein Ausreißer 331 ms), ID-Summe **~121 ms**. Die absoluten Zeiten schwanken mit der Maschinenlast. Vergleiche deshalb innerhalb eines Laufs ziehen.

Für die Umsetzung dieselbe Datengröße und Verteilung mit dem neuen Schema laden. Einen SQLx-Messtest ergänzen, der die echten Anweisungen des neuen Aktualitätspfads mit denselben Parametern erfasst und getrennt von Seite und `COUNT` auswertet. Prüfen, dass 1.000 Kandidaten aktuell sind, bevor die Warmmessung beginnt. Mit einem gesonderten real analysierten Match die API-Antwort und die Aktualisierung nach einem Auswahlwechsel vergleichen.

### B · HTTP-Latenz der Review-Seiten

```bash
NEONDOJO_PERF_MATCH="$(
  curl -fsS -F 'file=@backend/tests/fixtures/WC2014.xg' "$NEONDOJO_PERF_URL/api/imports" |
  jq -er '.match.id'
)"
export NEONDOJO_PERF_MATCH
"$perf_pg/psql" -X -v ON_ERROR_STOP=1 "$DATABASE_URL" -c 'ANALYZE'
python3 - <<'PY'
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import time
from urllib.parse import urlencode
from urllib.request import urlopen

match = os.environ["NEONDOJO_PERF_MATCH"]
path = f'{os.environ["NEONDOJO_PERF_URL"]}/api/matches/{match}/review-decisions'
cases = {
    "numbered-first": {"page": 1},
    "numbered-tenth": {"page": 10},
    "cursor-first": {},
    "numbered-errors": {"page": 1, "errors_only": "true"},
}
samples = {name: [] for name in cases}
pages = {}
fields = ("game_no", "decision_no", "kind", "player", "played", "equity_loss")
for iteration in range(14):
    for name, filters in cases.items():
        query = urlencode(dict(analysis_id=f"imported:{match}", limit=50, **filters))
        start = time.perf_counter()
        with urlopen(f"{path}?{query}", timeout=30) as response:
            body = response.read()
        elapsed = (time.perf_counter() - start) * 1000
        data = json.loads(body)
        assert len(data["items"]) == 50
        if name.startswith("numbered"):
            assert data["total"] == (74 if name == "numbered-errors" else 738)
        # Vergleichbare Felder ohne zufällige Match-, Job- und Positions-IDs.
        page = [[item[field] for field in fields] for item in data["items"]]
        assert page == pages.setdefault(name, page), name
        if iteration >= 2:
            samples[name].append(elapsed)
for name, values in samples.items():
    print(f"{name} ms median={statistics.median(values):.1f} range={min(values):.1f}–{max(values):.1f}")
directory = Path(os.environ["NEONDOJO_PERF_DIR"])
(directory / "review-pages.json").write_text(json.dumps(pages))
(directory / "review-times-ms.json").write_text(json.dumps(samples))

# SQL-Messung der ungefilterten ersten Charge auf d3a21e1.
source = Path("backend/src/review_decisions.rs").read_text()
query = re.search(r'const REVIEW_SQL: &str = r#"([\s\S]*?)"#;', source)
git_diff = subprocess.check_output(
    ["git", "diff", "d3a21e1", "--", "backend/src/review_decisions.rs"], text=True,
)
if query and not git_diff:
    statement = (
        "PREPARE review(uuid,uuid,integer,integer,text,integer,integer,boolean,text,bigint,boolean)"
        f" AS {query[1]}; EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)"
        f" EXECUTE review('{match}',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,513,true);"
    )
    pg = Path(subprocess.check_output(["pg_config", "--bindir"], text=True).strip())
    output = subprocess.check_output(
        [pg / "psql", "-XAt", "-v", "ON_ERROR_STOP=1", os.environ["DATABASE_URL"]],
        input=statement, text=True,
    ).split("\n", 1)[1]
    (Path(os.environ["NEONDOJO_PERF_DIR"]) / "review-first-batch.json").write_text(output)
    plan = json.loads(output)[0]
    print("SQL batch rows and ms", plan["Plan"]["Actual Rows"], plan["Execution Time"])
PY
```

Review-Baseline in derselben Reihenfolge war **162,0 / 167,5 / 86,4 / 161,3 ms**. Die Fehlerseite wählte 74 von 738 Entscheidungen aus. Der erste ungefilterte SQL-Batch lieferte 513 Zeilen in **13,2 ms** bei 3.943 Treffern im Shared Buffer und 0 Reads. Die API-Messung umfasst auch die folgenden Abfragen und die Projektion. Nach der Umstellung die neuen produktiven Anweisungen für Zählung und Seite mit denselben Filtern messen. Der bestehende Test `explain_real_xg_review_page` prüft zusätzlich den SQL-Plan für Spiel 1, Spieler 0 und Checker. Gegenprobe am 19. November 2026: Mediane **124,5 / 124,7 / 66,2 / 123,3 ms**, SQL-Batch 513 Zeilen in **12,3 ms**. Die Einzelwerte streuten zwischen etwa 50 und 129 ms. Deshalb Median und Spannweite berichten. Die beiden Dateien `review-pages.json` mit `cmp` vergleichen, um Reihenfolge und Fachdaten der Treffer abzugleichen.

### C · GNUbg-Prozessstarts und Jobdauer

```bash
NEONDOJO_PERF_OPENINGS="$(
  for sample in $(seq 1 5); do
    printf '(;GM[6]FF[4]PW[Alice %s]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe])\n' "$sample" > "$perf_dir/opening.sgf"
    curl -fsS -F "file=@$perf_dir/opening.sgf" "$NEONDOJO_PERF_URL/api/imports" | jq -er '.match.id' || exit 1
  done
)"
export NEONDOJO_PERF_OPENINGS
python3 - <<'PY'
from collections import Counter
import json
import os
from pathlib import Path
import statistics
import time
from urllib.request import Request, urlopen

base = os.environ["NEONDOJO_PERF_URL"]
matches = os.environ["NEONDOJO_PERF_OPENINGS"].splitlines()
assert len(set(matches)) == len(matches) == 5, matches
log = Path(os.environ["NEONDOJO_PERF_DIR"]) / "gnubg-launches"
plies = int(os.environ.get("NEONDOJO_PERF_PLIES", "1"))
timeout = float(os.environ.get("NEONDOJO_PERF_TIMEOUT", "180"))

def get(path):
    with urlopen(base + path, timeout=30) as reply:
        return json.load(reply)

expected = {(a, b): 1 if a == b else 2 for a in range(1, 7) for b in range(a, 7)}
submissions = []
durations = []
for match in matches:
    position = get(f"/api/positions?match_id={match}")["items"][0]["id"]
    log.write_text("")
    request = Request(
        base + "/api/jobs",
        json.dumps(dict(position_id=position, kind="dice", engine_id="gnubg", plies=plies)).encode(),
        {"Content-Type": "application/json"},
    )
    start = time.perf_counter()
    with urlopen(request, timeout=30) as reply:
        job_id = json.load(reply)["id"]
    submissions.append((time.perf_counter() - start) * 1000)
    while time.perf_counter() < start + timeout:
        job = get("/api/jobs/" + job_id)
        if job["status"] in ("completed", "failed"):
            break
        time.sleep(0.05)
    else:
        raise TimeoutError(f"Dice job did not reach a terminal state within {timeout:g} s")
    durations.append((time.perf_counter() - start) * 1000)
    assert job["status"] == "completed", job.get("error")
    rolls = job["result"]["rolls"]
    weights = {tuple(sorted(roll["dice"])): roll["weight"] for roll in rolls}
    assert len(rolls) == 21 and weights == expected, weights
    launches = Counter(log.read_text().splitlines())
    assert set(launches) == {"--version", "-t"}, launches
    threads = (job.get("engine_snapshot") or {}).get("gnubg_threads") or 1
    print(job["result"]["engine"], f"plies={plies} threads={threads} terminal_ms={durations[-1]:.1f}",
          f"version_starts={launches['--version']} evaluation_starts={launches['-t']}", flush=True)
for name, values in (("submit", submissions), ("terminal", durations)):
    print(f"{name} ms median={statistics.median(values):.1f} range={min(values):.1f}–{max(values):.1f}")
PY
```

Review-Baseline mit GNUbg 1.08.003 war **43 Versionsaufrufe**, **21 Auswertungsstarts** und **8,1 s** bis zum beobachteten Abschluss eines Auftrags. Die Gegenprobe am 19. November 2026 ergab dieselben Startzahlen und **5,1 s**. Für jede Profiltiefe einen eigenen Lauf starten und `NEONDOJO_PERF_PLIES` setzen. Bei tieferen Läufen gegebenenfalls das Beobachtungsfenster mit `NEONDOJO_PERF_TIMEOUT` in Sekunden verlängern.
