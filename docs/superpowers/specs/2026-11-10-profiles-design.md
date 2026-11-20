# Profile: funktionale und technische Spezifikation

Stand: 10. November 2026. Grundlage ist der vom Nutzer bestätigte Entwurf. Diese Spezifikation ist der fachliche Vertrag für die Umsetzung im vorhandenen Neondojo; Prüfergebnisse werden im Implementierungsplan festgehalten.

## 1. Ziel und Begriffe

Ein Profil ist eine benannte, vollständig wiederherstellbare Konfiguration. Es erleichtert beispielsweise den Wechsel zwischen schnellem Spielen, Training und gründlicher Analyse.

- **Profilstand:** ausdrücklich gespeicherter, versionierter Konfigurationsstand.
- **Aktives Profil:** das Profil, von dem die aktuelle Konfiguration ausgeht.
- **Aktuelle Konfiguration:** momentan angewendete Einstellungen. Sie wird automatisch dauerhaft gesichert, ohne dadurch den Profilstand zu überschreiben.
- **Geändert:** normalisierte aktuelle Konfiguration und Profilstand unterscheiden sich in mindestens einem enthaltenen Feld.
- **Dialogentwurf:** noch nicht angewendete Eingaben, beispielsweise im Einstellungsdialog. Sie sind keine aktuelle Konfiguration.
- **Sitzungskonfiguration:** beim Start einer Partie oder eines Analyseauftrags erfasste Werte. Ein Profilwechsel verändert diese nicht.

Profile sind vollständige Stände; Teilprofile, Vererbung und Kombinationen mehrerer Profile gehören nicht zur ersten Version.

## 2. Geltungsbereich und Einstellungsinventar

Die erste Version erfasst alle bestehenden persönlichen Konfigurations- und Ansichtseinstellungen aus der folgenden Tabelle. Auswahl, Entwürfe und Fortschritt einer konkreten Aufgabe werden ausdrücklich getrennt behandelt.

| Konfigurationsbereich | Inhalt                                                                                                                                             | Bestehende Quelle / Integrationspunkt                                           |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Allgemein             | Spielername, Sprache, Spielsound an/aus                                                                                                            | `settings.rs`, `i18n-core.ts`, `play-audio.ts`                                  |
| Analyse               | Analyse-Engine, Standard-Suchtiefe, Standardumfang Spiel/Match                                                                                     | `settings.rs`, `AnalysisWorkspace.tsx`, `MatchAnalysisPanel.tsx`                |
| Engine-Suchfilter     | Kennung der derzeit verwendeten Engine-Suchstrategie: `engine_default`                                                                             | `engine.rs`; derzeit keine frei einstellbare Movefilter-Konfiguration           |
| Spielen/Training      | Spiel-Engine, Spielstärke, Standardmodus, Tutor-Schwelle, automatische Trainingsanalyse, neue Partie: Menschenfarbe und Matchlänge                 | `settings.rs`, `PlayWorkspace.tsx`                                              |
| Darstellung           | UI-Theme, Hell/Dunkel/System, Board-Theme, Ausspielrichtung, Board-Vorschauart                                                                     | `ui-appearance.ts`, `ui-preferences.ts`, `settings.rs`, `AnalysisWorkspace.tsx` |
| Opening Book          | Provider `off`, `automatic`, `backgammondb` oder `xg`; Referenzen auf BGDB- und XG-Book                                                            | `opening_book.rs`, `OpeningBookDialog.tsx`                                      |
| Matchbibliothek       | Datenbankbereich, angewendete Bibliotheksfilter, Sortierspalte/-richtung, sichtbare Datenspalten, deren Reihenfolge und benutzerdefinierte Breiten | `MatchDatabase.tsx`, `library-api.ts`, `useLibraryColumns.tsx`                  |
| Positionsbrowser      | angewendete Filterdefinition einschließlich Sortierung, Spaltenbreiten, Sichtbarkeit des gespielten Zuges                                          | `PositionBrowser.tsx`, `position-browser-api.ts`, `position-table-columns.ts`   |
| Lernkarten            | ausgewähltes Deck/Alle Karten, angewendete Kartensuchfilter, Erzeugungsstandard Mindestverlust/Spieler                                             | `LearningPanel.tsx`, `card-search-api.ts`                                       |
| Notizensuche          | angewendete Suchkriterien Text/Tag/Art/Datenbankbereich; `null` bedeutet keine gestartete Suche                                                    | `AnnotationSearch.tsx`, `annotation-api.ts`                                     |
| Zug-/Fehleransicht    | Spieler-, Entscheidungsart-, Schwere- und Nur-Fehler-Filter                                                                                        | `AnalysisWorkspace.tsx`, `MatchAnalysisPanel.tsx`                               |

Abgrenzungen:

