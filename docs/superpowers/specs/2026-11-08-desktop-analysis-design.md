# Desktop-Analysearbeitsplatz

## Auftrag und Entscheidung

Die bestehende freigegebene Desktop-Ausrichtung wird umgesetzt: Brett, Zugliste, Kandidaten, Fehler und Statistiken sind in einer neuen Standardansicht **Übersicht** gleichzeitig zugänglich. Zielgrößen sind die genannten 1800×1169 und 3840×2160 sowie die unter macOS „More Space“ tatsächlich verfügbare logische Fensterfläche. Die Darstellung richtet sich nach CSS-Fenstermaßen; physische Auflösung und Skalierung werden nicht gleichgesetzt. Mobile Optimierung ist kein Ziel.

Eine zusätzliche Übersicht nutzt die bestehenden zustandsbehafteten Analysekomponenten. Ausschließlich einzelne Reiter beizubehalten würde weiterhin gegenseitig Informationen verdecken. Frei verschiebbare Fenster würden Docking, Persistenz und Tastatursteuerung unnötig erweitern; feste, nach Fenstergröße angepasste Bereiche reichen für diesen Schritt.

## Aufbau

Die Übersicht nutzt die volle Inhaltsbreite. Links steht die kompakte Zugliste. In der Mitte liegen Brett und Kandidaten nebeneinander; die filterbare Fehlerübersicht darunter. Rechts stehen Herkunft, Abdeckung, PR, Entscheidungszahlen und Fehlerstatistiken beider Spieler sowie die Partieübersicht. Lange Listen scrollen innerhalb ihres Bereichs. Die Kopfzeile für Spielauswahl und Analyseumfang bleibt kompakt. Die bisherigen Reiter Bewertung, Statistik, Fehler, Notizen und Lernkarten bleiben als Einzelansichten erhalten. Brettfokus blendet weiterhin die Zugliste aus.

Die Übersicht verwendet genau eine Instanz der Positionsbewertung und genau eine der Spiel-/Matchanalyse. Ein Reiterwechsel darf keine Abonnements, Filter, Notizentwürfe, Quellen- oder Kandidatenauswahl zurücksetzen. Kennzahlen filtern in der Übersicht die bereits sichtbare Fehlerliste; in den bisherigen Einzelansichten öffnen sie weiter den Fehlerreiter. Fortschritt und Fehler werden einmal angezeigt. Der anfänglich aktivierte Fehlerfilter bleibt beim Reiterwechsel unverändert; über den vorhandenen Schalter sind weiterhin alle Entscheidungen erreichbar. Fehlende oder partielle Analysen werden nicht als vollständige Statistik dargestellt.

## Konkretisierungen während der Umsetzung

- Die Brettschalter heißen in allen Sprachen wie ausdrücklich gewünscht **Initial**, **Arrows**, **Final**. Die redundante Zeile für den gespielten Zug unter diesen Schaltern entfällt. Eine gewählte Kandidatenvorschau behält Quelle und Rang sowie eine Rücksetzfunktion; die Zugnavigation bleibt erhalten.
- **Match öffnen** entfällt im Analysekopf. Die Matchdatenbank bleibt über die Hauptnavigation erreichbar.
- Die Matchtabelle zeigt keine redundanten Quelldateinamen. Ein eigener vergebener Anzeigename bleibt sinnvoll; Spielernamen und Metadaten bleiben sichtbar.
- Die Zugliste markiert belegte Ungenauigkeiten, Fehler und Blunder mit gelber, orangefarbener und roter Hervorhebung. Farbe wird durch zugängliche Bewertungs-/Quellenangaben ergänzt. Nicht analysierte oder nicht auswertbare Züge bleiben neutral. Native und importierte Werte behalten ihre Herkunft; Antwortwechsel dürfen keine fremden Spiel-/Auftragswerte zeigen.
- Die Meldung „11 Punkte, Ergebnis 9–9“ wird vom Original über die kanonische SGF und gespeicherte Metadaten bis zur Anzeige verfolgt. Das Ergebnis der letzten Partie muss einfließen, wenn aus der Quelle zuverlässig bestimmbar. Reparaturen bestehender automatisch abgeleiteter Metadaten dürfen keine bearbeiteten Nutzerdaten überschreiben; unbekannte, abgebrochene und Moneygame-Ergebnisse dürfen nicht erfunden werden.

## Prüfung und Grenzen

Browsertests prüfen die gleichzeitige Sichtbarkeit und nutzbare Flächen bei 1800×1169, 2560×1440 und 3840×2160, Kandidaten-/Zugvorschau, Kennzahlfilter, Notizen, Lernkarten und Rückkehr zur Übersicht. Weitere Regressionen prüfen klare Brettlabels und Ampelwerte einschließlich verspäteter Antworten. SQLx-Tests reproduzieren Ergebnis- und Bewertungsfälle in isolierten Datenbanken. Private Demo-Partien werden nur gelesen und bleiben außerhalb Git. Die Oberfläche erhält keine neue Analyseberechnung oder abweichende PR-Formel.
