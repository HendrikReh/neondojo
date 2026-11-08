# Backgammon-Datenbanken und Kommentare

Freigegebener Umfang: erweiterte Filter und Mehrfachverwaltung; benannte Datenbanken; Backgammon-Matchübersicht mit PR; Kommentare für Match, Partie und Zug. Die Rückfrage zur Kopiersemantik wurde mit eigenständigen Kopien und eigenen Notizen beantwortet.

## Datenmodell

Benannte Bestände liegen gemeinsam in PostgreSQL. Jeder Matchdatensatz gehört genau einem Bestand. Die Migration ordnet bestehende Matches dem Standardbestand zu. Die Duplikaterkennung gilt je Bestand. Kopieren erzeugt neue IDs für Match, Positionen, Partien, tatsächliche Züge und deren Notizen. Vorhandene Zielduplikate bleiben unverändert. Verschieben erhält IDs und Notizen; Zielduplikate werden übersprungen. Lernkarten und Lernfortschritt bleiben eigenständige Einträge.

## Arbeitsablauf

Der Datenbanktab enthält eine Bestandsauswahl, eine sortierbare Matchliste und eine Brettvorschau. Auswahl zeigt die Vorschau; Öffnen wechselt zum Analysearbeitsplatz. Filter kombinieren Spieler, gespieltes oder Importdatum, Tag, Kommentar, Position-ID, Turnier, Matchname, Match/Moneygame, Ergebnis und Matchlänge. Unbekannte Quelldaten bleiben unbekannt und können als Metadaten ergänzt werden. Mehrfachauswahl unterstützt Kopieren, Verschieben, Tags, Export und Löschen. Löschen erfordert eine konkrete Bestätigung und ist bei laufender Analyse gesperrt. Mehrfachimport meldet Ergebnisse je Datei im gewählten Bestand.

## Backgammon-Übersicht

Spalten enthalten Spieler und PR, Matchlänge oder Moneygame, Ergebnis, Datum, Turnier, Analysezustand, Tags und Kommentar. PR wird ausdrücklich als GNUbg-Methode bezeichnet und aus normalisiertem Equity-Verlust und den nativen relevanten Entscheidungen berechnet. Fehlende oder nur teilweise Analysen ergeben keinen scheinbar vollständigen Match-PR. Elo, Schacheröffnungen und schachspezifische Bewertungen werden nicht übernommen.

## Kommentare

Match, Partie und Checkerzug erhalten eigene Notizen und Tags. Bestehende Positionsnotizen bleiben erhalten. Tatsächlich gespielte Cube-Aktionen erhalten stabile Zug-IDs und ihre eigene Stellungsvorschau. Partien ohne Checkerzug bleiben kommentierbar. Alle Ebenen sind durchsuchbar, auf Wunsch nach Bestand eingeschränkt. Versionskonflikte überschreiben keine parallelen Änderungen.

## Prüfung

Backendtests prüfen Migration, Duplikate je Bestand, unabhängige Kopien, atomare Aktionen, Filterpagination, Notizsuche und PR gegen GNUbg. Browsertests prüfen Bestandswechsel, Filter, Mehrfachaktionen, Vorschau und Notizen bei Partiewechsel. Bestehende Import-, Analyse-, SSE-, Lern-, Theme- und Einstellungstests bleiben erhalten.

Quellen: [ChessBase-Vorschau](https://help.chessbase.com/CBase/16/Eng/preview_window_in_the_database.htm), [Datenbanken organisieren](https://en.chessbase.com/post/chessbase-18-tips-for-beginners-part-7-organising-databases), [GNUbg-Handbuch](https://www.gnu.org/software/gnubg/manual/gnubg.html).
