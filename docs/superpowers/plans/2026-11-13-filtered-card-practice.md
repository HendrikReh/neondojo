# Gefilterte Karten üben

**Ziel**. Eine aktive Runde aus den angewendeten Lernkartenfiltern starten und Fehler oder markierte Karten im Anschluss erneut lösen.

**Architektur**. PostgreSQL speichert die begrenzte Auswahl und die Versuche. Die Runde verwendet den vorhandenen Kartenfilter und die gemeinsame Rust-Auswertung. Die Brettübung erhält ihre Antworten und Abschlussaktionen vom aufrufenden Modus.

- [x] In `backend/tests/learning.rs` die API mit gefilterten Cube-Karten prüfen. Die Runde muss nach einer Antwort wieder abrufbar sein, Fehler und Markierungen in die Wiederholung übernehmen und FSRS-Datensätze unberührt lassen.
- [x] In `backend/src/learning/rounds.rs` und einer neuen Migration Auswahl, Version, Fortschritt und Wiederholung implementieren. `backend/src/learning/search.rs` liefert den gemeinsamen Kartenfilter. Ein fachlich geänderter Kartenstand wird neu geladen. Entfernte Karten werden sichtbar übersprungen.
- [x] Aus `web/src/StudyCardExercise.tsx` die gemeinsame Übungsansicht für FSRS und Zusatzrunden verwenden. In `web/src/LearningPanel.tsx` die angewendeten Filter an die neue Runde übergeben und das Wiederaufnehmen ermöglichen.
- [x] OpenAPI und TypeScript-Typen generieren. Ablauf mit PostgreSQL und im Browser prüfen und die Bedienung in `README.md` und `docs/ui-navigation.md` dokumentieren.

Die [Produkt-Roadmap](../../research/2026-11-13-ukbgf-anki-training.md) beschreibt die anschließenden Etappen zu Themen, Transfer und Spezialübungen.
