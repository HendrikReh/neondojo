# Lernstartseite für BG Tutor

Die Lernstartseite wird der erste Reiter unter **Study → Übersicht**. Sie zeigt den nächsten sinnvollen Schritt, die Zahl fälliger Lernkarten und die wichtigsten wiederkehrenden Fehler für den gewählten Spieler. Von hier aus startet ein Klick den vorhandenen Lernkartenablauf oder den Übungsdialog für eine konkrete Fehlergruppe.

## Ansicht und Empfehlung

| Bereich | Inhalt | Bedienung |
| --- | --- | --- |
| Nächster Schritt | Eine Empfehlung mit kurzer Begründung, etwa der Name des Decks oder die Fehlerart mit der Anzahl betroffener Partien. | Eine primäre Aktion. Der Nutzer startet die Übung bewusst per Klick oder Tastatur. |
| Fällige Lernkarten | Eindeutige Anzahl der jetzt lernbaren Karten über alle Decks. Bei null fälligen Karten den nächsten Termin oder einen leeren Bestand anzeigen. | „Lernkarten & Anki öffnen“ öffnet den vorhandenen Reiter. |
| Wiederkehrende Fehler | Spielerwahl und bis zu drei Gruppen mit Spielplan, Fehlerart, Vorkommen, betroffenen Partien und summiertem Equity-Verlust. | Jede Gruppe lässt sich direkt üben. „Fehlerübersicht öffnen“ führt zum bestehenden Reiter mit seinen Filtern. |

Die erste passende Zeile entscheidet über den nächsten Schritt.

| Situation | Primäre Aktion |
| --- | --- |
| Mindestens eine Karte ist fällig. | „Fällige Karten lernen“ öffnet das Deck der am längsten fälligen Karte und startet den bestehenden Lernablauf. Ein gespeicherter Erstversuch für diese Karte öffnet sich beim Bewertungsschritt. |
| Keine Karte ist fällig. Eine Fehlergruppe betrifft mindestens zwei verschiedene Partien. | „Fehlergruppe üben“ startet die höchstplatzierte Gruppe. Die Reihenfolge folgt dem summierten Equity-Verlust, dann der Zahl der Partien, dann der Zahl der Vorkommen, jeweils absteigend. Bei Gleichstand entscheiden die stabilen API-Werte für Spielplan und Fehlerart. |
| Keine Karte ist fällig. Die geladene Spielerliste enthält Namen und die Auswahl ist leer. | „Spieler wählen“ setzt den Fokus in die Auswahl. |
| Beide Datenquellen sind ausgewertet. Es gibt weder fällige Karten noch eine passende Fehlergruppe. | „Lernkarten & Anki öffnen“ führt zur Kartensammlung. Der Kartenbereich zeigt gegebenenfalls den nächsten Termin, der Fehlerbereich zeigt die vorhandene Analyseabdeckung. |

Die Fälligkeit hat Vorrang. Während sie lädt, zeigt der Empfehlungsbereich seinen Ladezustand. Scheitert der Abruf, kann eine bereits geladene Fehlergruppe geübt werden. Der Kartenbereich zeigt den Abruffehler und bietet einen eigenen Wiederholungsversuch an. Bei null fälligen Karten wartet die Empfehlung auf die laufende Fehlerabfrage. Scheitert diese, bleiben Karten und die Navigation zur Fehlerübersicht bedienbar. Eine nicht verfügbare Zahl wird als solche angezeigt.

## 1. Eindeutige Fälligkeit serverseitig abfragen

