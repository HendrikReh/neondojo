# Vollständige GNUbg-Partieanalyse

## Ziel und Architektur

Der Ausgangsstand ist als `bb89e53` gesichert. Die nächste Erweiterung analysiert alle Spiele einer importierten Partie mit GNUbgs nativer `analyse game`-Funktion. Checker- und Cube-Entscheidungen werden gegen die tatsächlich gespielte Entscheidung ausgewertet; ein Vergleich mit nur fünf Hint-Kandidaten reicht dafür nicht aus.

Die Analyse läuft als persistenter Match-Auftrag mit 0–2 Ply. Nach jedem vollständig ausgewerteten Spiel werden Zugergebnisse, Spielerstatistik und Fortschritt atomar gespeichert und ein wiederaufnehmbares SSE-Ereignis erzeugt. Ein Heartbeat hält die Worker-Lease auch während langer Engine-Aufrufe gültig. Wiederanläufe setzen bei gespeicherten Spielen fort; alte Worker dürfen keine Ergebnisse überschreiben. Fehler werden mit Kontext ausgegeben, bereits gespeicherte Ergebnisse bleiben lesbar.

REST erhält Start/letzter Auftrag pro Partie sowie paginierte Entscheidungen pro Auftrag. OpenAPI und generierte TypeScript-Typen bleiben die gemeinsame Schnittstelle. Die Oberfläche zeigt Analyse-Tiefe, echten Spielfortschritt, Spielerstatistik und eine filterbare Fehlerübersicht mit gespieltem/bestem Zug, Equity-Verlust und Navigation zu Checker-Positionen. Cube-Entscheidungen sind ausdrücklich Teil der Übersicht. Equity-Verluste werden aus GNUbgs normierter Equity berechnet; sie werden nicht als PR bezeichnet.

## Arbeitsschritte

1. Native GNUbg-Analyse pro Spiel implementieren; tatsächliche Checker- und Cube-Ergebnisse verlässlich parsen. Tests mit realem GNUbg, schlechten Zügen, erzwungenem Aussetzen, Double/Take/Drop und mehrteiligen Partien.
2. Additive SQL-Migration, Match-Aufträge, atomare Checkpoints, Lease-Heartbeat und Wiederaufnahme ergänzen. Integrationstests mit PostgreSQL und GNUbg für Fortschritt, Deduplizierung, Fehler, Neustart und SSE-Replay.
3. REST/OpenAPI und persistente, paginierte Fehlerübersicht implementieren. React integriert Start, Status, Ergebnisfilter und Positionsnavigation.
4. Browserprüfung, Rust fmt/clippy/tests, Frontend-Typprüfung/Build/Tests und unabhängige Anforderungs- sowie Codeprüfung. Dokumentation um tatsächliche Bedienung und Grenzen ergänzen.

## Abnahmekriterien

- Bestehende Imports, Positionsanalyse und Dice Distribution bleiben funktionsfähig.
- Ein realer Auftrag analysiert jede gespeicherte Checker-Entscheidung und alle von GNUbg bewertbaren Cube-Entscheidungen; fehlende Auswertung führt nicht zu scheinbarem Erfolg.
- Fortschritt ist dauerhaft, monoton und spielbasiert; SSE kann mit Last-Event-ID wiederaufgenommen werden.
- Die Übersicht enthält korrekt normalisierte Equity-Verluste und reproduzierbare Fehlerklassen.
- Ein fehlendes GNUbg oder eine abgelaufene Lease produziert einen sichtbaren Fehler statt einer hängenden Analyse.

## Abschluss am 8. November 2026

Implementiert und unabhängig auf Anforderungen und Codequalität geprüft. Die Prüfung führte zusätzlich zu Regressionstests für fehlende Cube-Auswertung, native SGF-Setups beider Farben, geerbte Matchdaten, leere/Cube-Spiele, gleichzeitige Checkpoint-/Heartbeat-Sperren und verspätete Positionsnavigation.

Verifikation des endgültigen Standes:

- `cargo fmt --all --check` und `cargo clippy --workspace --all-targets -- -D warnings`: erfolgreich.
- `cargo test --workspace`: 18 Tests erfolgreich; abhängige Tests anschließend ausdrücklich ausgeführt.
- `DATABASE_URL=… cargo test --workspace -- --ignored --nocapture`: 18 weitere Tests erfolgreich, mit PostgreSQL und lokalem GNUbg. Darunter 460 Checker-Züge über alle zehn Spiele jeweils aus SGF, MAT und XG.
- `npm run api:types`, `npm run format:check`, `npm test`, `npm run build`: erfolgreich; vier Frontend-Tests.
- `npm run test:e2e`: vier Browsertests erfolgreich, einschließlich Desktop/Mobil, Fehlerübersicht, Wiederladen und verzögerter Positionsnavigation.
- Gesamt: 44 erfolgreiche Tests. Lokales Backend neu gestartet; Health meldet PostgreSQL und GNUbg verfügbar.
