# Zugvorschau und interaktive Statistiken

Der Auftrag setzt die bereits recherchierten P1-Verbesserungen um und erhält Import, Datenbanken, Notizen, Lernen, Quellenwahl, Themes und SSE. Der bisherige Stand wurde vor Beginn als 941cff0 gesichert.

## Brettvorschau

Am Analysebrett stehen Vor dem Zug, Zugpfeile und Nach dem Zug bereit. Ausgangsstellung, Stellung-ID, Notizen und Analyseziele bleiben die gespeicherte Entscheidung vor dem Zug; eine sichtbare Vorschau ändert keine Daten oder Aufträge. Standard ist Zugpfeile. Der gespielte Zug ist die Ausgangsauswahl; Kandidaten aus Quellanalyse, BackgammonDB oder lokaler Engine lassen sich ausdrücklich auswählen und auf den gespielten Zug zurücksetzen. Beschriftung nennt Quelle, Rang und Notation. XG-Stellungsreferenzen und Cube-Verdikte ergeben keine erfundenen Checkerzüge. Positions- und Quellenwechsel verwerfen fremde Kandidaten, auch bei verspäteten Antworten.

Die Projektion ist eine reine, getestete TypeScript-Funktion: Notation in Spielerperspektive, vollständiger legaler Wurf, Bar-Eintritt, Hits, Mehrfachzüge, Zwischenpunkte, Doppelwürfe, Bear-off und erzwungene Tänze. Unsichere oder ungültige Notation wird ausdrücklich als nicht darstellbar gemeldet; die Ausgangsstellung bleibt bedienbar. Grüne Pfeile mit heller/dunkler Kontur zeigen die einzelnen legalen Wege. Nach dem Zug zeigt die projizierte Checkerstellung bei fester Orientierung und ohne Würfel. Alle integrierten und BGBlitz-Themes verwenden dieselbe Projektion; SVG-Geometrie folgt jeweils dem tatsächlichen Theme. Die Vorschau ist über beschriftete Buttons mit Tastatur bedienbar, Farben sind nicht die einzige Information.

## Statistiken und Navigation

Ein neuer Reiter Statistik ergänzt Bewertung, Fehler, Notizen und Lernkarten. Er verwendet ausschließlich den explizit gewählten nativen GNUbg-Analyseauftrag (Partie oder ganzes Match) und zeigt Quelle, Tiefe und Vollständigkeit. Importierte oder Buchwerte werden nicht vermischt. Fehlende Analyse und unbekannte PR sind keine Nullwerte. Pro Spieler erscheinen Checker-/Cube-Entscheidungen, normierter Equity-Verlust, Fehlerschwere und verfügbare native PR. Partiezeilen zeigen Quellspielstand, Crawford und Analysestand.

Klick auf einen Gesamtwert öffnet die zugehörigen Entscheidungen; Fehlerzahlen setzen den Schwerefilter, Checker/Cube setzt den Entscheidungstyp und Spielerwerte setzen den Spielerfilter. Einzelne Partiezeilen öffnen genau die Quellpartie; ihre Metriken filtern zusätzlich diese Partie. Alle Filter sind sichtbar und zurücksetzbar. Neue Filter beginnen eine neue Pagination, und verspätete Antworten dürfen keine neueren Filter oder Partien überschreiben. Auch Weiter zum Fehler respektiert Spieler und Fehlerschwere. Cube-Entscheidungen behalten ihre eigene Zugnotiz-Navigation.

Das Backend liefert Statistik pro Auftrag mit einer Zusammenfassung und den enthaltenen Quellpartien. Vollständig abgeschlossene Partie-Checkpoints sind die Abdeckungsgrenze. Eine additive Migration speichert pro künftigem Checkpoint die native Partiestatistik; ältere Aufträge behalten Entscheidungssummen, aber keine erfundene Partie-PR. Ein Schwerefilter ergänzt die vorhandene paginierte Entscheidungs-API einschließlich Cursorprüfung und OpenAPI.

## Grenzen und Prüfung

Keine neuen Engines, Rollouts, Notebooks oder automatischen Tags. Alle sechs Sprachen bleiben vollständig. Regressionen prüfen legale Vorschau und Zwischenpunkte für beide Spieler, alle Themes, Kandidaten- und Positionswechsel, Statistikfilter und Scope, aktive/fehlgeschlagene/ältere Analysen sowie bestehende SSE-, Import-, Datenbank- und Lernabläufe. Browserfixtures laufen in einer wegwerfbaren PostgreSQL-Datenbank, damit die drei privaten Demo-Bestände unverändert bleiben.
