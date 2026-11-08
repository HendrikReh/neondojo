# BGBlitz-Themes und sechs Oberflächensprachen

## Entscheidung

Beide Funktionen passen zum bestehenden React-Client und benötigen keine Änderung am Headless-Backend. Die Sprachabstraktion wird vor weiterem UI-Ausbau eingeführt. Vorhandene lokale ZIPs werden ausgelesen; Originalgrafiken, XML-Koordinaten und Dateinamen bleiben erhalten. Ein neuer Renderer zeichnet dieselben Partiepositionen mit diesen Assets. Die drei bisherigen SVG-Themes bleiben als eingebaute Auswahl verfügbar.

## Umsetzung

1. Build-Werkzeug mit Python-Standardbibliothek: `themes/*.zip` erkennen, XML und PNG/JPEG-Maße validieren, nur referenzierte Grafikdateien in `web/public/themes/` bereitstellen und einen typisierten Katalog erzeugen. Keine Upload-/Datenbankfunktion. Neue lokale Archive werden beim Start/Build eingelesen; ohne lokale Archive funktioniert der Client mit eingebauten Themes.
2. SVG-Renderer für originale Hintergründe, Checker, Würfel, Doppler und Nummernleisten in den jeweiligen Originalkoordinaten. Absolute Brettkoordinaten, spielerbezogene Punktnummern, Bar und abgetragene Steine bleiben korrekt. Auswahl und Name sind im Client verfügbar und bleiben nach Neuladen erhalten.
3. Zentraler typisierter React-Sprachkatalog für Deutsch, Español, Français, Polski, Italiano und English. Standard bleibt Deutsch; Umschaltung ohne Neustart und Speicherung lokal. Vollständige sichtbare Oberfläche einschließlich Analyse, Suche, Lade-/Fehlerzuständen, Labels und zugänglichen Beschreibungen. Spieler-/Dateinamen und originale Enginezugnotation bleiben Daten.
4. Echte Archive und synthetische Defektfälle testen; Sprachkataloge auf Schlüssel/Platzhalter-Konsistenz testen. Browserprüfung aller Themes und Sprachen einschließlich Persistenz, Brettdaten, Bildladefehlern und schmalem Layout. Bestehende Frontend-Tests und Produktionsbuild ausführen; unabhängige Anforderungs- und Codeprüfung.

## Quellen und lokale Dateien

Theme-Spezifikation: https://www.bgblitz.com/Create_a_theme_How_to.txt
Downloadseite: https://www.bgblitz.com/themes.html
Weitere Namen/Urheberangaben: https://www.bgblitz.com/themes_jorge.html

Die bereitgestellten Archive sind lokale Drittanbieterdateien; generierte Grafiken/Kataloge werden beim Start/Build erzeugt. Herkunft und urheberrechtliche Hinweise werden unter `themes/README.md` dokumentiert. Das Projekt erteilt keine neue Lizenz an diesen Grafiken.

## Abschluss

Die inzwischen 13 lokalen Archive wurden vollständig im Browser auf dekodierbare Originalgrafiken geprüft. Beide unabhängigen Prüfungen sind ohne offene Befunde abgeschlossen. Regressionen sichern Dateinamen-Groß-/Kleinschreibung, Dopplerkoordinaten, Bar/abgetragene Steine, Sprache/Theme nach Neuladen und die Zuordnung eines Analyseergebnisses zum neu gestarteten Auftrag ab.

Verifikation: zwölf Frontend-Tests, sechs Python-Archivtests und acht Browsertests erfolgreich (26 insgesamt), TypeScript-/Produktionsbuild, Prettier, Python-Formatprüfung und `git diff --check` erfolgreich. Ein isolierter Build ohne lokale ZIPs und ohne generierten Katalog war ebenfalls erfolgreich. Das Backend liefert den aktualisierten Client lokal unter Port 8787 aus.
