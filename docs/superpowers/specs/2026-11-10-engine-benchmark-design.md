# Engine-Benchmark – Entwurf

Stand: 2026-11-10. Status: vom Benutzer freigegeben und implementiert; Abschlussprüfung erfolgreich.

## Ziel und Einordnung

Die Laufzeit der vollständigen Analyse eines unveränderlichen Referenzmatches
für eine Kombination aus Engine und Ply messen und dauerhaft vergleichen.
Die Funktion misst Geschwindigkeit, nicht Spielstärke oder Performance Rating.

Empfehlung: In der Kopfzeile öffnet „Extras“ eine eigene Werkzeugansicht mit
„Benchmark“. Der Benchmark erhält Konfiguration, Fortschritt und Historie auf
einer gemeinsamen Seite. Die Hauptarbeitsbereiche bleiben erhalten. Ein Link
unter Einstellungen → Analyse öffnet dieselbe Ansicht.

Alternativen: Ein eigener Haupttab ist direkter erreichbar, beansprucht aber
dauerhaft Platz für eine gelegentliche Aufgabe. Ausschließlich Einstellungen
→ Analyse ist kompakt, bietet einer wachsenden Ergebnishistorie aber wenig Raum.

## Referenz und Konfiguration

- Version 1 verwendet ein fest mitgeliefertes Referenzmatch: WC2014,
  Akiko Yazawa gegen Douglas Mayfield, 11 Punkte, 10 Spiele und 460 Checkerzüge.
  Ausgangspunkt ist die vorhandene unanalysierte `WC2014.gnubg.sgf`-Fixture.
- Eine eigene Referenzressource bekommt eine stabile ID, Version und SHA-256
  des tatsächlichen SGF-Inhalts. Die Fixture wird dabei nicht verändert.
- Referenzname, Spieler, Matchlänge, Spiel- und Zuganzahl werden vor Start gezeigt.
- Version 1 bietet keine freie Referenzauswahl. Dadurch sind die Ergebnisse
  innerhalb derselben Referenzversion unmittelbar vergleichbar.
- Nur tatsächlich verfügbare Engines sind auswählbar. Aktuell ist das GNUbg
  mit 0–4 Ply; BGSage bleibt entsprechend dem vorhandenen Engine-Katalog inaktiv.
- Eine oder mehrere Ply-Stufen sind auswählbar. Initial ist ausschließlich die
  aktuelle Analyse-Ply-Stufe ausgewählt. 3 und 4 Ply starten nur nach Auswahl.
- „Starten“ legt pro Kombination einen neuen Lauf an. Mehrere Kombinationen
  werden nacheinander ausgeführt. Erneutes Starten erzeugt neue Messungen.

## Messprotokoll

Die vorhandene native vollständige Spielanalyse wird für beide Spieler,
Checker- und Cube-Entscheidungen verwendet: deterministisch, cubeful, ohne
Rauschen und ohne Luck-Analyse. Vorhandene Bewertungen werden gelöscht und
das Referenzmatch wird vollständig neu analysiert. Kein Ergebnis-Cache oder
bereits abgeschlossener Matchjob darf als Benchmark-Ergebnis dienen.

Ein versioniertes Messprotokoll beschreibt die Analyseparameter und die
verwendete Prozessstrategie. Engine-ID, installierte Version und aufgelöster
Programmpfad werden beim Start festgehalten. Eine Versionsänderung während
des Laufs macht ihn ungültig.

Die Gesamtanalysezeit ist die mit monotoner Uhr gemessene Laufzeit der
vollständigen Analysepipeline über alle Referenzspiele. Sie umfasst die
sequenziellen Engine-Aufrufe mit Prozessstarts, Laden, Checker-/Cube-Analyse
und Ergebnisaufbereitung. Wartezeit in der Queue, Engine-Installationsprüfung,
Hardware-Erkennung, UI-Netzwerkzeit und das abschließende Speichern sind
ausgeschlossen. Der genaue Umfang wird in der Oberfläche erläutert.

„Ø Analysezeit pro Zug“ = Gesamtanalysezeit / vollständig analysierte
Checkerzüge, einschließlich erzwungener Züge wie im vorhandenen
Matchanalyse-Zugzähler. Cube-Analyse und Prozessaufwand sind enthalten.
Der Wert ist ein Matchdurchschnitt, keine einzeln gemessene Zuglatenz.
Gemessene Millisekunden und Zuganzahl werden unverändert gespeichert;
Rundung erfolgt erst in der Anzeige.

Benchmarkläufe werden serialisiert. Während einer Messung verhindert eine
gemeinsame backendseitige Koordination konkurrierende, von dieser Anwendung
gestartete Engine-Arbeit. Bereits laufende Engine-Arbeit wird nicht abgebrochen;
der Benchmark wartet darauf und beginnt erst danach mit der Zeitmessung.
Die Oberfläche zeigt den Wartezustand und den Grund einer blockierten neuen
Engine-Aktion. Sonstige Prozesse auf dem Rechner bleiben außerhalb der Kontrolle.

## Gespeicherter Datensatz

