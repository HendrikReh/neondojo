# Position Browser mit gespeicherten Filtern

Der Benutzer beauftragt den nächsten bereits vorgeschlagenen Ausbau: Positionen finden, Suchfilter speichern und Treffer zur Analyse, zum bestehenden FSRS-Lernmodus und zum Rust-Anki-Export übernehmen. Zusätzlich entfällt ausschließlich der sichtbare Importknopf im Header. Datenbankimport, Dateieingabe, Drop und bestehende Analyseabläufe bleiben verfügbar. Die neun bisherigen XG-Ablehnungen werden anhand des privaten Importmanifests mit Dateinamen und präziser Parserdiagnose erklärt; keine Quelldaten werden still repariert oder Prüfungen abgeschwächt.

## Oberfläche und Suchmodell

Die Zielplattform ist der Desktop. Ergebnisliste, Brettvorschau, Analyseinformationen und Lernaktionen sollen den verfügbaren Platz für eine hohe Informationsdichte nutzen. Mobile Optimierung ist nach ausdrücklicher Benutzerentscheidung kein Projektziel und kein zusätzliches Abnahmekriterium.

Eine eigene Hauptansicht Positionen ergänzt Analyse, Matchdatenbank und Lernen. Eine paginierte Ergebnisliste zeigt Herkunft, Partie/Zug, Spieler am Zug, Wurf, anwendbare Notizen/Tags und vorhandene native Fehlerwerte. Eine Brettvorschau zeigt immer die gespeicherte Entscheidung vor dem Zug. Ein Treffer öffnet die vorhandene Analyse am exakten Position-UUID. Filter und Ergebnisposition bleiben beim Wechsel erhalten; spätere Antworten dürfen neuere Such- oder Vorschauauswahl nicht überschreiben.

Filter umfassen Datenbank, Freitext, Spieler am Zug, Matchname, Turnier, Runde, Tag, Kommentar, Checker-Position-ID, Matchpunkte, Würfel, Quellspielstand, Crawford, Cube und vorhandene lokale Analyse beziehungsweise nativen Equity-Verlust. Die erste Iteration umfasst gespeicherte Checkerentscheidungen, deren Positions-UUID die bestehenden Analyse-/Notiz-/Lern-APIs verwenden. Cube-Fragen bleiben im bestehenden Cube-/Zugdialog und den Statistiken erreichbar; es werden keine fingierten Checkerpositionen erzeugt. Eine identische Checkerstellung wird nicht über verschiedene Würfe, Scores oder Cube-Kontexte hinweg zusammengelegt.

Notizfilter prüfen nur zum Treffer gehörende Ebenen: Match, dessen Partie, Position und zugehöriger gespielter Zug. Eine Notiz an einem anderen Zug darf nicht alle Positionen eines Matches zum Treffer machen. Suchzeichen werden literal behandelt. Native Fehlerwerte behalten Engine, Tiefe und Auftrag; Quellanalysen und Opening Books werden nicht als identische lokale Fehlerbewertung ausgegeben. Fehlende Werte bleiben unbekannt.

## Gespeicherte Suchen

Benannte Filter sind versionierte, streng validierte JSON-Definitionen in PostgreSQL, unabhängig von Browserspeicher und separaten Matchbeständen. Laden, als neue Suche speichern, bestehende Suche aktualisieren/umbenennen und löschen sind möglich. Optimistische Revisionen verhindern verlorene Änderungen. Ein gelöschter Datenbankbezug führt zu einem sichtbaren Fehler statt einer stillen Suche in allen Datenbanken. Cursor sind an die gesamte kanonische Filterdefinition und Sortierung gebunden.

## Lernen und Anki

Die Auswahl von bis zu200 Treffern wird in einen gewählten oder neuen Stapel übernommen. Die Fragen verwenden dieselben stabilen Positions- und vollständigen Match-IDs sowie echten nativen Lösungen wie bisher. Identische Lernfragen teilen ihre bestehende FSRS-Historie. Wiederholtes Übernehmen erzeugt keine doppelten Karten. Jeder ausgewählte Treffer erhält ein Ergebnis: hinzugefügt, aktualisiert, bereits vertreten, Analyse erforderlich oder nicht mehr vorhanden. Nicht analysierte Treffer werden ausdrücklich gemeldet; keine gespielte Notation wird als richtige Antwort erfunden. Der Stapel kann anschließend direkt im Lernmodus geöffnet oder über den vorhandenen Rust-Export als APKG heruntergeladen werden. Die Oberfläche sagt ausdrücklich, wenn der gesamte gewählte Stapel exportiert wird.

## Architektur und Prüfung

Eine additive Migration ergänzt Suchindizes, sicher dekodierten GNUbg-Kontext und gespeicherte Filter. Das Backend bietet paginierte Positionssuche, Filter-CRUD und eine begrenzte Stapelübernahme. Die vorhandene Einzelkartenübernahme und der neue Mehrfachweg teilen die transaktionale Schreiblogik inklusive Größenlimit und stabiler Sperrreihenfolge. Alle sechs Sprachen werden unterstützt.

Tests prüfen Notiz-Zugehörigkeit, Wurf-/Score-/Cube-Kontext, Suchgrenzen, Cursorbindung, unabhängige Datenbanken/Kopien, Filterrevisionen, native Analyseherkunft sowie wiederholte und teilweise mögliche Lernübernahmen ohne Verlust der Wiederholungen. Browserabläufe laufen in einer separaten PostgreSQL-Testdatenbank; die drei privaten Demo-Bestände bleiben unverändert.