1. Analyse 0–4 Ply und Spielstärke 0–2 bleiben die vorhandenen Fähigkeiten. Engine-Auswahl wird je Rolle gespeichert; BGSage bleibt nicht verfügbar. Dieses Feature implementiert keine zusätzliche Engine.
2. Ein neuer frei parametrierbarer Engine-Movefilter ist nicht Bestandteil. `engine_default` dokumentiert die aktuelle Strategie. Suchfilter der Bibliothek, Positionen, Karten und Notizen werden unabhängig davon gespeichert.
3. Spaltenreihenfolge ist in Version 1 für die Matchbibliothek veränderbar: „Nach oben“/„Nach unten“ im Spaltenwähler, ohne Drag-and-drop. Auswahl-/Aktionsspalten bleiben fixiert. Andere Tabellen behalten ihre definierte Reihenfolge; vorhandene Breiten und Sichtbarkeit werden übernommen.
4. Book-Referenzen sind die vorhandenen absoluten Dateipfade `path` und `xg_path` plus Provider. Dateien werden weder kopiert noch eingebettet. Diese bewusste Anbindung an die aktuelle Installation wird in der Zusammenfassung sichtbar gemacht.
5. Engine-Installationspfad und ermittelte Engine-Version bleiben außerhalb des Profils. Bei Verwendung wird die ausgewählte Engine über die aktuelle Installation aufgelöst; gestartete Aufträge behalten ihren eigenen Snapshot.
6. Match-/Positionsauswahl, laufende Spielsitzung, aktiver Haupttab, Seiten-/Scrollposition, Pagination-Cursor, Zeilenauswahl, Suchtreffer, ausgewählter Zugkandidat, ausgewerteter Würfelwurf, Notizentwürfe und Lernfortschritt sind Arbeitszustand und kein Profilinhalt. Ein konkreter Spielnummernfilter bleibt ebenfalls beim aktuellen Match.
7. Ein Suchformular wird erst mit gültigem „Suchen“ zum angewendeten Profilwert. Unabgeschickte oder ungültige Eingaben bleiben Formularentwurf. Bibliotheksfilter, die bereits unmittelbar suchen, werden nach bestehender Validierung übernommen.
8. Die einmalige abweichende Ply-Auswahl unmittelbar vor einem Analyseauftrag bleibt ein Auftragsparameter. Der Profilwert ist der Standard für neue Aufträge. Änderungen der Standards in Einstellungen ändern die aktuelle Konfiguration.
9. Live-Änderungen von Modus/Tutor-Schwelle einer bestehenden Partie bleiben Sitzungsaktionen. Sie überschreiben weder Profil noch Standardwerte für neue Partien.

Neue persönliche Einstellungen müssen künftig einem dieser Bereiche zugeordnet und in Normalisierung, Zusammenfassung und Migration ergänzt werden. Eine nicht dokumentierte zweite dauerhafte Einstellungsquelle ist nicht zulässig.

## 3. Bedienung und Zustandsregeln

### 3.1 Kopfzeile

Die Kopfzeile zeigt immer `Profil: <Name>`; bei Abweichungen zusätzlich den Text `geändert`. Während des initialen Ladens steht dort `Profile werden geladen`. Bei einem Ladefehler werden Fehler und Wiederholen angeboten, keine angeblich aktive Auswahl.

Ein zugänglicher Auswahlknopf öffnet die Profile alphabetisch, markiert das aktive und bietet „Profile verwalten“. Auswahl desselben Profils ist ohne Wirkung. Wiederherstellen seines Standes erfolgt ausdrücklich über „Änderungen verwerfen“.

Bei markiertem Änderungsstand bietet dieses Menü zusätzlich „Änderungen anzeigen“. Eine per Escape schließbare Übersicht nennt das aktive Profil und ausschließlich dessen abweichende Einstellungen mit Bereich/Feld, gespeichertem Wert und aktuellem Wert. Der Vergleich verwendet dieselbe Normalisierung und Darstellung wie die Profilverwaltung, einschließlich noch nicht serverseitig bestätigter lokaler Konfigurationswerte. Das bloße Öffnen und Schließen verändert nichts. Nach dem Schließen kehrt der Fokus zur Profilauswahl zurück; bei unverändertem Profil entfällt der Menüeintrag.

