# BG Tutor: Analyse, Matchdatenbank und Einstellungen

Stand: 8. November 2026. **Vom Nutzer freigegeben; erste Etappe umgesetzt.** Die Anwendung hat jetzt die Hauptansichten Analyse, Matchdatenbank und Lernen sowie einen echten Einstellungsdialog. Bestehende Einzelspiel-/Matchanalyse, Notizen, Suche, Themes und Lernen bleiben erhalten. Verwaltung und erweiterte Metadatenfilter der Datenbank sowie Spielen/BGSage bleiben weitere Etappen. Details und Verifikation: [Umsetzungsplan](../plans/2026-11-08-analysis-workspace-settings.md). Der ursprüngliche anklickbare Konzeptentwurf verwendet weiterhin lokale Beispieldaten.

## Ziel und Entscheidung

Nach dem Öffnen steht die Matchanalyse im Mittelpunkt: Brett, Zugverlauf und Bewertung sollen gemeinsam sichtbar sein. Import, Verwaltung und Konfiguration haben eigene Orte. Die Anwendung bleibt React/TypeScript mit einem eigenständigen Rust-Backend über REST/SSE; die spätere Tauri-Hülle nutzt dieselben Verträge.

Drei betrachtete Ansätze:

| Ansatz | Vorteil | Nachteil | Entscheidung |
| --- | --- | --- | --- |
| Hauptnavigation mit Tabs, Einstellungen als Dialog | Drei aktuelle Arbeitsbereiche sind direkt erreichbar; keine dauerhafte Bibliothek neben dem Brett | Die Analyse braucht ihre eigene lokale Spielnavigation | **Empfohlen** |
| Dauerhafte vertikale App-Navigation | Gut bei vielen späteren Modulen | Nimmt dem Brett Platz; derzeit wenige Hauptbereiche | Später bei deutlich größerem Funktionsumfang prüfen |
| Eine Seite mit aufklappbaren Bereichen | Geringer Umbau | Behält den langen Seitenaufbau und konkurrierende Aufgaben | Für das neue Konzept ungeeignet |

Hauptnavigation: **Analyse · Matchdatenbank · Lernen**. **Spielen** wird als künftiger Bereich eingeplant und erst mit nutzbarem Mensch-gegen-Engine-Spiel regulär freigeschaltet. Im Konzept ist die geplante Ansicht ausdrücklich als Vorschau gekennzeichnet. **Einstellungen** ist ein beschrifteter Button mit Zahnrad am rechten Rand, kein zusätzlicher Haupt-Tab. Lernen/Anki bleibt als bereits vorhandene Funktion erhalten.

Startansicht ist Analyse: letztes geöffnetes Match, Spiel und Position wiederherstellen, sofern vorhanden. Ohne Match erscheinen „Match öffnen“ und „Importieren“ sowie eine kurze Liste zuletzt geöffneter Matches. Ein gelöschtes letztes Match führt zu diesem Leerzustand.

## Analyse als Arbeitsplatz

### Aufbau auf dem Desktop

- **Kopfzeile:** Matchname, Spieler, Matchlänge/Moneygame; Button „Match öffnen“ führt zur Datenbank. Ein kompakter Status nennt Engine und Qualität des angezeigten Ergebnisses.
- **Werkzeugleiste:** Spielauswahl mit Spielnummer/Startspielstand/Crawford, Umfang „Ausgewähltes Spiel / Gesamtes Match“, Analyseprofil und ein primärer Analysebutton. Standard bleibt ausgewähltes Spiel; keine Analyse startet allein durch Auswahl eines Matches oder Spiels.
- **Links:** kompakte Spielübersicht und Zugverlauf des gewählten Spiels. Fehler markieren zusätzlich zum Farbton ein Symbol/Text und den Equity-Verlust. „Nächster Fehler“ respektiert Spiel, Spieler- und Entscheidungsfilter.
- **Mitte:** größtes zusammenhängendes Element ist das Brett. Darunter vorheriger/nächster Zug, Zugnummer und gespielter Zug. Wechsel des Spiels lädt direkt dessen Positionen, keine vorherigen Spiele.
- **Rechts:** lokale Reiter **Bewertung · Fehler · Notizen**. Bewertung zeigt Kandidaten und Cube-Entscheidung; Fehler zeigt die gefilterte Liste für den gewählten Analyseumfang; Notizen enthält genau einen Editor mit Match-/Positionsauswahl. Ein Fehler öffnet die richtige Brettstellung, bei Bedarf das andere Spiel.
- **Dice Distribution:** als Unteransicht der Positionsbewertung öffnen; beobachtete Spielwürfe als zweite Unteransicht. Diese Ansichten ersetzen Kandidaten im Detailbereich, statt weitere Sektionen an die Seite anzuhängen.
- **Matchübersicht:** kompakte aufklappbare Zusammenfassung bei Matchanalyse, mit Spielerstatistik und analysierten Spielen. Kein erfundener PR-Wert: zunächst die tatsächlich gespeicherten Kennzahlen wie Equity-Verlust und Fehlerzahlen verwenden.

