# Open-Sage-Laufzeitoptimierungen

> Umsetzung nach ausdrücklichem Auftrag vom 12. November 2026; Skills: subagent-driven-development, test-driven-development, verification-before-completion.

**Ziel:** Die drei gemessenen Engpässe reduzieren, ohne Enginewerte, Integrität, Checkpoints oder bestehende lokale Arbeit zu verändern.
**Architektur:** Hash-Deduplizierung gilt nur innerhalb einer einzelnen vollständigen Prüfung. Eine Spielanalyse hält einen exklusiven Sage-Worker bis zur bestätigten Endprüfung; Zwischenantworten bleiben unveröffentlicht. GNUbg bleibt innerhalb eines Sage-Zugablaufs geöffnet, mit begrenztem Protokoll, festgehaltener Version, wiederherstellbarem SGF und zuverlässigem Kill/Reap bei Abbruch.

## 1. Ausgangsstand sichern
- [x] Hauptworktree-Änderungen separat sichern; isolierter Worktree auf d2e0291.
- [x] Bestehende Python-/Rust-Basistests prüfen. Keine Benutzerbestände für Tests verwenden.

## 2. Integritätsprüfungen und atomare Analyseblöcke
Dateien: tools/probe_open_sage.py, tools/test_probe_open_sage.py, tools/test_open_sage_worker.py, backend/src/engines/{worker.py,process.rs,mod.rs,game.rs}, backend/tests/open_sage.rs.
- [x] Zuerst Regressionen: jede Datei pro Prüfung einmal lesen, unveränderte Fingerprints, Änderungen zwischen Prüfungen erkannt, fehlende/geänderte Dateien und RECORD weiterhin abgelehnt.
- [x] Hashwerte nur innerhalb einer Prüfung wiederverwenden; kein langlebiger Hashcache.
- [x] Block vor erster und nach letzter Entscheidung vollständig prüfen. Worker nicht zwischen parallelen Arbeiten teilen; bei Fehler, Timeout, Abbruch, ungültiger Antwort oder Endprüfung töten/reapen und alle Blockresultate verwerfen.
- [x] Antworten und Speicher bleiben begrenzt. Keine neuen Datenformate für fachliche Analysewerte. Gemeinsame Request-/Context-/Resultatlogik wiederverwenden.
- [x] Tests für Zwischenänderung, verschachtelten/fehlenden Abschluss, defekte Antworten, Konkurrenz und Abbruch; native Referenzwerte und vollständige 460 Checkerzüge vergleichen.

## 3. GNUbg während des Sage-Zugablaufs wiederverwenden
Dateien: backend/src/play.rs, backend/src/play/{native.py,open_sage.rs}, abgegrenztes neues Prozessmodul und passende Tests.
- [x] Fehlerfälle zuerst absichern: genau ein Prozess für Inspektion und Ausführung; Version-/Zustandsabweichung, invalides/frühes/überlanges Protokoll, Tod, Timeout und Abbruch speichern keinen Teilzug.
- [x] Bestehende Python-Regellogik und SGF-/Snapshot-Auswertung wiederverwenden. Beide Spieler extern; Sage bestimmt die Aktion. Nach erfolgreichem Zug unveränderte legale Stellung, Historie und SGF liefern.
- [x] Native Referenzzüge, beide Farben, Pass, Cube, Aufgabe, Pause/Fortsetzung und parallele lokale Erweiterungen bei späterem Zusammenführen berücksichtigen.

## 4. Integration und Prüfung
- [x] Neue Benchmark-Protokollkennung für den optimierten Sage-Pfad; alte Ergebnisse/Protokolle nicht vermischen und alte Snapshots weiterhin lesbar lassen.
- [x] Spec-Review, anschließend Code-Review; Befunde beheben.
- [x] Rust fmt, Clippy, Workspace-Tests, Python-/Frontendtests und Build; betroffene echte PostgreSQL-/GNUbg-/Sage-Tests auf isolierten Datenbanken.
- [x] Nur Optimierungsänderungen mit dem aktuellen Hauptworktree zusammenführen, vorhandene/unabhängige Änderungen erhalten. Keine automatischen Commits/Pushes.

## 5. Messung und lokale Nutzung
- [x] Optimierten Release-Build prüfen. Vor einer nötigen Migration/Neustart Benutzerbestand sichern; keine angefangenen fremden Schemaänderungen ungeprüft aktivieren.
- [x] Pro Engine drei Analyse- und drei Spiel-Benchmarks bei 2-ply, sequenziell mit wechselnder Reihenfolge; vollständige Referenzabdeckung und feste Snapshots prüfen.
- [x] Median/Spanne gegenüber den ursprünglichen Messungen aus local-data/benchmarks/20261112-065856-2ply-comparison vergleichen. Messgrenzen, Ergebnisse, Testumfang und verbleibende Einschränkungen dokumentieren.

Ergebnis und Messdaten: `docs/open-sage-local.md`, Abschnitt „Nachmessung der drei Optimierungen“. Bei der Nachmessung blieb die Benutzerinstanz unverändert; keine fremde Migration wurde aktiviert. Vier bestehende GNUbg-Testfehler am Ausgangscommit reproduziert.