Jede abweichende Einstellung kann dort einzeln auf den gespeicherten Wert des aktiven Profils zurückgesetzt werden. Die Aktion übernimmt genau dieses Feld in die aktuelle Konfiguration und sichert es über den bestehenden atomaren, versionsgeprüften Schreibkanal; andere aktuelle Einstellungen und der gespeicherte Profilstand bleiben erhalten. Fehlende optionale Profilwerte werden entfernt, nullable Werte und ganze Arrays korrekt übernommen. Erforderliche kanonische Normalisierung bleibt wirksam: Spaltensichtbarkeit wird als Menge behandelt, deren Darstellung der separat geänderten Spaltenreihenfolge folgt; ein reiner Reihenfolgeunterschied erzeugt in dieser Übersicht keine zweite Sichtbarkeitszeile. Bei Erfolg aktualisiert sich die Übersicht ohne Schließen; nach der letzten Rücksetzung zeigt sie, dass keine Abweichungen mehr bestehen, und die Markierung „geändert“ entfällt. Während einer Rücksetzung sind weitere Rücksetzungen und das Schließen gesperrt. Bei Speicher-/Ressourcenfehlern bleibt der bisherige Wert sichtbar und der Fehler erscheint im Dialog; bei Versionskonflikten gibt es kein automatisches Überschreiben. Laufende Partien und Aufträge behalten ihre Snapshots.

Logisch gekoppelte Filter erscheinen als eine Einstellung und werden ausdrücklich gemeinsam zurückgesetzt: Würfelpaar, Matchpunktebereich und Equity-Verlustbereich der Positionssuche sowie Matchpunkte- und Datumsbereich der Matchbibliothek. Der Vergleich zeigt das gesamte Paar beziehungsweise Intervall; Einzelgrenzen werden nicht als separate Rücksetzungsaktion angeboten. So vertauscht die Normalisierung keine unbemerkt betroffenen Gegenwerte und erzeugt keine ungültigen Zwischenintervalle. Unbegrenzte Endpunkte bleiben optional. Spaltenbreiten und Kartenerzeugung erhalten einen eigenen Kontext im Feldnamen, damit sie von gleichnamigen Suchfiltern unterscheidbar sind.

Die Übersicht bietet zusätzlich „Alle Änderungen zurücksetzen“. Die Bestätigung erfolgt innerhalb desselben Dialogs mit dem betroffenen Profilnamen; Abbrechen oder Escape kehrt ohne Schreibzugriff zur Übersicht zurück. Nach Bestätigung stellt dieselbe atomare Profilwiederherstellung wie in der Profilverwaltung die vollständige aktuelle Konfiguration wieder her, ohne das gespeicherte Profil zu überschreiben oder Einzelaktionen nacheinander auszuführen. Die Bestätigung bindet sich an Profil-ID und Profilrevision; zwischenzeitlich geänderte Ziele werden nicht automatisch übernommen. Während der Bestätigung sind Einzelrücksetzungen gesperrt, während des Speicherns auch Bestätigung/Abbrechen/Schließen. Bei Fehlern bleiben die Werte bestehen und die Meldung im Dialog sichtbar; ein Ressourcenfehler erlaubt ausdrückliches Wiederholen, ein Versionskonflikt erfordert Konfliktauflösung. Nach Erfolg bleibt die Übersicht offen, zeigt keine Abweichungen mehr und bietet die Sammelaktion nicht mehr an. Neuere lokale Eingaben, die während der Operation eingehen, bleiben über die vorhandene Warteschlange erhalten.

Die Markierung „geändert“ betrifft den Profilinhalt. Ein zusätzlicher Status „Aktuelle Einstellungen werden gesichert“ beziehungsweise „Sicherung fehlgeschlagen“ bezeichnet die Persistenz. Ein geänderter Profilstand kann vollständig dauerhaft gesichert sein.

### 3.2 Einstellungen → Profile

Eigene Kategorie neben den bestehenden Kategorien. Liste mit Name, Aktiv-Markierung und Änderungsdatum. Detailansicht mit gegliederter vollständiger Zusammenfassung und einer Ansicht „Abweichungen zur aktuellen Konfiguration“ mit Feld, gespeichertem Wert und aktuellem Wert. Fachliche Bezeichnungen werden übersetzt; technische IDs/Pfade werden nur dort angezeigt, wo sie die Auswahl einer Engine oder Datei erklären.

Verfügbare Aktionen und eindeutige Wirkung:

| Aktion                                            | Wirkung                                                                                                             |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Aktuelle Konfiguration als neues Profil speichern | Vollständige aktuelle Konfiguration speichern und das neue Profil aktivieren; danach unverändert                    |
| Mit aktueller Konfiguration überschreiben         | Gewähltes Profil durch die aktuelle Konfiguration ersetzen; bei inaktivem Ziel bleibt das aktive Profil unverändert |
| Aktivieren                                        | Vollständigen Stand des gewählten Profils anwenden; Wechselregeln beachten                                          |
| Umbenennen                                        | Nur Name/Profilversion/Änderungszeit ändern, niemals Konfiguration oder Aktivierung                                 |
| Löschen                                           | Nur inaktive Profile löschen; Bestätigung mit Profilname erforderlich                                               |
| Änderungen verwerfen                              | Aktuellen Stand des aktiven Profils nach Bestätigung wiederherstellen                                               |

