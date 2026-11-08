# BackgammonDB: Produkt- und UX-Auswertung für BG Tutor

Stand: 8. November 2026. Recherche über Blog, Handbuch, Opening-Book-Seite und eine tatsächlich geöffnete Review-Oberfläche. Empfehlungen sind von der in dieser Iteration umgesetzten Arbeit getrennt.

## Was tatsächlich angesehen wurde

Der angegebene Matchlink mit dem abschließenden Punkt ist nicht aufrufbar. Ohne Punkt zeigt die Website „This match isn’t on this device“: Ihre Matches liegen im lokalen Browserspeicher. Das konkrete Match kann deshalb hier nicht beurteilt werden. Über „Try a demo match“ ließ sich das eingebaute Match **Hideaki Ueda gegen Takahiro No, 9 Punkte** öffnen. Die folgenden Beobachtungen zur Review-Oberfläche stammen aus dieser Demo; es sind keine Behauptungen über das private verlinkte Match.

Auf dem Desktop steht eine durchgehende Aktionsliste links, ein großes Brett rechts oben und eine Kandidaten-/Vergleichsleiste darunter. Im Kopf bleiben Spieler, Spielstand und Zugänge zu Partie-/Matchstatistik erreichbar. Die Liste gliedert Aktionen nach Spielphasen und enthält Checkerzüge, No double, Double und Pass. Filter können offensichtliche Entscheidungen ausblenden. Die Modi Initial, Arrows und Result unterscheiden Stellung vor dem Zug, Pfeilvorschau und resultierende Stellung. Grüne Pfeile beziehen sich dabei auf den ausgewählten Kandidaten; im beobachteten Anfangszustand ist dieser zugleich der beste und gespielte Zug. Sie sind also nicht pauschal ein unabhängiger „letzter Zug“.

