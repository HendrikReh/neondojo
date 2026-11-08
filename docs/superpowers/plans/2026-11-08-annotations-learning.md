# Tags, Kommentare, Anki-Export und integriertes Lernen

## Autorisierter Umfang

Partien und Positionen erhalten bearbeitbare Tags und einen Kommentar. Beides ist durchsuchbar. Der Nutzer hat ausdrücklich die Kombination aus `.apkg`-Export und eigenem Lernmodus gewählt. Eine gemeinsame Kartenbasis versorgt beide Wege; der Lernstand wird zunächst im jeweiligen Lernsystem gespeichert.

## Architektur und Schritte

1. Additive PostgreSQL-Migration für Annotationen, atomare Speicherung mit Versionsschutz, normalisierte Tags und begrenzte Kommentargrößen. REST für Partie-/Positionsannotation sowie paginierte Suche über Kommentar und Tags; indizierte Suche. Bibliothekssuche berücksichtigt Annotationen. Bestehende Positionen werden weiterhin durch ihre importierte UUID identifiziert.
2. Gemeinsame Karten-/Deckverwaltung vollständig in Rust mit PostgreSQL. Analysierte Einzelpositionen oder Entscheidungen aus einer Partieanalyse werden aufgenommen. Karten enthalten Brett, Würfel/Doppler/Score, Fragestellung, native Lösung/Equity, Kommentare/Tags und Analyseherkunft. Kartenidentität ist stabil und umfasst GNUbg Position ID, vollständige Match-ID und Entscheidungstyp. Wiederholtes Hinzufügen aktualisiert Inhalte und erhält Lernfortschritt.
3. APKG-Erzeugung in Rust: stabiles Notizmodell und GUIDs, eigenständiges Brettdiagramm und selbstenthaltende Inhalte ohne BGBlitz-Bildabhängigkeit. Tags und Kommentare werden übernommen, HTML-Daten escaped. Die Vorderseite verrät die Lösung nicht. Exportpaket strukturell und möglichst mit echter Anki-Importbibliothek prüfen.
4. Integriertes Lernen: fällige Karte, Antwort aufdecken, Again/Hard/Good/Easy; FSRS in Rust mit persistiertem Zustand und Review-Historie. Bewertung wird mit Versions-/Idempotenzschutz gespeichert. Anfängliche Lernschritte und Zielbehaltensrate werden dokumentiert. Derselbe Kartensatz kann als Anki-Deck exportiert werden.
5. React: Annotationen an Partie und Brettposition, Suche mit direktem Positionssprung, Deckauswahl/Kartenaufnahme, Lernansicht und Export. Neue Texte in allen sechs Sprachen; vorhandene Theme-Auswahl wird auch beim Lernen verwendet.
6. OpenAPI/TypeScript regenerieren; PostgreSQL-/API-/Scheduler-/Pakettests, Browserablauf und unabhängige Prüfung. Kein neuer Analyse- oder SRS-Dienst außerhalb des vorhandenen Headless-Backends.

## Referenzen

- https://www.dirk-schiemann.de/the-anki-method.html
- https://xg2anki.de/
- https://ankigammon.com/ und https://ankigammon.com/train/
- https://github.com/HendrikReh/AnkiGammon
- https://ukbgf.com/extending-the-anki-open-source-digital-flashcard-application/
- https://docs.ankiweb.net/manual/importing/packaged-decks
- https://docs.rs/fsrs/latest/fsrs/
- https://docs.rs/genanki-rs/latest/genanki_rs/

Die Quellen sprechen für thematische und persönliche Fehlerdecks. BG Tutor nutzt seine vorhandenen Import-/Analyseergebnisse und setzt Export und Lernzustand in Rust um. Fremde Trainingsdecks oder Texte werden nicht übernommen.

## Umsetzung und Verifikation

Umgesetzt: Migrationen 0003–0005, Annotationen mit Versionsschutz, indizierte Suche, gemeinsame Rust-Kartenbibliothek, APKG-Export, FSRS-Lernmodus und Oberfläche in allen sechs Sprachen. Die Card-Fragen zeigen Matchlänge, Spielstand, Doppler, Spieler am Zug und bei Money Games die Jacoby-Regel. Anki und BG Tutor behalten getrennte Lernstände.

Verifiziert am 8. November 2026:

- 22 schnelle Rust-Tests und 34 zusätzliche PostgreSQL-/GNUbg-/Performancetests bestanden. Enthalten: 10 Annotationstests, 6 Learning-Integrationen, parallele Reviews und deterministisch verzögerte Exporte über überlappende Decks.
- 13 Frontend-Tests, 6 Theme-Archivtests und 12 Browserabläufe bestanden. Der nachträgliche Kontextfix wurde mit den betroffenen Unit-, Paket- und Browserprüfungen erneut geprüft.
- `cargo fmt`, Clippy mit `-D warnings`, Prettier, Python-Formatprüfung und Produktionsbuild erfolgreich. OpenAPI und Clienttypen regeneriert.
- Ankis offizielle Importbibliothek 26.9.4 hat Pakete in einer temporären, leeren Sammlung importiert. Wiederholter Import erzeugt keine Duplikate und aktualisiert geänderte Kommentare/Sprachinhalte. Die vorhandene Benutzer-Sammlung wurde nicht geöffnet.
- Unabhängige Anforderungs- und Qualitätsprüfungen abgeschlossen. Gefundene Navigations-, Export- und Nummerierungsfehler wurden mit Regressionstests behoben.

Keine AnkiConnect-Anbindung, Synchronisierung von Review-Historien oder Übernahme fremder Trainingsdecks. Der Export selbst benötigt ausschließlich Rust; Python/Anki dient nur als optionales Integrationsprüfwerkzeug.