„Überschreiben“ zeigt vor der Bestätigung den Zielnamen und die Unterschiede. Bei identischem Inhalt erfolgt keine neue Inhaltsrevision. Namen werden mit Unicode-NFC normalisiert und außen getrimmt; 1–80 Unicode-Zeichen, keine Steuerzeichen, innerhalb der Installation nach PostgreSQL-`lower` ohne Beachtung der Groß-/Kleinschreibung eindeutig. Namensnormalisierung und Schlüsselbildung erfolgen zentral in PostgreSQL (`normalize(..., NFC)`, `lower`) und werden von Rust verwendet; ein eindeutiger Index sichert die Eindeutigkeit. Die Originalschreibweise bleibt sichtbar. Leerzeichen innerhalb des Namens werden beibehalten.

Das aktive Profil ist nicht löschbar; die Oberfläche erklärt „Zuerst ein anderes Profil aktivieren“. Es bleibt immer mindestens ein Profil. „Standard“ ist nach der Migration ein normales umbenennbares/überschreibbares Profil und darf nach Aktivierung eines anderen Profils gelöscht werden.

### 3.3 Anwenden und Wechseln

Die bisherige Einstellungsaktion „Speichern“ wird „Anwenden“. Sie übernimmt einen validierten Dialogentwurf in die aktuelle Konfiguration und sichert diesen, ohne ein Profil zu überschreiben. „Abbrechen“ verwirft nur den Dialogentwurf. Das Opening-Book-Formular folgt derselben Regel; Installation/Download eines Books bleibt eine eigene vorhandene Aktion.

Bei geänderter aktueller Konfiguration vor einem Profilwechsel:

- **Speichern und wechseln:** aktuelle Konfiguration in das bisher aktive Profil übernehmen, anschließend Zielprofil aktivieren; beides atomar.
- **Verwerfen und wechseln:** bisherigen Profilstand erhalten, Zielprofil aktivieren.
- **Abbrechen:** nichts ändern.

Existiert zusätzlich ein nicht angewendeter Einstellungsdialogentwurf, muss zuerst „Anwenden und fortfahren“, „Entwurf verwerfen“ oder „Abbrechen“ gewählt werden. Anschließend gelten die normalen Wechselregeln. „Anwenden und fortfahren“ ist keine Erlaubnis, das Profil zu überschreiben.

Der gleiche Entwurfsschutz gilt beim Eintritt in die Profilverwaltung und beim Speichern/Überschreiben eines Profils. Damit werden niemals scheinbar sichtbare, tatsächlich aber noch nicht angewendete Werte gespeichert. In der Profilverwaltung selbst existieren nur lokale Namenseingaben, keine zweite Konfiguration.

Ein fehlgeschlagener Wechsel verändert weder aktives Profil, bisherige Profilstände, aktuelle Konfiguration noch Darstellung. Ein zweiter Klick während einer laufenden Profiloperation ist gesperrt.

### 3.4 Wirkung auf laufende Arbeit

- UI-/Board-Theme, Sprache, Sound und Tabellenlayout wirken nach erfolgreicher Aktivierung sofort.
- Profilfilter werden angewendet; alte Suchanfragen werden abgebrochen beziehungsweise anhand einer Generation ignoriert. Pagination und Mehrfachauswahl werden verworfen. Eine fachlich unveränderte Suchdefinition löst keinen unnötigen Reset aus.
- Workspaces werden dabei nicht neu gemountet. Notizentwürfe, Jobabonnements und laufende Spielsitzungen bleiben erhalten.
- Neue Analyseaufträge und neue **Spielsitzungen/Matches** verwenden die neue aktuelle Konfiguration. Auch das nächste Spiel innerhalb eines bereits laufenden Matches gehört weiterhin zur bestehenden Sitzung.
- Bestehende Engine-/Ply-/Spieler-/Richtungssnapshots bleiben gültig. Insbesondere behält ein laufendes Match seine erfasste Ausspielrichtung; das Board-Theme darf sich trotzdem ändern.
- Die Änderung eines Lernkartenfilters unterbricht keine bereits aufgedeckte oder in Beantwortung befindliche Karte. Suchbrowser und Defaults ändern sich, der aktive Lernlauf behält seinen vorhandenen Kontext bis zum regulären Ende/Abbruch.
- Ein Profilwechsel startet niemals automatisch Analyse, Partie, Lernlauf, Import oder Book-Download. Er darf nur normale lesende Such-/Darstellungsabfragen auslösen.

## 4. Verfügbarkeit, Wiederherstellung und Fehler

Vor Aktivierung und vor „Änderungen verwerfen“ werden Konfiguration und aktuell erforderliche Ressourcen geprüft. Eine Prüfung allein ist noch keine Aktivierung. Der eigentliche Schreibvorgang prüft die Ressourcen erneut.

