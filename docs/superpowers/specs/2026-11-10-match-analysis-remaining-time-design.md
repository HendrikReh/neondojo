# Geschätzte Restzeit während der Matchanalyse

Stand: 2026-11-10. Status: Benutzerfreigabe erteilt; implementiert und geprüft.

## Ziel und Darstellung

Im bestehenden Fortschrittsbereich einer laufenden Spiel- oder Matchanalyse
steht zusätzlich „Geschätzte Restzeit: ca. …“. Die Zeit bleibt eine Prognose.
Die Startschätzung vor dem Analyseauftrag bleibt wie bisher bestehen.

Der Engineadapter liefert echte Fortschrittsmeldungen derzeit je vollständig
analysiertem Spiel. Die neue Anzeige behauptet daher keine einzelnen laufend
bestätigten Zugabschlüsse innerhalb eines Spiels. Sie berechnet nach jedem
bestätigten Spiel neu und zählt dazwischen die Zeit herunter.

Wartende Aufträge zeigen „Wartet auf Analysebeginn“. Bei Fehler oder Abschluss
verschwindet die aktive Restzeit. Wenn die Prognose innerhalb des aktuellen
Spiels aufgebraucht ist, während GNUbg noch arbeitet, erscheint „Restzeit wird
neu geschätzt“ statt einer irreführenden Null oder einer negativen Dauer.
Ein eigener Bestätigungsdialog ist nicht erforderlich.

## Berechnung

N sei die Zuganzahl des tatsächlichen Auftragsumfangs, C die Zahl der bereits
bestätigten analysierten Checkerzüge, T die kumulierte gemessene Analysezeit
für die tatsächlich zeitgemessenen Spielabschnitte, M deren Checkerzugzahl und A die aktive Zeit seit dem jüngsten
Spielbeginn.

- Vor dem ersten verwertbaren Abschnitt: r = passende Benchmarkzeit pro Zug.
- Sobald M > 0 und T > 0: r = T / M, also das bisher tatsächlich gemessene Tempo.
- Prognose direkt nach einem Fortschrittscheckpoint: R = r × (N − C).
- Während des nächsten Spielabschnitts: angezeigte Restzeit = R − A.
- R − A <= 0 bei weiterhin laufendem Auftrag ergibt den Zustand „neu schätzen“.

Die bisher gemessene Geschwindigkeit ersetzt zunächst bewusst ohne komplexes
Glättungsmodell die Kalibrierung. Unterschiedliche Stellungstypen dürfen die
Prognose bei einem neuen Checkpoint erhöhen oder senken. Die Anzeige wird
sinnvoll auf Sekunden beziehungsweise Minuten gerundet.

Ohne passenden Benchmark bleibt die Anfangsrestzeit unbekannt. Nach einem
bestätigten Spiel mit Checkerzügen kann sie trotzdem aus den tatsächlichen
Messwerten entstehen. Abschnitte ohne Checkerzüge liefern allein keinen
brauchbaren Zugdurchschnitt; ihre gemessene Zeit wird für den späteren
kumulierten Arbeitsaufwand berücksichtigt. Es wird kein Ersatzbenchmark gestartet.

## Zeitmessung und Zustand

Die Engine/Ply/Installationsbasis und der Umfang gehören zum gestarteten Job,
nicht zu später geänderten Benutzerprofilen. Als Kalibrierung gelten dieselben
vollständigen kompatiblen Messungen wie bei der bereits vorhandenen Startprognose.

Das Backend erfasst den Beginn der tatsächlichen Analyse und die Dauer jedes
fertigen Spielabschnitts mit monotoner Zeitmessung. Warteschlangenzeit und
Stillstand zwischen Worker-Versuchen zählen nicht als Analysezeit. Zeitwerte und
bestätigte Zugzahlen werden zusammen mit den vorhandenen abgesicherten
Fortschrittscheckpoints dauerhaft gespeichert; keine Teilergebnisse eines
abgebrochenen Spielabschnitts werden als erledigte Arbeit gezählt.

Bei Wiederaufnahme bleibt die bereits bestätigte Arbeit erhalten. Der aktive
Zeitzähler des neu gestarteten Spielabschnitts beginnt von vorn. Veraltete
Worker dürfen weder Fortschritt noch Zeitwerte überschreiben. Ein Neuladen
bezieht die Restzeit aus dem gespeicherten Jobzustand. Alte Jobs ohne Zeitdaten
zeigen eine verständliche unbekannte Restzeit, bis neue verwertbare Messwerte
vorliegen; `created_at` wird nicht als vermeintlicher Analysebeginn verwendet.

Die Oberfläche nutzt vorhandene Job-Ereignisse und Aktualisierungen für neue
Fortschrittsdaten sowie einen lokalen Sekunden-Takt für den Countdown. Die
Zeitbasis wird beim Empfang neu verankert, sodass eine abweichende Browseruhr
keine grob falsche Restzeit erzeugt. Keine zusätzlichen Engineprozesse entstehen.

## Architektur und Prüfung

Die Implementierung ergänzt persistierte Timingdaten zum Matchanalysejob sowie
entsprechende API-Daten. Die Restzeitberechnung hat eine zentrale, testbare
fachliche Definition. Eine kleine Anzeige im vorhandenen Fortschrittsbereich
verwendet die bestehende Dauerformatierung und das vorhandene Sprachschema.

Zu prüfen sind Benchmarkstart, Wechsel auf tatsächliches Tempo nach Fortschritt,
gewichtete Zuganzahl statt ungewichteter Spielprozente, Sekunden-Countdown,
überschrittene Prognose, fehlender Benchmark, Nullzugabschnitte, Warteschlange,
Fehler/Abschluss, Neuladen, Worker-Wiederaufnahme und Schutz vor veralteten
Updates. Native Integration benötigt nur einen kurzen Testlauf auf kleiner
Referenz beziehungsweise kontrollierter Zeitquelle, keinen neuen 4-Ply-Benchmark.

Bei alten wiederaufgenommenen Aufträgen kann C bereits ungemessene Züge enthalten.
Daher bleibt M getrennt von C; nur M bildet den Nenner des gemessenen Tempos.
Die aktive monotone Dauer wird ungefähr jede Sekunde leasegesichert persistiert;
der Zeitstempel des Spielbeginns dient nur als Metadatum und Aktivitätskennzeichen.