- [ ] In `backend/src/learning/mod.rs` einen kompakten `GET /api/study/learning-summary` im vorhandenen Learning-Router mit OpenAPI-Schema registrieren. `LearningSummary` liefert `card_count`, `due_count`, `next_due_at` und `next_deck`. `card_count` zählt Karten mit mindestens einer Deckzuordnung. `due_count` zählt davon diejenigen mit `due_at <= now()`. `next_due_at` ist der früheste zukünftige Termin dieser Karten oder `null`. `next_deck` enthält `id` und `name` oder ist bei null fälligen Karten `null`.
- [ ] In einer SQL-Abfrage `learning_cards` als Ausgangspunkt und `EXISTS` auf `deck_cards` für die Zuordnung nutzen. Damit zählt dieselbe Karte auch bei mehreren Decks einmal. Die älteste fällige Karte nach `(due_at, id)` bestimmen, wie in `next_card`. Bei mehreren zugehörigen Decks das früheste nach `(created_at, id)` wählen. Fälligkeitsgrenze und Reihenfolge mit `next_card` abgleichen. Mit der global ältesten Karte als Deckkopf stimmt die Empfehlung bei unverändertem Datenstand mit der nächsten Karte im Deck überein. Die bestehenden Indizes auf `(due_at, id)` und `deck_cards(card_id)` decken die Zugriffswege ab. Die Abfrage auf repräsentativen Testdaten mit `EXPLAIN (ANALYZE, BUFFERS)` prüfen, bevor über weitere Indizes entschieden wird.
- [ ] `npm run api:types --prefix web` ausführen und `docs/openapi.json` sowie `web/src/generated-api.d.ts` prüfen. In `web/src/learning-api.ts` den generierten Typ und die Abfrage mit dem vorhandenen `request`-Helfer und `AbortSignal` anbinden. Der Start nutzt weiter `GET /api/decks/{id}/next` über `LearningPanel.tsx`. Wenn zwischen Anzeige und Klick eine Karte anderweitig bewertet wurde, lädt der bestehende Lernablauf den aktuellen Deckkopf. Beim gelöschten Deck die vorhandene Deckliste aktualisieren und das fehlende Ziel am Einstieg kenntlich machen. Beim Zurückkehren wird die Zusammenfassung neu geladen.

## 2. Fehlergruppen für die Übersicht aufbereiten

- [ ] In der neuen `web/src/StudyOverview.tsx` `loadMistakes` aus `web/src/recurring-mistakes-api.ts` verwenden. `GET /api/study/mistakes` liefert die Spielerliste. Für den gewählten Spieler dieselbe Route mit `player` aufrufen. So gilt der serverseitige Standard von `0,08` normalisierter Equity über alle Datenbanken. `groups`, `analyzed_matches` und `analyzed_decisions` direkt auswerten. Gruppen mit weniger als zwei verschiedenen Partien erscheinen weiterhin im bestehenden Fehlerreiter. In der Übersicht bei leerem Ergebnis zwischen fehlender Analyseabdeckung und fehlender Wiederholung über mehrere Partien unterscheiden.
- [ ] Den Spielereintrag aus `general.player_name` des aktiven Profils vorbelegen, wenn er nach Abgleich des vollständigen Namens ohne Groß-/Kleinschreibung und Rand-Leerzeichen eindeutig in der Liste vorkommt. Andernfalls bleibt die Auswahl offen. Eine manuelle Wahl gilt für die laufende Study-Ansicht. Beim Profilwechsel neu vorbelegen. Den beschrifteten Spieler-Select und die lokalisierten Namen für Spielpläne und Fehlerarten aus dem Fehlerreiter wiederverwenden. Das Profil und die Filter des Fehlerreiters bleiben beim Wechsel der Auswahl in der Übersicht erhalten.
- [ ] Rangfolge und Entscheidung über den nächsten Schritt in `web/src/study-overview.ts` als testbare Funktionen halten. Für Gruppenaktionen und die empfohlene Gruppe denselben `MistakePracticeRequest` aus dem kanonischen Spielernamen, `game_plan` und `mistake_kind` verwenden. `RecurringMistakePractice.tsx` mit Theme und Referenz auf den auslösenden Button öffnen. Der Dialog verwendet das gemeinsame Übungsbrett, die Cube-Auswahl und die vorhandene Bewertung. Beim Schließen Fokus zurückgeben und beim Wechsel von Spieler, Profil oder Reiter eine offene Übung beenden. Die Rangfolge beschreibt vorhandene Fehler. Das Üben einer Gruppe ändert ihre Position nur, wenn sich ihre Quelldaten ändern.

## 3. Übersicht in Study einbinden