- Gewählte Engines müssen vorhanden sein und die gespeicherten Tiefen unterstützen. Keine automatische Reduzierung der Tiefe und keine andere Engine als Ersatz.
- `off` benötigt kein lesbares Book. `backgammondb`/`xg` benötigen die ausdrücklich gewählte Ressource. `automatic` benötigt mindestens ein unterstütztes Book; dessen bereits definierte Provider-Auswahl ist beabsichtigtes Verhalten.
- Gespeicherte Datenbank-/Deckreferenzen müssen existieren. Bei fehlenden Referenzen Aktivierung blockieren und betroffene Felder nennen. Keine stille Auswahl „Alle Karten“ oder einer anderen Datenbank.
- Unbekanntes Theme, unbekannte Spalte oder ungültiger Wert blockiert die Aktivierung, sofern keine explizite Versionsmigration existiert. Unverfügbare Profile bleiben einsehbar, umbenennbar und löschbar.
- Bereits aktive Konfigurationen dürfen durch eine vorübergehend ausgefallene Engine weiterhin lesbar sein. Das Ändern ausschließlich von UI/Filtern benötigt keinen Engine-Prozess. Nur geänderte Engine-/Book-Auswahlen werden beim normalen Anwenden geprüft; Aktivierung prüft alle benötigten Ressourcen.

Neustart: Die zuletzt serverseitig bestätigte aktuelle Konfiguration wird zusammen mit dem aktiven Profil geladen. Unterschiede bleiben als „geändert“ sichtbar. Ist inzwischen eine Ressource ausgefallen, bleibt die gespeicherte Konfiguration erhalten; die UI nennt das Problem und verhindert nur davon abhängige neue Arbeit. Es erfolgt kein automatischer Profilwechsel.

UI-Änderungen werden nach spätestens 300 ms Ruhe gesichert; Diskrete Aktionen wie Spaltenumschalten, gültige Suche und abgeschlossener Resize können sofort sichern. Während eines Drags bleiben Zwischenbreiten rein lokal. Vor Profiloperationen und dem Start neuer konfigurationsabhängiger Arbeit muss eine ausstehende Sicherung erfolgreich abgeschlossen sein. Vor lokalem Seitenverlassen wird ein noch ausstehender Stand synchron in einem Wiederherstellungsjournal abgelegt; ein serverseitiger Konflikt darf damit nicht überschrieben werden.

Fehlerstatus unterscheidet Validierung (422), fehlende Referenz/Profil (404), Versions-/Namenskonflikt (409), Infrastrukturfehler (5xx/Netzwerk). Fehlermeldungen enthalten Feldpfade und verständliche Gründe. Bei 409 bleiben lokale Änderungen erhalten; Aktionen: „Serverstand laden“ nach Verwerfbestätigung oder „Lokale Änderungen als neues Profil sichern“ auf Basis der frisch geladenen Revision. Kein automatischer Retry mit einer neuen Version und altem Vollstand.

## 5. Speicherung, Konsistenz und API

### 5.1 Gemeinsame Quelle

Profile und aktuelle Konfiguration liegen in PostgreSQL. Die bestehende Anwendung hat installationsweite Singleton-Einstellungen; Version 1 behält dieses Modell bei. Alle Browser derselben Installation teilen damit Profile **und** aktuelle Konfiguration. Es wird weder eine neue Benutzerverwaltung noch ein geräteübergreifender Synchronisierungsdienst eingeführt.

Frontend: ein `ConfigurationProvider` hält den bestätigten Stand sowie ausstehende lokale Änderungen. Komponenten lesen daraus und melden fachliche Änderungen über dessen Aktionen. `localStorage` dient nur als Startdarstellungs-Cache, Migrationsquelle und explizites Wiederherstellungsjournal. Es ist keine konkurrierende Einstellungsquelle.

Tabellen-Hooks werden kontrollierbar gemacht; bloßes Überschreiben von `localStorage` reicht nicht, da bereits gemountete Komponenten sonst ihre alten Werte behalten.

### 5.2 Datenmodell

`ProfileConfiguration` hat `schema_version: 1` und die typisierten Bereiche `general`, `analysis`, `play`, `appearance`, `opening_book`, `library`, `positions`, `learning`, `annotations`, `review` gemäß Inventar. Bestehende fachliche Filtertypen und Enumerationen werden wiederverwendet. Keine Profile-ID, Revisionsnummern, Verfügbarkeitsstatus oder Ergebnisdaten innerhalb des Konfigurationsvergleichs.

Tabellen:

- `configuration_profiles`: UUID, Name, normalisierter eindeutiger Namensschlüssel, `revision` (anfangs 1), `configuration` JSONB, Erstellungs-/Änderungszeit.
- `configuration_state`: Singleton, `revision` (anfangs 1), Fremdschlüssel `active_profile_id` mit Löschschutz, `current_configuration` JSONB, `legacy_preferences_imported`, Änderungszeit.

