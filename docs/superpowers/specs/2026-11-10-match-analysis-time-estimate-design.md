# Benchmarkbasierte Laufzeitschätzung vor der Matchanalyse

Stand: 2026-11-10. Status: Benutzerfreigabe erteilt; implementiert und geprüft.

## Verhalten

Direkt im bestehenden Analyse-Steuerbereich neben Umfang, Ply und Startknopf
erscheint vor dem Start eine voraussichtliche Analysezeit. Die Anzeige gilt für
den gewählten Umfang: ein einzelnes Spiel oder das gesamte Match.

Beispiel: „Geschätzte Analysezeit: ca. 40 Sekunden · GNUbg, 2 Ply, 1.000 Züge“.
Eine ergänzende Zeile nennt das Benchmarkdatum und erklärt die Rechnung als
Näherung. Die Schätzung beinhaltet Checker-/Cube-Analyse und Prozessstarts wie
die zugrunde liegende Benchmarkmessung. Wartezeiten anderer Aufträge sind nicht
enthalten. Es entsteht kein zusätzlicher Bestätigungsdialog.

## Berechnung und Auswahl

`estimated_ms = checker_move_count_in_scope × benchmark.average_move_ms`

Gezählt werden die tatsächlich importierten Checkerzüge einschließlich
Forced-Pass-Zügen entsprechend dem Benchmarknenner. Betrachtungsfilter für
Spieler, Fehlerart oder Schwere verändern diese Zahl nicht, weil sie den Umfang
des Analyseauftrags nicht begrenzen.

Der Server wählt den neuesten vollständig abgeschlossenen passenden Benchmark,
nicht einen abgebrochenen, fehlgeschlagenen oder noch laufenden Versuch. Er
prüft Engine-ID, kanonischen Installationspfad, tatsächlich installierte Version,
Ply, lokale Maschinenkennung und verfügbare Hardwaremerkmale. Ein älterer Lauf
einer anderen Installation oder Maschine wird nicht still als Grundlage benutzt.
Die vorhandenen erfolgreichen Messungen für 0–4 Ply werden weiterverwendet.
Die alten und neuen Timeout-Protokolle sind für erfolgreiche komplette Läufe
kompatibel, sofern alle übrigen Analyseparameter übereinstimmen: Das Zeitlimit
begrenzt einen Lauf, verändert aber keine abgeschlossene native Analyse.

Version und Hardware werden außerhalb einer Engine-Analyse ermittelt; eine
Schätzungsanfrage startet weder eine neue Referenzanalyse noch einen Matchjob.
Während eines laufenden Benchmarks kann die bereits validierte Engine-Version
aus dessen Snapshot verwendet werden. Eine gerade nicht ermittelbare Installation
führt zu einer unbekannten Schätzung statt zu einem lang blockierten Formular.

Die Formel wird zunächst bewusst einfach gehalten. Eine Medianbildung über
mehrere Läufe oder ein getrenntes Modell für Cube-/Prozesskosten sind spätere
Erweiterungen. Feste Faktoren je Ply würden vorhandene Maschinenmessungen
ungenutzt lassen und sind hier nicht vorgesehen.

## Anzeige und Grenzen

- Sekunden unter einer Minute, Minuten/Sekunden beziehungsweise Stunden/Minuten
  bei längeren Dauern; sinnvoll gerundet und ausdrücklich mit „ca.“ bezeichnet.
- Basis: Datum, Engine-Version, Ply und Anzahl der zu analysierenden Züge.
- Wechsel von Match, Spielnummer, Umfang, Ply oder angewendeter Konfiguration
  invalidiert die vorige Anzeige. Verspätete Antworten dürfen keine Schätzung
  für die neue Auswahl überschreiben.
- Laden zeigt „Dauer wird geschätzt …“. Netzwerkfehler oder fehlende passende
  Daten zeigen „Schätzung nicht verfügbar“ mit verständlicher Begründung.
- Bei null Checkerzügen wird keine Null-Sekunden-Schätzung ausgegeben: Ein
  solches Spiel kann dennoch Cube-Arbeit benötigen und hat keinen passenden
  zugbasierten Maßstab.
- Schätzungsausfall blockiert den Analysestart nicht. Vorhandene Quellen- und
  Wettbewerbssperren bleiben auch für die lesende Schätzungsanfrage wirksam.
- Die Anzeige ist eine Prognose vor dem Start, kein neuer Live-Restzeitmesser.

## Technische Einordnung und Prüfung

Ein lesender Matchanalyse-Schätzungsendpunkt berechnet Umfang und Basis zentral
im Backend. Eine kleine Frontend-Komponente/Hookschnittstelle bindet das Ergebnis
in `MatchAnalysisPanel` ein und folgt den vorhandenen Profil- und Übersetzungs-
mustern. Keine neue Migration und kein neuer dauerhafter Einstellungswert nötig.

Prüfung: exakte Formel, neuester passender abgeschlossener Lauf, Ausschluss
fremder Version/Hardware/Ply und ungültiger Ergebnisse, Match-/Spielumfang,
Nullzug- und fehlende Daten, zugriffsgeschützte Quellen, Antwortwechsel bei
schneller Navigation/Ply-Auswahl sowie sichtbare Schätzung vor dem Start ohne
zusätzlichen Analyseauftrag. Die Tests nutzen gespeicherte Messdaten; ein neuer
32-Minuten-Referenzlauf ist für die Verifikation nicht erforderlich.