Vorhandene Notizen, Lernkartenübernahme, exakte Positionssuche und Anki-Export erhalten klare Zugänge: „Zu Lernkarten“ in der Positionsbewertung, Export in Lernen, globale/exakte Positionssuche in der Matchdatenbank. Tastenkürzel für Zugnavigation gelten nur außerhalb von Eingabefeldern und Dialogen.

Analyseprofiländerungen betreffen den nächsten Auftrag. Das sichtbare Ergebnis behält sein tatsächliches Engine-/Profilkennzeichen. Bei einem Wechsel des Umfangs bleiben Ergebnisse verschiedener Spiele/Engines strikt getrennt. Laufende Aufträge laufen beim Wechsel in Datenbank, Lernen oder Einstellungen weiter; ein unaufdringlicher globaler Fortschrittsindikator erlaubt die Rückkehr. SSE bleibt der Transport, mit bestehender Wiederaufnahme/Polling-Absicherung. Mehrere Aufträge sind einzeln identifizierbar; es darf kein Auftrag eines anderen Spiels als aktuelles Spielergebnis erscheinen.

### Kleine Fenster und Mobilgeräte

Unterhalb einer geeigneten Mindestbreite wird aus der linken Spielübersicht eine Spielauswahl. Brett und Werkzeugleiste bleiben oben; darunter wechselt ein lokaler Reiter zwischen Zügen, Bewertung, Fehlern und Notizen. Keine drei schmalen Spalten und keine horizontale Seitenverbreiterung. Breite Datentabellen scrollen nur innerhalb ihres Bereichs. In der späteren Desktop-App lassen sich Spaltenbreiten speichern; eine erste Web-Umsetzung benötigt dafür keine eigene Fensterverwaltung.

## Einstellungsdialog

Desktop: modaler Dialog mit Kategorien links und Formular rechts; auf Mobilgeräten eine bildschirmfüllende Dialogansicht. Fester Abschluss mit **Abbrechen / Speichern**, Schließen über Escape und Fokus zurück zum Auslöser. Ungespeicherte Änderungen werden beim Verwerfen kenntlich gemacht. Live-Themevorschau gehört nur zum Entwurf und wird bei Abbrechen zurückgesetzt; die gespeicherte Appdarstellung wechselt bei Speichern.

### Allgemein

- **Sprache:** Deutsch, English, Español, Français, Italiano, Polski, alphabetisch nach den sichtbaren Eigennamen. **English** als Standard einer neuen Installation bzw. ohne gespeicherte Wahl. Eine vorhandene explizite Sprachwahl wird beim Update erhalten.
- **Spielername:** frei editierbarer Anzeigename für künftige Spiele; benennt keine Spieler in importierten Matches um. Eine spätere Zuordnung mehrerer Importnamen zum eigenen Spieler ist ein gesondertes Feature.

### Analyse