| Feld | Bedeutung |
| --- | --- |
| ID, Status | Eindeutiger Lauf; queued, running, completed, failed, cancelled |
| Datum | Angelegt, gestartet und beendet als UTC-Zeitpunkte; lokal anzeigen |
| Maschine | Stabile lokale Kennung und lesbarer Maschinenname des Backend-Rechners |
| Hardware | Betriebssystem, Architektur, CPU-Modell, logische CPUs, RAM; unbekannte Werte explizit leer |
| Engine | ID, Name, Version und aufgelöster Programmpfad als Snapshot |
| Ply | Ausgewählte Analysestufe |
| Protokoll | Version und vollständiger Snapshot der relevanten Analyseparameter |
| Referenz | ID, Version, Name, SGF-Snapshot, SHA-256, Matchlänge, Spiel-/Zuganzahl |
| Zeit | Gesamtanalysezeit in Millisekunden und daraus berechnete Ø-Zeit pro Zug |
| Fortschritt | Abgeschlossene Spiele und bestätigte analysierte Checkerzüge |
| Fehler | Verständlicher Grund bei Fehler, Versionswechsel oder Unterbrechung |

Maschinenangaben beziehen sich auf den ausführenden Backend-Rechner, auch
wenn die Oberfläche auf einem anderen Gerät geöffnet ist. Keine Seriennummern
oder Hardware-Adressen erheben. Fehlende optionale Hardwareinformationen
verhindern die Messung nicht. Ergebnisse hängen nicht von löschbaren
Bibliotheksmatches oder persönlichen Einstellungsprofilen ab.

## Ablauf, Fehler und Architektur

Der Client legt Läufe an und liest ihre Zustände über eigene Benchmark-API-
Endpunkte. Backend-Modul, Migration und Ergebnistypen bleiben vom fachlichen
Matchanalyse-Ergebnis getrennt. Die vorhandene Engine-Analyse wird wiederverwendet.
Der Hintergrundworker erhält Benchmark-Scheduling; es entsteht kein an einen
offenen Browser gebundener Messprozess.

Die Ergebnisseite zeigt den aktuellen Lauf mit Spiel-Fortschritt und „Abbrechen“
sowie eine nach Datum absteigend sortierte Historie mit Maschine, Engine-Version,
Ply, Referenz, Gesamtzeit und Ø-Zeit pro Zug. Fehler und abgebrochene Läufe sind
sichtbar, erhalten aber keine gültige Gesamt- oder Durchschnittszeit.

Navigation oder Schließen der Ansicht beendet keinen Lauf. Ein expliziter Abbruch
beendet auch den aktiven Engine-Prozess. Nach Backend-Neustart gilt ein zuvor
laufender Benchmark als unterbrochen; eine neue Messung beginnt von vorn.
Teilzeiten verschiedener Ausführungsversuche werden niemals kombiniert.
Queued-Läufe können nach Neustart weiterhin starten. Doppelte Verarbeitung wird
über atomare Beanspruchung und abgesicherte Abschluss-/Abbruchupdates verhindert.

Ein Ergebnis wird nur completed, wenn alle zehn Spiele und alle 460 Checkerzüge
bestätigt sind. Fehler, Zeitüberschreitungen oder eine veränderte Installation
erzeugen einen fehlgeschlagenen Datensatz. Auf Benutzerwunsch vom 2026-11-10
haben neue Benchmarkläufe kein festes Zeitlimit. Das Messprotokoll
`gnubg-match-v2-unlimited` speichert `game_timeout_seconds: null`; alte
Protokoll-Snapshots bleiben erhalten. Manueller Abbruch und Worker-Lease gelten
weiterhin. Eine unterbrochene Messung liefert keinen gültigen Teilbenchmark.

## Umsetzung und Prüfung

1. Referenzressource und versioniertes Messprotokoll mit Inhaltsprüfung anlegen.
2. Migration, persistierte Lauf-Snapshots und API mit OpenAPI-Typen ergänzen.
3. Hintergrundausführung, gemeinsame Engine-Koordination, monotone Zeitmessung,
   Abbruch und Behandlung unterbrochener Läufe implementieren.
4. Extras-Navigation, Benchmarkansicht, Fortschritt, Historie und Übersetzungen
   nach den vorhandenen Textnavigations- und Darstellungsmustern integrieren.
5. Messgrenzen und Division, vollständige Referenzabdeckung, Append-Historie,
   Snapshot-Stabilität, Serialisierung, Fehler, Abbruch und Neustart prüfen.
6. Mit echter GNUbg-Installation einen vollständigen 0-Ply-Lauf prüfen und
   Navigation/Neuladen einschließlich gespeicherter Historie im Browser testen.

Bestehende uncommittete Änderungen im Repository bleiben erhalten. Migrationen
werden anhand des bei Umsetzung aktuellen Bestands nummeriert.

## Präzisierungen bei der Umsetzung

- Engine-Versionsprüfungen vor/nach jedem Spiel liegen außerhalb der gemessenen
  Pipeline; zusätzlich muss die vom Analyseprozess gemeldete Version passen.
- Engine-Katalog und Health-Check nutzen während einer laufenden Messung den
  bereits geprüften Installations-Snapshot und starten keinen Probeprozess.
- Ein kompakter Aktivstatus hält den Wartehinweis in allen Arbeitsbereichen
  sichtbar, ohne die vollständige Historie im Hintergrund abzufragen.
- Ein Worker erneuert seine 30-Sekunden-Lease während der Messung. Verwaiste
  Läufe werden auch nach einem vorübergehenden Datenbankausfall automatisch
  als fehlgeschlagen abgeschlossen. Keine Wiederaufnahme von Teilzeiten.
- Ein neuer Start während eines laufenden Benchmarks liefert HTTP 409; mehrere
  Ply-Stufen werden gemeinsam vor Beginn als sequenzielle Läufe angelegt.