`schema_version` beschreibt das Datenformat, `revision` den erwarteten Stand einer Schreiboperation. JSONB wird am API-Rand strikt in Rust-Typen geprüft. Unbekannte Felder/Schema-Versionen werden zurückgewiesen. Zukünftige additive Änderungen benötigen eine explizite Migration von gespeichertem Profil **und** aktueller Konfiguration.

Die vorhandenen `app_settings` und `opening_book_settings` bleiben für Engine-/Jobverbraucher als Projektion verfügbar. Profilaktivierung, aktuelle Konfigurationsänderungen und Änderungen über die bestehenden PUT-Routen aktualisieren alle betroffenen Daten in **einer** SQL-Transaktion. Einrichtungsfelder wie `gnubg_path` bleiben unabhängig. Projektionen werden nur bei tatsächlichen Änderungen ihrer jeweiligen Werte revidiert.

Alle Schreibwege nehmen zuerst denselben Advisory-Transaction-Lock (auch vor Erzeugung des Singletons) und sperren danach in derselben Reihenfolge: Konfigurations-Singleton, Profilzeilen nach UUID, App-Settings, Book-Settings. Neue Job-/Sitzungssnapshots lesen die notwendige Projektion innerhalb einer konsistenten Transaktion. Ein paralleler Start erfasst entweder den alten oder den neuen vollständigen Stand.

### 5.3 Normalisierung und Änderungen

Eine fachlich versionierte Normalisierung:

- ergänzt explizite Standardwerte, ordnet Objektschlüssel stabil und verwendet vorhandene Filtervalidierung;
- unterscheidet fehlende Breitenüberschreibung von gemessenen Browserbreiten; automatische Layoutmessung macht das Profil nicht „geändert“;
- behandelt Spaltenreihenfolge als bedeutungsvolles Array und sichtbare Spalten als Menge in dieser Reihenfolge;
- vergleicht gespeicherten Modus `system`, nicht die gerade aufgelöste Systemfarbe;
- schließt Revisionen, Namen, Zeitstempel und Verfügbarkeitsinformationen aus.

Der Server liefert `dirty` und strukturierte Feldunterschiede aus dem kanonischen Vergleich. Das Frontend verwendet denselben fachlichen Vergleich für noch nicht bestätigte Änderungen; gemeinsame JSON-Testvektoren sichern die Parität.

### 5.4 API-Vertrag

Alle neuen Routen liegen unter `/api`. Ein Zustandsenvelope enthält `revision`, `active_profile`, `current_configuration`, `dirty` und `differences`. `active_profile` enthält ID, Name, Profilrevision und gespeicherte Konfiguration; dadurch ist der lokale Vergleich ohne zusätzlichen Request möglich. Eine Profildetailantwort enthält Name, Revision, Konfiguration und Zeitstempel. Listen enthalten keine vorausgesetzten Verfügbarkeitsprüfungen. Die einmalige Initialisierung liefert bei zurückgesetzten Legacyreferenzen zusätzlich `initialization_warnings`. Fehler der neuen Routen haben `{error, code, issues: [{path, code, message}]}`; vorhandene Clients können weiterhin `error` lesen.

| Methode / Route                       | Eingabe / Wirkung                                                                                                                                                                                 |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET `/configuration`                  | Aktuellen Stand lesen; noch nicht initialisiert: 409 `configuration_not_initialized`                                                                                                              |
| GET `/configuration/availability`     | Ressourcen der aktuellen Konfiguration ohne Schreibwirkung prüfen; Antwort `{revision, valid, issues}`. Der Client übernimmt Ergebnisse nur für dieselbe Zustandsrevision.                        |
| POST `/configuration/initialize`      | Validierte Browserpräferenzen plus erwartete App-/Book-Versionen; einmalig „Standard“ aus Backendstand + Browserwerten erzeugen; wiederholter Aufruf liefert bestehenden Stand ohne Überschreiben |
| PUT `/configuration`                  | `{expected_revision, configuration}`; aktuelle Konfiguration und Projektionen sichern, Profilstand unverändert                                                                                    |
| GET `/profiles`                       | Profile alphabetisch lesen                                                                                                                                                                        |
| GET `/profiles/{id}`                  | Vollständigen Profilstand lesen                                                                                                                                                                   |
| POST `/profiles`                      | `{name, expected_state_revision, configuration}`; aktuellen lokalen Stand speichern und neues Profil aktivieren; keine vorherige separate Sicherung nötig                                         |
| PATCH `/profiles/{id}`                | `{name, expected_state_revision, expected_profile_revision}`; umbenennen, Inhalt unverändert                                                                                                      |
| PUT `/profiles/{id}/snapshot`         | `{expected_state_revision, expected_profile_revision, configuration}`; aktuelle Konfiguration sichern und Zielprofil damit überschreiben; aktives Profil unverändert                              |
| DELETE `/profiles/{id}`               | `{expected_state_revision, expected_profile_revision}`; nur inaktiv, andernfalls 409 `active_profile`                                                                                             |
| POST `/profiles/{id}/validate`        | Erwartete Zielrevision; strukturierte Aktivierungsprobleme prüfen, nichts schreiben                                                                                                               |
| POST `/profiles/{id}/activate`        | `{expected_state_revision, expected_target_revision, expected_source_revision, source_action: "keep"                                                                                              | "save_current", current_configuration}`; aktuellen Stand berücksichtigen, ggf. Quellprofil speichern und Ziel atomar aktivieren |
| POST `/configuration/restore-profile` | `{expected_state_revision, expected_profile_revision}`; aktiven gespeicherten Stand nach Ressourcenprüfung anwenden                                                                               |

