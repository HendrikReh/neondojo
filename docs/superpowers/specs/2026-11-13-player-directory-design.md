# Spielerdatenbank

Der Hauptmenüpunkt zeigt importierte Spieler mit editierbarem Vor- und Nachnamen, Match- und Spielanzahl, zugeordneten Aliasnamen und einer verlinkten Matchliste. Die Matchliste lässt sich nach Datenbank filtern. PostgreSQL speichert Profile, Aliaszuordnungen und die Spielerseiten eines Matches. Bestehende Importe werden bei der Migration zugeordnet, neue Importe in ihrer bestehenden Transaktion.

Online-Aliasse sind auf ihre Plattform bezogen. Die Zuordnung zu einer Person erfolgt explizit durch Hinzufügen eines Alias oder Zusammenführen von Profilen. Profiländerungen lassen die importierte Schreibweise am Match als Quellenangabe bestehen. Versionsprüfungen schützen Namensänderungen und Zusammenführungen.

Die PR verwendet die jüngste vollständige, gültige Matchanalyse je Analyseprofil. Sie wird mit dem vorhandenen GNUbg-PR-Verfahren über die Summen von Equity-Verlusten und Entscheidungen berechnet. Der Steckbrief zeigt die Zahl der beteiligten Matches. Fehlende Bewertungen bleiben leer.