Referenzen: [angegebener Matchlink, ohne Satzpunkt](https://backgammondb.com/match/m-mw88v2zb-yfc2fqjh), [Game Review](https://backgammondb.com/manual/game-review/).

## Was BG Tutor davon übernehmen sollte

| Priorität | Vorschlag für BG Tutor | Begründung und konkrete Umsetzung |
| --- | --- | --- |
| P1 | Zugpfeile und eindeutige Brettmodi | „Vor dem Zug / Zugpfeile / Nach dem Zug“ für den ausgewählten Quellzug; gespielten Zug und gewählten Kandidaten sichtbar beschriften. „Bester Zug“ als gesonderte Vergleichsoption. Pfeile erhalten Kontur und Legende; Farbe allein reicht nicht. Bar-Eingaben, Treffer, Zwischenpunkte, Doppelwürfe und Bear-off verwenden dieselbe getestete Zug-/Brettlogik. Die Darstellung muss für alle BGBlitz-Themes funktionieren. |
| P1 | Quellenwahl bei den Kandidaten | Datei, Opening Book und neue Engineanalyse direkt am Ergebnis wählen. Engine, Version, Original-Tiefe und Equity-Einheit immer sichtbar. Diese Trennung ist in dieser Iteration umgesetzt. |
| P1 | Cube-Frage direkt anzeigen | „Soll ich doppeln?“ gegenüber „Annehmen oder passen?“ eindeutig unterscheiden. Angebotenes Cube-Level und Entscheider bleiben sichtbar. Eine Bewertung darf beim Vergleich nicht unbemerkt die Spielerperspektive wechseln. |
| P1 | Interaktive Partie-/Matchstatistik | Klick auf Fehlerzahl setzt Spieler-, Fehlerschwere- und Entscheidungstypfilter. Klick auf eine Partiezeile öffnet genau diese Partie. Bestehende Fehlerliste, Partiewahl und native GNUbg-Zusammenfassung liefern die Grundlage. |
| P2 | Gemeinsame Aktionsliste | Checker- und Cube-Aktionen im selben Verlauf; stabile Quellzug-IDs sind bereits vorhanden. Offensichtliche Entscheidungen optional ausblenden, ohne Nummerierung oder Rücksprungziel zu verändern. |
| P2 | Ausblendbare Teilflächen | Brettfokus behalten; Kandidatenfläche alternativ unter das Brett legen. Die zuletzt gewählte Anordnung wird lokal gespeichert. Die Datenbank bleibt eine eigene Ansicht. |
| P2 | Vergleichsansicht | Ausgewählter Kandidat und beste Alternative mit Equity-Differenz nebeneinander. Wahrscheinlichkeiten folgen dem aktuell betrachteten Kandidaten; Überschrift und Datenquelle ändern sich gemeinsam. |

Die Referenz macht viele Funktionen über kleine Symbole erreichbar. BG Tutor sollte zentrale Aktionen zusätzlich beschriften und auf schmalen Fenstern Reihenfolge statt Miniaturisierung verwenden. Kandidatenwechsel soll keine neue Engineberechnung auslösen, wenn das Ergebnis bereits vorliegt. Review-Hotkeys dürfen beim Schreiben von Kommentaren und in Dialogen nicht eingreifen.

Quellen für Statistikinteraktion und Kandidatenbezug: [Match & Game Stats](https://backgammondb.com/manual/game-review/match-and-game-stats/), [Release-Notiz zur Evaluationsleiste](https://backgammondb.com/blog/new-features-and-fixes/).

## Produktideen nach Nutzen und Abhängigkeiten

### 1. Position Browser mit gespeicherten Filtern — P1

Die Referenz bündelt analysierte Entscheidungen, Filter und benannte Voreinstellungen in einer positionsbezogenen Ansicht. Für BG Tutor wäre dies der nächste große Schritt: Treffer als Brettkarten oder dichte Liste, gefiltert nach Datenbank, Spieler, Entscheidungstyp, Fehlergröße, Würfeln, Score/Crawford, Tags und Kommentar. Aus Treffern startet man Analyse, Lernkarten oder einen Durchgang durch die Auswahl. Bereits vorhanden sind normalisierte Positionen, genaue Position-IDs, Notizsuche, Fehlerentscheidungen und Pagination.

Zusätzliche BG-Tutor-Regel: Identische Checkerstellung bedeutet nicht identische Lernfrage. Gruppierung muss Wurf, Score, Cube und Regeln berücksichtigen. Gespeicherte Filter gehören als versionierte JSON-Spezifikation ins Backend und werden von Positionssuche, Statistiken und Lernen gemeinsam benutzt. Ein optionaler Spielerprofilfilter sollte Aliasnamen verstehen.

Quelle: [Positions Browser](https://backgammondb.com/manual/positions-browser/).

### 2. Notebooks für eigenständige Stellungen — P1/P2

Sinnvoll sind benannte Sammlungen aus Matchpositionen, eigenen Aufstellungen und später XGIDs. Jeder Eintrag behält seinen eigenen Score, Cube, Kommentar und die gewählte Analyseherkunft. Ein Notebook ist eine thematische Lernsammlung, kein Ersatz für eine Matchdatenbank. Es sollte aus der Analyse mit einem einzigen „Zum Notebook hinzufügen“ erreichbar sein und direkt nach Anki beziehungsweise in unseren Lernmodus führen.

Dafür braucht BG Tutor eine eigenständige Stellung mit vollständigem Kontext sowie einen Notebook-Eintrag als eigene Notizebene. Man sollte keine künstlichen Ein-Zug-Matches erzeugen. Ein späterer Editor muss Checkeranzahl, Bar, Spieler, Würfel und Cube konsistent halten und Undo/Redo besitzen. Die vorhandenen Analyse- und Brettkomponenten werden wiederverwendet. Die Referenzdokumentation kennzeichnet Notebooks als feature-flag-abhängig; diese Beschreibung ist keine Aussage über deren universelle Verfügbarkeit.

Quelle: [Notebooks](https://backgammondb.com/manual/notebooks/).

### 3. Lernmodus gezielt erweitern — P1

Unser FSRS-Lernmodus und Rust-Anki-Export sind bereits eine gute Grundlage. Als Nächstes lohnen sich Sitzungen aus gespeicherten Positionsfiltern, „nur heute falsch beantwortet“, Sitzungen fortsetzen sowie ein sichtbarer Ergebnisrückblick. Vor der Antwort bleiben Kandidatenrang, Fehlerklassifikation, hilfreiche Tags und Buch-/Engineverdict verborgen. Die Auswahl eines Stapels darf die richtige Entscheidung nicht verraten.

Bei der Bewertung sollten nahezu gleichwertige Züge innerhalb einer nachvollziehbaren Toleranz als korrekt gelten. Jede Karte braucht eine feste Analyseversion und Einheit; ein Hintergrund-Reanalyseauftrag darf ihre Antwort nicht unbemerkt ändern. FSRS bleibt für langfristige Wiederholungen zuständig, ein unmittelbarer Fehlerdurchgang ist eine gesonderte Sitzungsfunktion. Eine pauschale Regel „dreimal richtig = endgültig gemeistert“ sollte unser bestehendes Wiederholungsmodell nicht ersetzen.

Quelle: [Study Mode](https://backgammondb.com/manual/study/).

### 4. Partie-, Match- und Bibliotheksstatistik — P1/P2

Für die aktuelle Analyseansicht zuerst: pro Partie Checker-/Cube-Verlust, relevante Entscheidungen, Fehlerhäufigkeiten und vorhandene PR-Werte. Fehlende Analyse wird als fehlend gezeigt. Die Referenz macht Zusammenfassungen durch anklickbare Zellen zu Navigation; das ist wertvoller als zusätzliche passive Kennzahlen.

Bibliotheksweit wären zeitlicher Verlauf und Fehlerverteilung hilfreich. Gesamt-PR muss entscheidungsgewichtet berechnet werden; der Durchschnitt einzelner Match-PRs ist dafür ungeeignet. Engine, Profil, Quelle und Analysevollständigkeit müssen filterbar bleiben. Unabhängige Kopien desselben Matches dürfen persönliche Trends nicht unbeabsichtigt mehrfach gewichten. Demo-/Testbestände sollten aus persönlichen Statistiken ausgeschlossen werden können.

„Luck“ ist keine andere Darstellung unserer beobachteten Würfelhäufigkeiten. Dafür werden passende native Luck-Werte oder eine definierte positionsabhängige Berechnung benötigt. Fehlende Werte sind nicht null.

Quellen: [Match & Game Stats](https://backgammondb.com/manual/game-review/match-and-game-stats/), [Library Statistics](https://backgammondb.com/manual/library-statistics/).

### 5. Automatische Positionsmerkmale — P2

Eine erste nützliche Klassifikation umfasst Eröffnung, Mittelspiel, Endspiel mit Kontakt, Rennen und Bear-off. „Phase“ betrifft das ganze Brett; Strukturmerkmale wie Haltespiel oder Backgame betreffen dagegen einzelne Seiten. Ereignisse wie Treffer gehören zum Zug. Diese Ebenen dürfen nicht zu einer beliebigen gemeinsamen Tagliste werden.

Für BG Tutor: deterministische Regeln in Rust, nachvollziehbare Kriterien und eine Klassifikatorversion speichern. Rennen erfordert nachweislich keinen möglichen Kontakt; ein Bear-off mit Kontakt ist gesondert zu behandeln. Eröffnungszuordnung benötigt Verlauf, nicht nur den Pipcount. Manuelle Tags bleiben unabhängig. Ein späterer Neuaufbau der automatischen Merkmale läuft als begrenzter Hintergrundauftrag mit Fortschritt. Zuerst belastbare Phasen, danach komplexe Strukturen und „verpasste“ Ereignisse aus dem Vergleich von gespielt/best.

Quellen: [Position Tagging](https://backgammondb.com/manual/position-tagging/), [Game Phases](https://backgammondb.com/manual/position-tagging/game-phases/).

### 6. Datenbankverwaltung und Sicherung — P1/P2

Mehrere Bestände, getrennte Kopien, Notizen und Mehrfachverwaltung sind in BG Tutor bereits umgesetzt. Zusätzlich sinnvoll: eine sichtbare Kennzeichnung von Demo-/Testbeständen, gespeicherte Suchansichten und ein vollständiges portables Bibliotheksbackup mit Schema-Version. Dieses sollte Quellen, Notizen, Quellanalyse, Engineergebnisse, Notebooks und Lernhistorie erhalten. Quellexport als MAT/XG/SGF ist weiterhin ein eigener Vorgang.

Das große Opening Book wird zweckmäßig über Anbieter, Version und Prüfsumme referenziert; nicht bei jedem kleinen Bibliotheksbackup erneut eingepackt. Für die jetzige Neuinitialisierung existiert ein überprüfter PostgreSQL-Dump außerhalb Git. Anders als die browserlokale Referenz hat unser Headless-Backend eine gemeinsame Datenhaltung und muss nicht an einen bestimmten Browser gebunden sein.

Quelle: [Database Management](https://backgammondb.com/manual/database-management/).

## Opening Books und Analysetiefe

Das neue BackgammonDB-Buch ist eine andere Datenquelle als ein XG-Resultat. Es adressiert Stellung plus Wurf und kanonischen Matchkontext, liefert geordnete Kandidaten und besitzt separate Cube-Fragen ohne Würfel. Die dritte Wurfebene ist beschnitten; ein fehlender Eintrag ist deshalb kein Importfehler. Buchantworten werden mit ihrer echten GNUbg-Version und Tiefe angezeigt. Die Baumabdeckung eines Spielstandkontextes ist nicht die Engine-Suchtiefe.

Die Auswahl gehört in die Analyse-Einstellungen als Standardquelle und an die Kandidaten als konkrete Vergleichsquelle. Ein manueller Engineauftrag bedeutet eine neue Berechnung. Für einen automatischen Enginecache muss zusätzlich die Engineversion passen. XG- und GNUbg-Plybezeichnungen werden nicht als gleich starke Qualitätsstufen behandelt; übernommene Labels wie XGRoller++ oder Rollout bleiben erhalten. Im derzeitigen Repository gab es vor dieser Arbeit keine eigene XG-Buchdatei-Abfrage; XG-Buchinformationen können aber in importierten XG-Analysen vorkommen.

Quellen: [Opening Book](https://backgammondb.com/opening-book), [Buch-Veröffentlichung](https://backgammondb.com/blog/opening-book/), [Ply-Bezeichnungen](https://backgammondb.com/blog/engine-ply/).

## Ergänzung: offizielle XG-Datei

Der nachgereichte [offizielle XG-Link](https://www.extremegammon.com/Openingbook.aspx) führt zur frei nutzbaren Datei mit dokumentiertem Binärformat. Sie ist in dieser Iteration als echte zweite Buchquelle ergänzt. Entscheidend bleibt die unterschiedliche Frage: XG liefert einen Wert der erreichten Stellung ohne Wurf, BackgammonDB bewertet die möglichen Züge bei einem konkreten Wurf. Es entsteht deshalb keine gemeinsame, scheinbar gleichartige Kandidatenrangliste.

Die XG-Ansicht zeigt jede Quelle beziehungsweise den ursprünglichen Rollout-Autor. Ihre Werte werden für den aktuellen Spieler am Zug dargestellt; Originalwerte und vollständige Datei bleiben erhalten. Buchversion3.70 ist nicht die Engineversion. Bei nicht eindeutig dokumentierten historischen Profiltiefen steht die native Bezeichnung. Cube-Wert/-Besitz und Beaver werden exakt abgeglichen. [Format und Nutzungsbedingungen](https://www.extremegammon.com/GetOb.aspx).

## Empfohlene Reihenfolge

1. Diese Iteration abschließen: verlässliche Quellanalysen, reales Buch, Rundenfilter und getrennte Demo-Bestände.
2. Zugpfeile mit Vorher-/Nachher-Modus und direkte Kandidatenvorschau; interaktive Partie-/Matchstatistik.
3. Position Browser und gespeicherte Suchfilter als gemeinsame Grundlage für Lernen und Statistiken.
4. Notebooks mit eigenständigen Stellungen und anschließend ein begrenzter Positionseditor.
5. Automatische Phasen/Strukturen und profilbezogene Fortschrittsstatistik auf dieser stabilen Datengrundlage.

Diese Reihenfolge verbessert zunächst den bestehenden Analysearbeitsplatz und verhindert, dass Notebooks, Suche und Lernen jeweils eigene inkompatible Stellungs- oder Filtermodelle entwickeln.
