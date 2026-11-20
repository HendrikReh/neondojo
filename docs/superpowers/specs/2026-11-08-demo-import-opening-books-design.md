# Demo-Bestände, Analyseherkunft und Opening Books

## Auftrag und Grenzen

Der Benutzer hat die Neuinitialisierung der Matchbibliothek aus games/bgf, games/mat und games/xg mit je einem eigenen Bestand beauftragt. Bestehende Quellanalyse soll erhalten bleiben; Runde wird filterbar. Zusätzlich werden BackgammonDB-Produktideen und die konkrete Review-Oberfläche untersucht. Die Recherche ist eine priorisierte Empfehlung, keine pauschale Freigabe zur Implementierung aller Fremdproduktfunktionen.

## Demo-Bibliothek

Vorher liegt ein vollständiger lokaler PostgreSQL-Dump außerhalb des Git-Repositories. Drei temporäre Bestände werden durch den regulären REST-Importer befüllt und geprüft; erst danach ersetzt eine Transaktion die alte Matchbibliothek durch Demo BGF, Demo MAT und Demo XG. Der feste Standardbestand wird für BGF weiterverwendet, damit Standardimporte weiter funktionieren. Quelldateien, Buchdaten, Backups und Importberichte liegen ausschließlich lokal beziehungsweise außerhalb des Repositories. Lernkarten und Wiederholungsverlauf werden nicht durch einen Match-Reset gelöscht. Beschädigte XG-Dateien bleiben als abgewiesene Dateien im lokalen Bericht sichtbar.

## Erhaltene Quellanalyse

Importierte Resultate erhalten getrennte Datensätze mit Engine, Original-Tiefenbezeichnung, Methode, Kandidaten, Cube-Informationen, Equity-Einheit und Originaldaten. GNUBG, XG und BGBlitz werden nur dann benannt, wenn die Quelle das belegt. MAT ohne Analyse bleibt ohne Analyse. Eine neue GNUbg-Berechnung ergänzt die Quelle, ohne sie zu überschreiben. Quellen sind im Analyseinspektor umschaltbar; fehlende oder nicht vergleichbare Equity-Werte werden nicht vermischt. Match-PR aus importierten oder gemischten Tiefen wird nicht vorgetäuscht.

## Opening Books

BackgammonDB wird als reales lokales SQLite-Buch unter Application Support/NeonDojo installiert und mit Attribution (CC BY4.0) versehen. Lookup berücksichtigt Stellung, Wurf, normalisierte Away-Scores, Spieler am Zug, Cube, Crawford und Jacoby. Checkerantworten und Cube-Antworten ohne Würfel bleiben getrennte Fragen. Der Schlüssel ist kein reiner Checkerpositions-Hash. Ein Buchmiss lässt die normale Engineanalyse verfügbar.

Die Buchverwaltung ist von Einstellungen → Analyse sowie beim Analyseergebnis erreichbar. Ein eigener Dialog speichert Standardquelle und Buchpfad ausdrücklich. Im Analyseinspektor sind importierte Analyse, Buch und neue Engineberechnung als getrennte Quellen mit Engine/Version/Tiefe sichtbar. Das veröffentlichte Buch wird mit seiner eigenen GNUbg-Version angezeigt; ein automatischer Enginecache muss die native Version zusätzlich prüfen. Manuelle Engineaufträge bleiben echte neue Berechnungen. Die nachgereichte offizielle XG-Datei ist als zweite lokale Quelle ergänzt: eine Referenz der erreichten Stellung vor dem Wurf, ohne erfundene Kandidaten oder Cube-Verdikte. Jede Bewertung behält den ursprünglichen Autor, die Engineversion und die native Tiefenbezeichnung. Cube-Wert und -Besitz sowie Moneygame-Regeln werden exakt berücksichtigt; Beaver ist bei Punktmatches wirkungslos.

## Recherche und Validierung

Produktvorschläge werden auf bestehende NeonDojo-Bausteine bezogen: Position Browser und gespeicherte Filter, thematische Notebooks, spoilerfreies Lernen mit bestehendem FSRS/Anki, interaktive Match-/Partiestatistik, automatische Phasen und vollständige Sicherung. Die konkrete Oberfläche wird im Browser angesehen; nicht zugängliche Teile werden als solche benannt. Ein grüner Zugpfeil ist eine Empfehlung beziehungsweise klar begrenzte UI-Verbesserung und muss gespielten Zug, Kandidatenvorschau und die Stellung vor/nach dem Zug auseinanderhalten.

Tests prüfen Quellanalysen ohne Engineaufruf, Copy-Erhalt, Units/Depth, Canonical-XGID-Vektoren, Buchtreffer und Miss, Cube/Crawford, Rundenfilter sowie bestehende UI-/Import-/SSE-/Lernabläufe. Der lokale Demo-Reset wird erst nach diesen Schnittstellenänderungen ausgeführt.