- **Analyse-Engine:** zunächst GNU Backgammon. Open Sage/BGSage wird erst nach funktionierender Integration als auswählbare Engine angeboten; vorher ausdrücklich „geplant / nicht verfügbar“.
- **Standardprofil:** zunächst „Schnell — GNUbg 0-ply“, „Vertieft — GNUbg 1-ply“, „Gründlich — GNUbg 2-ply“. Empfehlung für neue Installationen: schnell. Die tatsächlich gemessene Qualität wird bei Ergebnissen immer genannt.
- **Engineabhängige Konfiguration:** bei GNUbg Programmpfad, erkannte Version, Prüfstatus und „Installation prüfen“. Die Prüfung umfasst neben der Version eine kleine echte Positionsbewertung, damit fehlende Netzgewichte erkannt werden. Fehler am Feld nennen, z. B. Datei nicht gefunden oder Netzgewichte fehlen; bestehende funktionierende Konfiguration bleibt aktiv.
- **Programmpfad bezeichnet den Rechner des Backends.** Eine reine Weboberfläche öffnet hierfür keinen lokalen Dateidialog des Browserrechners. Später kann Tauri bei einem lokalen Sidecar einen nativen Pfadwähler verwenden.
- Aktuelle Aufträge behalten einen unveränderlichen Snapshot von Engine, Version, Profil und ausführungsrelevanter Konfiguration; neue Einstellungen gelten für neue Aufträge.
- GNUbg bleibt zunächst auch eine Abhängigkeit des MAT-Imports. Die Auswahl einer anderen Analyse-Engine beseitigt diese Importabhängigkeit nicht automatisch.

### Spielen

- Getrennte Wahl von **Spiel-Engine** und **Spielstärke**, unabhängig von der Analyse-Engine und Analysequalität. Änderungen hier beeinflussen keine bestehenden Analysen.
- Die Engineinstallation wird mit Analyse geteilt, nicht dupliziert. Der Dialog benennt die künftige Nutzung; bis zum Spielmodus startet er weder eine Partie noch simuliert er eine existierende Spielfunktion.

### Brett

- Galerie aller tatsächlich verfügbaren eingebauten und `themes/*.zip`-Themes mit Name und Vorschaubrett.
- Größere Vorschau mit heller/dunkler Seite, Würfeln, Doppler und Bar; dasselbe Board-Rendering wie in Analyse/Lernen verwenden.
- Auswahl ändert die Vorschau sofort. Anwenden speichert die Theme-ID für Analyse, Lernen und später Spielen. Bei fehlenden Bildern bestehendes Forest-Fallback mit verständlicher Meldung.
- Board-Theme und Oberflächendarstellung bleiben getrennte Konzepte. Die erste Umsetzung führt keinen zusätzlichen UI-Farbschemaeditor ein.

### Engineerweiterung