Aktivieren: `keep` entspricht „Verwerfen und wechseln“ oder einem unveränderten Ausgangsstand. `save_current` entspricht „Speichern und wechseln“. Der Server vergleicht erwartete Profilrevisionen selbst. Neue/überschriebene Snapshots werden typgeprüft gespeichert, auch wenn externe Ressourcen gerade fehlen; sie erhalten dadurch keine vorgetäuschte Verfügbarkeit. Sobald Speichern zugleich aktiviert, ist hingegen die vollständige Aktivierungsprüfung erforderlich.

Schreibantworten enthalten den neuen Zustandsenvelope und gegebenenfalls das veränderte Profil. Namensänderung/-löschung liefern zusätzlich den aktuellen Zustand, damit die Kopfzeile ohne weitere Rennen aktualisiert werden kann. Zustandsrevision steigt bei jeder tatsächlichen Mutation einschließlich Profilmetadaten; identische Wiederholungen sind ohne Wirkung. Veraltete Antworten dürfen nie einen neueren Zustand ersetzen.

### 5.5 Konkurrenz und Netzwerk

Alle Mutationen nutzen erwartete Revisionen. Ein sequenzieller Frontend-Schreibkanal verhindert, dass ein älterer Autosave nach einer Aktivierung ankommt. Ausstehende Text-/Filteränderungen werden zusammengefasst, nicht unkontrolliert parallel geschrieben. Spätere Antworten werden anhand von Anforderungsgeneration und Zustandsrevision abgegrenzt.

Beim Fokuswechsel zurück zur Anwendung wird der Serverstand gelesen. Ist lokal nichts ausstehend, wird ein neuerer Stand übernommen. Bei lokalen Änderungen wird ein Konflikt angezeigt. Innerhalb eines Browsers kann `BroadcastChannel` einen Refetch anstoßen; die maßgebliche Prüfung bleibt die Serverrevision. Eine sofortige Live-Aktualisierung anderer Browser ist nicht zugesagt.

Nach unklarem Netzwerkabbruch wird zuerst der Zustand gelesen. Eine Namensoperation oder Aktivierung wird nicht blind wiederholt. Ein lokales Journal enthält Basisrevision und Konfiguration. Stimmt die Serverrevision noch überein, kann der Client die ausstehende Konfiguration erneut sichern; andernfalls bleibt sie als explizit wiederherstellbarer Konflikt erhalten.

## 6. Erstübernahme und Einführung

1. SQL-Migration erzeugt die Tabellen, übernimmt aber keine erfundenen Browserwerte und ändert keine laufenden Sitzungen.
2. Beim ersten aktualisierten Browserstart werden bestätigte App-/Book-Einstellungen und validierte bestehende Browserwerte zu „Standard“ zusammengeführt. Nicht bisher persistierte Ansichtswerte erhalten ihre bisherigen Code-Defaults.
3. Übernommene Schlüssel: `neondojo.locale`, `neondojo.theme`, `neondojo.ui-appearance`, `neondojo.play-sounds`, `neondojo.library.visibleColumns`, `neondojo.library.columns`, `neondojo.positions.columns`, `neondojo.positions.showPlayed`, `neondojo.database`, `neondojo.deck`. Arbeitszustandsschlüssel wie `neondojo.workspace`, `neondojo.play-session` und `neondojo.jobs.*` bleiben getrennt.
4. Initialisierung ist transaktional und konkurrenzfest: Von zwei Browsern gewinnt genau einer. Andere Browser zeigen anschließend die bereits vorhandene zentrale Konfiguration; kein zweites „Standard“ und kein erneuter Import.
5. Beschädigte Legacywerte verwenden denselben dokumentierten Fallback wie bisher. Nicht mehr existierende Legacy-Datenbank-/Deckauswahl fällt ausschließlich bei dieser Erstübernahme mit sichtbarem Hinweis auf den bisherigen Standard zurück. Ein fehlender bisher konfigurierter Book-Pfad bleibt erhalten und wird als nicht verfügbar gemeldet.
6. Vorhandene Cache-Schlüssel bleiben für das frühe Theme-/Sprach-Rendering verwendbar, werden nach bestätigter Übernahme nur noch aus dem Provider aktualisiert. Wiederholtes Laden/Initialisieren verändert keine Profile.
7. Die Profilinitialisierung wird abgewartet, bevor neue Konfigurationsänderungen oder Job-/Sitzungsstarts möglich sind. Bestehende gespeicherte Partien und Ergebnisse bleiben lesbar, auch bei Initialisierungsfehlern.

