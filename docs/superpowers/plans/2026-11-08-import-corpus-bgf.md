# XG, MAT and BGBlitz BGF corpus import

> Execution: inline in the existing workspace. User-authorized scope is testing every file in games/xg, games/mat and games/bgf and fixing discovered import failures.

**Goal:** Every valid supplied file imports through REST with complete positions; invalid input is rejected atomically with a precise reason.

**Architecture:** Keep the common SGF/replay model. XG and MAT retain their current paths. Decode BGBlitz BGF 1.0 (JSON header, optional bounded gzip, JSON or Smile payload) in Rust, convert native game records to SGF, and validate every played board/cube/score context. Add format support to PostgreSQL, REST/OpenAPI and file selection. Foreign stored analysis is not used as a replacement for GNUbg.

1. Add an optional SQLx corpus test in backend/tests/import_corpus.rs; send every supplied file through multipart REST in a disposable database, verify persisted counts and duplicate identity, and write a complete per-file report before asserting failures.
2. Reproduce all failures before fixes. Inspect BGF transport/schema and public format/library sources; choose strict supported match records rather than guessing about unknown variants.
3. Add portable BGF transport and game regression fixtures; run them red. Implement backend/src/import/bgf.rs with size/structure limits, player/color conversion, game scores, checker plays and cube actions. Reuse the existing rule engine and strict SGF replay.
4. Add migration 0006 for bgf source format, route dispatch, OpenAPI upload documentation and frontend file types/hints. Regenerate API types.
5. Fix further XG/MAT failures one at a time using portable failing regressions. Retain complete turns and match context; do not suppress diagnostics or skip invalid played records.
6. Re-run the complete local corpus, isolated database and native engine tests, frontend build and one real BGF browser import/analysis. Review the patch and report per-format counts and any real invalid source records. Local game files remain untracked.

## Ergebnis

Alle sechs Arbeitsschritte umgesetzt. Der vollständige REST-Bestandslauf ist grün: 37 Dateien, 17 verschiedene Inhalte, 20 Duplikate und 14.836 persistierte Positionen in einer isolierten Datenbank. Portable BGF-Regressionsdaten und eine echte GNUbg-Analyse sind Teil der API-Tests. Der native BGF-Decoder prüft Transportgrenzen, gzip-CRC, Formatversion, Kontrollaktionen und nicht unterstützte Regeln; begrenzte Dopplerregeln werden ausdrücklich abgelehnt. Unabhängige Prüfung ohne offene Befunde abgeschlossen. Vollständiger Ergebnisbericht: `docs/import-corpus-results.md`.

Verifikation des endgültigen Stands: 48 schnelle Rust-Tests und 36 zusätzliche GNUbg-/PostgreSQL-/Bestandstests bestanden, darunter der vollständige 37-Dateien-Lauf. 13 Frontend-Tests, Produktionsbuild, Rustfmt, Clippy mit `-D warnings`, Prettier und Diff-Prüfung sind grün. OpenAPI/TypeScript neu erzeugt. Migration 0006 ist lokal angewendet; Backend läuft mit dem gebauten Client. Ein tatsächlicher BGF-Upload mit 460 Positionen und die native 10-Spiele-Partieanalyse wurden zusätzlich in Chrome geprüft.