- [ ] `web/src/StudyWorkspace.tsx` um den ersten Reiter `overview` ergänzen. Bestehende Tab-Semantik, Pfeiltasten sowie Home und End fortführen. Reiterwechsel aus der Übersicht setzen den Fokus auf den Zielreiter. `LearningWorkspace` und `RecurringMistakes` mit ihrer bisherigen Lebensdauer und ihren bisherigen Filtern weiterverwenden. Eine Übersicht bleibt beim Tabwechsel gemountet, damit ihre manuelle Spielerwahl beim Zurückkehren verfügbar ist.
- [ ] In `web/src/App.tsx` den vorhandenen Positions-Einstieg als gemeinsame Funktion `openStudyDeck(id)` ausdrücken. Positionsansicht und Empfehlung rufen denselben Einstieg mit dem bisherigen `requestedDeck`-Token auf. Ein neuer Token wählt „Lernkarten & Anki“ und startet genau einmal. Manuell „Übersicht“ wählen und externe Decklinks weiterhin wie gewohnt benutzen können.
- [ ] Beide Bereiche unabhängig laden. Karten beim Öffnen und bei `deckRevision` aktualisieren. Wenn die Übersicht sichtbar bleibt, beim nächsten bekannten Termin erneut abfragen. Timer und Anfragen beim Ausblenden aufräumen. Fehlergruppen beim Betreten der Übersicht, beim Spieler- oder Profilwechsel und bei manueller Aktualisierung abrufen. Abgebrochene oder verspätete Antworten dürfen aktuelle Ergebnisse auch bei schnellem Wechsel A → B → A nicht überschreiben. Eine Lade-, Leer- oder Fehlermeldung pro Bereich halten, damit der andere Bereich weiter bedienbar bleibt.
- [ ] `ActionButton`, die Abstände und Typografie aus `study.css` sowie die bestehenden Status- und Tabellenmuster verwenden. Neue Texte in `study-messages.ts` für Deutsch, Englisch, Spanisch, Französisch, Italienisch und Polnisch ergänzen. Datum und Zahlen mit vorhandenen Locale-Formatierern anzeigen. `README.md` und `docs/ui-navigation.md` um Empfehlung, Tastaturreihenfolge und direkte Deck-Einstiege ergänzen.

## 4. Abnahme

- [ ] `backend/tests/learning.rs` mit einer isolierten SQLx-Datenbank ergänzen. Eine gemeinsame fällige Karte in zwei Decks zählt einmal, eine Karte ohne Deck zählt nicht und eine zukünftige Karte liefert den nächsten Termin. Leeren Bestand, exakte Fälligkeitsgrenze und Gleichstände bei Karten und Decks prüfen. Für die vorgeschlagene Deck-ID muss `GET /api/decks/{id}/next` dieselbe fällige Karte liefern. Ein GET auf die Übersicht ändert weder Review-Zähler noch gespeicherte Erstversuche.
- [ ] `web/src/study-overview.test.ts` deckt die Rangfolge, alle Empfehlungszweige, Gleichstände, fehlende Abdeckung sowie Lade- und Teilausfälle ab. Mindestens ein Test prüft, dass mehrere Fehler aus nur einer Partie keine Empfehlung erzeugen.
- [ ] `web/e2e/study-overview.spec.ts` prüft den Standardreiter, die Tastaturnavigation, den vorhandenen direkten Einstieg aus Positions, eine Deckempfehlung mit gespeichertem Erstversuch und aktualisierte Fälligkeit nach einem FSRS-Review. Dazu gelöschtes Deck, Profilvorbelegung, manuelle Auswahl, A → B → A, isolierte Abruffehler und erneutes Laden prüfen. Aus der Übersicht eine konkrete Fehlergruppe starten, eine Antwort prüfen und Fokus nach Escape kontrollieren. Für Checker, Cube-Angebot und Cube-Antwort die vorhandenen E2E-Fälle weiter ausführen. Bestehende Browsertests, die „Lernkarten & Anki“ implizit als Standardreiter voraussetzen, gezielt zum Reiter navigieren lassen und den vollständigen Browserlauf prüfen. Desktop-Ansicht bei 1800 × 1169, 2560 × 1440 und 3840 × 2160 prüfen.
- [ ] Nach der Implementierung die fokussierten Prüfungen und anschließend die Qualitätsprüfungen ausführen. PostgreSQL und GNUbg wie im README einrichten. SQLx und Playwright verwenden isolierte Testdatenbanken.

```sh
npm run api:types --prefix web
cargo test -p bgtutor --test learning learning_summary_counts_unique_due_cards
npm test --prefix web -- src/study-overview.test.ts
npm run test:e2e --prefix web -- e2e/study-overview.spec.ts e2e/active-study-answers.spec.ts e2e/recurring-mistakes.spec.ts
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test --prefix web
npm run build --prefix web
npm run test:e2e --prefix web
```