BGSage/Open Sage ist laut Primärquelle eine Bibliothek mit Python- und C++-Schnittstellen, kein zugesicherter GNUbg-CLI-Ersatz. Die Suchebene wird außerdem anders gezählt: Sage 1-ply entspricht der Zählkonvention von GNUbg 0-ply für die direkte Netzbewertung. Daher keine ungeprüfte gemeinsame numerische Ply-Einstellung und keine stillschweigende Gleichsetzung der Ergebnisqualität. Quelle: [Projekt-README](https://github.com/markbgsage/bgsage/blob/main/README.md).

Das Rust-Backend erhält ein Engineverzeichnis mit Fähigkeiten und verfügbaren Profilen sowie einen Adapter pro tatsächlich integrierter Engine. Import, Positionsmodell, Jobs/SSE und UI hängen an diesem Vertrag. GNUbg wird zuerst in diesen Vertrag überführt; eine C++-Anbindung bzw. ein isolierter Sage-Adapter wird separat umgesetzt und anhand gleicher Positionen/Matchkontexte geprüft. Exakte Positions-IDs bleiben unabhängig von der gewählten Analyse-Engine verwendbar.

## Matchdatenbank als eigene Ansicht

### Standardansicht und Details

Eine breite, sortierbare Tabelle mit **Name · Spieler · Spieldatum · Typ/Länge · Ergebnis · Tags · Analysestatus**. Die Kommentarvorschau erscheint als zweite Zeile unter dem Namen oder in der Detailansicht; nicht jede Metadateninformation wird als weitere permanente Spalte dargestellt. Optionen für zusätzliche Spalten: Turnier, Importdatum, Format und Spieleanzahl. Unbekannte Werte stehen als „—“ und bleiben filterbar.

Ein Klick markiert einen Eintrag und öffnet seine Detailansicht mit vollständigem Kommentar, Tags und editierbaren Metadaten. Ein klarer Button oder Doppelklick **„In Analyse öffnen“** öffnet genau ein Match. Die Rückkehr stellt Filter, Sortierung und Markierung wieder her. Eine Positionssuche öffnet dagegen direkt das betreffende Spiel und den Trefferzug.

### Aktionen und Mehrfachauswahl

| Aktion | Einzeln | Mehrfach | Verhalten |
| --- | --- | --- | --- |
| In Analyse öffnen | Ja | Nein | Eine konkrete aktive Analyse; keine Dutzend gleichzeitig geöffneten Matchfenster |
| Importieren | Ja | Ja | Mehrere Dateien/Drag-and-drop; Fortschritt und Ergebnis pro Datei, Duplikate ausdrücklich ausweisen |
| Exportieren | Ja | Ja | Unveränderte Originaldatei oder kanonische SGF; mehrere Dateien als ZIP mit kollisionsfreien Namen |
| Umbenennen | Ja | Nein | Anzeigename ändert sich; Originaldateiname, Originalbytes, Hash und Positionen bleiben unverändert |
| Tags hinzufügen/entfernen | Ja | Ja | Mengenoperation; individuelle andere Tags bleiben erhalten |
| Kommentar/Metadaten bearbeiten | Ja | Nein | Vermeidet versehentliches Überschreiben unterschiedlicher Inhalte |
| Löschen | Ja | Ja | Konkrete Namen/Anzahl und Auswirkungen vor endgültiger Löschung zeigen; laufende Analysen als Hindernis ausweisen |

Checkboxen und Shift-Bereichsauswahl. „Alle auf dieser Seite“ ist der erste klare Umfang. Auswahl über Seiten bleibt als ausgewählte IDs erhalten; „Alle Treffer“ ist eine spätere ausdrücklich benannte Aktion, keine implizite Folge der Kopf-Checkbox. Nach Filteränderung wird die Auswahl geleert, damit ausgeblendete Datensätze nicht versehentlich betroffen sind.

Stapelaktionen liefern ein Ergebnis pro Eintrag; ein fehlerhafter Import löscht keine erfolgreichen Importe. Beim Löschen entfallen Original, Positionen, Quellannotation und zugehörige Analysen dieses Matches. Lernkarten und Lernfortschritt bleiben erhalten: das aktuelle Schema speichert Karteninhalte bereits unabhängig als JSON ohne Fremdschlüssel zum Match. Der Löschdialog weist ausdrücklich darauf hin, dass die Match-/Positionsnotizen als Quellannotation mitgelöscht werden. Karten aus entfernten Quellen werden als solche gekennzeichnet; keine stille Löschung von Decks oder Reviews. Ein dauerhafter Schnappschuss auch dieser Quellnotizen ist ein gesondert festzulegender Kartenvertrag, keine Zusage des bisherigen Datenmodells. Laufende Jobs werden zunächst nicht nebenbei abgebrochen: betroffene Matches sind nicht löschbar und werden mit Grund gemeldet.

Original-/SGF-Export enthält zunächst die jeweilige Datei, keine garantierte Sicherung aller BG-Tutor-Notizen und Lernstände. Ein vollständiger BG-Tutor-Backup-/Restore-Vertrag ist ein eigener Ausbau. ZIPs werden deterministisch benannt und mengen-/größenbegrenzt gestreamt; keine stillen Teilarchive.

### Suche und Filter

Eine gemeinsame Suchzeile für Namen, Spieler, Turnier und Kommentare; daneben **Filter** mit folgenden strukturierten Feldern. Verschiedene Filter werden mit UND kombiniert; mehrere Werte innerhalb eines Felds mit ODER. Ausgewählte Filter werden als einzeln entfernbare Chips angezeigt. Serverseitige Sortierung und Cursor-Paginierung verhindern das Laden der gesamten Bibliothek.

| Feld | Konkrete Bedeutung |
| --- | --- |
| Spieler | Weiß oder Schwarz; Suchname/Mehrfachwerte, optional Farbe |
| Datum | Spieldatum von/bis; ausdrücklich umschaltbar auf Importdatum; unbekannt separat |
| Tag | Match-Tags oder Match- plus Positions-Tags, klar als Filterumfang wählen |
| Kommentar | Freitext in Match-Kommentaren oder zusätzlich Positionskommentaren; Trefferart nennen |
| Position | Exakte GNUbg-Position-ID; optional vollständige Position:Match-ID zur Eingrenzung des Kontexts; Treffer zeigt Spiel/Zug und öffnet die Stellung |
| Turnier | Veranstaltung/Turniername aus Metadaten oder manueller Ergänzung |
| Matchname | Editierbarer Anzeigename, Originaldateiname bleibt zusätzlich durchsuchbar |
| Spielart | Punktmatch / Moneygame bzw. Money-Session; nicht anhand des Dateinamens raten |
| Ergebnis | Abgeschlossen / laufend / unbekannt, Sieger und Endstand bzw. Money-Punktbilanz; ohne eindeutig ausgewählten Bezugsspieler kein willkürliches „gewonnen/verloren“ |
| Matchpoints | Ziel-Matchlänge, z. B. 5/7/11 oder Wertebereich; für Money nicht als Zielpunktzahl interpretieren |

Die bestehende eigene Positionsspeicherung und die Indizes werden wiederverwendet. Kein clientseitiges Durchsuchen heruntergeladener Originaldateien. Ähnlichkeitssuche wird nicht als bereits vorhandene exakte Suche ausgegeben.

### Fehlende Metadaten

`MatchSummary` enthält derzeit Spieler, Format, Dateiname, Matchlänge, Positionszahl und Importzeit. **Spieldatum, Turnier, Anzeigename und dauerhaftes Gesamtergebnis fehlen als eigenständige Felder.** Das neue Metadatenmodell muss sie ergänzen. SGF-Ergebnisse werden bereits bei der Kontextrekonstruktion berücksichtigt, aber nicht als fertiger filterbarer Matchdatensatz gespeichert.

Für jeden ergänzten Wert gilt: aus einer verlässlichen Quelle importieren oder manuell setzen; fehlend bleibt NULL. Ein Importdatum darf nicht zum Spieldatum werden. Ein Zwischenstand oder ein vom Sieger abweichender letzter Checkermove darf nicht als Endergebnis gelten. Unvollständige XG-Saves und Money-Sessions behalten ihren Status. Originalmetadaten und bearbeitete Anzeigeangaben bleiben unterscheidbar; nachgetragene Werte verändern weder Analysekontext noch Source-Hash.

Bestehende Bibliotheken erhalten Metadaten durch eine versionierte, wiederaufnehmbare Nachbefüllung aus bereits gespeicherten Quellen; keine erneuten manuellen Imports. Pro Format wird dokumentiert und getestet, welche Felder verlässlich übernommen werden. Nicht verfügbare Turnier-/Datumsfelder bleiben editierbar und unbekannt, statt einen scheinbar vollständigen Filter anzubieten.

## Technische Aufteilung und Umsetzung in Etappen

1. **Navigation und Analysearbeitsplatz:** App-Rahmen, separate AnalysisWorkspace/MatchDatabase/LearningWorkspace, bisherige Komponenten weiterverwenden. Auswahlzustand bleibt beim Wechsel erhalten, Analyse als Start, genau ein Notizeditor. Einstellungen zunächst zusammen mit dem nächsten Schritt liefern, nicht als wirkungslosen Produktbutton.
2. **Einstellungen und Enginevertrag:** SettingsDialog mit echten gespeicherten Werten, sechs Sprachen/English-Standard, Spielername, Themegalerie, GNUbg-Pfadprüfung und Standardprofil. Backend-Engineverzeichnis/-konfiguration; Jobs erhalten Engine-/Profilsnapshot. Bestehende Jobs/Ergebnisse bleiben als GNUbg-Aufträge interpretierbar. BGSage und Spielen sind hier noch keine Integration.
3. **Datenbankverwaltung und Metadaten:** Schema/Backfill, echte Filter und serverseitige Sortierung, Details, Anzeigenamen, Original-/SGF-Export, Mehrfachimport und Auswahlaktionen. Lösch- und Lernkartenbeziehungen vor Freischaltung durch Integrationstests sichern.
4. **Spielen und Sage:** eigener anschließender Ausbau auf dem Enginevertrag; keine angeblich spielbereite Platzhalterseite in der normalen Navigation ausliefern.

Frontendzustand: Sprache, Theme und Layout lokal; Engineinstallation/-profile und persönlicher Spielername als App-Einstellungen im Rust-Backend, damit REST und spätere Tauri-App dieselben Werte verwenden. Fehlerhafte gespeicherte Theme-/Sprachwerte haben definierte Fallbacks. Dialogwerte werden erst nach erfolgreicher Validierung übernommen; beim Speichernfehler bleiben Eingaben erhalten.

Geplante API-Verträge: Engine-/Profilkatalog lesen, Einstellungen lesen/validieren/speichern, Metadaten bearbeiten, Matches gefiltert/sortiert listen, Mehrfachaktionen mit Einzelresultaten und Original-/SGF-/ZIP-Export. Ein genauer OpenAPI-Vertrag wird pro Umsetzungsetappe festgelegt und in die TS-Typen generiert. Die bestehenden Import-, Positions-, Analyse-, Lern- und SSE-Endpunkte bleiben verwendbar.

## Abnahmekriterien

- Ohne dauerhafte Bibliotheksleiste sind Brett, Zugliste und Kandidaten im üblichen Desktopfenster zusammen sichtbar.
- Einzelspiel bleibt Standard; gesamtes Match ist explizit erreichbar. Keine Analyse eines anderen Spiels oder einer anderen Engine wird als Ergebnis des aktuellen Umfangs angezeigt.
- Hauptansichtswechsel verliert weder aktive Position noch ungespeicherte Notizen; Hintergrundanalyse und SSE laufen weiter.
- Einstellungen öffnen/schließen per Maus und Tastatur; Abbrechen verwirft Entwürfe und Vorschau. Ein ungültiger Enginepfad aktiviert keine defekte Konfiguration.
- Neue Installation startet Englisch; vorhandene explizite Sprache bleibt erhalten. Alle Sprachen stehen alphabetisch und alle verfügbaren Themes sind mit Vorschau wählbar.
- Analyse- und Spielprofil sind getrennt; ein Profilwechsel schreibt keine alten Ergebnisse um.
- Mehrfachauswahl benennt ihren Umfang; Umbenennen bewahrt Originale/Duplikaterkennung. Löschung beschädigt weder laufende Jobs noch selbständige Lernkarten.
- Datums-/Ergebnisfilter unterscheiden unbekannte bzw. laufende Matches von abgeschlossenen; alle gewünschten Filter sind serverseitig geprüft.
- Exakte Positionssuche öffnet direkt Spiel und Zug; Bibliotheksfilter und Sortierung bleiben bei Rückkehr erhalten.
- Tastaturbedienung, Fokus, Labels und 390-px-Layout werden im Browser geprüft. SQLx-/GNUbg-Tests sichern neue Metadaten, Settings-Snapshots, Mehrfachaktionen und bestehende Importdateien ab.


## Prüfung des Konzeptentwurfs

Der isolierte Entwurf wurde in Chrome bedient: Hauptnavigation, Brettfokus, alphabetische Sprachliste/English-Auswahl, Themevorschau mit Abbrechen und Übernehmen, Datenbankfilter, Mehrfachauswahl, Tags ergänzen, Anzeigenamen ändern und Rückkehr zur Analyse. Layoutprüfung bei 736, 390 und 320 Pixeln: kein horizontaler Dokumentüberlauf; Einstellungsdialog und Abschlussaktionen erreichbar. Keine JavaScript-Laufzeitfehler in diesen Abläufen. Die Vorschau verwendet keine API-Anfragen und verändert keine importierten Matches oder App-Einstellungen; sie zeigt schematische Bretter statt der originalen ZIP-Bilder. Sie ersetzt keine Funktionsprüfung einer späteren Umsetzung.