## 7. Akzeptanzkriterien

| ID  | Nachweisbares Verhalten                                                                                                                                               |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P01 | Erstübernahme erzeugt genau „Standard“ mit bestehenden Backend- und Browserwerten; wiederholte/parallele Initialisierung ist ohne Datenverlust                        |
| P02 | Zwei Profile können sich in jedem enthaltenen Bereich unterscheiden; wiederholtes Wechseln stellt jeweils alle Werte auch in bereits gemounteten Ansichten her        |
| P03 | Eine Änderung zeigt „geändert“; Rückkehr zum gespeicherten Wert entfernt die Markierung, auch bei anderer Objekt-Schlüsselreihenfolge                                 |
| P04 | Automatische Sicherung überlebt Reload/Neustart, während der gespeicherte Profilstand unverändert bleibt; fehlgeschlagene Sicherung wird sichtbar                     |
| P05 | Neu speichern, aktives/inaktives Profil überschreiben und umbenennen haben genau die beschriebenen Aktivierungswirkungen                                              |
| P06 | Wechsel mit Änderungen unterstützt alle drei Entscheidungen; fehlgeschlagene Zielprüfung speichert auch das Quellprofil nicht teilweise                               |
| P07 | Aktives/letztes Profil kann nicht gelöscht werden; inaktive Löschung und normalisierte Namenseindeutigkeit sind serverseitig abgesichert                              |
| P08 | Fehlende Engine/Book/Datenbank/Deck und nicht unterstützte Tiefe werden feldbezogen angezeigt; keine stille Ersatzkonfiguration                                       |
| P09 | Laufende/queued/recovered Jobs und laufende Matches behalten ihre Snapshots; erst neue Arbeit verwendet die neue Konfiguration                                        |
| P10 | Profilwechsel löst keine schreibende Fachaktion aus, erhält Notizentwürfe und aktive Lernkarten und verwirft veraltete Suchantworten                                  |
| P11 | Veraltete Serverantworten, parallele Tabs, veraltete Profilrevisionen und Autosave während Aktivierung überschreiben keinen neueren Stand                             |
| P12 | Nicht angewendete Dialog-/Suchentwürfe gelangen nicht versehentlich in einen Profilstand                                                                              |
| P13 | Zusammenfassung enthält alle Bereiche; Unterschiede zeigen fachliche Feldnamen und beide Werte; systembedingte Darstellungsänderungen zählen nicht als Profiländerung |
| P14 | Kopfzeile und Verwaltung funktionieren per Tastatur mit Fokusführung und verständlichen Statusmeldungen; alle sechs vorhandenen Sprachen sind vollständig             |
| P15 | Darstellung bei 1440, 1024 und 390 Pixeln erzeugt keinen zusätzlichen horizontalen Seitenüberlauf; Profilname darf gekürzt, aber vollständig zugänglich bleiben       |
| P16 | Bestehende Einstellungs-/Book-Schreibwege aktualisieren den zentralen Stand konsistent; ausgefallene Ressourcen verhindern keine reine Theme-/Filteränderung          |
| P17 | Ein lokales Wiederherstellungsjournal wird nur bei passender Basisrevision automatisch übernommen; Konflikte bleiben sichtbar und ausdrücklich lösbar                 |

## 8. Bewusst außerhalb der ersten Version

Profilimport/-export, Cloud-Synchronisierung, Benutzerkonten, Profilhistorie/Rollback, automatische Profilwahl nach Workspace, vorkonfigurierte Trainingspakete, neue Engine-Movefilter und frei anpassbare Spalten aller Tabellen. Die vorhandenen Daten- und Analyseexporte bleiben eigenständige Funktionen.

## 9. Lieferumfang und Verweis

Implementierung erfolgt in den aufeinander aufbauenden Schritten des [Implementierungsplans](../plans/2026-11-10-profiles.md). Diese Datei ist die fachliche Referenz; ein abweichender Implementierungsbefund muss zuerst als konkrete Änderung dieser Regeln dokumentiert werden.
